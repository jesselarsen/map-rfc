//! Regression test for Role 7 finding A8 (no rate limits on discovery/status
//! queries). Binding DHT-9 requires a per-peer token bucket; `serve_one`
//! answers a peer over budget with ERROR 1005.

use map_identity::Keypair;
use map_mesh::RateLimiter;

#[test]
fn burst_then_refill_per_peer() {
    let a = Keypair::from_seed([1; 32]).agent_id();
    let b = Keypair::from_seed([2; 32]).agent_id();
    let mut rl = RateLimiter::new(4, 2); // burst 4, 2 per second
    let t0 = 1_790_000_000_000u64;
    for _ in 0..4 {
        assert!(rl.allow(&a, t0));
    }
    assert!(!rl.allow(&a, t0), "burst exhausted");
    // Another peer has its own bucket.
    assert!(rl.allow(&b, t0));
    // 499 ms refill ~0.998 tokens: still limited; at 500 ms one token is back.
    assert!(!rl.allow(&a, t0 + 499));
    assert!(rl.allow(&a, t0 + 500));
    assert!(!rl.allow(&a, t0 + 500));
    // Long idle refills only up to the burst, never beyond.
    let later = t0 + 3_600_000;
    for _ in 0..4 {
        assert!(rl.allow(&a, later));
    }
    assert!(!rl.allow(&a, later));
}

#[test]
fn flood_is_bounded_by_the_budget() {
    let attacker = Keypair::from_seed([9; 32]).agent_id();
    let mut rl = RateLimiter::default(); // 32 burst, 8/s
    let t0 = 1_790_000_000_000u64;
    // 10_000 queries spread over 10 s: at most 32 + 8*10 = 112 are served.
    let served = (0..10_000u64).filter(|i| rl.allow(&attacker, t0 + i)).count();
    assert!(served <= 112, "served {served}");
    assert!(served >= 100, "served {served}");
}
