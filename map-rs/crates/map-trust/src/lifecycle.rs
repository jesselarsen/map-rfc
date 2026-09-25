//! Countersignature tolerance (spec/layer.md §6.5). The second signer of a
//! receipt or checkpoint checks the proposed usage counters and timestamps
//! against its OWN observation before signing. The window is a local policy
//! parameter ([`Tolerance`]); v1 fixed it normatively at ±5% / ±2000 ms,
//! which remain the defaults. The caller separately checks task-id,
//! capability and identities for exact equality.

use crate::ReceiptCore;

/// Local countersignature policy (layer §6.5: `tolerance.usage_pct`,
/// `tolerance.time_ms`). The window is anchored to the countersigner's
/// OBSERVED value, never `max(claimed, observed)`, so an inflated claim
/// cannot widen its own acceptance band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tolerance {
    /// Allowed deviation per usage counter, percent of the observed value
    /// (minimum ±1).
    pub usage_pct: u64,
    /// Allowed deviation on `started`/`ended`, milliseconds.
    pub time_ms: u64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Tolerance { usage_pct: 5, time_ms: 2000 }
    }
}

impl Tolerance {
    fn field_ok(&self, claimed: u64, observed: u64) -> bool {
        let tol = std::cmp::max(1, observed.saturating_mul(self.usage_pct) / 100);
        claimed.abs_diff(observed) <= tol
    }

    /// True iff `claimed` agrees with the countersigner's `observed` core
    /// within this window on all four usage fields and both timestamps.
    pub fn accepts(&self, claimed: &ReceiptCore, observed: &ReceiptCore) -> bool {
        self.field_ok(claimed.metrics.tokens_in, observed.metrics.tokens_in)
            && self.field_ok(claimed.metrics.tokens_out, observed.metrics.tokens_out)
            && self.field_ok(claimed.metrics.ttft_ms, observed.metrics.ttft_ms)
            && self.field_ok(claimed.metrics.wall_ms, observed.metrics.wall_ms)
            && claimed.started_ms.abs_diff(observed.started_ms) <= self.time_ms
            && claimed.ended_ms.abs_diff(observed.ended_ms) <= self.time_ms
    }
}

/// [`Tolerance::accepts`] under the default policy.
pub fn within_tolerance(claimed: &ReceiptCore, observed: &ReceiptCore) -> bool {
    Tolerance::default().accepts(claimed, observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Metrics, Outcome, ReceiptCore};
    use map_identity::Keypair;
    use map_core::TaskId;

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
