//! Prepaid (terms 1) and escrow (terms 2) payment runtimes (pay §5.2), on the
//! same pump conventions as [`crate::payments`] and [`crate::metered`]:
//! `Incoming` in, `Vec<Outgoing>` out, idempotent settlement, envelope-byte
//! quote digests (I1).
//!
//! Prepaid (§5.2.2): one pre-delivery invoice equal to the quoted total; the
//! provider's `execution_permitted()` stays false until it is *settled* — the
//! gate the task pump consults before starting billable work.
//!
//! Escrow (§5.2.3): TWO standard invoice→proof→receipt rounds. The hold round
//! locks the quoted total on the rail (kind-1 receipt; delivery keys off
//! *held*); after COMPLETE + verified transcript-root the release round moves
//! the final owed amount to the payee and refunds the remainder to the payer
//! in the same rail operation (kind-2 receipt). The release authorization is
//! PAYER-signed (authorization symmetry — the payee can never self-release);
//! on CANCEL the release anchors to the last countersigned checkpoint via the
//! §4.1 formula. Verification is [`map_pay::escrow::verify_escrow_invoice`];
//! this module only sequences.
//!
//! Method payload (pay §5.2.4, `pay:credit-ledger`): a phase-1 (hold) invoice
//! carries the hold's expiry-ts as a deterministic-CBOR uint in the opaque
//! method payload — the requester passes it to the operator, and the payee
//! verifies the resulting hold entry commits to exactly that expiry.

use crate::Outgoing;
use map_core::cbor::{decode, encode, Value};
use map_core::errors::{codes, ErrorMsg};
use map_core::msg_type;
use map_identity::{AgentId, Keypair};
use map_pay::billing::{billed, AgreedTerms};
use map_pay::codec::{
    pay_ref_digest, AssetAmount, PayAgree, PayInvoice, PayProof, PayQuote, PayReceipt, Usage,
    KIND_HELD, KIND_RELEASED, PHASE_HOLD, PHASE_RELEASE, TERMS_ESCROW, TERMS_PREPAID,
};
use map_pay::credit_ledger::{
    release_auth_bytes, verify_entry, verify_hold_entry, verify_release_entry, EntryProof,
    LedgerError, LedgerOperator, CREDIT_LEDGER_METHOD,
};
use map_pay::escrow::{verify_escrow_invoice, EscrowContext};
use map_pay::{InvoiceId, Settle, SettlementLedger};
use map_session::Incoming;
use std::collections::HashMap;

fn err(code: u16, retryable: bool) -> Vec<Outgoing> {
    vec![(msg_type::ERROR, ErrorMsg::new(code, retryable).to_value())]
}

/// Shared quote-side checks for both requesters (task binding, expiry, method,
/// required terms + total — pay §3/§5.2). Returns the agreed terms on success.
fn accept_quote(
    q: &PayQuote,
    expected_task: [u8; 32],
    required_terms: u64,
    now_ms: u64,
) -> Result<AgreedTerms, u16> {
    if q.task_binding != Some(expected_task) {
        return Err(codes::TASK_BINDING_MISMATCH);
    }
    if now_ms > q.expiry_ms {
        return Err(codes::QUOTE_EXPIRED);
    }
    if !q.methods.iter().any(|m| m.uri == CREDIT_LEDGER_METHOD) {
        return Err(codes::METHOD_UNSUPPORTED);
    }
    // pay §5.2: terms 1/2 quotes MUST carry key 4 (the settled/held total).
    if q.terms != Some(required_terms) || q.total.is_none() {
        return Err(codes::ESCROW_VIOLATION);
    }
    Ok(AgreedTerms {
        task: expected_task,
        pricing_model: q.pricing_model,
        rate_table: q.rate_table.clone(),
        terms: required_terms,
        authorized_cap: q.total.clone(),
        quoted_total: q.total.clone(),
    })
}

// ---- prepaid provider (terms 1) --------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrepaidProviderState {
    AwaitAgree,
    AwaitPayment,
    Settled,
}

pub struct PrepaidProviderPayment {
    provider_aid: AgentId,
    requester_aid: AgentId,
    operator_aid: AgentId,
    quote: PayQuote,
    quote_digest: Vec<u8>,
    ledger: SettlementLedger,
    receipts: Vec<PayReceipt>,
    invoiced: Option<([u8; 16], AssetAmount)>,
    state: PrepaidProviderState,
}

impl PrepaidProviderPayment {
    /// `quote` MUST be terms 1 with key 4 (total) present (pay §5.2.2).
    pub fn new(provider_aid: AgentId, requester_aid: AgentId, operator_aid: AgentId, quote: PayQuote) -> Self {
        debug_assert_eq!(quote.terms, Some(TERMS_PREPAID));
        debug_assert!(quote.total.is_some());
        PrepaidProviderPayment {
            provider_aid,
            requester_aid,
            operator_aid,
            quote,
            quote_digest: Vec::new(),
            ledger: SettlementLedger::new(),
            receipts: Vec::new(),
            invoiced: None,
            state: PrepaidProviderState::AwaitAgree,
        }
    }

    pub fn state(&self) -> PrepaidProviderState {
        self.state
    }
    pub fn receipts(&self) -> &[PayReceipt] {
        &self.receipts
    }

    /// The §5.2.2 gate: billable execution MUST NOT start before *settled*.
    pub fn execution_permitted(&self) -> bool {
        self.state == PrepaidProviderState::Settled
    }

    pub fn bind_quote_envelope(&mut self, envelope: &[u8]) {
        self.quote_digest = pay_ref_digest(envelope);
    }

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
        self.state = PrepaidProviderState::AwaitPayment;
        Vec::new()
    }

    /// The single pre-delivery PAY_INVOICE, equal to the quoted total
    /// (pay §5.2.2). Idempotent by invoice-id.
    pub fn invoice(&mut self, invoice_id: [u8; 16], now_ms: u64) -> Vec<Outgoing> {
        let total = self.quote.total.clone().expect("terms-1 quote carries key 4");
        self.invoiced = Some((invoice_id, total.clone()));
        let inv = PayInvoice {
            invoice_id,
            quote_id: self.quote.quote_id,
            amount: total,
            watermark: None,
            method_payload: Vec::new(),
            timestamp_ms: now_ms,
            phase: None,
        };
        vec![(msg_type::PAY_INVOICE, inv.to_value())]
    }

    /// Verify the proof's ledger entry, settle once, reply PAY_RECEIPT
    /// (kind absent = settled), and open the execution gate.
    pub fn on_proof(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let proof = match PayProof::from_value(&inc.content) {
            Ok(p) => p,
            Err(_) => return err(codes::MALFORMED, false),
        };
        let (invoice_id, amount) = match &self.invoiced {
            Some((id, aa)) if *id == proof.invoice_id => (*id, aa.clone()),
            _ => return err(codes::SETTLEMENT_FAILED, false),
        };
        let entry = match EntryProof::from_bytes(&proof.proof_payload) {
            Ok(e) => e,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if !verify_entry(&entry, &self.operator_aid, invoice_id, &self.requester_aid, &self.provider_aid, &amount) {
            return err(codes::INVALID_PAYMENT_PROOF, false);
        }
        match self.ledger.settle_once(InvoiceId(invoice_id)) {
            Settle::FirstTime => {
                let rcpt = PayReceipt {
                    invoice_id,
                    amount,
                    timestamp_ms: now_ms,
                    proof_digest: Some(pay_ref_digest(&proof.proof_payload)),
                    kind: None,
                };
                self.receipts.push(rcpt.clone());
                self.state = PrepaidProviderState::Settled;
                vec![(msg_type::PAY_RECEIPT, rcpt.to_value())]
            }
            Settle::AlreadySettled => match self.receipts.iter().find(|r| r.invoice_id == invoice_id) {
                Some(r) => vec![(msg_type::PAY_RECEIPT, r.to_value())],
                None => err(codes::SETTLEMENT_FAILED, false),
            },
        }
    }
}

// ---- prepaid requester (terms 1) --------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrepaidRequesterState {
    AwaitQuote,
    Agreed,
    Paid,
    Failed,
}

pub struct PrepaidRequesterPayment {
    expected_task: [u8; 32],
    requester_aid: AgentId,
    provider_aid: AgentId,
    agreed: Option<AgreedTerms>,
    quote_id: Option<[u8; 16]>,
    receipts: Vec<PayReceipt>,
    state: PrepaidRequesterState,
}

impl PrepaidRequesterPayment {
    pub fn new(expected_task: [u8; 32], requester_aid: AgentId, provider_aid: AgentId) -> Self {
        PrepaidRequesterPayment {
            expected_task,
            requester_aid,
            provider_aid,
            agreed: None,
            quote_id: None,
            receipts: Vec::new(),
            state: PrepaidRequesterState::AwaitQuote,
        }
    }

    pub fn state(&self) -> PrepaidRequesterState {
        self.state
    }
    pub fn receipts(&self) -> &[PayReceipt] {
        &self.receipts
    }

    pub fn on_quote(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let q = match PayQuote::from_value(&inc.content) {
            Ok(q) => q,
            Err(_) => return err(codes::MALFORMED, false),
        };
        let agreed = match accept_quote(&q, self.expected_task, TERMS_PREPAID, now_ms) {
            Ok(a) => a,
            Err(code) => {
                self.state = PrepaidRequesterState::Failed;
                return err(code, false);
            }
        };
        self.quote_id = Some(q.quote_id);
        let cap = agreed.quoted_total.clone();
        self.agreed = Some(agreed);
        self.state = PrepaidRequesterState::Agreed;
        let agree = PayAgree {
            quote_id: q.quote_id,
            quote_digest: pay_ref_digest(&inc.raw),
            method: CREDIT_LEDGER_METHOD.into(),
            authorized_cap: cap,
        };
        vec![(msg_type::PAY_AGREE, agree.to_value())]
    }

    /// Verify (pay §5.2.2 via [`verify_escrow_invoice`]) and pay the single
    /// prepaid invoice. Idempotent: the operator dedups by invoice-id.
    pub fn on_invoice(&mut self, inc: &Incoming, operator: &mut LedgerOperator, now_ms: u64) -> Vec<Outgoing> {
        let inv = match PayInvoice::from_value(&inc.content) {
            Ok(i) => i,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if Some(inv.quote_id) != self.quote_id {
            return err(codes::SETTLEMENT_FAILED, false);
        }
        let agreed = self.agreed.as_ref().expect("agreed before invoice");
        if let Err(code) = verify_escrow_invoice(agreed, &EscrowContext::default(), &inv) {
            return err(code, false);
        }
        match operator.pay(inv.invoice_id, &self.requester_aid, &self.provider_aid, &inv.amount, now_ms) {
            Ok(entry) => {
                self.state = PrepaidRequesterState::Paid;
                let proof = PayProof {
                    invoice_id: inv.invoice_id,
                    method: CREDIT_LEDGER_METHOD.into(),
                    proof_payload: entry.to_bytes(),
                };
                vec![(msg_type::PAY_PROOF, proof.to_value())]
            }
            Err(LedgerError::Insufficient) => Vec::new(), // proof absence, as postpaid
            Err(_) => err(codes::SETTLEMENT_FAILED, false),
        }
    }

    pub fn on_receipt(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        if let Ok(r) = PayReceipt::from_value(&inc.content) {
            self.receipts.push(r);
        }
        Vec::new()
    }
}

// ---- escrow provider (terms 2) ----------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscrowProviderState {
    AwaitAgree,
    AwaitHold,
    Held,
    AwaitRelease,
    Released,
}

pub struct EscrowProviderPayment {
    provider_aid: AgentId,
    requester_aid: AgentId,
    operator_aid: AgentId,
    quote: PayQuote,
    quote_digest: Vec<u8>,
    /// The expiry this provider demands on the hold (pay §5.2.3: SHOULD be
    /// deadline + release latency with margin — its own policy).
    hold_expiry_ts: u64,
    ledger: SettlementLedger,
    receipts: Vec<PayReceipt>,
    hold: Option<([u8; 16], AssetAmount)>,
    release: Option<([u8; 16], AssetAmount)>,
    complete_sent: bool,
    state: EscrowProviderState,
}

impl EscrowProviderPayment {
    /// `quote` MUST be terms 2 with key 4 (total) present (pay §5.2.3).
    pub fn new(
        provider_aid: AgentId,
        requester_aid: AgentId,
        operator_aid: AgentId,
        quote: PayQuote,
        hold_expiry_ts: u64,
    ) -> Self {
        debug_assert_eq!(quote.terms, Some(TERMS_ESCROW));
        debug_assert!(quote.total.is_some());
        EscrowProviderPayment {
            provider_aid,
            requester_aid,
            operator_aid,
            quote,
            quote_digest: Vec::new(),
            hold_expiry_ts,
            ledger: SettlementLedger::new(),
            receipts: Vec::new(),
            hold: None,
            release: None,
            complete_sent: false,
            state: EscrowProviderState::AwaitAgree,
        }
    }

    pub fn state(&self) -> EscrowProviderState {
        self.state
    }
    pub fn receipts(&self) -> &[PayReceipt] {
        &self.receipts
    }

    /// The §5.2.3 gate: delivery obligations key off *held*.
    pub fn execution_permitted(&self) -> bool {
        matches!(
            self.state,
            EscrowProviderState::Held | EscrowProviderState::AwaitRelease | EscrowProviderState::Released
        )
    }

    pub fn bind_quote_envelope(&mut self, envelope: &[u8]) {
        self.quote_digest = pay_ref_digest(envelope);
    }

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
        self.state = EscrowProviderState::AwaitHold;
        Vec::new()
    }

    /// The phase-1 (hold) PAY_INVOICE: amount = the quoted total, method
    /// payload = the demanded expiry-ts (pay §5.2.4). Idempotent by id.
    pub fn invoice_hold(&mut self, invoice_id: [u8; 16], now_ms: u64) -> Vec<Outgoing> {
        let total = self.quote.total.clone().expect("terms-2 quote carries key 4");
        self.hold = Some((invoice_id, total.clone()));
        let inv = PayInvoice {
            invoice_id,
            quote_id: self.quote.quote_id,
            amount: total,
            watermark: None,
            method_payload: encode(&Value::Uint(self.hold_expiry_ts)).expect("uint encodes"),
            timestamp_ms: now_ms,
            phase: Some(PHASE_HOLD),
        };
        vec![(msg_type::PAY_INVOICE, inv.to_value())]
    }

    /// Verify a hold proof (operator-signed `MAPLedgerHold`, committing to
    /// exactly the demanded expiry), settle the hold round once, reply a
    /// kind-1 (held) receipt, and open the execution gate.
    pub fn on_hold_proof(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let proof = match PayProof::from_value(&inc.content) {
            Ok(p) => p,
            Err(_) => return err(codes::MALFORMED, false),
        };
        let (invoice_id, amount) = match &self.hold {
            Some((id, aa)) if *id == proof.invoice_id => (*id, aa.clone()),
            _ => return err(codes::SETTLEMENT_FAILED, false),
        };
        let entry = match EntryProof::from_bytes(&proof.proof_payload) {
            Ok(e) => e,
            Err(_) => return err(codes::MALFORMED, false),
        };
        match verify_hold_entry(&entry, &self.operator_aid, invoice_id, &self.requester_aid, &self.provider_aid, &amount) {
            Some(expiry) if expiry == self.hold_expiry_ts => {}
            _ => return err(codes::INVALID_PAYMENT_PROOF, false),
        }
        match self.ledger.settle_once(InvoiceId(invoice_id)) {
            Settle::FirstTime => {
                let rcpt = PayReceipt {
                    invoice_id,
                    amount,
                    timestamp_ms: now_ms,
                    proof_digest: Some(pay_ref_digest(&proof.proof_payload)),
                    kind: Some(KIND_HELD),
                };
                self.receipts.push(rcpt.clone());
                self.state = EscrowProviderState::Held;
                vec![(msg_type::PAY_RECEIPT, rcpt.to_value())]
            }
            Settle::AlreadySettled => match self.receipts.iter().find(|r| r.invoice_id == invoice_id) {
                Some(r) => vec![(msg_type::PAY_RECEIPT, r.to_value())],
                None => err(codes::SETTLEMENT_FAILED, false),
            },
        }
    }

    /// Record that COMPLETE went out (the task pump calls this) — the
    /// precondition for issuing the release invoice.
    pub fn mark_complete_sent(&mut self) {
        self.complete_sent = true;
    }

    /// The phase-2 (release) PAY_INVOICE for the final owed amount ≤ held
    /// (pay §5.2.3). Requires a settled hold and a sent COMPLETE (or a
    /// checkpoint-anchored CANCEL amount the caller computed via
    /// [`owed_at_checkpoint`]).
    pub fn invoice_release(&mut self, invoice_id: [u8; 16], amount: AssetAmount, now_ms: u64) -> Vec<Outgoing> {
        if !self.execution_permitted() || !self.complete_sent {
            return Vec::new();
        }
        self.release = Some((invoice_id, amount.clone()));
        self.state = EscrowProviderState::AwaitRelease;
        let inv = PayInvoice {
            invoice_id,
            quote_id: self.quote.quote_id,
            amount,
            watermark: None,
            method_payload: Vec::new(),
            timestamp_ms: now_ms,
            phase: Some(PHASE_RELEASE),
        };
        vec![(msg_type::PAY_INVOICE, inv.to_value())]
    }

    /// Verify a release proof (operator-signed `MAPLedgerRelease` binding the
    /// hold, this release, and the amount), settle once, reply kind-2.
    pub fn on_release_proof(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let proof = match PayProof::from_value(&inc.content) {
            Ok(p) => p,
            Err(_) => return err(codes::MALFORMED, false),
        };
        let (release_id, amount) = match &self.release {
            Some((id, aa)) if *id == proof.invoice_id => (*id, aa.clone()),
            _ => return err(codes::SETTLEMENT_FAILED, false),
        };
        let (hold_id, _) = self.hold.as_ref().map(|(id, aa)| (*id, aa.clone())).expect("release only after hold");
        let entry = match EntryProof::from_bytes(&proof.proof_payload) {
            Ok(e) => e,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if !verify_release_entry(&entry, &self.operator_aid, hold_id, release_id, &self.requester_aid, &self.provider_aid, &amount) {
            return err(codes::INVALID_PAYMENT_PROOF, false);
        }
        match self.ledger.settle_once(InvoiceId(release_id)) {
            Settle::FirstTime => {
                let rcpt = PayReceipt {
                    invoice_id: release_id,
                    amount,
                    timestamp_ms: now_ms,
                    proof_digest: Some(pay_ref_digest(&proof.proof_payload)),
                    kind: Some(KIND_RELEASED),
                };
                self.receipts.push(rcpt.clone());
                self.state = EscrowProviderState::Released;
                vec![(msg_type::PAY_RECEIPT, rcpt.to_value())]
            }
            Settle::AlreadySettled => match self.receipts.iter().find(|r| r.invoice_id == release_id) {
                Some(r) => vec![(msg_type::PAY_RECEIPT, r.to_value())],
                None => err(codes::SETTLEMENT_FAILED, false),
            },
        }
    }

    /// The release amount if the task is CANCELled now, billed from the last
    /// countersigned checkpoint's usage (pay §5.2.3 check 5 / §4.2 anchor).
    pub fn owed_at_checkpoint(&self, checkpoint_usage: &Usage) -> AssetAmount {
        billed(self.quote.pricing_model, &self.quote.rate_table, checkpoint_usage)
            .expect("provider rates bill exactly")
    }
}

// ---- escrow requester (terms 2) ----------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscrowRequesterState {
    AwaitQuote,
    Agreed,
    Held,
    Released,
    Failed,
}

pub struct EscrowRequesterPayment {
    expected_task: [u8; 32],
    /// The requester's own keypair — it SIGNS release authorizations
    /// (pay §5.2.4 `MAPLedgerReleaseAuth`; inner signature, decision-9 style).
    kp: Keypair,
    requester_aid: AgentId,
    provider_aid: AgentId,
    agreed: Option<AgreedTerms>,
    quote_id: Option<[u8; 16]>,
    hold: Option<([u8; 16], AssetAmount)>,
    complete_verified: bool,
    cancel_checkpoint_usage: Option<Usage>,
    /// Idempotency: proofs already produced, by invoice-id.
    proofs: HashMap<[u8; 16], Vec<u8>>,
    receipts: Vec<PayReceipt>,
    state: EscrowRequesterState,
}

impl EscrowRequesterPayment {
    pub fn new(expected_task: [u8; 32], kp: Keypair, provider_aid: AgentId) -> Self {
        let requester_aid = kp.agent_id();
        EscrowRequesterPayment {
            expected_task,
            kp,
            requester_aid,
            provider_aid,
            agreed: None,
            quote_id: None,
            hold: None,
            complete_verified: false,
            cancel_checkpoint_usage: None,
            proofs: HashMap::new(),
            receipts: Vec::new(),
            state: EscrowRequesterState::AwaitQuote,
        }
    }

    pub fn state(&self) -> EscrowRequesterState {
        self.state
    }
    pub fn receipts(&self) -> &[PayReceipt] {
        &self.receipts
    }

    /// The canonical release trigger (pay §5.2.3 check 4): the task pump calls
    /// this after COMPLETE arrived AND its transcript-root verified (tasks §4).
    pub fn set_complete_verified(&mut self) {
        self.complete_verified = true;
    }

    /// The CANCEL anchor (pay §5.2.3 check 5): the last countersigned
    /// checkpoint's usage, passed in by the task runtime (as TASK-022).
    pub fn set_cancel_checkpoint(&mut self, usage: Usage) {
        self.cancel_checkpoint_usage = Some(usage);
    }

    pub fn on_quote(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let q = match PayQuote::from_value(&inc.content) {
            Ok(q) => q,
            Err(_) => return err(codes::MALFORMED, false),
        };
        let agreed = match accept_quote(&q, self.expected_task, TERMS_ESCROW, now_ms) {
            Ok(a) => a,
            Err(code) => {
                self.state = EscrowRequesterState::Failed;
                return err(code, false);
            }
        };
        self.quote_id = Some(q.quote_id);
        let cap = agreed.quoted_total.clone();
        self.agreed = Some(agreed);
        self.state = EscrowRequesterState::Agreed;
        let agree = PayAgree {
            quote_id: q.quote_id,
            quote_digest: pay_ref_digest(&inc.raw),
            method: CREDIT_LEDGER_METHOD.into(),
            authorized_cap: cap,
        };
        vec![(msg_type::PAY_AGREE, agree.to_value())]
    }

    /// Verify a terms-2 invoice (the six §5.2.3 checks via
    /// [`verify_escrow_invoice`]) and act on its phase: place the hold, or
    /// sign the release authorization and release. Idempotent by invoice-id
    /// (a re-sent invoice re-emits the identical proof).
    pub fn on_invoice(&mut self, inc: &Incoming, operator: &mut LedgerOperator, now_ms: u64) -> Vec<Outgoing> {
        let inv = match PayInvoice::from_value(&inc.content) {
            Ok(i) => i,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if Some(inv.quote_id) != self.quote_id {
            return err(codes::SETTLEMENT_FAILED, false);
        }
        if let Some(payload) = self.proofs.get(&inv.invoice_id) {
            let proof = PayProof {
                invoice_id: inv.invoice_id,
                method: CREDIT_LEDGER_METHOD.into(),
                proof_payload: payload.clone(),
            };
            return vec![(msg_type::PAY_PROOF, proof.to_value())];
        }
        let ctx = EscrowContext {
            held: self.hold.as_ref().map(|(_, aa)| aa.clone()),
            complete_verified: self.complete_verified,
            cancel_checkpoint_usage: self.cancel_checkpoint_usage,
        };
        let agreed = self.agreed.as_ref().expect("agreed before invoice");
        if let Err(code) = verify_escrow_invoice(agreed, &ctx, &inv) {
            return err(code, false);
        }
        let entry = match inv.phase {
            Some(PHASE_HOLD) => {
                // Method payload = the demanded expiry-ts (pay §5.2.4).
                let expiry = match decode(&inv.method_payload).ok().and_then(|v| v.as_uint()) {
                    Some(e) => e,
                    None => return err(codes::MALFORMED, false),
                };
                match operator.hold(inv.invoice_id, &self.requester_aid, &self.provider_aid, &inv.amount, expiry, now_ms) {
                    Ok(e) => {
                        self.hold = Some((inv.invoice_id, inv.amount.clone()));
                        self.state = EscrowRequesterState::Held;
                        e
                    }
                    Err(LedgerError::Insufficient) => return Vec::new(), // proof absence
                    Err(_) => return err(codes::SETTLEMENT_FAILED, false),
                }
            }
            Some(PHASE_RELEASE) => {
                let (hold_id, _) = self.hold.as_ref().map(|(id, aa)| (*id, aa.clone())).expect("verified: release requires a hold");
                // Authorization symmetry (pay §5.2.3): the PAYER signs the
                // exact (hold, release, amount) triple.
                let auth = self.kp.sign(&release_auth_bytes(hold_id, inv.invoice_id, &inv.amount));
                match operator.release(inv.invoice_id, hold_id, &auth, &inv.amount, now_ms) {
                    Ok(e) => {
                        self.state = EscrowRequesterState::Released;
                        e
                    }
                    Err(LedgerError::Expired) => return err(codes::SETTLEMENT_FAILED, false),
                    Err(_) => return err(codes::SETTLEMENT_FAILED, false),
                }
            }
            _ => unreachable!("verify_escrow_invoice enforces phase presence under terms 2"),
        };
        let payload = entry.to_bytes();
        self.proofs.insert(inv.invoice_id, payload.clone());
        let proof = PayProof { invoice_id: inv.invoice_id, method: CREDIT_LEDGER_METHOD.into(), proof_payload: payload };
        vec![(msg_type::PAY_PROOF, proof.to_value())]
    }

    pub fn on_receipt(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        if let Ok(r) = PayReceipt::from_value(&inc.content) {
            self.receipts.push(r);
        }
        Vec::new()
    }
}
