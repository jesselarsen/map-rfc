//! Receipt construction for scenarios. Real mode builds, signs and verifies
//! actual `map-trust` receipts (Ed25519); fast mode (`--fast-crypto`)
//! produces the identical `Verified` value without the signature work.
//! Keys are always real (keygen is cheap), and Ed25519 signing consumes no
//! randomness, so both modes draw the same RNG sequence and must yield the
//! same results (tests/fast_real.rs).

use map_core::{TaskId, Usage};
use map_identity::Keypair;
use map_trust::{
    sign_receipt, Metrics, Outcome, Receipt, ReceiptCore, Verified, ROLE_PROVIDER, ROLE_REQUESTER,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug)]
pub struct CryptoMode {
    pub fast: bool,
}

/// A deterministic TaskId for the n-th receipt of a run.
pub fn task_id(n: u64) -> TaskId {
    let mut h = Sha256::new();
    h.update(b"map-sim/task\0");
    h.update(n.to_le_bytes());
    TaskId(h.finalize().into())
}

pub struct ReceiptSpec<'a> {
    pub n: u64,
    pub requester: &'a Keypair,
    pub provider: &'a Keypair,
    pub capability: &'a str,
    pub started_ms: u64,
    pub ended_ms: u64,
    pub outcome: Outcome,
    pub usage: Usage,
    pub requester_signs: bool,
    pub provider_signs: bool,
}

pub fn receipt(mode: CryptoMode, s: &ReceiptSpec) -> Verified {
    let core = ReceiptCore {
        task: task_id(s.n),
        requester: s.requester.agent_id(),
        provider: s.provider.agent_id(),
        capability: s.capability.to_string(),
        started_ms: s.started_ms,
        ended_ms: s.ended_ms,
        outcome: s.outcome,
        metrics: Metrics {
            tokens_in: s.usage.tokens_in,
            tokens_out: s.usage.tokens_out,
            ttft_ms: s.usage.ttft_ms,
            wall_ms: s.usage.wall_ms,
        },
        rating: None,
        payment_ref: None,
        transcript_root: None,
        zone: None,
        resumed_from: None,
    };
    if mode.fast {
        return Verified {
            dual: s.requester_signs && s.provider_signs,
            requester_signed: s.requester_signs,
            provider_signed: s.provider_signs,
            core,
        };
    }
    let core_bytes = core.encode().expect("receipt core encodes");
    let r = Receipt {
        sig_requester: s
            .requester_signs
            .then(|| sign_receipt(s.requester, ROLE_REQUESTER, &core_bytes)),
        sig_provider: s
            .provider_signs
            .then(|| sign_receipt(s.provider, ROLE_PROVIDER, &core_bytes)),
        core_bytes,
    };
    r.verify().expect("a freshly signed receipt verifies")
}
