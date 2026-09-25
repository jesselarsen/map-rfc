use map_identity::Keypair;
use map_tasks::TaskId;
use map_trust::*;

fn core(req: &Keypair, prov: &Keypair, outcome: Outcome) -> ReceiptCore {
    ReceiptCore {
        task: TaskId([7u8; 32]),
        requester: req.agent_id(),
        provider: prov.agent_id(),
        capability: "map:cap:inference/text-generation".into(),
        started_ms: 1_000_000,
        ended_ms: 1_000_500,
        outcome,
        metrics: Metrics { tokens_in: 10, tokens_out: 20, ttft_ms: 50, wall_ms: 500 },
        rating: Some(100),
        payment_ref: None,
        transcript_root: None,
    }
}

fn main() {
    // "Reputable" requester R — the attacker does NOT control this key.
    let reputable = Keypair::from_seed([0xAA; 32]);
    // Attacker-controlled provider P.
    let attacker = Keypair::from_seed([0xBB; 32]);

    // ATTACK: P mints a receipt NAMING R as requester, signs ONLY role 2.
    let c = core(&reputable, &attacker, Outcome::Ok);
    let cb = c.encode().unwrap();
    let forged = Receipt {
        core_bytes: cb.clone(),
        sig_requester: None,
        sig_provider: Some(sign_receipt(&attacker, 2, &cb)),
    };

    match forged.verify() {
        Ok(v) => println!("[1] forged unilateral VERIFIES. dual={} provider={:?}", v.dual, v.core.provider.as_bytes()[0..2].to_vec()),
        Err(e) => println!("[1] rejected: {e:?}"),
    }

    let mut store = ReceiptStore::new();
    println!("[2] store.ingest -> {:?}", store.ingest(forged.clone()).unwrap());

    // The normative rebuild rule's implementation. trust_in_issuer is keyed on
    // core.requester -- a field the ATTACKER chose. Model an evaluator that
    // trusts R at 1.0 and unknown agents at 0.0.
    let r_id = reputable.agent_id();
    let proj = TrustProjection::rebuild(&store, 1_100_000, ProjParams::default(), |a| {
        if *a == r_id { 1.0 } else { 0.0 }
    })
    .unwrap();
    println!(
        "[3] rebuild() score for attacker = {:?}",
        proj.score(&attacker.agent_id(), "map:cap:inference/text-generation")
    );

    // Incremental path used by map-taskrun gates on dual: build the same
    // projection the way apply_verified_evidence does.
    let v = forged.verify().unwrap();
    let mut inc = TrustProjection::new(ProjParams::default());
    if v.dual { inc.apply(&v, 1_100_000, 1.0); }
    println!(
        "[4] dual-gated incremental score = {:?}  <-- diverges from [3]",
        inc.score(&attacker.agent_id(), "map:cap:inference/text-generation")
    );
}
