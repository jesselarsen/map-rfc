//! Regression tests for Role 7 finding A1 (receipt farming), ported from the
//! reviewer probes in `attic/probes/{adv,rt7probe}`.
//!
//! Each test asserts the SECURE behaviour. They were `#[ignore]`d while the
//! finding was open and pass since P3: layer rules R-2/R-3/R-4 inside
//! `TrustProjection::apply` (T-2) and zero-default issuer weighting (T-3,
//! `IssuerWeights`).

use map_identity::Keypair;
use map_core::TaskId;
use map_trust::*;

const CAP: &str = "map:cap:inference/text-generation";
const NOW: u64 = 1_790_000_000_000; // ~2026-09
const DAY: u64 = 86_400_000;

fn mint(req: &Keypair, prov: &Keypair, ended_ms: u64, outcome: Outcome, n: u8) -> ReceiptCore {
    ReceiptCore {
        task: TaskId([n; 32]),
        requester: req.agent_id(),
        provider: prov.agent_id(),
        capability: CAP.into(),
        started_ms: ended_ms - 1000,
        ended_ms,
        outcome,
        metrics: Metrics { tokens_in: 10, tokens_out: 10, ttft_ms: 50, wall_ms: 1000 },
        rating: Some(100),
        payment_ref: None,
        transcript_root: None,
        zone: None,
        resumed_from: None,
    }
}

fn dual(req: &Keypair, prov: &Keypair, ended_ms: u64, outcome: Outcome, n: u8) -> Receipt {
    let cb = mint(req, prov, ended_ms, outcome, n).encode().unwrap();
    Receipt {
        sig_requester: Some(sign_receipt(req, ROLE_REQUESTER, &cb)),
        sig_provider: Some(sign_receipt(prov, ROLE_PROVIDER, &cb)),
        core_bytes: cb,
    }
}

/// A receipt whose requester and provider are the same key is not evidence
/// about anyone (rt7probe case A: it verifies with `dual=true` today).
#[test]
fn self_dealing_receipt_earns_no_score() {
    let me = Keypair::from_seed([7; 32]);
    let mut store = ReceiptStore::new();
    for n in 0..50u8 {
        // Rejecting at ingest is an acceptable fix too.
        let _ = store.ingest(dual(&me, &me, NOW, Outcome::Ok, n));
    }
    let p = TrustProjection::rebuild(&store, NOW, ProjParams::default(), |_| 1.0).unwrap();
    assert_eq!(p.score(&me.agent_id(), CAP), None);
}

/// Receipts dated in the future must not count: they never decay and so
/// overtake honest, ageing evidence (rt7probe cases B/C, year-2100 receipts).
#[test]
fn future_dated_receipts_do_not_count() {
    let attacker = Keypair::from_seed([9; 32]);
    let accomplice = Keypair::from_seed([8; 32]);
    let far = 4_102_444_800_000u64; // 2100-01-01
    let mut store = ReceiptStore::new();
    for n in 0..50u8 {
        let _ = store.ingest(dual(&accomplice, &attacker, far, Outcome::Ok, n));
    }
    let p = TrustProjection::rebuild(&store, NOW, ProjParams::default(), |_| 1.0).unwrap();
    assert_eq!(p.score(&attacker.agent_id(), CAP), None);
}

/// With the same weights, an honest provider must not end up below a
/// self-dealer as time passes (rt7probe: at now+10y honest H = 0.500 while
/// self-dealer X = 0.981).
#[test]
fn honest_provider_is_not_overtaken_by_self_dealer() {
    let h = Keypair::from_seed([1; 32]);
    let honest_req = Keypair::from_seed([2; 32]);
    let x = Keypair::from_seed([9; 32]);
    let far = 4_102_444_800_000u64;
    let mut store = ReceiptStore::new();
    for n in 0..50u8 {
        store.ingest(dual(&honest_req, &h, NOW - 30 * DAY, Outcome::Ok, n)).unwrap();
        let _ = store.ingest(dual(&x, &x, far, Outcome::Ok, 100 + n));
    }
    let _ = store.ingest(dual(&honest_req, &x, NOW, Outcome::Failed, 200));
    for t in [NOW, NOW + 365 * DAY, NOW + 3650 * DAY] {
        let p = TrustProjection::rebuild(&store, t, ProjParams::default(), |_| 1.0).unwrap();
        let hs = p.score(&h.agent_id(), CAP).map(|s| s.0).unwrap_or(0.5);
        let xs = p.score(&x.agent_id(), CAP).map(|s| s.0).unwrap_or(0.5);
        assert!(xs <= hs, "at t={t}: self-dealer {xs:.3} > honest {hs:.3}");
    }
}

/// A provider-only (unilateral) receipt that names a reputable requester it
/// never worked for must not raise the provider's score, and `rebuild` must
/// agree with the dual-gated incremental path (adv probe: rebuild gives 0.667,
/// incremental gives None).
#[test]
fn forged_unilateral_receipt_earns_no_score_and_rebuild_matches_incremental() {
    let reputable = Keypair::from_seed([0xAA; 32]);
    let attacker = Keypair::from_seed([0xBB; 32]);
    let cb = mint(&reputable, &attacker, NOW, Outcome::Ok, 1).encode().unwrap();
    let forged = Receipt {
        core_bytes: cb.clone(),
        sig_requester: None,
        sig_provider: Some(sign_receipt(&attacker, ROLE_PROVIDER, &cb)),
    };
    let mut store = ReceiptStore::new();
    let _ = store.ingest(forged.clone());

    let r_id = reputable.agent_id();
    let weight = |a: &map_identity::AgentId| if *a == r_id { 1.0 } else { 0.0 };
    let rebuilt = TrustProjection::rebuild(&store, NOW, ProjParams::default(), weight).unwrap();
    assert_eq!(rebuilt.score(&attacker.agent_id(), CAP), None);

    let mut incremental = TrustProjection::new(ProjParams::default());
    if let Ok(v) = forged.verify() {
        if v.dual {
            let _ = incremental.apply(&v, NOW, weight(&v.core.requester));
        }
    }
    assert_eq!(rebuilt, incremental);
}

/// The Sybil ring (`adv/src/bin/sybil.rs`): the attacker controls its provider
/// key AND many throwaway requester keys (300 here; the probe used 1000, and
/// the assertion is an exact baseline independent of ring size), so every receipt is genuinely
/// dual-signed by two distinct identities; R-2..R-4 cannot help. Under v1's
/// neutral-prior issuer weight (0.5) this drove the score to 0.998 at
/// confidence 501. Under T-3 (`IssuerWeights`: unknown issuers weigh 0) the
/// ring contributes nothing, and one honest failure from the observer itself
/// dominates.
#[test]
fn sybil_ring_of_fresh_requesters_contributes_nothing() {
    let attacker = Keypair::from_seed([0xBB; 32]);
    let observer = Keypair::from_seed([0x11; 32]);
    let weights = IssuerWeights::new(observer.agent_id());
    let mut store = ReceiptStore::new();
    store.ingest(dual(&observer, &attacker, NOW, Outcome::Ok, 1)).unwrap();
    for i in 0..300u32 {
        let mut seed = [0u8; 32];
        seed[..4].copy_from_slice(&i.to_le_bytes());
        seed[31] = 0x5A;
        let sybil = Keypair::from_seed(seed);
        let cb = mint(&sybil, &attacker, NOW, Outcome::Ok, (i % 251) as u8).encode().unwrap();
        let r = Receipt {
            sig_requester: Some(sign_receipt(&sybil, ROLE_REQUESTER, &cb)),
            sig_provider: Some(sign_receipt(&attacker, ROLE_PROVIDER, &cb)),
            core_bytes: cb,
        };
        store.ingest(r).unwrap();
    }
    let p = TrustProjection::rebuild(&store, NOW, ProjParams::default(), |a| weights.weight(a)).unwrap();
    let (score, conf) = p.score(&attacker.agent_id(), CAP).unwrap();
    // Only the observer's own receipt counts: exactly the one-receipt baseline.
    assert!((conf - 1.0).abs() < 1e-9, "confidence {conf}");
    assert!((score - 2.0 / 3.0).abs() < 1e-9, "score {score}");

    // One honest failure observed first-hand now outweighs the whole ring.
    store.ingest(dual(&observer, &attacker, NOW, Outcome::Failed, 250)).unwrap();
    let p = TrustProjection::rebuild(&store, NOW, ProjParams::default(), |a| weights.weight(a)).unwrap();
    let (score, _) = p.score(&attacker.agent_id(), CAP).unwrap();
    assert!((score - 0.5).abs() < 1e-9, "score {score}");
}

/// Rebuild equals incremental over a pseudo-random stream of admissible and
/// inadmissible receipts (T-1/T-2): self-dealt, future-dated, unilateral
/// (both directions), cancelled and zero-weight receipts included.
#[test]
fn rebuild_equals_incremental_on_mixed_streams() {
    let keys: Vec<Keypair> = (1..=6u8).map(|s| Keypair::from_seed([s; 32])).collect();
    let observer = keys[0].agent_id();
    let weights = IssuerWeights::new(observer).with(keys[1].agent_id(), 0.5);
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for trial in 0..6u64 {
        let mut store = ReceiptStore::new();
        let mut inc = TrustProjection::new(ProjParams { issuer_cap: Some(3.0), ..ProjParams::default() });
        for n in 0..40u8 {
            let req = &keys[(next() % 6) as usize];
            let prov = &keys[(next() % 6) as usize];
            let outcome = Outcome::from_u64(next() % 5).unwrap();
            let ended = match next() % 4 {
                0 => NOW + 10 * DAY, // future-dated
                _ => NOW - (next() % 30) * DAY,
            };
            let cb = mint(req, prov, ended, outcome, n).encode().unwrap();
            let r = Receipt {
                sig_requester: (next() % 4 != 0).then(|| sign_receipt(req, ROLE_REQUESTER, &cb)),
                sig_provider: (next() % 4 != 0).then(|| sign_receipt(prov, ROLE_PROVIDER, &cb)),
                core_bytes: cb,
            };
            if let Ok(Ingest::Ingested) = store.ingest(r.clone()) {
                let v = r.verify().unwrap();
                let _ = inc.apply(&v, NOW, weights.weight(&v.core.requester));
            }
        }
        let rebuilt = TrustProjection::rebuild(
            &store,
            NOW,
            ProjParams { issuer_cap: Some(3.0), ..ProjParams::default() },
            |a| weights.weight(a),
        )
        .unwrap();
        assert_eq!(rebuilt, inc, "trial {trial}");
    }
}
