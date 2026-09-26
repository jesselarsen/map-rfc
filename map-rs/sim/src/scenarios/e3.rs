//! E3 (eval/HYPOTHESES.md): discovery.
//!
//! - `e3a` shard-flood poisoning (finding A3, rule D-6). N identities; the
//!   attacker owns a fraction a. Honest providers publish only to their own
//!   shard (`map_discovery::shard` of a real AgentID). Every attacker record
//!   is published into every shard (the flood). A querier samples r = 3
//!   distinct random shards and collects candidates, deduplicated by
//!   identity, either as found or after the D-6 filter (a record found under
//!   shard s is kept only if shard(agent) == s). Metric: the attacker
//!   fraction among the candidates, averaged over queries.
//! - `e3b` identity grinding (finding A4, D-8). Measures, with real keygen
//!   and `shard` / `shard_epoch`, the probability that a fresh identity
//!   lands in a target shard. It then derives the attacker's expected share
//!   of that shard for a grinding rate and window: fixed `shard()` allows
//!   grinding over the whole window; an epoch salt caps the useful window at
//!   one epoch.
//! - `e3c` Kademlia lookups under malicious routers (D-9); see `kad`.
//! - `e3d` volatile-plane staleness (H4): status pulled at selection versus
//!   queue length carried in a descriptor refreshed every 0.8·TTL.

use crate::config::Params;
use crate::crypto::CryptoMode;
use crate::rng::Rng;
use map_discovery::{shard, shard_epoch};
use map_identity::Keypair;

pub const METRICS_A: &[&str] = &["attacker_share", "fair_share", "candidates_per_query"];

pub fn run_a(p: &Params, rng: &mut Rng, _mode: CryptoMode) -> Result<Vec<f64>, String> {
    let n = p.u64("n")? as usize;
    let a = p.f64("a")?;
    let filter = p.str("filter")? == "on";
    let r = p.u64_or("r", 3)? as usize;
    let queries = p.u64_or("queries", 200)? as usize;
    let n_att = (a * n as f64).round() as usize;
    // shard_of[i] from real AgentIDs; honest records live in their shard.
    let shard_of: Vec<u8> = (0..n).map(|_| shard(&Keypair::from_seed(rng.bytes32()).agent_id())).collect();
    let mut by_shard: Vec<Vec<usize>> = vec![Vec::new(); 256];
    for (i, s) in shard_of.iter().enumerate() {
        if i >= n_att {
            by_shard[*s as usize].push(i);
        }
    }
    let (mut share_sum, mut cand_sum) = (0.0, 0.0);
    for _ in 0..queries {
        let shards = rng.sample_indices(256, r);
        let mut seen = vec![false; n];
        let (mut att, mut total) = (0usize, 0usize);
        for &s in &shards {
            // Honest records under s, then the flooded attacker records.
            let flooded = 0..n_att;
            for i in by_shard[s].iter().copied().chain(flooded) {
                if filter && shard_of[i] as usize != s {
                    continue; // D-6: a record found under the wrong shard is discarded
                }
                if !seen[i] {
                    seen[i] = true;
                    total += 1;
                    if i < n_att {
                        att += 1;
                    }
                }
            }
        }
        if total > 0 {
            share_sum += att as f64 / total as f64;
        }
        cand_sum += total as f64;
    }
    Ok(vec![share_sum / queries as f64, a, cand_sum / queries as f64])
}

pub const METRICS_B: &[&str] = &["p_target_measured", "attacker_ids_in_shard", "honest_ids_in_shard", "attacker_shard_share"];

pub fn run_b(p: &Params, rng: &mut Rng, _mode: CryptoMode) -> Result<Vec<f64>, String> {
    let n = p.f64("n")?;
    let rate = p.f64("grind_rate_per_s")?; // measured by the keygen+shard bench
    let window = p.f64("window_s")?;
    let epoch = p.f64("epoch_s")?; // 0 = fixed shard(), no salt
    let samples = p.u64_or("samples", 20_000)? as usize;
    // Empirical probability that a fresh identity lands in a target shard,
    // with the real functions (salted when an epoch is configured).
    let target: u8 = (rng.below(256)) as u8;
    let salt = rng.bytes32();
    let mut hits = 0usize;
    for _ in 0..samples {
        let id = Keypair::from_seed(rng.bytes32()).agent_id();
        let s = if epoch > 0.0 { shard_epoch(&id, &salt) } else { shard(&id) };
        if s == target {
            hits += 1;
        }
    }
    let p_hit = hits as f64 / samples as f64;
    // Useful grinding time: all of the window with a fixed shard; at most one
    // epoch with a rotating salt (identities ground for an old salt land in
    // random shards afterwards).
    let useful = if epoch > 0.0 { window.min(epoch) } else { window };
    let att = rate * useful * p_hit;
    let honest = n * p_hit;
    Ok(vec![p_hit, att, honest, att / (att + honest)])
}

pub const METRICS_D: &[&str] = &["pull_abs_error", "desc_abs_error", "pull_regret", "desc_regret", "pull_msgs_per_query", "desc_msgs_per_query"];

/// E3d: K candidate providers, each an M/M/1 queue (service rate 1/s,
/// arrival rate ρ). Queries arrive at `query_rate`. `pull` reads the true
/// queue one RTT before selection; `descriptor` reads the value carried at
/// the provider's last refresh (every 0.8·TTL with ±10% jitter). Both
/// select the minimum reported queue. Regret = true queue of the chosen
/// provider minus the true minimum.
pub fn run_d(p: &Params, rng: &mut Rng, _mode: CryptoMode) -> Result<Vec<f64>, String> {
    let rho = p.f64("rho")?;
    let ttl = p.f64("ttl_s")?;
    let k = p.u64_or("candidates", 10)? as usize;
    let horizon = p.f64_or("horizon_s", 3600.0)?;
    let qrate = p.f64_or("query_rate", 1.0)?;
    let rtt = p.f64_or("rtt_s", 0.1)?;
    // Event-driven birth-death per provider, advanced to each query time.
    let mut q = vec![0u64; k];
    let mut next_change: Vec<f64> = vec![0.0; k];
    let mut next_refresh: Vec<f64> = (0..k).map(|_| rng.uniform(0.0, 0.8 * ttl)).collect();
    let mut carried = vec![0u64; k];
    let mut hist_prev = vec![0u64; k]; // queue value one RTT ago (approximation below)
    let (mut t, mut n_q) = (0.0, 0u64);
    let (mut e_pull, mut e_desc, mut r_pull, mut r_desc) = (0.0, 0.0, 0.0, 0.0);
    let mut refreshes = 0u64;
    let rate_of = |qi: u64| if qi == 0 { rho } else { rho + 1.0 };
    for i in 0..k {
        next_change[i] = rng.exp(rate_of(q[i]));
    }
    // Warm-up of 10·TTL so the queues and descriptor phases are stationary.
    let warm = 10.0 * ttl;
    loop {
        t += rng.exp(qrate);
        if t > warm + horizon {
            break;
        }
        for i in 0..k {
            // Advance queue i to time t - rtt (the pull's read time), then t.
            for (stop, rec) in [(t - rtt, true), (t, false)] {
                loop {
                    let next_evt = next_change[i].min(next_refresh[i]);
                    if next_evt > stop {
                        break;
                    }
                    if next_refresh[i] <= next_change[i] {
                        carried[i] = q[i];
                        refreshes += u64::from(next_refresh[i] > warm);
                        next_refresh[i] += 0.8 * ttl * (1.0 + rng.uniform(-0.1, 0.1));
                    } else {
                        let arrive = q[i] == 0 || rng.bernoulli(rho / (rho + 1.0));
                        if arrive {
                            q[i] += 1;
                        } else {
                            q[i] -= 1;
                        }
                        next_change[i] += rng.exp(rate_of(q[i]));
                    }
                }
                if rec {
                    hist_prev[i] = q[i];
                }
            }
        }
        if t < warm {
            continue;
        }
        n_q += 1;
        let min_true = *q.iter().min().expect("k > 0") as f64;
        let pick = |reported: &[u64]| -> usize {
            (0..k).min_by_key(|&i| (reported[i], i)).expect("k > 0")
        };
        let pp = pick(&hist_prev);
        let pd = pick(&carried);
        r_pull += q[pp] as f64 - min_true;
        r_desc += q[pd] as f64 - min_true;
        e_pull += (0..k).map(|i| (hist_prev[i] as f64 - q[i] as f64).abs()).sum::<f64>() / k as f64;
        e_desc += (0..k).map(|i| (carried[i] as f64 - q[i] as f64).abs()).sum::<f64>() / k as f64;
    }
    let nq = n_q.max(1) as f64;
    Ok(vec![e_pull / nq, e_desc / nq, r_pull / nq, r_desc / nq, 2.0 * k as f64, refreshes as f64 / nq])
}
