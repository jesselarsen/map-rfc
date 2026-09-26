//! F1 (eval/HYPOTHESES.md): re-implementation of the arXiv:2603.03753
//! simulator (`plan-lab-szu/agentic_p2p_sim` v0.3.10: sim.py, node.py,
//! discovery.py, registry.py), extended with MAP evidence arms.
//!
//! Ported faithfully:
//! - Peers 1..=N; the first round(α·N) are Sybils. Capability is image
//!   generator with probability 0.7, else publisher.
//! - Every peer publishes a capability descriptor (CD) at t = 0 and
//!   refreshes it every TTL·0.8·(1 ± 0.1). Each (re)publication draws fresh
//!   self-reported hints: Sybils overclaim.
//! - Workflows arrive as Poisson(rate) from a uniformly random honest
//!   requester. The steps run sequentially; a failed step fails the
//!   workflow. A workflow counts as a success only once its last step
//!   completes within the horizon.
//! - Per step: query the registry (unexpired CDs of that capability), rank
//!   by 0.45·semantic + 0.20·trust + 0.20·(1−latency) + 0.15·(1−cost), and
//!   shortlist the top K. With L2 enabled for the capability, append up to
//!   `l2_random_extra` random others. Probe in order (canary if L2, which
//!   costs no execution attempt) and execute; stop at the first success or
//!   after `fallback_max_attempts` executions.
//! - Execution: a Sybil succeeds with probability 0.05. An image step fails
//!   on quality with probability `gen_quality_threshold` (0.5).
//! - Latency (it only matters for end-of-horizon truncation): registry RTT
//!   (70–130 ms, 5% tail 150–250) + processing (3 + 3·log2(N/20) +
//!   N(0, 1.25) ms + load delay), peer RTT mixture, and execution times.
//!
//! MAP arms (pre-registered F1-b):
//! - Each requester keeps a local `map-trust` projection over its own
//!   receipts (weight 1: its own evidence, W0).
//! - A success yields a dual-signed outcome-0 receipt. A Sybil execution
//!   failure yields a requester-only outcome-1 receipt (the Sybil refuses
//!   to countersign). An honest quality failure yields a dual-signed
//!   outcome-1 receipt.
//! - Ranking, with probability 1 − ε: known providers scoring ≥ 0.5 by
//!   score, then unknown providers in self-reported order, then known
//!   providers scoring < 0.5. With probability ε the self-reported order
//!   is used (exploration).
//! - `map_ra` adds the canary and extras on top.
//! - `whitewash_s` > 0 makes each Sybil re-enter under a fresh identity at
//!   that period.

use crate::config::Params;
use crate::crypto::{receipt, CryptoMode, ReceiptSpec};
use crate::engine::Engine;
use crate::rng::Rng;
use map_core::Usage;
use map_identity::Keypair;
use map_trust::{Outcome, ProjParams, TrustProjection};
use std::collections::{BTreeMap, HashMap};

pub const METRICS: &[&str] = &[
    "workflow_success",
    "workflows",
    "sybil_exec_share",
    "probes_per_workflow",
    "receipts",
];

const CAPS: [&str; 2] = ["map:cap:image-generator", "map:cap:publisher"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Policy {
    NoVerify,
    RiskAware,
    Map,
    MapRa,
}

struct Cfg {
    n: usize,
    alpha: f64,
    policy: Policy,
    horizon: f64,
    rate: f64,
    ttl: f64,
    refresh_ratio: f64,
    refresh_jitter: f64,
    k: usize,
    adv_exec: f64,
    quality_fail: f64,
    pass_honest: f64,
    pass_adv: f64,
    random_extra: usize,
    fallback: usize,
    epsilon: f64,
    whitewash: f64,
}

struct Peer {
    kp: Keypair,
    cap: usize,
    sybil: bool,
    /// Current identity index (changes when a Sybil whitewashes).
    id: u64,
}

#[derive(Clone)]
struct Cd {
    peer: usize,
    id: u64,
    issued: f64,
    semantic: f64,
    trust: f64,
    cost: f64,
    latency: f64,
}

impl Cd {
    fn score(&self) -> f64 {
        0.45 * self.semantic
            + 0.20 * self.trust
            + 0.20 * (1.0 - self.latency)
            + 0.15 * (1.0 - self.cost)
    }
}

enum Ev {
    Refresh(usize),
    Arrival,
    Step { requester: usize, step: usize },
    Complete,
    Whitewash(usize),
}

struct World<'a> {
    cfg: Cfg,
    rng: &'a mut Rng,
    mode: CryptoMode,
    peers: Vec<Peer>,
    next_id: u64,
    /// (capability, peer) -> CD, iterated in key order.
    registry: BTreeMap<(usize, usize), Cd>,
    proj: HashMap<usize, TrustProjection>,
    workflows: u64,
    successes: u64,
    exec_attempts: u64,
    sybil_execs: u64,
    probes: u64,
    receipts: u64,
}

fn policy(s: &str) -> Result<Policy, String> {
    Ok(match s {
        "noverify" => Policy::NoVerify,
        "riskaware" => Policy::RiskAware,
        "map" => Policy::Map,
        "map_ra" => Policy::MapRa,
        other => return Err(format!("unknown policy `{other}`")),
    })
}

pub fn run(p: &Params, rng: &mut Rng, mode: CryptoMode) -> Result<Vec<f64>, String> {
    let pol = policy(p.str("policy")?)?;
    let cfg = Cfg {
        n: p.u64_or("n", 100)? as usize,
        alpha: p.f64("alpha")?,
        policy: pol,
        horizon: p.f64_or("horizon_s", 60.0)?,
        rate: p.f64_or("workflow_rate_per_s", 0.5)?,
        ttl: p.f64_or("ttl_s", 15.0)?,
        refresh_ratio: p.f64_or("refresh_ratio", 0.8)?,
        refresh_jitter: p.f64_or("refresh_jitter", 0.1)?,
        k: p.u64_or("shortlist_k", 25)? as usize,
        adv_exec: p.f64_or("adv_exec_success_prob", 0.05)?,
        quality_fail: p.f64_or("gen_quality_threshold", 0.5)?,
        pass_honest: p.f64_or("l2_pass_prob_honest", 0.97)?,
        pass_adv: p.f64_or("l2_pass_prob_adversary", 0.08)?,
        random_extra: p.u64_or("l2_random_extra", 30)? as usize,
        fallback: p.u64_or("fallback_max_attempts", 3)? as usize,
        epsilon: p.f64_or("epsilon", 0.1)?,
        whitewash: p.f64_or("whitewash_s", 0.0)?,
    };
    let mut w = World::new(cfg, rng, mode);
    Ok(w.simulate())
}

impl<'a> World<'a> {
    fn new(cfg: Cfg, rng: &'a mut Rng, mode: CryptoMode) -> World<'a> {
        let sybils = (cfg.alpha * cfg.n as f64).round() as usize;
        let mut peers = Vec::with_capacity(cfg.n);
        for i in 0..cfg.n {
            let cap = rng.weighted(&[0.7, 0.3]);
            peers.push(Peer {
                kp: Keypair::from_seed(rng.bytes32()),
                cap,
                sybil: i < sybils,
                id: i as u64,
            });
        }
        let next_id = cfg.n as u64;
        World {
            cfg,
            rng,
            mode,
            peers,
            next_id,
            registry: BTreeMap::new(),
            proj: HashMap::new(),
            workflows: 0,
            successes: 0,
            exec_attempts: 0,
            sybil_execs: 0,
            probes: 0,
            receipts: 0,
        }
    }

    fn uses_l2(&self) -> bool {
        matches!(self.cfg.policy, Policy::RiskAware | Policy::MapRa)
    }

    fn uses_map(&self) -> bool {
        matches!(self.cfg.policy, Policy::Map | Policy::MapRa)
    }

    fn publish(&mut self, peer: usize, now: f64) {
        let (semantic, trust, cost, latency) = if self.peers[peer].sybil {
            (
                self.rng.uniform(0.92, 1.0),
                self.rng.uniform(0.88, 1.0),
                self.rng.uniform(0.0, 0.2),
                self.rng.uniform(0.0, 0.3),
            )
        } else {
            (
                self.rng.uniform(0.55, 0.95),
                self.rng.uniform(0.40, 0.90),
                self.rng.uniform(0.20, 0.90),
                self.rng.uniform(0.10, 0.90),
            )
        };
        let cap = self.peers[peer].cap;
        let id = self.peers[peer].id;
        self.registry.insert(
            (cap, peer),
            Cd {
                peer,
                id,
                issued: now,
                semantic,
                trust,
                cost,
                latency,
            },
        );
    }

    fn refresh_delay(&mut self) -> f64 {
        let base = self.cfg.ttl * self.cfg.refresh_ratio;
        let j = self.cfg.refresh_jitter;
        (base * (1.0 + self.rng.uniform(-j, j))).max(0.01)
    }

    fn peer_rtt_ms(&mut self) -> f64 {
        if self.rng.bernoulli(0.02) {
            return self.rng.uniform(200.0, 400.0);
        }
        if self.rng.bernoulli(0.5) {
            self.rng.uniform(20.0, 80.0)
        } else {
            self.rng.uniform(80.0, 200.0)
        }
    }

    fn registry_rtt_ms(&mut self) -> f64 {
        let net = if self.rng.bernoulli(0.05) {
            self.rng.uniform(150.0, 250.0)
        } else {
            self.rng.uniform(70.0, 130.0)
        };
        let n = self.cfg.n.max(1) as f64;
        let log_term = (n / 20.0).max(1.0).log2();
        let proc = (3.0 + 3.0 * log_term + self.rng.normal(0.0, 1.25)).max(0.0);
        let msg_rate = n / self.cfg.ttl.max(1e-6) + 2.0 * self.cfg.rate * 2.0;
        let load = (msg_rate / 50.0).clamp(0.0, 0.95);
        let load_delay = ((load / 0.5) * 10.0 + self.rng.normal(0.0, 1.5)).clamp(0.0, 30.0);
        net + proc + load_delay
    }

    fn simulate(&mut self) -> Vec<f64> {
        let mut eng: Engine<Ev> = Engine::default();
        for i in 0..self.peers.len() {
            self.publish(i, 0.0);
            let d = self.refresh_delay();
            eng.after(d, Ev::Refresh(i));
            if self.cfg.whitewash > 0.0 && self.peers[i].sybil {
                eng.after(self.cfg.whitewash, Ev::Whitewash(i));
            }
        }
        if self.cfg.rate > 0.0 {
            let t0 = self.rng.exp(self.cfg.rate);
            eng.at(t0, Ev::Arrival);
        }
        while let Some(ev) = eng.next(self.cfg.horizon) {
            let now = eng.now();
            match ev {
                Ev::Refresh(i) => {
                    self.publish(i, now);
                    let d = self.refresh_delay();
                    eng.after(d, Ev::Refresh(i));
                }
                Ev::Whitewash(i) => {
                    self.next_id += 1;
                    self.peers[i].id = self.next_id;
                    self.peers[i].kp = Keypair::from_seed(self.rng.bytes32());
                    self.publish(i, now);
                    eng.after(self.cfg.whitewash, Ev::Whitewash(i));
                }
                Ev::Arrival => {
                    self.workflows += 1;
                    let honest: Vec<usize> = (0..self.peers.len())
                        .filter(|&i| !self.peers[i].sybil)
                        .collect();
                    let requester = if honest.is_empty() {
                        0
                    } else {
                        honest[self.rng.below(honest.len())]
                    };
                    eng.at(now, Ev::Step { requester, step: 0 });
                    let dt = self.rng.exp(self.cfg.rate);
                    eng.after(dt, Ev::Arrival);
                }
                Ev::Step { requester, step } => {
                    let (ok, latency_s) = self.do_step(requester, step, now);
                    if ok {
                        if step + 1 == CAPS.len() {
                            eng.after(latency_s, Ev::Complete);
                        } else {
                            eng.after(
                                latency_s,
                                Ev::Step {
                                    requester,
                                    step: step + 1,
                                },
                            );
                        }
                    }
                }
                Ev::Complete => self.successes += 1,
            }
        }
        let wf = self.workflows as f64;
        vec![
            if wf > 0.0 {
                self.successes as f64 / wf
            } else {
                0.0
            },
            wf,
            if self.exec_attempts > 0 {
                self.sybil_execs as f64 / self.exec_attempts as f64
            } else {
                0.0
            },
            if wf > 0.0 {
                self.probes as f64 / wf
            } else {
                0.0
            },
            self.receipts as f64,
        ]
    }

    /// MAP ranking of `cands` for `requester` (see module docs).
    fn map_order(&mut self, requester: usize, cap: usize, cands: &mut Vec<Cd>) {
        if self.rng.bernoulli(self.cfg.epsilon) {
            return; // explore: keep the self-reported order
        }
        let proj = self.proj.get(&requester);
        let key = |cd: &Cd| -> (u8, f64) {
            let s = proj
                .and_then(|p| p.score(&self.peers[cd.peer].kp.agent_id(), CAPS[cap]))
                .map(|(s, _)| s);
            match s {
                Some(s) if s >= 0.5 => (0, -s),
                None => (1, 0.0),
                Some(s) => (2, -s),
            }
        };
        let mut keyed: Vec<((u8, f64), usize, Cd)> = cands
            .drain(..)
            .enumerate()
            .map(|(i, cd)| (key(&cd), i, cd))
            .collect();
        keyed.sort_by(|a, b| {
            a.0 .0
                .cmp(&b.0 .0)
                .then(a.0 .1.total_cmp(&b.0 .1))
                .then(a.1.cmp(&b.1))
        });
        cands.extend(keyed.into_iter().map(|(_, _, cd)| cd));
    }

    fn record(
        &mut self,
        requester: usize,
        cd: &Cd,
        cap: usize,
        now: f64,
        outcome: Outcome,
        provider_signs: bool,
    ) {
        if !self.uses_map() {
            return;
        }
        let now_ms = (now * 1000.0) as u64;
        self.receipts += 1;
        let v = receipt(
            self.mode,
            &ReceiptSpec {
                n: self.receipts,
                requester: &self.peers[requester].kp,
                provider: &self.peers[cd.peer].kp,
                capability: CAPS[cap],
                started_ms: now_ms,
                ended_ms: now_ms,
                outcome,
                usage: Usage {
                    tokens_in: 1,
                    tokens_out: 1,
                    ttft_ms: 0,
                    wall_ms: 0,
                },
                requester_signs: true,
                provider_signs,
            },
        );
        let proj = self
            .proj
            .entry(requester)
            .or_insert_with(|| TrustProjection::new(ProjParams::default()));
        let _ = proj.apply(&v, now_ms, 1.0);
    }

    /// One step: returns (ok, latency seconds).
    fn do_step(&mut self, requester: usize, cap: usize, now: f64) -> (bool, f64) {
        let rtt = self.registry_rtt_ms();
        let mut latency = rtt / 1000.0;
        let mut filtered: Vec<Cd> = self
            .registry
            .iter()
            .filter(|((c, _), cd)| {
                *c == cap && now <= cd.issued + self.cfg.ttl && cd.id == self.peers[cd.peer].id
            })
            .map(|(_, cd)| cd.clone())
            .collect();
        if filtered.is_empty() {
            return (false, latency);
        }
        // Rank (stable sort, descending) and shortlist.
        let mut ranked: Vec<(f64, usize, Cd)> = filtered
            .drain(..)
            .enumerate()
            .map(|(i, cd)| (cd.score(), i, cd))
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut all: Vec<Cd> = ranked.into_iter().map(|(_, _, cd)| cd).collect();
        if self.uses_map() {
            self.map_order(requester, cap, &mut all);
        }
        let k = self.cfg.k.min(all.len());
        let mut probe: Vec<Cd> = all[..k].to_vec();
        if self.uses_l2() && self.cfg.random_extra > 0 && all.len() > k {
            let rest = &all[k..];
            for i in self.rng.sample_indices(rest.len(), self.cfg.random_extra) {
                probe.push(rest[i].clone());
            }
        }
        let mut attempts = 0;
        for cd in probe {
            self.probes += 1;
            if self.uses_l2() {
                latency += self.peer_rtt_ms() / 1000.0;
                let p = if self.peers[cd.peer].sybil {
                    self.cfg.pass_adv
                } else {
                    self.cfg.pass_honest
                };
                if self.rng.f64() > p {
                    continue;
                }
            }
            attempts += 1;
            self.exec_attempts += 1;
            let exec_ms = if cap == 0 {
                self.rng.uniform(150.0, 350.0)
            } else {
                self.rng.uniform(80.0, 180.0)
            };
            latency += exec_ms / 1000.0;
            let sybil = self.peers[cd.peer].sybil;
            if sybil {
                self.sybil_execs += 1;
            }
            let ok = if sybil && self.rng.f64() > self.cfg.adv_exec {
                self.record(requester, &cd, cap, now, Outcome::Failed, false);
                false
            } else if cap == 0 && self.rng.f64() < self.cfg.quality_fail {
                self.record(requester, &cd, cap, now, Outcome::Failed, true);
                false
            } else {
                self.record(requester, &cd, cap, now, Outcome::Ok, true);
                true
            };
            if ok {
                return (true, latency);
            }
            if attempts >= self.cfg.fallback {
                break;
            }
        }
        (false, latency)
    }
}
