//! Postpaid `pay:credit-ledger` payment flow (pay §2–§5.1), pump-driven and
//! composed alongside the TASK-007 task runtimes:
//!   provider PAY_QUOTE (flat, task-bound) → requester PAY_AGREE →
//!   task runs → provider PAY_INVOICE → requester pays via the ledger operator,
//!   PAY_PROOF → provider verifies + settles once → PAY_RECEIPT.
//!
//! Digest binding is over the **received envelope bytes** of the referenced
//! message (invariant I1, pay §3.1) — the requester digests `Incoming.raw`;
//! the provider digests the sealed PAY_QUOTE bytes fed back via
//! [`ProviderPayment::bind_quote_envelope`] after `Session::send`. (Was over
//! re-encoded content; corrected per ERRATA 2026-07-12 / TASK-017 — a
//! content digest never matched frozen TV-013 or map-go.) Every settlement
//! link is checked; retries move money at most once (the operator +
//! SettlementLedger are both idempotent by id).
//!
//! Rejection codes are specific per core §9 (2026-07-12): 6005 cap-exceeded,
//! 6006 agreement-digest-mismatch (price switch), 6007 task-binding-mismatch,
//! 6008 invalid-payment-proof; 6002 SETTLEMENT_FAILED for causes without a
//! dedicated code.

use crate::Outgoing;
use map_core::cbor::encode;
use map_core::errors::{codes, ErrorMsg};
use map_core::msg_type;
use map_identity::AgentId;
use map_pay::codec::{pay_ref_digest, AssetAmount, MethodOffer, PayAgree, PayInvoice, PayProof, PayQuote, PayReceipt};
use map_pay::credit_ledger::{verify_entry, EntryProof, LedgerError, LedgerOperator, CREDIT_LEDGER_METHOD};
use map_pay::{InvoiceId, Settle, SettlementLedger};
use map_session::Incoming;

fn err(code: u16) -> Vec<Outgoing> {
    vec![(msg_type::ERROR, ErrorMsg::new(code, false).to_value())]
}

/// `a ≤ cap` in the same asset and exponent (no cross-exponent normalization —
/// a mismatch is a rejection, §9).
fn amount_within(a: &AssetAmount, cap: &AssetAmount) -> bool {
    a.asset == cap.asset && a.amount.exponent == cap.amount.exponent && a.amount.mantissa <= cap.amount.mantissa
}

// ---- provider -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderPayState {
    AwaitAgree,
    Agreed,
    Invoiced,
    Settled,
    Failed,
}

pub struct ProviderPayment {
    provider_aid: AgentId, // payee
    requester_aid: AgentId, // payer
    operator_aid: AgentId,
    quote: Option<PayQuote>,
    quote_digest: Vec<u8>,
    pending_invoice: Option<([u8; 16], AssetAmount)>,
    ledger: SettlementLedger,
    receipt: Option<PayReceipt>,
    state: ProviderPayState,
}

impl ProviderPayment {
    pub fn new(provider_aid: AgentId, requester_aid: AgentId, operator_aid: AgentId) -> Self {
        ProviderPayment {
            provider_aid,
            requester_aid,
            operator_aid,
            quote: None,
            quote_digest: Vec::new(),
            pending_invoice: None,
            ledger: SettlementLedger::new(),
            receipt: None,
            state: ProviderPayState::AwaitAgree,
        }
    }

    pub fn state(&self) -> ProviderPayState {
        self.state
    }
    pub fn receipt(&self) -> Option<&PayReceipt> {
        self.receipt.as_ref()
    }

    /// Issue a flat (model 0) PAY_QUOTE bound to `task`, alongside the task
    /// QUOTE. The digest the requester will agree to is over the SEALED
    /// envelope bytes (I1), which do not exist yet — the caller MUST feed
    /// them back via [`bind_quote_envelope`] with the value `Session::send`
    /// returned, before processing the PAY_AGREE.
    pub fn issue_quote(&mut self, quote_id: [u8; 16], task: [u8; 32], total: AssetAmount, expiry_ms: u64) -> Vec<Outgoing> {
        let q = PayQuote {
            quote_id,
            pricing_model: 0,
            rate_table: vec![(0, total.clone())],
            total: Some(total),
            methods: vec![MethodOffer { uri: CREDIT_LEDGER_METHOD.into(), params: vec![] }],
            expiry_ms,
            terms: Some(0),
            ext: None,
            task_binding: Some(task),
        };
        self.quote_digest.clear(); // set by bind_quote_envelope once sealed
        let out = vec![(msg_type::PAY_QUOTE, q.to_value())];
        self.quote = Some(q);
        out
    }

    /// Record the sealed PAY_QUOTE envelope bytes (what `Session::send`
    /// returned) so the incoming PAY_AGREE's digest can be checked against
    /// the exact frame the requester received (I1, pay §3.1).
    pub fn bind_quote_envelope(&mut self, envelope: &[u8]) {
        self.quote_digest = pay_ref_digest(envelope);
    }

    /// Verify PAY_AGREE binds THIS quote (id + envelope digest) and picks the
    /// offered method. A digest mismatch is the price-switch defense
    /// (pay §3.1 → ERROR 6006); an unoffered method is 6004.
    pub fn on_agree(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        let agree = match PayAgree::from_value(&inc.content) {
            Ok(a) => a,
            Err(_) => return err(codes::MALFORMED),
        };
        let q = match &self.quote {
            Some(q) => q,
            None => return err(codes::SETTLEMENT_FAILED), // no active quote
        };
        debug_assert!(!self.quote_digest.is_empty(), "bind_quote_envelope must precede on_agree");
        if agree.method != CREDIT_LEDGER_METHOD {
            self.state = ProviderPayState::Failed;
            return err(codes::METHOD_UNSUPPORTED);
        }
        if agree.quote_id != q.quote_id || agree.quote_digest != self.quote_digest {
            self.state = ProviderPayState::Failed;
            return err(codes::AGREEMENT_DIGEST_MISMATCH); // price switch
        }
        self.state = ProviderPayState::Agreed;
        Vec::new()
    }

    /// After a Verified COMPLETE: invoice the flat total (idempotent by id).
    pub fn invoice(&mut self, invoice_id: [u8; 16], now_ms: u64) -> Vec<Outgoing> {
        if !matches!(self.state, ProviderPayState::Agreed | ProviderPayState::Invoiced) {
            return Vec::new();
        }
        let q = self.quote.as_ref().expect("quote set before invoice");
        let amount = q.total.clone().expect("flat quote carries a total");
        self.pending_invoice = Some((invoice_id, amount.clone()));
        let inv = PayInvoice { invoice_id, quote_id: q.quote_id, amount, method_payload: Vec::new(), timestamp_ms: now_ms };
        self.state = ProviderPayState::Invoiced;
        vec![(msg_type::PAY_INVOICE, inv.to_value())]
    }

    /// Verify the ledger-entry proof (operator sig + invoice binding + amount),
    /// settle once, and reply PAY_RECEIPT. Idempotent: a re-sent PROOF gets the
    /// same receipt with no second settlement.
    pub fn on_proof(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let proof = match PayProof::from_value(&inc.content) {
            Ok(p) => p,
            Err(_) => return err(codes::MALFORMED),
        };
        let (inv_id, amount) = match &self.pending_invoice {
            Some(x) => x.clone(),
            None => return err(codes::SETTLEMENT_FAILED), // no active invoice
        };
        if proof.invoice_id != inv_id {
            return err(codes::SETTLEMENT_FAILED); // proof for a different invoice
        }
        let entry = match EntryProof::from_bytes(&proof.proof_payload) {
            Ok(e) => e,
            Err(_) => return err(codes::MALFORMED),
        };
        if !verify_entry(&entry, &self.operator_aid, inv_id, &self.requester_aid, &self.provider_aid, &amount) {
            return err(codes::INVALID_PAYMENT_PROOF); // forged / mismatched entry
        }
        match self.ledger.settle_once(InvoiceId(inv_id)) {
            Settle::FirstTime => {
                let rcpt = PayReceipt {
                    invoice_id: inv_id,
                    amount,
                    timestamp_ms: now_ms,
                    proof_digest: Some(pay_ref_digest(&proof.proof_payload)),
                };
                self.receipt = Some(rcpt.clone());
                self.state = ProviderPayState::Settled;
                vec![(msg_type::PAY_RECEIPT, rcpt.to_value())]
            }
            Settle::AlreadySettled => match &self.receipt {
                Some(r) => vec![(msg_type::PAY_RECEIPT, r.to_value())],
                None => err(codes::SETTLEMENT_FAILED),
            },
        }
    }
}

// ---- requester ------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequesterPayState {
    AwaitQuote,
    Agreed,
    Paid,
    Settled,
    /// Balance too low to pay the invoice; provider may re-invoice later.
    Insufficient,
    Failed,
}

pub struct RequesterPayment {
    /// `Some(task)` when our OFFER was task-bound → the quote's key 9 MUST equal
    /// it; `None` for a standing quote (no task binding required, FR-2).
    expected_task: Option<[u8; 32]>,
    requester_aid: AgentId, // payer (me)
    provider_aid: AgentId, // payee
    agreed_cap: Option<AssetAmount>,
    quote_id: Option<[u8; 16]>,
    receipt_bytes: Option<Vec<u8>>,
    state: RequesterPayState,
}

impl RequesterPayment {
    /// `expected_task = Some(id)` binds to a task-bound OFFER (key 9 required);
    /// `None` accepts a standing quote.
    pub fn new(expected_task: Option<[u8; 32]>, requester_aid: AgentId, provider_aid: AgentId) -> Self {
        RequesterPayment {
            expected_task,
            requester_aid,
            provider_aid,
            agreed_cap: None,
            quote_id: None,
            receipt_bytes: None,
            state: RequesterPayState::AwaitQuote,
        }
    }

    pub fn state(&self) -> RequesterPayState {
        self.state
    }
    pub fn receipt_bytes(&self) -> Option<&[u8]> {
        self.receipt_bytes.as_deref()
    }

    /// Evaluate a PAY_QUOTE answering our task-bound OFFER and PAY_AGREE. The
    /// task binding (key 9) MUST be present and equal our TaskID (FR-2); the
    /// authorized cap is the quoted total (the runaway-cost ceiling).
    pub fn on_quote(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let q = match PayQuote::from_value(&inc.content) {
            Ok(q) => q,
            Err(_) => return err(codes::MALFORMED),
        };
        if let Some(task) = self.expected_task {
            if q.task_binding != Some(task) {
                self.state = RequesterPayState::Failed;
                return err(codes::TASK_BINDING_MISMATCH); // binding mismatch/absence (FR-2)
            }
        }
        if now_ms > q.expiry_ms {
            self.state = RequesterPayState::Failed;
            return err(codes::QUOTE_EXPIRED);
        }
        if !q.methods.iter().any(|m| m.uri == CREDIT_LEDGER_METHOD) {
            self.state = RequesterPayState::Failed;
            return err(codes::METHOD_UNSUPPORTED);
        }
        // Digest the RECEIVED PAY_QUOTE envelope bytes, not a re-encoding of
        // its content (I1, pay §3.1). The provider bound the same frame via
        // bind_quote_envelope, so the digests match iff no byte was switched.
        let digest = pay_ref_digest(&inc.raw);
        self.agreed_cap = q.total.clone();
        self.quote_id = Some(q.quote_id);
        self.state = RequesterPayState::Agreed;
        let agree = PayAgree {
            quote_id: q.quote_id,
            quote_digest: digest,
            method: CREDIT_LEDGER_METHOD.into(),
            authorized_cap: q.total,
        };
        vec![(msg_type::PAY_AGREE, agree.to_value())]
    }

    /// Pay a PAY_INVOICE via the ledger operator and reply PAY_PROOF. Refuses
    /// (ERROR) if the amount exceeds the authorized cap (FR-6 runaway-cost
    /// defense). Insufficient balance → no proof, provider may re-invoice.
    pub fn on_invoice(&mut self, inc: &Incoming, operator: &mut LedgerOperator, now_ms: u64) -> Vec<Outgoing> {
        let inv = match PayInvoice::from_value(&inc.content) {
            Ok(i) => i,
            Err(_) => return err(codes::MALFORMED),
        };
        if Some(inv.quote_id) != self.quote_id {
            return err(codes::SETTLEMENT_FAILED); // invoice for an unknown quote
        }
        let cap = match &self.agreed_cap {
            Some(c) => c.clone(),
            None => return err(codes::SETTLEMENT_FAILED), // no agreed cap
        };
        if !amount_within(&inv.amount, &cap) {
            self.state = RequesterPayState::Failed;
            return err(codes::CAP_EXCEEDED); // over the authorized ceiling / exponent mismatch
        }
        match operator.pay(inv.invoice_id, &self.requester_aid, &self.provider_aid, &inv.amount, now_ms) {
            Ok(entry) => {
                let proof = PayProof {
                    invoice_id: inv.invoice_id,
                    method: CREDIT_LEDGER_METHOD.into(),
                    proof_payload: entry.to_bytes(),
                };
                self.state = RequesterPayState::Paid;
                vec![(msg_type::PAY_PROOF, proof.to_value())]
            }
            Err(LedgerError::Insufficient) => {
                self.state = RequesterPayState::Insufficient;
                Vec::new()
            }
            Err(LedgerError::ExponentMismatch) => {
                self.state = RequesterPayState::Failed;
                err(codes::CAP_EXCEEDED) // exponent mismatch against the cap
            }
        }
    }

    pub fn on_receipt(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        if PayReceipt::from_value(&inc.content).is_ok() {
            self.receipt_bytes = Some(encode(&inc.content).expect("receipt content encodes"));
            self.state = RequesterPayState::Settled;
        }
        Vec::new()
    }
}
