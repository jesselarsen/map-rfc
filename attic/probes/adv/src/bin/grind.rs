use map_identity::Keypair;
use std::time::Instant;

// How fast can an attacker mint AgentIDs and grind for a target DHT position?
// AgentID = Ed25519 pubkey; libp2p PeerID = multihash(identity, pubkey) for
// Ed25519, so the DHT position is a deterministic function of a FREELY CHOSEN
// keypair. Nothing in identity-01 or discovery-01 costs a node ID.
fn main() {
    let n = 200_000u64;
    let t = Instant::now();
    let mut best_prefix = 0u32;
    let target = [0x5Au8; 32]; // stand-in for K'(cap, s)
    let mut seed = [0u8; 32];
    for i in 0..n {
        seed[..8].copy_from_slice(&i.to_le_bytes());
        let kp = Keypair::from_seed(seed);
        let id = kp.agent_id();
        let b = id.as_bytes();
        // XOR-distance prefix match against the target key (Kademlia closeness)
        let mut bits = 0u32;
        for (x, y) in b[2..].iter().zip(target.iter()) {
            let z = x ^ y;
            bits += z.leading_zeros();
            if z != 0 { break; }
        }
        if bits > best_prefix { best_prefix = bits; }
    }
    let d = t.elapsed();
    let rate = n as f64 / d.as_secs_f64();
    println!("derived {n} AgentIDs in {:.2}s = {:.0} ids/sec (single core, debug-free release)", d.as_secs_f64(), rate);
    println!("best XOR prefix match found: {best_prefix} bits");
    for bits in [16u32, 20, 24, 28, 32] {
        let tries = 2f64.powi(bits as i32);
        println!("  {bits}-bit prefix: ~{:.0} keygens = {:.1} core-seconds", tries, tries / rate);
    }
}
