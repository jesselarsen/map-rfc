//! The fast D5 weightings equal their direct definitions exactly, on the
//! pool state of a real E1 run (camouflage attack, so Sybils do build
//! agreement with honest requesters).

use map_sim::config::Params;
use map_sim::crypto::CryptoMode;
use map_sim::rng::Rng;
use map_sim::scenarios::e1::{cfg_from, simulate, Dump, Pool};
use map_sim::scenarios::weights::{w1, w1_reference, w2, w2_reference};
use map_trust::Outcome;
use toml::Value;

fn pool_after(rounds: i64) -> (Pool, usize) {
    let p = Params::from_pairs(&[
        ("providers", Value::Integer(40)),
        ("requesters", Value::Integer(40)),
        ("rounds", Value::Integer(rounds)),
        ("tasks_per_round", Value::Float(4.0)),
        ("sybils_per_adv", Value::Integer(3)),
        ("alpha", Value::Float(0.2)),
        ("arm", Value::String("b1".into())),
        ("attack", Value::String("camouflage".into())),
    ]);
    let cfg = cfg_from(&p).unwrap();
    let mut dump: Dump = Vec::new();
    simulate(&cfg, &mut Rng::for_run("weights-equiv", 0, 0), CryptoMode { fast: true }, Some(&mut dump));
    let mut pool = Pool::new(0.98, 50.0);
    for (issuer, prov, ended, v) in &dump {
        let (a, b) = if v.core.outcome == Outcome::Ok { (1.0, 0.0) } else { (0.0, 1.0) };
        pool.add(*issuer, *prov, *ended, a, b);
    }
    (pool, cfg.requesters)
}

#[test]
fn fast_weightings_equal_their_definitions_exactly() {
    for rounds in [8, 20] {
        let (pool, r) = pool_after(rounds);
        let (fast1, ref1) = (w1(&pool, r), w1_reference(&pool, r));
        assert_eq!(fast1, ref1, "W1 differs after {rounds} rounds");
        let (fast2, ref2) = (w2(&pool, r), w2_reference(&pool, r));
        assert_eq!(fast2, ref2, "W2 differs after {rounds} rounds");
        // Non-trivial: weights exist, and not all of them are 1.
        let all: Vec<f64> = fast2.iter().flat_map(|m| m.values().copied()).collect();
        assert!(fast1.iter().map(|m| m.len()).sum::<usize>() > 50, "too few W1 weights");
        assert!(all.len() > 50 && all.iter().any(|w| *w < 1.0), "W2 weights trivial: {}", all.len());
    }
}
