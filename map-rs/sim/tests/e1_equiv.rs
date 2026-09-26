//! E1's evidence pool computes exactly what `map_trust::TrustProjection`
//! computes over the same receipts: the same scores for every provider
//! and every weight function, including the per-issuer cap and decay at
//! several evaluation times. Receipts come from a real E1 run under the
//! combined attack (farming + slander + selective countersigning).

use map_identity::AgentId;
use map_sim::config::Params;
use map_sim::crypto::CryptoMode;
use map_sim::rng::Rng;
use map_sim::scenarios::e1::{cfg_from, simulate, Dump, Pool, CAP, DAY_MS};
use map_trust::{ProjParams, TrustProjection};
use std::collections::HashMap;
use toml::Value;

fn small(arm: &str) -> Params {
    Params::from_pairs(&[
        ("providers", Value::Integer(30)),
        ("requesters", Value::Integer(25)),
        ("rounds", Value::Integer(12)),
        ("sybils_per_adv", Value::Integer(3)),
        ("alpha", Value::Float(0.2)),
        ("arm", Value::String(arm.into())),
        ("attack", Value::String("combined".into())),
    ])
}

#[test]
fn pool_scores_equal_a_rebuilt_trust_projection() {
    let cfg = cfg_from(&small("b1")).unwrap();
    let mut dump: Dump = Vec::new();
    simulate(&cfg, &mut Rng::for_run("e1-equiv", 0, 0), CryptoMode { fast: true }, Some(&mut dump));
    assert!(dump.len() > 500, "the run should produce plenty of evidence, got {}", dump.len());

    let cap = 3.0; // small, so the cap actually binds
    let weight = |issuer: u32| -> f64 { [0.0, 0.25, 0.5, 1.0][(issuer as usize * 7) % 4] };
    let mut pool = Pool::new(0.98, cap);
    let mut agent: HashMap<u32, AgentId> = HashMap::new();
    for (issuer, prov, ended, v) in &dump {
        let (a, b) = match v.core.outcome {
            map_trust::Outcome::Ok => (1.0, 0.0),
            map_trust::Outcome::Disputed => (0.0, 2.0),
            _ => (0.0, 1.0),
        };
        pool.add(*issuer, *prov, *ended, a, b);
        agent.insert(*prov, v.core.provider.clone());
    }
    for eval_day in [12u64, 15, 40] {
        let now = eval_day * DAY_MS;
        let mut proj = TrustProjection::new(ProjParams {
            daily_decay: 0.98,
            prior: 1.0,
            skew_ms: 120_000,
            issuer_cap: Some(cap),
        });
        for (issuer, _, _, v) in &dump {
            let _ = proj.apply(v, now, weight(*issuer));
        }
        let mut checked = 0;
        for (ident, id) in &agent {
            let (s_pool, known) = pool.score(*ident, now, weight);
            match proj.score(id, CAP) {
                Some((s_proj, _)) => {
                    assert!((s_pool - s_proj).abs() < 1e-9, "provider {ident} day {eval_day}: pool {s_pool} vs projection {s_proj}");
                    checked += 1;
                }
                None => assert!(!known, "pool used evidence the projection skipped"),
            }
        }
        assert!(checked > 10, "too few providers compared: {checked}");
    }
}
