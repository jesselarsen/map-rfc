//! Receipt-lifecycle helpers (trust §3): the countersignature agreement
//! tolerance. `make_receipt_core` (which maps a *task* terminal to an outcome)
//! lives in `map-taskrun`, not here — it needs the task-runtime's ReceiptInputs
//! and a trust→taskrun dep would be circular; this crate keeps the pure,
//! data-only tolerance check the provider applies before it countersigns.

use crate::ReceiptCore;

/// Usage tolerance: ±5% of the OBSERVED value (min ±1). Anchored to the
/// provider-observed (trusted) side, NOT `max(claimed, observed)`, so an
/// inflated requester claim cannot widen its own acceptance band (the §11
/// countersignature-farming defense). Assigned by TASK-008; recorded in
/// trust §3.
pub const USAGE_TOLERANCE_NUM: u64 = 1; // 1/20 = 5%
pub const USAGE_TOLERANCE_DEN: u64 = 20;
/// Timestamp tolerance: ±2000 ms on `started`/`ended`.
pub const TIMESTAMP_TOLERANCE_MS: u64 = 2000;

fn field_ok(claimed: u64, observed: u64) -> bool {
    let tol = std::cmp::max(1, observed * USAGE_TOLERANCE_NUM / USAGE_TOLERANCE_DEN);
    claimed.abs_diff(observed) <= tol
}

/// True iff a requester's `claimed` ReceiptCore agrees with the provider's
/// `observed` core within tolerance on all four usage fields and both
/// timestamps. The caller separately checks task-id / capability / identities
/// for exact equality — this is the numeric agreement the countersignature
/// attests (trust §3).
pub fn within_tolerance(claimed: &ReceiptCore, observed: &ReceiptCore) -> bool {
    field_ok(claimed.metrics.tokens_in, observed.metrics.tokens_in)
        && field_ok(claimed.metrics.tokens_out, observed.metrics.tokens_out)
        && field_ok(claimed.metrics.ttft_ms, observed.metrics.ttft_ms)
        && field_ok(claimed.metrics.wall_ms, observed.metrics.wall_ms)
        && claimed.started_ms.abs_diff(observed.started_ms) <= TIMESTAMP_TOLERANCE_MS
        && claimed.ended_ms.abs_diff(observed.ended_ms) <= TIMESTAMP_TOLERANCE_MS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Metrics, Outcome, ReceiptCore};
    use map_identity::Keypair;
    use map_tasks::TaskId;

    fn core(metrics: Metrics, started: u64, ended: u64) -> ReceiptCore {
        ReceiptCore {
            task: TaskId([1; 32]),
            requester: Keypair::from_seed([1; 32]).agent_id(),
            provider: Keypair::from_seed([2; 32]).agent_id(),
            capability: "map:cap:inference/text-generation".into(),
            started_ms: started,
            ended_ms: ended,
            outcome: Outcome::Ok,
            metrics,
            rating: None,
            payment_ref: None,
            transcript_root: None,
        }
    }

    #[test]
    fn tolerance_anchors_to_observed_not_the_claim() {
        let observed = core(Metrics { tokens_in: 100, tokens_out: 100, ttft_ms: 100, wall_ms: 1000 }, 1_000, 2_000);
        // Within 5% of observed (100 → ±5): claim 105 ok, 106 not.
        let ok = core(Metrics { tokens_in: 105, tokens_out: 100, ttft_ms: 100, wall_ms: 1000 }, 1_000, 2_000);
        let bad = core(Metrics { tokens_in: 106, tokens_out: 100, ttft_ms: 100, wall_ms: 1000 }, 1_000, 2_000);
        assert!(within_tolerance(&ok, &observed));
        assert!(!within_tolerance(&bad, &observed));
        // The farming case: observed 0, claim 100 → window is max(1, 0) = 1 → rejected.
        let obs0 = core(Metrics { tokens_in: 0, tokens_out: 0, ttft_ms: 0, wall_ms: 0 }, 1_000, 2_000);
        let inflated = core(Metrics { tokens_in: 100, tokens_out: 0, ttft_ms: 0, wall_ms: 0 }, 1_000, 2_000);
        assert!(!within_tolerance(&inflated, &obs0));
    }

    #[test]
    fn timestamp_window_is_two_seconds() {
        let observed = core(Metrics { tokens_in: 1, tokens_out: 1, ttft_ms: 1, wall_ms: 1 }, 10_000, 20_000);
        let ok = core(Metrics { tokens_in: 1, tokens_out: 1, ttft_ms: 1, wall_ms: 1 }, 12_000, 20_000);
        let bad = core(Metrics { tokens_in: 1, tokens_out: 1, ttft_ms: 1, wall_ms: 1 }, 12_001, 20_000);
        assert!(within_tolerance(&ok, &observed));
        assert!(!within_tolerance(&bad, &observed));
    }
}
