//! Metered-incremental (terms 3) payment runtime (pay §4/§4.1), the streaming
//! counterpart of the postpaid pair in [`crate::payments`]. Same pump
//! conventions (`Incoming` in, `Vec<Outgoing>` out) and the same idempotency
//! discipline (the operator + SettlementLedger settle each invoice-id once).
//!
//! The provider meters as it streams — a kind-3 usage-update CHUNK carrying
//! cumulative `Usage`, then a PAY_INVOICE with the delta amount and the
//! cumulative UsageWatermark — and the requester runs the six pay §4.1 checks
//! IN ORDER before paying, replying the specific core §9 code on the first
//! failure. Billing arithmetic is `map_pay::billing::billed` (exact, checked);
//! this module only sequences and orders the checks.
//!
//! INTERIM pause/resume (pay §4.1 leaves the wire signaling under-specified —
//! see the 2026-07-14 PENDING-ARCHITECT entry in map-spec/ERRATA.md): the
//! provider's [`MeteredProviderPayment::pause`] emits the spec'd ERROR 6001 +
//! `{1 => UsageWatermark}` detail; the requester's
//! [`MeteredRequesterPayment::retry_pending`] re-attempts a withheld proof
//! after a top-up; resume is the provider clearing its paused flag on the
//! covering `on_proof`. No new wire message is introduced.

use crate::Outgoing;
use map_core::cbor::{decode, encode, Value};
use map_core::errors::{codes, ErrorMsg};
use map_core::msg_type;
use map_identity::AgentId;
use map_pay::billing::billed;
use map_pay::codec::{
    pay_ref_digest, AssetAmount, PayAgree, PayInvoice, PayProof, PayQuote, PayReceipt, Usage, UsageWatermark,
};
use map_pay::credit_ledger::{verify_entry, EntryProof, LedgerError, LedgerOperator, CREDIT_LEDGER_METHOD};
use map_pay::{Amount, InvoiceId, Settle, SettlementLedger};
use map_session::Incoming;
use map_tasks::msg::ChunkMsg;
use map_tasks::{ChunkKind, TaskId};
use std::collections::HashMap;

fn err(code: u16, retryable: bool) -> Vec<Outgoing> {
    vec![(msg_type::ERROR, ErrorMsg::new(code, retryable).to_value())]
}

/// `a <= b` requiring the same asset AND exponent (pay §3: mantissa-only
/// comparison; a mismatch is not comparable and thus a violation).
fn le(a: &AssetAmount, b: &AssetAmount) -> bool {
    a.asset == b.asset && a.amount.exponent == b.amount.exponent && a.amount.mantissa <= b.amount.mantissa
}

// ---- provider -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeteredProviderState {
    AwaitAgree,
    Streaming,
    Paused,
    Cancelled,
}

/// A metered (terms 3) provider: bills incrementally at watermarks and settles
/// each invoice via the credit ledger. Model 1 (per-token) in this package.
pub struct MeteredProviderPayment {
    provider_aid: AgentId,
    requester_aid: AgentId,
    operator_aid: AgentId,
    task: [u8; 32],
    quote: PayQuote,
    quote_digest: Vec<u8>,
    ledger: SettlementLedger,
    receipts: Vec<PayReceipt>,
    /// Invoices this provider has billed, id → (amount, watermark). Not pruned
    /// on settlement — the ledger's settle_once provides idempotency, and a
    /// re-sent proof must still re-find its invoice.
    billed_invoices: HashMap<[u8; 16], (AssetAmount, UsageWatermark)>,
    /// Idempotency: the exact Outgoings emitted for each invoice-id.
    emitted: HashMap<[u8; 16], Vec<Outgoing>>,
    /// Cumulative-billed mantissa of the most recent invoice (delta basis).
    last_cumulative: i64,
    paused_on: Option<UsageWatermark>,
    state: MeteredProviderState,
}

impl MeteredProviderPayment {
    /// `quote` MUST be terms 3 with a metered pricing model and rate table
    /// (pay §3/§4). The digest is bound after `Session::send` via
    /// [`bind_quote_envelope`], exactly like the postpaid provider.
    pub fn new(provider_aid: AgentId, requester_aid: AgentId, operator_aid: AgentId, task: [u8; 32], quote: PayQuote) -> Self {
        MeteredProviderPayment {
            provider_aid,
            requester_aid,
            operator_aid,
            task,
            quote,
            quote_digest: Vec::new(),
            ledger: SettlementLedger::new(),
            receipts: Vec::new(),
            billed_invoices: HashMap::new(),
            emitted: HashMap::new(),
            last_cumulative: 0,
            paused_on: None,
            state: MeteredProviderState::AwaitAgree,
        }
    }

    pub fn state(&self) -> MeteredProviderState {
        self.state
    }
    pub fn receipts(&self) -> &[PayReceipt] {
        &self.receipts
    }

    /// Record the sealed PAY_QUOTE envelope bytes (I1, pay §3.1) so the incoming
    /// PAY_AGREE's digest can be checked against the exact received frame.
    pub fn bind_quote_envelope(&mut self, envelope: &[u8]) {
        self.quote_digest = pay_ref_digest(envelope);
    }

    /// Verify the PAY_AGREE binds this quote and picks the offered method.
    pub fn on_agree(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        let agree = match PayAgree::from_value(&inc.content) {
            Ok(a) => a,
            Err(_) => return err(codes::MALFORMED, false),
        };
        debug_assert!(!self.quote_digest.is_empty(), "bind_quote_envelope must precede on_agree");
        if agree.method != CREDIT_LEDGER_METHOD {
            return err(codes::METHOD_UNSUPPORTED, false);
        }
        if agree.quote_id != self.quote.quote_id || agree.quote_digest != self.quote_digest {
            return err(codes::AGREEMENT_DIGEST_MISMATCH, false);
        }
        self.state = MeteredProviderState::Streaming;
        Vec::new()
    }

    /// The exact cumulative bill for `usage` under this quote's model/rates.
    fn bill_cumulative(&self, usage: &Usage) -> AssetAmount {
        billed(self.quote.pricing_model, &self.quote.rate_table, usage).expect("provider rates bill exactly")
    }

    /// Emit a kind-3 usage-update CHUNK (cumulative `Usage`) followed by a
    /// PAY_INVOICE for the delta since the last invoice, carrying the cumulative
    /// watermark (pay §4.1). Idempotent by `invoice_id`: a re-billed id re-emits
    /// the identical Outgoings and does not re-advance the cumulative basis.
    pub fn bill(&mut self, usage: Usage, through_seq: u64, invoice_id: [u8; 16], now_ms: u64) -> Vec<Outgoing> {
        if let Some(out) = self.emitted.get(&invoice_id) {
            return out.clone();
        }
        let cumulative = self.bill_cumulative(&usage);
        let delta = AssetAmount {
            asset: cumulative.asset.clone(),
            amount: Amount::new(cumulative.amount.exponent, cumulative.amount.mantissa - self.last_cumulative),
        };
        let watermark = UsageWatermark { task: self.task, through_seq, usage, cumulative_billed: cumulative.clone() };
        let chunk = ChunkMsg {
            task: TaskId(self.task),
            seq: through_seq,
            kind: ChunkKind::UsageUpdate,
            content: encode(&usage.to_value()).expect("usage encodes"),
            fin: false,
        };
        let invoice = PayInvoice {
            invoice_id,
            quote_id: self.quote.quote_id,
            amount: delta.clone(),
            watermark: Some(watermark.clone()),
            method_payload: Vec::new(),
            timestamp_ms: now_ms,
        };
        let out = vec![
            (msg_type::CHUNK, chunk.to_value()),
            (msg_type::PAY_INVOICE, invoice.to_value()),
        ];
        self.billed_invoices.insert(invoice_id, (delta, watermark));
        self.emitted.insert(invoice_id, out.clone());
        self.last_cumulative = cumulative.amount.mantissa;
        out
    }

    /// Verify a PAY_PROOF's ledger entry, settle the invoice once, and reply
    /// PAY_RECEIPT. Clears a pause when the covering invoice settles (resume).
    pub fn on_proof(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let proof = match PayProof::from_value(&inc.content) {
            Ok(p) => p,
            Err(_) => return err(codes::MALFORMED, false),
        };
        let (amount, watermark) = match self.billed_invoices.get(&proof.invoice_id) {
            Some(x) => x.clone(),
            None => return err(codes::SETTLEMENT_FAILED, false),
        };
        let entry = match EntryProof::from_bytes(&proof.proof_payload) {
            Ok(e) => e,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if !verify_entry(&entry, &self.operator_aid, proof.invoice_id, &self.requester_aid, &self.provider_aid, &amount) {
            return err(codes::INVALID_PAYMENT_PROOF, false);
        }
        let out = match self.ledger.settle_once(InvoiceId(proof.invoice_id)) {
            Settle::FirstTime => {
                let rcpt = PayReceipt {
                    invoice_id: proof.invoice_id,
                    amount,
                    timestamp_ms: now_ms,
                    proof_digest: Some(pay_ref_digest(&proof.proof_payload)),
                };
                self.receipts.push(rcpt.clone());
                vec![(msg_type::PAY_RECEIPT, rcpt.to_value())]
            }
            Settle::AlreadySettled => match self.receipts.iter().find(|r| r.invoice_id == proof.invoice_id) {
                Some(r) => vec![(msg_type::PAY_RECEIPT, r.to_value())],
                None => err(codes::SETTLEMENT_FAILED, false),
            },
        };
        // `pending` is not pruned: a re-sent proof re-finds the invoice and the
        // ledger's settle_once makes the second settlement a no-op (idempotency).
        // Resume: if the settled invoice was the one we paused on, un-pause.
        if self.paused_on.as_ref() == Some(&watermark) {
            self.paused_on = None;
            self.state = MeteredProviderState::Streaming;
        }
        out
    }

    /// Pause an exhausted stream (pay §4.1): emit ERROR 6001 (retryable) whose
    /// detail carries the final unpaid watermark as `{1 => UsageWatermark}`.
    /// INTERIM trigger (see the module note / ERRATA): called by the runtime
    /// when an emitted invoice is not covered by a proof.
    pub fn pause(&mut self, invoice_id: [u8; 16]) -> Vec<Outgoing> {
        let (_, watermark) = match self.billed_invoices.get(&invoice_id) {
            Some(x) => x.clone(),
            None => return Vec::new(), // nothing outstanding to pause on
        };
        self.paused_on = Some(watermark.clone());
        self.state = MeteredProviderState::Paused;
        let mut em = ErrorMsg::new(codes::PAYMENT_REQUIRED, true);
        em.detail = Some(Value::Map(vec![(Value::Uint(1), watermark.to_value())]));
        vec![(msg_type::ERROR, em.to_value())]
    }

    /// The amount owed if the task is CANCELled now, billed from the last
    /// countersigned checkpoint's usage (pay §4.1; tasks §8.2 key 5). The caller
    /// passes the checkpoint usage IN — checkpoint state lives in the task
    /// runtime, not here (no circular dependency).
    pub fn owed_at_checkpoint(&self, checkpoint_usage: &Usage) -> AssetAmount {
        self.bill_cumulative(checkpoint_usage)
    }
}

// ---- requester ------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeteredRequesterState {
    AwaitQuote,
    Agreed,
    Paying,
    Insufficient,
    Failed,
}

pub struct MeteredRequesterPayment {
    expected_task: [u8; 32],
    requester_aid: AgentId,
    provider_aid: AgentId,
    /// Agreed billing basis, from the PAY_QUOTE (pay §3).
    model: u64,
    rate_table: Vec<(u64, AssetAmount)>,
    authorized_cap: Option<AssetAmount>,
    quote_id: Option<[u8; 16]>,
    /// Received kind-3 usage-update chunks, (chunk-seq, cumulative Usage) — the
    /// meter state pay §4.1 check 4 verifies invoices against.
    received_meters: Vec<(u64, Usage)>,
    last_accepted: Option<UsageWatermark>,
    /// Invoice-ids already accepted (their watermark checks passed once); a
    /// re-sent invoice re-pays idempotently without re-checking against the
    /// now-advanced watermark.
    accepted: HashMap<[u8; 16], AssetAmount>,
    /// An invoice we could not cover, awaiting a top-up (retry_pending).
    pending_unpaid: Option<PayInvoice>,
    receipts: Vec<PayReceipt>,
    state: MeteredRequesterState,
}

impl MeteredRequesterPayment {
    pub fn new(expected_task: [u8; 32], requester_aid: AgentId, provider_aid: AgentId) -> Self {
        MeteredRequesterPayment {
            expected_task,
            requester_aid,
            provider_aid,
            model: 0,
            rate_table: Vec::new(),
            authorized_cap: None,
            quote_id: None,
            received_meters: Vec::new(),
            last_accepted: None,
            accepted: HashMap::new(),
            pending_unpaid: None,
            receipts: Vec::new(),
            state: MeteredRequesterState::AwaitQuote,
        }
    }

    pub fn state(&self) -> MeteredRequesterState {
        self.state
    }
    pub fn receipts(&self) -> &[PayReceipt] {
        &self.receipts
    }

    /// Evaluate a metered PAY_QUOTE (terms 3) and agree. Records the billing
    /// basis; the authorized cap is the quote's maximum total (pay §3).
    pub fn on_quote(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let q = match PayQuote::from_value(&inc.content) {
            Ok(q) => q,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if q.task_binding != Some(self.expected_task) {
            self.state = MeteredRequesterState::Failed;
            return err(codes::TASK_BINDING_MISMATCH, false);
        }
        if now_ms > q.expiry_ms {
            self.state = MeteredRequesterState::Failed;
            return err(codes::QUOTE_EXPIRED, false);
        }
        if !q.methods.iter().any(|m| m.uri == CREDIT_LEDGER_METHOD) {
            self.state = MeteredRequesterState::Failed;
            return err(codes::METHOD_UNSUPPORTED, false);
        }
        self.model = q.pricing_model;
        self.rate_table = q.rate_table.clone();
        self.authorized_cap = q.total.clone();
        self.quote_id = Some(q.quote_id);
        self.state = MeteredRequesterState::Agreed;
        let agree = PayAgree {
            quote_id: q.quote_id,
            quote_digest: pay_ref_digest(&inc.raw),
            method: CREDIT_LEDGER_METHOD.into(),
            authorized_cap: q.total,
        };
        vec![(msg_type::PAY_AGREE, agree.to_value())]
    }

    /// Record a kind-3 usage-update CHUNK — the meter reading pay §4.1 check 4
    /// compares invoices against. Non-usage chunks are ignored here.
    pub fn on_chunk(&mut self, inc: &Incoming) {
        if let Ok(chunk) = ChunkMsg::from_value(&inc.content) {
            if chunk.kind == ChunkKind::UsageUpdate {
                if let Ok(u) = decode(&chunk.content).map_err(|_| ()).and_then(|v| Usage::from_value(&v).map_err(|_| ())) {
                    self.received_meters.push((chunk.seq, u));
                }
            }
        }
    }

    /// The latest metered usage at or before `through_seq` (pay §4.1 check 4).
    fn metered_usage(&self, through_seq: u64) -> Option<Usage> {
        self.received_meters
            .iter()
            .filter(|(s, _)| *s <= through_seq)
            .max_by_key(|(s, _)| *s)
            .map(|(_, u)| *u)
    }

    /// Verify a metered PAY_INVOICE against the six pay §4.1 checks IN ORDER and,
    /// on pass, pay via the ledger operator and reply PAY_PROOF. A rejected
    /// invoice does not advance the accepted watermark. Insufficient balance →
    /// no proof, `pending_unpaid` set for a later [`retry_pending`].
    pub fn on_invoice(&mut self, inc: &Incoming, operator: &mut LedgerOperator, now_ms: u64) -> Vec<Outgoing> {
        let inv = match PayInvoice::from_value(&inc.content) {
            Ok(i) => i,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if Some(inv.quote_id) != self.quote_id {
            return err(codes::SETTLEMENT_FAILED, false);
        }
        // Idempotency: a re-sent already-accepted invoice re-pays (the operator
        // dedups) without re-running the checks against the advanced watermark.
        if self.accepted.contains_key(&inv.invoice_id) {
            return self.pay_invoice(&inv, operator, now_ms);
        }
        // 1. presence: metered invoices MUST carry a watermark.
        let w = match &inv.watermark {
            Some(w) => w.clone(),
            None => return err(codes::INVOICE_WATERMARK_VIOLATION, false),
        };
        // 2. task binding.
        if w.task != self.expected_task {
            return err(codes::TASK_BINDING_MISMATCH, false);
        }
        // 3. monotonicity vs the last accepted watermark.
        if let Some(prev) = &self.last_accepted {
            let usage_monotone = w.usage.tokens_in >= prev.usage.tokens_in
                && w.usage.tokens_out >= prev.usage.tokens_out
                && w.usage.ttft_ms >= prev.usage.ttft_ms
                && w.usage.wall_ms >= prev.usage.wall_ms;
            if w.through_seq < prev.through_seq || !usage_monotone || !le(&prev.cumulative_billed, &w.cumulative_billed) {
                return err(codes::INVOICE_WATERMARK_VIOLATION, false);
            }
        }
        // 4. meter consistency: the watermark's usage MUST equal a usage we were
        //    actually metered at or before through-seq (a provider MUST meter
        //    before it bills).
        match self.metered_usage(w.through_seq) {
            Some(u) if u == w.usage => {}
            _ => return err(codes::INVOICE_WATERMARK_VIOLATION, false),
        }
        // 5. arithmetic: cumulative-billed == rate × usage, and the invoice
        //    amount is the delta since the last accepted watermark.
        let computed = match billed(self.model, &self.rate_table, &w.usage) {
            Ok(a) => a,
            Err(_) => return err(codes::INVOICE_WATERMARK_VIOLATION, false),
        };
        if computed != w.cumulative_billed {
            return err(codes::INVOICE_WATERMARK_VIOLATION, false);
        }
        let prev_billed = self.last_accepted.as_ref().map(|p| p.cumulative_billed.amount.mantissa).unwrap_or(0);
        let expected_delta = w.cumulative_billed.amount.mantissa - prev_billed;
        let amount_ok = expected_delta >= 0
            && inv.amount.asset == w.cumulative_billed.asset
            && inv.amount.amount.exponent == w.cumulative_billed.amount.exponent
            && inv.amount.amount.mantissa == expected_delta;
        if !amount_ok {
            return err(codes::INVOICE_WATERMARK_VIOLATION, false);
        }
        // 6. cap: cumulative-billed does not exceed the authorized cap.
        if let Some(cap) = &self.authorized_cap {
            if !le(&w.cumulative_billed, cap) {
                return err(codes::CAP_EXCEEDED, false);
            }
        }
        // Accept: record the id, advance the watermark, and pay the delta.
        self.accepted.insert(inv.invoice_id, inv.amount.clone());
        self.last_accepted = Some(w);
        self.pay_invoice(&inv, operator, now_ms)
    }

    fn pay_invoice(&mut self, inv: &PayInvoice, operator: &mut LedgerOperator, now_ms: u64) -> Vec<Outgoing> {
        match operator.pay(inv.invoice_id, &self.requester_aid, &self.provider_aid, &inv.amount, now_ms) {
            Ok(entry) => {
                self.pending_unpaid = None;
                self.state = MeteredRequesterState::Paying;
                let proof = PayProof {
                    invoice_id: inv.invoice_id,
                    method: CREDIT_LEDGER_METHOD.into(),
                    proof_payload: entry.to_bytes(),
                };
                vec![(msg_type::PAY_PROOF, proof.to_value())]
            }
            Err(LedgerError::Insufficient) => {
                self.pending_unpaid = Some(inv.clone());
                self.state = MeteredRequesterState::Insufficient;
                Vec::new()
            }
            Err(LedgerError::ExponentMismatch) => {
                self.state = MeteredRequesterState::Failed;
                err(codes::CAP_EXCEEDED, false)
            }
        }
    }

    /// Retry the invoice we could not cover, after the payer topped up its
    /// ledger balance (INTERIM resume, see the module note). Yields a PAY_PROOF
    /// once the balance suffices, or nothing while still short.
    pub fn retry_pending(&mut self, operator: &mut LedgerOperator, now_ms: u64) -> Vec<Outgoing> {
        let inv = match self.pending_unpaid.clone() {
            Some(i) => i,
            None => return Vec::new(),
        };
        self.pay_invoice(&inv, operator, now_ms)
    }

    pub fn on_receipt(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        if let Ok(r) = PayReceipt::from_value(&inc.content) {
            self.receipts.push(r);
        }
        Vec::new()
    }
}
