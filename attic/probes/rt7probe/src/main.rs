// Red-team probe (read-only against the repo): self-dealing + future-dated receipts.
use map_identity::Keypair;
use map_tasks::TaskId;
use map_trust::*;

fn mint(req: &Keypair, prov: &Keypair, ended_ms: u64, outcome: Outcome) -> Receipt {
    let core = ReceiptCore {
        task: TaskId::random(),
        requester: req.agent_id(),
        provider: prov.agent_id(),
        capability: "map:cap:inference/text-generation".into(),
        started_ms: ended_ms - 1000,
        ended_ms,
        outcome,
        metrics: Metrics { tokens_in: 10, tokens_out: 10, ttft_ms: 50, wall_ms: 1000 },
        rating: Some(100),
        payment_ref: None,
        transcript_root: None,
    };
    let cb = core.encode().unwrap();
    Receipt {
        sig_requester: Some(sign_receipt(req, ROLE_REQUESTER, &cb)),
        sig_provider: Some(sign_receipt(prov, ROLE_PROVIDER, &cb)),
        core_bytes: cb,
    }
}

fn main() {
    let now: u64 = 1_790_000_000_000; // ~2026-09
    let day: u64 = 86_400_000;
    let me = Keypair::from_seed([7; 32]);

    // A. ONE identity signs both roles -> verify() says dual=true.
    let r = mint(&me, &me, now, Outcome::Ok);
    let v = r.verify().unwrap();
    println!("A self-dealing receipt: requester==provider={} dual={}", v.core.requester == v.core.provider, v.dual);

    // B. Honest provider H: 50 real ok receipts, dated 30 days ago (decayed).
    let h = Keypair::from_seed([1; 32]);
    let honest_req = Keypair::from_seed([2; 32]);
    // C. Attacker X: 50 self-dealt receipts dated year 2100 (never decay).
    let x = Keypair::from_seed([9; 32]);
    let far = 4_102_444_800_000u64; // 2100-01-01
    let mut store = ReceiptStore::new();
    for _ in 0..50 { store.ingest(mint(&honest_req, &h, now - 30 * day, Outcome::Ok)).unwrap(); }
    for _ in 0..50 { store.ingest(mint(&x, &x, far, Outcome::Ok)).unwrap(); }
    // Also one honest negative against X, from a real requester.
    store.ingest(mint(&honest_req, &x, now, Outcome::Failed)).unwrap();
    for (label, t) in [("now", now), ("now+1y", now + 365 * day), ("now+10y", now + 3650 * day)] {
        let p = TrustProjection::rebuild(&store, t, ProjParams::default(), |_| 1.0).unwrap();
        let (hs, hc) = p.score(&h.agent_id(), "map:cap:inference/text-generation").unwrap();
        let (xs, xc) = p.score(&x.agent_id(), "map:cap:inference/text-generation").unwrap();
        println!("{label:8} honest H score={hs:.3} conf={hc:.2} | self-dealer X score={xs:.3} conf={xc:.2}");
    }
    // D. Cost of minting.
    let t0 = std::time::Instant::now();
    let mut s2 = ReceiptStore::new();
    for _ in 0..10_000 { s2.ingest(mint(&me, &me, far, Outcome::Ok)).unwrap(); }
    println!("D minted+ingested 10k self-dealt dual receipts in {:?}", t0.elapsed());
}

#[allow(dead_code)]
pub fn keygen_rate() {
    // Grinding cost: keypairs/s, and hits for a target shard byte (discovery §6).
    use map_discovery_shim::shard_of;
    let _ = shard_of;
}
mod map_discovery_shim { pub fn shard_of() {} }
