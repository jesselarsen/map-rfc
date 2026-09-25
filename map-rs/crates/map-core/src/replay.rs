//! Anti-replay state (draft-map-core-01 §7): per-sender msg-id uniqueness
//! cache with a ±skew timestamp window (§7.1) and strict per-direction
//! session sequence tracking (§7.2). Channel binding (§7.3) lives in
//! [`crate::hello::channel_binding`] and is enforced by [`crate::envelope`]
//! via `external_aad`.

use crate::envelope::MsgId;
use map_identity::AgentId;
use std::collections::{HashMap, VecDeque};

/// ±120 s clock-skew window (core §4.3 rule 5).
pub const DEFAULT_SKEW_MS: u64 = 120_000;
/// 10-minute uniqueness cache (core §7.1).
pub const DEFAULT_WINDOW_MS: u64 = 600_000;

#[derive(Debug, PartialEq, Eq)]
pub enum ReplayCheck {
    Fresh,
    /// (sender, msg-id) already seen inside the window — error 1003.
    Replayed,
    /// Timestamp outside ±skew — error 1003.
    OutOfWindow,
}

/// Bounded by rate limits; entries expire after `window_ms`.
///
/// Expiry is amortized O(1) per check (reviewer finding A8): entries are also
/// queued in arrival order, and only the queue's expired front is popped,
/// instead of rescanning the whole map on every message. If the caller's clock
/// steps backwards, entries may stay longer than `window_ms`. That errs on the
/// safe side (a replay is still caught), never the unsafe one.
pub struct ReplayCache {
    skew_ms: u64,
    window_ms: u64,
    seen: HashMap<(AgentId, MsgId), u64>,
    order: VecDeque<(u64, (AgentId, MsgId))>,
}

impl Default for ReplayCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplayCache {
    pub fn new() -> Self {
        Self::with_params(DEFAULT_SKEW_MS, DEFAULT_WINDOW_MS)
    }

    pub fn with_params(skew_ms: u64, window_ms: u64) -> Self {
        ReplayCache {
            skew_ms,
            window_ms,
            seen: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Check-and-record. Callers pass the envelope's `timestamp_ms` and
    /// their local clock; a `Fresh` result records the id.
    pub fn check(
        &mut self,
        sender: &AgentId,
        msg_id: MsgId,
        msg_timestamp_ms: u64,
        now_ms: u64,
    ) -> ReplayCheck {
        if msg_timestamp_ms < now_ms.saturating_sub(self.skew_ms)
            || msg_timestamp_ms > now_ms.saturating_add(self.skew_ms)
        {
            return ReplayCheck::OutOfWindow;
        }
        let cutoff = now_ms.saturating_sub(self.window_ms);
        while let Some((arrived, _)) = self.order.front() {
            if *arrived >= cutoff {
                break;
            }
            let (arrived, key) = self.order.pop_front().expect("front exists");
            // Remove only if the map still holds this exact arrival.
            if self.seen.get(&key) == Some(&arrived) {
                self.seen.remove(&key);
            }
        }

        let key = (sender.clone(), msg_id);
        if self.seen.contains_key(&key) {
            return ReplayCheck::Replayed;
        }
        self.seen.insert(key.clone(), now_ms);
        self.order.push_back((now_ms, key));
        ReplayCheck::Fresh
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// Per-direction session sequence numbers: dense, monotonic from 0
/// (core §7.2). Receivers reject anything else with 1003.
#[derive(Default)]
pub struct SeqTracker {
    next: u64,
}

impl SeqTracker {
    pub fn new() -> Self {
        SeqTracker { next: 0 }
    }
    pub fn accept(&mut self, seq: u64) -> bool {
        if seq == self.next {
            self.next += 1;
            true
        } else {
            false
        }
    }
    pub fn expected(&self) -> u64 {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;

    #[test]
    fn replay_and_window_semantics() {
        let sender = Keypair::from_seed([3u8; 32]).agent_id();
        let mut cache = ReplayCache::new();
        let now = 1_780_000_000_000u64;
        let id = MsgId([5u8; 16]);

        assert_eq!(cache.check(&sender, id, now, now), ReplayCheck::Fresh);
        assert_eq!(cache.check(&sender, id, now, now), ReplayCheck::Replayed);
        // Same id from a different sender is a different key.
        let other = Keypair::from_seed([4u8; 32]).agent_id();
        assert_eq!(cache.check(&other, id, now, now), ReplayCheck::Fresh);
        // Outside ±120 s.
        assert_eq!(
            cache.check(&sender, MsgId([6u8; 16]), now - 121_000, now),
            ReplayCheck::OutOfWindow
        );
        // Expiry: after the window passes, the id is purged (fresh again;
        // the timestamp window is what makes this safe).
        let later = now + DEFAULT_WINDOW_MS + 1;
        assert_eq!(cache.check(&sender, id, later, later), ReplayCheck::Fresh);
    }

    #[test]
    fn seq_is_dense_monotonic() {
        let mut t = SeqTracker::new();
        assert!(t.accept(0));
        assert!(t.accept(1));
        assert!(!t.accept(1)); // duplicate
        assert!(!t.accept(3)); // gap
        assert!(t.accept(2));
    }
}
