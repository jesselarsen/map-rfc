use map_identity::Keypair;
fn main() {
    let n = 200_000u32;
    let t0 = std::time::Instant::now();
    let mut hits = 0u32;
    for i in 0..n {
        let mut seed = [0u8; 32];
        seed[..4].copy_from_slice(&i.to_le_bytes());
        let id = Keypair::from_seed(seed).agent_id();
        if map_discovery::shard(&id) == 141 { hits += 1; }
    }
    let el = t0.elapsed();
    println!("{} keygens+shard in {:?} = {:.2} us each; shard-141 hits={} (expect {})",
        n, el, el.as_secs_f64() * 1e6 / n as f64, hits, n / 256);
}
