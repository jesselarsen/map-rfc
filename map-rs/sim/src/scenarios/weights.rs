//! D5 issuer weightings for E1 (eval/HYPOTHESES.md):
//! - W1 agreement: issuer i weighs max(0, 1 − 2·MAD) for observer o. MAD is
//!   the mean |rate_i(p) − rate_o(p)| over providers p that both hold ≥ 3
//!   receipts for, and it needs ≥ 3 such providers.
//! - W2 personalized EigenTrust: 10 power-iteration steps, restart 0.15 to
//!   the observer, over the row-normalized W1 agreement graph among all
//!   issuers; weight = min(1, t[i] / t[o]).
//!
//! **Determinism.** Every floating-point accumulation runs in a fixed order
//! (ascending provider, then ascending issuer). An earlier version summed
//! while iterating a `std::HashMap`, whose order is randomized per
//! instance; no CSV was observed to change, but it was a latent defect.
//!
//! The fast versions (`w1`, `w2`) use an inverted provider index and a
//! batched, transposed power iteration. `w1_reference` / `w2_reference` are
//! the direct definitions; `tests/weights_equiv.rs` asserts the two agree
//! exactly on real E1 pool states.

use super::e1::Pool;
use std::collections::HashMap;

/// Per round: each issuer's qualified cells (n ≥ 3) as (provider, rate),
/// sorted by provider, and the inverted index provider -> (issuer, rate)
/// sorted by issuer.
pub struct Qualified {
    pub per_issuer: Vec<Vec<(u32, f64)>>,
    pub by_provider: HashMap<u32, Vec<(u32, f64)>>,
}

pub fn qualify(pool: &Pool, n: usize) -> Qualified {
    let mut per_issuer = vec![Vec::new(); n];
    let mut by_provider: HashMap<u32, Vec<(u32, f64)>> = HashMap::new();
    for (i, v) in per_issuer.iter_mut().enumerate() {
        if let Some(cells) = pool.cells.get(i) {
            for (p, c) in cells {
                if c.n >= 3 {
                    v.push((*p, c.a / (c.a + c.b)));
                }
            }
        }
        v.sort_by_key(|(p, _)| *p);
    }
    // Issuers ascending, so each provider's list is sorted by issuer.
    for (i, v) in per_issuer.iter().enumerate() {
        for (p, r) in v {
            by_provider.entry(*p).or_default().push((i as u32, *r));
        }
    }
    Qualified { per_issuer, by_provider }
}

/// Direct definition: merge two provider-sorted lists.
pub fn agreement_ref(x: &[(u32, f64)], y: &[(u32, f64)]) -> f64 {
    let (mut a, mut b, mut sum, mut k) = (0, 0, 0.0f64, 0u32);
    while a < x.len() && b < y.len() {
        match x[a].0.cmp(&y[b].0) {
            std::cmp::Ordering::Less => a += 1,
            std::cmp::Ordering::Greater => b += 1,
            std::cmp::Ordering::Equal => {
                sum += (x[a].1 - y[b].1).abs();
                k += 1;
                a += 1;
                b += 1;
            }
        }
    }
    finish(sum, k)
}

fn finish(sum: f64, k: u32) -> f64 {
    if k < 3 {
        return 0.0;
    }
    (1.0 - 2.0 * sum / k as f64).max(0.0)
}

/// Agreement of `x` with every issuer sharing a qualified provider, via the
/// inverted index. `only_above` restricts to issuers with a larger index.
/// Returns (issuer, agreement > 0) in ascending issuer order. Per-issuer
/// sums accumulate in ascending provider order, exactly as `agreement_ref`.
fn agreements_from(q: &Qualified, x: usize, only_above: bool, sum: &mut [f64], k: &mut [u32], touched: &mut Vec<u32>) -> Vec<(u32, f64)> {
    touched.clear();
    for (p, rx) in &q.per_issuer[x] {
        for (i, ri) in &q.by_provider[p] {
            let iu = *i as usize;
            if iu == x || (only_above && iu < x) {
                continue;
            }
            if k[iu] == 0 {
                touched.push(*i);
            }
            sum[iu] += (rx - ri).abs();
            k[iu] += 1;
        }
    }
    touched.sort_unstable();
    let mut out = Vec::new();
    for &i in touched.iter() {
        let iu = i as usize;
        let a = finish(sum[iu], k[iu]);
        if a > 0.0 {
            out.push((i, a));
        }
        sum[iu] = 0.0;
        k[iu] = 0;
    }
    out
}

pub fn w1(pool: &Pool, requesters: usize) -> Vec<HashMap<u32, f64>> {
    let n = pool.cells.len().max(requesters);
    let q = qualify(pool, n);
    let (mut sum, mut k, mut touched) = (vec![0.0; n], vec![0u32; n], Vec::new());
    (0..requesters)
        .map(|o| {
            if q.per_issuer[o].len() < 3 {
                return HashMap::new();
            }
            agreements_from(&q, o, false, &mut sum, &mut k, &mut touched).into_iter().collect()
        })
        .collect()
}

pub fn w1_reference(pool: &Pool, requesters: usize) -> Vec<HashMap<u32, f64>> {
    let n = pool.cells.len().max(requesters);
    let q = qualify(pool, n);
    (0..requesters)
        .map(|o| {
            let mut m = HashMap::new();
            if q.per_issuer[o].len() >= 3 {
                for i in 0..n {
                    if i != o {
                        let a = agreement_ref(&q.per_issuer[o], &q.per_issuer[i]);
                        if a > 0.0 {
                            m.insert(i as u32, a);
                        }
                    }
                }
            }
            m
        })
        .collect()
}

/// The agreement graph as adjacency lists of (neighbor, w) with
/// w = 0.85·a / rowsum, neighbors ascending; also whether each node is
/// dangling (no edges).
fn graph(q: &Qualified, n: usize, fast: bool) -> (Vec<Vec<(u32, f64)>>, Vec<bool>) {
    let mut adj: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
    if fast {
        let (mut sum, mut k, mut touched) = (vec![0.0; n], vec![0u32; n], Vec::new());
        for x in 0..n {
            for (y, a) in agreements_from(q, x, true, &mut sum, &mut k, &mut touched) {
                adj[x].push((y, a));
                adj[y as usize].push((x as u32, a));
            }
        }
    } else {
        for x in 0..n {
            for y in x + 1..n {
                let a = agreement_ref(&q.per_issuer[x], &q.per_issuer[y]);
                if a > 0.0 {
                    adj[x].push((y as u32, a));
                    adj[y].push((x as u32, a));
                }
            }
        }
    }
    let dangling: Vec<bool> = adj.iter().map(|e| e.is_empty()).collect();
    for e in adj.iter_mut() {
        let rs: f64 = e.iter().map(|(_, a)| a).sum();
        for (_, a) in e.iter_mut() {
            *a = 0.85 * *a / rs;
        }
    }
    (adj, dangling)
}

fn extract(t_of: impl Fn(usize) -> f64, o: usize, n: usize) -> HashMap<u32, f64> {
    let self_t = t_of(o);
    let mut m = HashMap::new();
    for i in 0..n {
        let ti = t_of(i);
        if i != o && ti > 0.0 && self_t > 0.0 {
            m.insert(i as u32, (ti / self_t).min(1.0));
        }
    }
    m
}

/// Direct definition: one power iteration per observer.
pub fn w2_reference(pool: &Pool, requesters: usize) -> Vec<HashMap<u32, f64>> {
    let n = pool.cells.len().max(requesters);
    let q = qualify(pool, n);
    let (adj, dangling) = graph(&q, n, false);
    (0..requesters)
        .map(|o| {
            let mut t = vec![0.0; n];
            t[o] = 1.0;
            for _ in 0..10 {
                let mut next = vec![0.0; n];
                next[o] += 0.15;
                for i in 0..n {
                    if dangling[i] {
                        next[o] += 0.85 * t[i]; // dangling mass returns to the observer
                    } else {
                        for (j, w) in &adj[i] {
                            next[*j as usize] += t[i] * w;
                        }
                    }
                }
                t = next;
            }
            extract(|i| t[i], o, n)
        })
        .collect()
}

/// Observers per cache block in `w2`. One block's slice of `t` and of
/// `next` is n × 32 × 8 bytes each (≈ 0.6 MB at n = 2500), small enough to
/// stay in cache while every edge is visited.
const W2_BLOCK: usize = 32;

/// All observers at once, stored block-major: within a block of
/// `W2_BLOCK` observers, node i's mass for those observers is one
/// contiguous row, so each edge updates a short, cache-resident row.
/// Blocking only reorders work *across* accumulators. Each accumulator
/// (node j, observer o) still receives the restart term first and then its
/// contributions in ascending source order, exactly as in `w2_reference`,
/// so the result is bit-identical (`tests/weights_equiv.rs`).
pub fn w2(pool: &Pool, requesters: usize) -> Vec<HashMap<u32, f64>> {
    let n = pool.cells.len().max(requesters);
    let r = requesters;
    let q = qualify(pool, n);
    let (adj, dangling) = graph(&q, n, true);
    // Block [b0, b1) occupies [n·b0, n·b1); within it, node i and observer
    // o = b0 + k sit at i·(b1 − b0) + k. Observer o's own node is node o.
    let blocks: Vec<(usize, usize)> = (0..r).step_by(W2_BLOCK).map(|b| (b, (b + W2_BLOCK).min(r))).collect();
    let mut t = vec![0.0f64; n * r];
    for &(b0, b1) in &blocks {
        let w = b1 - b0;
        for k in 0..w {
            t[n * b0 + (b0 + k) * w + k] = 1.0;
        }
    }
    let mut next = vec![0.0f64; n * r];
    for _ in 0..10 {
        next.iter_mut().for_each(|x| *x = 0.0);
        for &(b0, b1) in &blocks {
            let w = b1 - b0;
            let (tb, nb) = (&t[n * b0..n * b1], &mut next[n * b0..n * b1]);
            for k in 0..w {
                nb[(b0 + k) * w + k] += 0.15;
            }
            for i in 0..n {
                let s = &tb[i * w..i * w + w];
                if dangling[i] {
                    // dangling mass returns to each observer's own node
                    for k in 0..w {
                        nb[(b0 + k) * w + k] += 0.85 * s[k];
                    }
                } else {
                    for (j, wt) in &adj[i] {
                        let d = &mut nb[*j as usize * w..*j as usize * w + w];
                        for k in 0..w {
                            d[k] += s[k] * wt;
                        }
                    }
                }
            }
        }
        std::mem::swap(&mut t, &mut next);
    }
    let mut out = Vec::with_capacity(r);
    for &(b0, b1) in &blocks {
        let w = b1 - b0;
        for k in 0..w {
            out.push(extract(|i| t[n * b0 + i * w + k], b0 + k, n));
        }
    }
    out
}
