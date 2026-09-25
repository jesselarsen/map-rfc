//! Applying evidence obtained from an untrusted holder (spec/layer.md §6.4, §8).
//!
//! Carried over from v1 map-taskrun without its session-bound
//! EVIDENCE_QUERY/EVIDENCE_SET exchange, which retired with the v1 task
//! protocol. How receipts travel between parties is a binding concern. What
//! stays here is the pure gate: a receipt from any holder is applied only if
//! it is dual-signed AND names the queried subject as its provider. The
//! provider check stops a holder filing a receipt under the wrong
//! (provider, capability) cell.
//!
//! Planned for P3: layer rule T-2 requires these gates inside
//! [`TrustProjection::apply`] itself, so that `rebuild` enforces them too
//! (reviewer finding A1; see `tests/attack_farming.rs`). Until then this
//! function is the only place they are applied.

use crate::{TrustProjection, Verified};
use map_identity::AgentId;

/// Apply verified evidence about `subject` to a projection, enforcing both
/// gates (dual-signed; `subject` is the provider). Returns the count applied.
pub fn apply_verified_evidence(
    evidence: &[Verified],
    subject: &AgentId,
    proj: &mut TrustProjection,
    now_ms: u64,
    trust_in_issuer: f64,
) -> usize {
    let mut applied = 0;
    for v in evidence {
        if v.dual && &v.core.provider == subject {
            proj.apply(v, now_ms, trust_in_issuer);
            applied += 1;
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sign_receipt, Metrics, Outcome, ProjParams, Receipt, ReceiptCore, ROLE_PROVIDER, ROLE_REQUESTER};
    use map_core::TaskId;
    use map_identity::Keypair;

    const CAP: &str = "map:cap:inference/text-generation";

    fn verified(req: &Keypair, prov: &Keypair, dual: bool) -> Verified {
        let core = ReceiptCore {
            task: TaskId([3; 32]),
            requester: req.agent_id(),
            provider: prov.agent_id(),
            capability: CAP.into(),
            started_ms: 1_000,
            ended_ms: 2_000,
            outcome: Outcome::Ok,
            metrics: Metrics { tokens_in: 1, tokens_out: 1, ttft_ms: 1, wall_ms: 1 },
            rating: None,
            payment_ref: None,
            transcript_root: None,
        };
        let cb = core.encode().unwrap();
        Receipt {
            sig_requester: dual.then(|| sign_receipt(req, ROLE_REQUESTER, &cb)),
            sig_provider: Some(sign_receipt(prov, ROLE_PROVIDER, &cb)),
            core_bytes: cb,
        }
        .verify()
        .unwrap()
    }

    #[test]
    fn only_dual_signed_receipts_about_the_subject_apply() {
        let (req, prov, other) = (Keypair::from_seed([1; 32]), Keypair::from_seed([2; 32]), Keypair::from_seed([3; 32]));
        let evidence = vec![
            verified(&req, &prov, true),   // applies
            verified(&req, &prov, false),  // provider-only: rejected
            verified(&req, &other, true),  // wrong provider: rejected
        ];
        let mut proj = TrustProjection::new(ProjParams::default());
        assert_eq!(apply_verified_evidence(&evidence, &prov.agent_id(), &mut proj, 3_000, 1.0), 1);
        assert!(proj.score(&prov.agent_id(), CAP).is_some());
        assert!(proj.score(&other.agent_id(), CAP).is_none());
    }
}
