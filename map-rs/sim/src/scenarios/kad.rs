//! E3c: Kademlia lookups under colluding malicious routers, with one path
//! or d disjoint paths (S/Kademlia, rule D-9).
//!
//! **Model.**
//! - IDs are random 64-bit values with the XOR metric.
//! - Node x's bucket b holds up to k contacts drawn uniformly from the nodes
//!   whose XOR distance to x has its top bit at b. These form a contiguous
//!   range of the sorted ID array. Contacts are sampled deterministically
//!   from hash(run, x, b), so tables are never materialized. This is an
//!   idealized fully-populated table.
//! - Churn: a fraction of nodes is offline. Routing entries still point at
//!   them, and a query to one costs a 1 s timeout.
//! - A fraction f of nodes is malicious and colludes. Queried about a
//!   target, a malicious node answers with the k malicious nodes closest to
//!   it. By default that list includes offline colluders, which then time
//!   out, so churn weakens the adversary: an artifact (review finding
//!   R2-12). With `colluders_live_only = 1` (Amendment A14), colluders
//!   advertise only online colluders, as a coordinating adversary would.
//!
//! **Lookup.** Iterative with α = 3 parallel queries per round, and a
//! shortlist of the k closest nodes seen. It ends when the k closest known
//! nodes have all been queried.
//! - Disjoint paths: the querier's initial k contacts are dealt round-robin
//!   to d independent lookups, and no node is queried by two paths.
//! - Success: the responsible node (the closest online honest node to the
//!   target) was queried by some path, i.e. its record was reached.
//!
//! Metrics: success rate, hops (rounds of the slowest path), messages, and
//! latency (per round, the maximum RTT of its queries, drawn from F1's RTT
//! mixture).

use crate::config::Params;
use crate::crypto::CryptoMode;
use crate::rng::Rng;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const METRICS: &[&str] = &["success_rate", "hops", "messages", "latency_ms"];

struct Net {
    ids: Vec<u64>, // sorted
    online: Vec<bool>,
    malicious: Vec<bool>,
    mal_sorted: Vec<u64>, // sorted IDs of malicious nodes
    k: usize,
    salt: [u8; 32],
}

impl Net {
    fn idx(&self, id: u64) -> usize {
        self.ids.binary_search(&id).expect("known id")
    }

    /// Nodes whose distance to `x` has top bit `b`: a contiguous id range.
    fn bucket_range(&self, x: u64, b: u32) -> (usize, usize) {
        let span = 1u64 << b;
        let prefix = if b == 63 { 0 } else { (x >> (b + 1)) << (b + 1) };
        let lo = prefix | ((!x) & span);
        let hi = lo | (span - 1);
        let a = self.ids.partition_point(|v| *v < lo);
        let z = self.ids.partition_point(|v| *v <= hi);
        (a, z)
    }

    /// Up to k contacts of `x` in bucket `b`, deterministic per (x, b).
    fn contacts(&self, x: u64, b: u32, out: &mut Vec<u64>) {
        let (a, z) = self.bucket_range(x, b);
        let len = z - a;
        if len <= self.k {
            out.extend_from_slice(&self.ids[a..z]);
            return;
        }
        let mut h = Sha256::new();
        h.update(self.salt);
        h.update(x.to_le_bytes());
        h.update(b.to_le_bytes());
        let mut state = u64::from_le_bytes(h.finalize()[..8].try_into().expect("8 bytes"));
        let mut chosen = HashSet::with_capacity(self.k);
        while chosen.len() < self.k {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            chosen.insert(a + (state % len as u64) as usize);
        }
        let mut v: Vec<usize> = chosen.into_iter().collect();
        v.sort_unstable();
        out.extend(v.into_iter().map(|i| self.ids[i]));
    }

    /// The k contacts of `x` closest to `target` (an honest node's answer).
    fn closest_known(&self, x: u64, target: u64) -> Vec<u64> {
        let top = 63 - (x ^ target).max(1).leading_zeros();
        let mut c = Vec::new();
        // The target's bucket and every closer-to-x bucket below it.
        for b in (0..=top).rev() {
            self.contacts(x, b, &mut c);
        }
        c.sort_by_key(|y| y ^ target);
        c.truncate(self.k);
        c
    }

    fn answer(&self, y: u64, target: u64) -> Vec<u64> {
        if self.malicious[self.idx(y)] {
            // Colluders answer with the malicious nodes nearest the target.
            let at = self.mal_sorted.partition_point(|v| *v < target);
            // XOR-nearest lies near the numeric position; a 4k window each
            // side covers it except at rare prefix boundaries.
            let lo = at.saturating_sub(4 * self.k);
            let hi = (at + 4 * self.k).min(self.mal_sorted.len());
            let mut c: Vec<u64> = self.mal_sorted[lo..hi].to_vec();
            c.sort_by_key(|v| v ^ target);
            c.truncate(self.k);
            c
        } else {
            self.closest_known(y, target)
        }
    }
}

fn rtt_ms(rng: &mut Rng) -> f64 {
    if rng.bernoulli(0.02) {
        rng.uniform(200.0, 400.0)
    } else if rng.bernoulli(0.5) {
        rng.uniform(20.0, 80.0)
    } else {
        rng.uniform(80.0, 200.0)
    }
}

struct PathState {
    known: Vec<u64>, // sorted by distance, deduplicated
    queried: HashSet<u64>,
    hops: u32,
    latency: f64,
    done: bool,
}

pub fn run(p: &Params, rng: &mut Rng, _mode: CryptoMode) -> Result<Vec<f64>, String> {
    let n = p.u64("n")? as usize;
    let churn = p.f64("churn")?;
    let f = p.f64("f_malicious")?;
    let paths = p.u64("paths")? as usize;
    let k = p.u64_or("k", 20)? as usize;
    let alpha = p.u64_or("alpha", 3)? as usize;
    let lookups = p.u64_or("lookups", 200)? as usize;
    let live_only = p.u64_or("colluders_live_only", 0)? != 0;
    let mut ids: Vec<u64> = (0..n).map(|_| u64::from_le_bytes(rng.bytes32()[..8].try_into().expect("8"))).collect();
    ids.sort_unstable();
    ids.dedup();
    let n = ids.len();
    let online: Vec<bool> = (0..n).map(|_| !rng.bernoulli(churn)).collect();
    let malicious: Vec<bool> = (0..n).map(|_| rng.bernoulli(f)).collect();
    let mal_sorted: Vec<u64> = (0..n).filter(|i| malicious[*i] && (online[*i] || !live_only)).map(|i| ids[i]).collect();
    let net = Net { ids, online, malicious, mal_sorted, k, salt: rng.bytes32() };
    let honest_online: Vec<usize> = (0..n).filter(|i| net.online[*i] && !net.malicious[*i]).collect();
    if honest_online.len() < 2 {
        return Err("no honest online nodes".into());
    }
    let (mut succ, mut hops, mut msgs, mut lat) = (0usize, 0f64, 0f64, 0f64);
    for _ in 0..lookups {
        let target = u64::from_le_bytes(rng.bytes32()[..8].try_into().expect("8"));
        let querier = net.ids[honest_online[rng.below(honest_online.len())]];
        // The responsible node: the closest online honest node.
        let responsible = honest_online.iter().map(|i| net.ids[*i]).min_by_key(|v| v ^ target).expect("nonempty");
        let init = net.closest_known(querier, target);
        let mut ps: Vec<PathState> = (0..paths)
            .map(|_| PathState { known: Vec::new(), queried: HashSet::new(), hops: 0, latency: 0.0, done: false })
            .collect();
        for (i, c) in init.into_iter().enumerate() {
            ps[i % paths].known.push(c);
        }
        let mut used: HashSet<u64> = HashSet::new(); // disjointness across paths
        used.insert(querier);
        let mut found = false;
        let mut messages = 0usize;
        while ps.iter().any(|s| !s.done) {
            for s in ps.iter_mut().filter(|s| !s.done) {
                s.known.sort_by_key(|v| v ^ target);
                s.known.dedup();
                let batch: Vec<u64> = s
                    .known
                    .iter()
                    .take(k)
                    .filter(|v| !s.queried.contains(*v) && (paths == 1 || !used.contains(*v)))
                    .take(alpha)
                    .copied()
                    .collect();
                if batch.is_empty() {
                    s.done = true;
                    continue;
                }
                s.hops += 1;
                let mut round = 0f64;
                for y in batch {
                    s.queried.insert(y);
                    used.insert(y);
                    messages += 1;
                    let yi = net.idx(y);
                    if !net.online[yi] {
                        round = round.max(1000.0);
                        s.known.retain(|v| *v != y);
                        continue;
                    }
                    round = round.max(rtt_ms(rng));
                    if y == responsible {
                        found = true;
                    }
                    for c in net.answer(y, target) {
                        if !s.queried.contains(&c) {
                            s.known.push(c);
                        }
                    }
                }
                s.latency += round;
            }
        }
        if found {
            succ += 1;
        }
        hops += ps.iter().map(|s| s.hops).max().unwrap_or(0) as f64;
        lat += ps.iter().map(|s| s.latency).fold(0.0, f64::max);
        msgs += messages as f64;
    }
    let l = lookups as f64;
    Ok(vec![succ as f64 / l, hops / l, msgs / l, lat / l])
}
