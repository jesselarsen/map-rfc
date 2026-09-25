use map_core::envelope::MsgId;
use map_core::replay::ReplayCache;
use map_identity::Keypair;
use std::time::Instant;

fn main() {
    let a = Keypair::from_seed([1u8; 32]).agent_id();
    for n in [2_000u64, 8_000, 32_000, 64_000] {
        let mut c = ReplayCache::new();
        let now = 1_000_000_000u64;
        let t = Instant::now();
        for i in 0..n {
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&i.to_le_bytes());
            c.check(&a, MsgId(id), now, now);
        }
        let d = t.elapsed();
        println!(
            "n={:>6}  total={:>9.1}ms  per-msg={:>8.2}us  cache_len={}",
            n, d.as_secs_f64()*1000.0, d.as_secs_f64()*1e6/n as f64, c.len()
        );
    }
}
