//! Regression tests for Role 7 finding A1 (receipt farming), ported from the
//! reviewer probes in `attic/probes/{adv,rt7probe}`.
//!
//! Each test asserts the SECURE behaviour and is `#[ignore]`d until the fix
//! lands (accountability-layer plan, P3). Run with
//! `cargo test -p map-trust --test attack_farming -- --ignored`.
//!
//! The Sybil-ring case (`adv/src/bin/sybil.rs`: 1000 throwaway dual-signers →
//! score 0.998) is not ported yet. It is defeated by the library's default
//! issuer weighting (decision D5), which has no API yet; it is added with that
//! API in P3.

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
#[ignore = "Role 7 A1 open; fixed in accountability-layer P3"]
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
#[ignore = "Role 7 A1 open; fixed in accountability-layer P3"]
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
#[ignore = "Role 7 A1 open; fixed in accountability-layer P3"]
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
#[ignore = "Role 7 A1 open; fixed in accountability-layer P3"]
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
            incremental.apply(&v, NOW, weight(&v.core.requester));
        }
    }
    assert_eq!(rebuilt, incremental);
}
