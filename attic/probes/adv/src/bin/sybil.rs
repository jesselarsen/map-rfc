use map_identity::Keypair;
use map_tasks::TaskId;
use map_trust::*;
use std::time::Instant;

fn dual(req:&Keypair, prov:&Keypair, out:Outcome, t:u64) -> Receipt {
    let c = ReceiptCore {
        task: TaskId([t as u8;32]), requester: req.agent_id(), provider: prov.agent_id(),
        capability: "map:cap:inference/text-generation".into(),
        started_ms: 1_000_000, ended_ms: 1_000_500, outcome: out,
        metrics: Metrics{tokens_in:10,tokens_out:20,ttft_ms:50,wall_ms:500},
        rating: Some(100), payment_ref: None, transcript_root: None,
    };
    let cb = c.encode().unwrap();
    Receipt{ core_bytes: cb.clone(),
        sig_requester: Some(sign_receipt(req,1,&cb)),
        sig_provider:  Some(sign_receipt(prov,2,&cb)) }
}

fn main() {
    let attacker = Keypair::from_seed([0xBB;32]);
    let honest_req = Keypair::from_seed([0x11;32]);
    let cap = "map:cap:inference/text-generation";
    let now = 1_001_000u64;

    // Honest baseline: one real interaction, requester fully trusted (1.0).
    let mut base = TrustProjection::new(ProjParams::default());
    base.apply(&dual(&honest_req,&attacker,Outcome::Ok,1).verify().unwrap(), now, 1.0);
    println!("honest 1 receipt        -> {:?}", base.score(&attacker.agent_id(), cap));

    // Sybil ring: N throwaway keypairs, each DUAL-signing a receipt for the
    // attacker. Dual-signing is free -- the attacker holds both keys.
    // trust §7's own formula scores an unknown issuer at alpha0/(alpha0+beta0)
    // = 0.5, so each Sybil receipt lands at weight 0.5, not "~0" (trust §9).
    for n in [10u32, 100, 1000] {
        let t0 = Instant::now();
        let mut p = TrustProjection::new(ProjParams::default());
        p.apply(&dual(&honest_req,&attacker,Outcome::Ok,1).verify().unwrap(), now, 1.0);
        for i in 0..n {
            let s = Keypair::from_seed([(i%251) as u8 + 1; 32]);
            let r = dual(&s,&attacker,Outcome::Ok, 2 + i as u64);
            p.apply(&r.verify().unwrap(), now, 0.5); // unknown-issuer prior weight
        }
        let (sc, conf) = p.score(&attacker.agent_id(), cap).unwrap();
        println!("+{n:>5} sybil receipts  -> score={sc:.4} confidence={conf:.1}   minted in {:?}", t0.elapsed());
    }

    // What one honest NEGATIVE receipt is worth against that wall.
    let mut p = TrustProjection::new(ProjParams::default());
    for i in 0..1000u32 {
        let s = Keypair::from_seed([(i%251) as u8 + 1; 32]);
        p.apply(&dual(&s,&attacker,Outcome::Ok,2+i as u64).verify().unwrap(), now, 0.5);
    }
    p.apply(&dual(&honest_req,&attacker,Outcome::Failed,9999).verify().unwrap(), now, 1.0);
    println!("1000 sybil + 1 honest FAIL -> {:?}", p.score(&attacker.agent_id(), cap));
}
