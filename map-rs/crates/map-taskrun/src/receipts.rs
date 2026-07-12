//! Receipt lifecycle over a session (trust §3, §2): requester issues a
//! unilateral RECEIPT, the provider verifies it against its OWN observation and
//! countersigns into a RECEIPT_ACK carrying the dual-signed receipt. Pump-driven
//! (returns messages to send; the caller owns session I/O).
//!
//! Two non-negotiable defenses: (1) the countersignature is over the requester's
//! RECEIVED `core_bytes`, never a re-encoding (I1); (2) the provider countersigns
//! only a receipt whose task/cap/identity match its own observation and whose
//! usage/timestamps agree within tolerance — the §11 farming defense.

use crate::{Outgoing, ReceiptInputs};
use map_core::errors::{codes, ErrorMsg};
use map_core::msg_type;
use map_identity::{AgentId, Keypair};
use map_session::Incoming;
use map_trust::lifecycle::within_tolerance;
use map_trust::{
    sign_receipt, Metrics, Outcome, Receipt, ReceiptCore, ReceiptStore, TrustError, ROLE_PROVIDER,
    ROLE_REQUESTER,
};

/// Wire code for a countersignature refusal: the provider declines to attest
/// the requester's claimed usage/timestamps because they fall outside its
/// tolerance (trust §3). Dedicated code 2006 COUNTERSIGN_REFUSED, registered
/// 2026-07-12 (ERRATA ruling; was the misleading 5000 TASK_FAILED — the task
/// did not fail, the attestation was refused). retryable = false.
const RECEIPT_REJECT_CODE: u16 = codes::COUNTERSIGN_REFUSED;

#[derive(Debug)]
pub enum ReceiptError {
    Trust(TrustError),
    Malformed(String),
}

impl From<TrustError> for ReceiptError {
    fn from(e: TrustError) -> Self {
        ReceiptError::Trust(e)
    }
}

/// The requester's task terminal, mapped to a trust `Outcome` (FR-1; the mapping
/// table is recorded in trust §3): Verified→ok, Failed→failed, Cancelled (incl.
/// TTFT cancel)→cancelled. Timeout(2)/disputed(4) are not task-terminal here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskOutcome {
    Verified,
    Failed,
    Cancelled,
}

fn to_outcome(t: TaskOutcome) -> Outcome {
    match t {
        TaskOutcome::Verified => Outcome::Ok,
        TaskOutcome::Failed => Outcome::Failed,
        TaskOutcome::Cancelled => Outcome::Cancelled,
    }
}

/// FR-1: build a `ReceiptCore` from a finished task's inputs + the two
/// identities (the caller supplies them; a task run doesn't know them).
pub fn make_receipt_core(
    inputs: &ReceiptInputs,
    requester: &AgentId,
    provider: &AgentId,
    outcome: TaskOutcome,
    rating: Option<u8>,
    transcript_root: Option<[u8; 32]>,
) -> ReceiptCore {
    ReceiptCore {
        task: inputs.task,
        requester: requester.clone(),
        provider: provider.clone(),
        capability: inputs.capability.clone(),
        started_ms: inputs.started_ms,
        ended_ms: inputs.ended_ms,
        outcome: to_outcome(outcome),
        metrics: Metrics {
            tokens_in: inputs.usage.tokens_in,
            tokens_out: inputs.usage.tokens_out,
            ttft_ms: inputs.usage.ttft_ms,
            wall_ms: inputs.usage.wall_ms,
        },
        rating,
        payment_ref: None,
        transcript_root,
    }
}

/// FR-2 requester side: sign a unilateral receipt (role 1), hold it (trust §2:
/// a unilateral receipt is still valid evidence), and emit RECEIPT.
pub fn requester_issue(
    inputs: &ReceiptInputs,
    transcript_root: [u8; 32],
    requester_kp: &Keypair,
    provider: &AgentId,
    outcome: TaskOutcome,
    rating: Option<u8>,
    store: &mut ReceiptStore,
) -> Result<Vec<Outgoing>, ReceiptError> {
    let core = make_receipt_core(
        inputs,
        &requester_kp.agent_id(),
        provider,
        outcome,
        rating,
        Some(transcript_root),
    );
    let core_bytes = core.encode()?;
    let sig_req = sign_receipt(requester_kp, ROLE_REQUESTER, &core_bytes);
    let receipt = Receipt {
        core_bytes,
        sig_requester: Some(sig_req),
        sig_provider: None,
    };
    store.ingest(receipt.clone())?;
    Ok(vec![(msg_type::RECEIPT, receipt.to_value())])
}

/// FR-2 requester side: ingest the dual-signed receipt returned in RECEIPT_ACK.
pub fn requester_on_ack(inc: &Incoming, store: &mut ReceiptStore) -> Result<(), ReceiptError> {
    let receipt = Receipt::from_value(&inc.content)?;
    let verified = receipt.verify()?;
    if !verified.dual {
        return Err(ReceiptError::Malformed("RECEIPT_ACK not dual-signed".into()));
    }
    store.ingest(receipt)?;
    Ok(())
}

/// FR-2 provider side: verify a received RECEIPT against `observation`, then
/// countersign and reply RECEIPT_ACK with the full dual receipt. On any check
/// failure reply ERROR and do NOT countersign (§11). Idempotent: a retried
/// RECEIPT re-derives the identical ACK (Ed25519 over identical bytes).
pub fn provider_countersign(
    inc: &Incoming,
    observation: &ReceiptInputs,
    provider_kp: &Keypair,
    store: &mut ReceiptStore,
) -> Vec<Outgoing> {
    let receipt = match Receipt::from_value(&inc.content) {
        Ok(r) => r,
        Err(_) => return error_reply(codes::MALFORMED),
    };
    let verified = match receipt.verify() {
        Ok(v) => v,
        Err(_) => return error_reply(codes::BAD_SIGNATURE),
    };
    let claimed = &verified.core;
    // I must be the named provider, I must have observed THIS task, cap matches.
    if claimed.provider != provider_kp.agent_id()
        || claimed.task != observation.task
        || claimed.capability != observation.capability
    {
        return error_reply(RECEIPT_REJECT_CODE);
    }
    // Compare the claim against my own observed core (usage ±5%, ts ±2 s).
    let observed = make_receipt_core(
        observation,
        &claimed.requester,
        &provider_kp.agent_id(),
        TaskOutcome::Verified,
        claimed.rating,
        None,
    );
    if !within_tolerance(claimed, &observed) {
        return error_reply(RECEIPT_REJECT_CODE);
    }
    // Countersign over the RECEIVED core bytes (I1), never a re-encoding.
    let sig_prov = sign_receipt(provider_kp, ROLE_PROVIDER, &receipt.core_bytes);
    let dual = Receipt {
        core_bytes: receipt.core_bytes.clone(),
        sig_requester: receipt.sig_requester,
        sig_provider: Some(sig_prov),
    };
    // Idempotent store (duplicate RECEIPT → Duplicate); ACK re-sent regardless.
    let _ = store.ingest(dual.clone());
    vec![(msg_type::RECEIPT_ACK, dual.to_value())]
}

fn error_reply(code: u16) -> Vec<Outgoing> {
    vec![(msg_type::ERROR, ErrorMsg::new(code, false).to_value())]
}
