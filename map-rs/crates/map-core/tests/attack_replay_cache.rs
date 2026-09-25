//! Regression test for Role 7 finding A8 (quadratic replay cache), ported
//! from `attic/probes/adv/src/bin/replay.rs`. `ReplayCache::check` scans the
//! whole cache on every call, so per-message cost grows with cache size
//! (probe, release build: 1.26 µs at n=2000, 40.82 µs at n=64000).
//!
//! Asserts the SECURE behaviour (roughly constant per-message cost) and is
//! `#[ignore]`d until the fix lands (accountability-layer plan, P3). It is a
//! timing ratio with a generous bound, not an absolute timing.

use map_core::envelope::MsgId;
use map_core::replay::ReplayCache;
use map_identity::Keypair;
use std::time::Instant;

fn per_msg_nanos(n: u64) -> f64 {
    let sender = Keypair::from_seed([1u8; 32]).agent_id();
    let now = 1_790_000_000_000u64;
    let mut cache = ReplayCache::new();
    let t = Instant::now();
    for i in 0..n {
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&i.to_le_bytes());
        cache.check(&sender, MsgId(id), now, now);
    }
    t.elapsed().as_nanos() as f64 / n as f64
}

#[test]
#[ignore = "Role 7 A8 open; fixed in accountability-layer P3"]
fn replay_cache_per_message_cost_is_flat() {
    let small = per_msg_nanos(1_000);
    let large = per_msg_nanos(16_000);
    let ratio = large / small;
    assert!(
        ratio < 4.0,
        "per-message cost grew {ratio:.1}x from n=1000 to n=16000 ({small:.0} ns -> {large:.0} ns)"
    );
}
