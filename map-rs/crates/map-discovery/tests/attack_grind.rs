//! Measurement for Role 7 finding A4 (node-ID grinding), ported from
//! `attic/probes/{adv,rt7probe}/src/bin/grind.rs`. An AgentID is a freely
//! chosen Ed25519 key, so its discovery shard can be ground for cheaply
//! (probe, release build: ~61k keygens/s; 16.5 µs per keygen+shard).
//!
//! This is a measurement, not a pass/fail check: it prints the grinding rate
//! used by evaluation E3 (the attacker-budget axis) and sanity-checks that
//! shard hits are uniform. Run in release mode:
//! `cargo test -p map-discovery --release --test attack_grind -- --ignored --nocapture`

use map_discovery::shard;
use map_identity::Keypair;
use std::time::Instant;

#[test]
#[ignore = "measurement for evaluation E3; run with --release --nocapture"]
fn shard_grinding_rate() {
    let n = 200_000u32;
    let target = 141u8;
    let t = Instant::now();
    let mut hits = 0u32;
    for i in 0..n {
        let mut seed = [0u8; 32];
        seed[..4].copy_from_slice(&i.to_le_bytes());
        if shard(&Keypair::from_seed(seed).agent_id()) == target {
            hits += 1;
        }
    }
    let secs = t.elapsed().as_secs_f64();
    println!(
        "{n} keygens+shard in {secs:.2}s = {:.0}/s; shard-{target} hits={hits} (expect ~{})",
        n as f64 / secs,
        n / 256
    );
    let expected = n / 256;
    assert!(hits > expected / 2 && hits < expected * 2, "shard hits not uniform: {hits}");
}
