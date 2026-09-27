//! E1 (eval/HYPOTHESES.md): trust under attack, and decision D5.
//!
//! **World.** One capability; P providers, R honest requesters, T rounds
//! (1 round = 1 day of decay). Each honest requester issues Poisson(λ) tasks
//! per round, each with a shortlist of m providers drawn uniformly. A
//! fraction α of providers is adversarial: success probability 0.20
//! (on-off: 0.95/0.20 alternating every 10 rounds). Honest providers have
//! q ~ U[0.80, 0.98]. Every provider also publishes a self-reported quality
//! hint (the 2603.03753 model): honest U[0.40, 0.90], adversarial
//! U[0.88, 1.0].
//!
//! **Evidence.**
//! - A success yields a dual-signed outcome-0 receipt.
//! - An honest provider's failure yields a dual-signed outcome-1 receipt.
//! - An adversarial provider's failure yields a requester-only outcome-1
//!   receipt (selective countersigning).
//! - Receipts reach one public pool at the end of each round. They are
//!   admitted by the arm's rule (the real `map_trust::admissible` for MAP
//!   arms and B1) and accumulated once per (issuer, provider) cell.
//!
//! **Scoring.** Each cell keeps α and β in reference units:
//! weight × decay^(−ended_day), so the value at evaluation day d is the
//! stored value × decay^d. An observer's score for provider p is
//! (A + 1)/(A + B + 2), where
//!   A = Σ_i w(o, i) · scale_ip · α_ip(d),   B likewise with β,
//! and scale_ip = min(1, cap / (α_ip + β_ip)). This is the quantity a
//! per-observer `TrustProjection` over the same receipts computes (§8.2
//! with the T-3 cap); tests/e1_equiv.rs checks it against a rebuilt
//! `TrustProjection`.
//!
//! **Selection (all arms).** With probability ε the shortlist is taken in
//! random order. Otherwise candidates are ordered by the arm's score,
//! unknown = prior mean 0.5, ties broken by the self-reported hint for B2
//! and B2L and at random otherwise. Execution attempts follow that order,
//! up to 3 (fallback). Canary arms (B2, B2L, Ours-L+C) probe first (honest
//! pass 0.97, adversarial 0.08); a failed canary costs no execution.

use crate::config::Params;
use crate::crypto::{receipt, CryptoMode, ReceiptSpec};
use crate::rng::Rng;
use crate::scenarios::weights;
use map_core::Usage;
use map_identity::Keypair;
use map_trust::{admissible, Outcome, Verified};
use std::collections::HashMap;

pub const METRICS: &[&str] = &[
    "attacker_share_ss",
    "attacker_share_all",
    "honest_success_ss",
    "honest_success_all",
    "time_to_detect",
    "attacker_share_r1_5",
    "attacker_share_off_start",
    "receipts",
];

/// Scenario `e1t` (Amendment A12): E1's metrics plus the share of honest
/// requesters' executions that go to the slander targets.
pub const METRICS_T: &[&str] = &[
    "attacker_share_ss",
    "attacker_share_all",
    "honest_success_ss",
    "honest_success_all",
    "time_to_detect",
    "attacker_share_r1_5",
    "attacker_share_off_start",
    "receipts",
    "target_share_ss",
    "target_share_all",
];

pub const CAP: &str = "map:cap:e1";
pub const DAY_MS: u64 = 86_400_000;
const SKEW_MS: u64 = 120_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arm {
    B0,
    B1,
    B2,
    B2L,
    Ours,
    OursC,
    APrior,
    AUnilateral,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weighting {
    W0,
    W1,
    W2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Attack {
    None,
    Farming,
    Camouflage,
    Collusion,
    Whitewash,
    OnOff,
    Slander,
    Combined,
    /// A12: camouflaging Sybils that file negatives only against a few
    /// fixed honest targets.
    TargetedSlander,
}

impl Attack {
    fn farming(self) -> bool {
        matches!(self, Attack::Farming | Attack::Camouflage | Attack::Combined)
    }
    fn slander(self) -> bool {
        matches!(self, Attack::Slander | Attack::Combined)
    }
}

pub struct Cfg {
    pub providers: usize,
    pub requesters: usize,
    pub rounds: usize,
    pub tasks_per_round: f64,
    pub shortlist: usize,
    pub alpha: f64,
    pub arm: Arm,
    pub weighting: Weighting,
    pub attack: Attack,
    pub epsilon: f64,
    pub decay: f64,
    pub issuer_cap: f64,
    pub sybils_per_adv: usize,
    pub farm_receipts: usize,
    pub camouflage_tasks: usize,
    pub collusion_receipts: usize,
    pub slander_receipts: usize,
    /// A12: number of honest providers `TargetedSlander` slanders.
    pub slander_targets: usize,
    pub whitewash_every: usize,
    pub onoff_period: usize,
    pub q_adv: f64,
    pub q_adv_on: f64,
    pub pass_honest: f64,
    pub pass_adv: f64,
    pub fallback: usize,
    /// Discovery flooding (Amendment A11): zero-evidence adversarial
    /// identities per honest provider that discovery returns alongside the
    /// real providers. 0 = honest discovery, the pre-registered E1 world.
    pub flood_per_honest: f64,
    /// Local shortlist policy (Amendment A11): this many slots hold the
    /// requester's best-scored providers among those it has a success
    /// receipt for; the rest come from discovery. 0 = discovery only.
    pub shortlist_memory: usize,
    /// Partial receipt visibility (Amendment A13): the probability that an
    /// observer sees a given other honest requester's receipts, fixed per
    /// (observer, issuer) pair. Sybil and colluder receipts stay visible to
    /// everyone. 1 = one public pool, the pre-registered E1 world.
    pub visibility: f64,
}

pub fn cfg_from(p: &Params) -> Result<Cfg, String> {
    let arm = match p.str("arm")? {
        "b0" => Arm::B0,
        "b1" => Arm::B1,
        "b2" => Arm::B2,
        "b2l" => Arm::B2L,
        "ours" => Arm::Ours,
        "ours_c" => Arm::OursC,
        "a_prior" => Arm::APrior,
        "a_unilateral" => Arm::AUnilateral,
        a => return Err(format!("unknown arm `{a}`")),
    };
    let weighting = match p.str("weights").unwrap_or("w0") {
        "w0" => Weighting::W0,
        "w1" => Weighting::W1,
        "w2" => Weighting::W2,
        w => return Err(format!("unknown weights `{w}`")),
    };
    let attack = match p.str("attack")? {
        "none" => Attack::None,
        "farming" => Attack::Farming,
        "camouflage" => Attack::Camouflage,
        "collusion" => Attack::Collusion,
        "whitewash" => Attack::Whitewash,
        "onoff" => Attack::OnOff,
        "slander" => Attack::Slander,
        "combined" => Attack::Combined,
        "targeted_slander" => Attack::TargetedSlander,
        a => return Err(format!("unknown attack `{a}`")),
    };
    Ok(Cfg {
        providers: p.u64_or("providers", 200)? as usize,
        requesters: p.u64_or("requesters", 500)? as usize,
        rounds: p.u64_or("rounds", 60)? as usize,
        tasks_per_round: p.f64_or("tasks_per_round", 2.0)?,
        shortlist: p.u64_or("shortlist", 10)? as usize,
        alpha: p.f64("alpha")?,
        arm,
        weighting,
        attack,
        epsilon: p.f64_or("epsilon", 0.1)?,
        decay: p.f64_or("decay", 0.98)?,
        issuer_cap: p.f64_or("issuer_cap", 50.0)?,
        sybils_per_adv: p.u64_or("sybils_per_adv", 20)? as usize,
        farm_receipts: p.u64_or("farm_receipts", 10)? as usize,
        camouflage_tasks: p.u64_or("camouflage_tasks", 2)? as usize,
        collusion_receipts: p.u64_or("collusion_receipts", 10)? as usize,
        slander_receipts: p.u64_or("slander_receipts", 10)? as usize,
        slander_targets: p.u64_or("slander_targets", 5)? as usize,
        whitewash_every: p.u64_or("whitewash_every", 5)? as usize,
        onoff_period: p.u64_or("onoff_period", 10)? as usize,
        q_adv: p.f64_or("q_adv", 0.20)?,
        q_adv_on: p.f64_or("q_adv_on", 0.95)?,
        pass_honest: p.f64_or("pass_honest", 0.97)?,
        pass_adv: p.f64_or("pass_adv", 0.08)?,
        fallback: p.u64_or("fallback", 3)? as usize,
        flood_per_honest: p.f64_or("flood_per_honest", 0.0)?,
        shortlist_memory: p.u64_or("shortlist_memory", 0)? as usize,
        visibility: p.f64_or("visibility", 1.0)?,
    })
}

/// A (issuer, provider) evidence cell in reference units.
#[derive(Clone, Copy, Default, Debug)]
pub struct Cell {
    pub a: f64,
    pub b: f64,
    pub n: u32,
}

/// The public evidence pool, accumulated per (issuer, provider identity).
pub struct Pool {
    pub decay: f64,
    pub cap: f64,
    /// cells[issuer][provider identity]
    pub cells: Vec<HashMap<u32, Cell>>,
    /// provider identity -> issuers holding a cell for it
    pub by_provider: HashMap<u32, Vec<u32>>,
}

impl Pool {
    pub fn new(decay: f64, cap: f64) -> Pool {
        Pool { decay, cap, cells: Vec::new(), by_provider: HashMap::new() }
    }

    /// Add one admitted receipt: `positive` increments α, else β by `w`
    /// (2 for disputed, handled by the caller), at its `ended` day.
    pub fn add(&mut self, issuer: u32, provider: u32, ended_ms: u64, alpha_w: f64, beta_w: f64) {
        let refw = self.decay.powf(-(ended_ms as f64) / DAY_MS as f64);
        if self.cells.len() <= issuer as usize {
            self.cells.resize_with(issuer as usize + 1, HashMap::new);
        }
        if !self.cells[issuer as usize].contains_key(&provider) {
            let list = self.by_provider.entry(provider).or_default();
            let at = list.binary_search(&issuer).unwrap_or_else(|x| x);
            list.insert(at, issuer); // kept sorted: deterministic summation order
        }
        let cell = self.cells[issuer as usize].entry(provider).or_default();
        cell.a += alpha_w * refw;
        cell.b += beta_w * refw;
        cell.n += 1;
    }

    /// (A, B) contributed by `issuer` about `provider` at `now_ms` with
    /// issuer weight `w`. The T-3 cap applies to the weighted, decayed
    /// contribution, exactly as `TrustProjection::score` applies it.
    pub fn contribution(&self, issuer: u32, provider: u32, now_ms: u64, w: f64) -> (f64, f64) {
        let Some(c) = self.cells.get(issuer as usize).and_then(|m| m.get(&provider)) else {
            return (0.0, 0.0);
        };
        let f = w * self.decay.powf(now_ms as f64 / DAY_MS as f64);
        let (a, b) = (c.a * f, c.b * f);
        let tot = a + b;
        let scale = if tot > self.cap && tot > 0.0 { self.cap / tot } else { 1.0 };
        (a * scale, b * scale)
    }

    /// Score (A+1)/(A+B+2) and whether any evidence was used, for a weight
    /// function over issuers.
    pub fn score<F: Fn(u32) -> f64>(&self, provider: u32, now_ms: u64, weight: F) -> (f64, bool) {
        let (mut sa, mut sb) = (0.0, 0.0);
        for &i in self.by_provider.get(&provider).map(|v| v.as_slice()).unwrap_or(&[]) {
            let w = weight(i);
            if !(w.is_finite() && w > 0.0) {
                continue;
            }
            let (a, b) = self.contribution(i, provider, now_ms, w);
            sa += a;
            sb += b;
        }
        ((sa + 1.0) / (sa + sb + 2.0), sa + sb > 0.0)
    }
}

struct Provider {
    q: f64,
    adv: bool,
    kp: Keypair,
    /// Current identity (a pool key); changes on whitewashing.
    ident: u32,
    hint: f64,
}

struct Pending {
    issuer: u32,
    provider: u32,
    ended_ms: u64,
    v: Verified,
}

/// Admitted receipts, as (issuer, provider identity, ended_ms, receipt).
pub type Dump = Vec<(u32, u32, u64, Verified)>;

pub fn run(p: &Params, rng: &mut Rng, mode: CryptoMode) -> Result<Vec<f64>, String> {
    let cfg = cfg_from(p)?;
    Ok(simulate(&cfg, rng, mode, None))
}

/// Scenario `e1t` (A12): E1 plus target-share metrics.
pub fn run_targeted(p: &Params, rng: &mut Rng, mode: CryptoMode) -> Result<Vec<f64>, String> {
    let cfg = cfg_from(p)?;
    let mut target = [0.0f64; 2];
    let mut out = simulate_with(&cfg, rng, mode, None, Some(&mut target));
    out.extend_from_slice(&target);
    Ok(out)
}

/// Run E1. If `dump` is set, every admitted receipt is also appended to it
/// (issuer, provider identity, ended_ms, Verified), for the
/// TrustProjection equivalence test.
pub fn simulate(cfg: &Cfg, rng: &mut Rng, mode: CryptoMode, dump: Option<&mut Dump>) -> Vec<f64> {
    simulate_with(cfg, rng, mode, dump, None)
}

/// As `simulate`. If `target_out` is set, it receives the steady-state and
/// all-rounds share of honest executions that went to the slander targets
/// (A12). Tracking the targets draws nothing from the RNG.
pub fn simulate_with(cfg: &Cfg, rng: &mut Rng, mode: CryptoMode, mut dump: Option<&mut Dump>, target_out: Option<&mut [f64; 2]>) -> Vec<f64> {
    let n_adv = (cfg.alpha * cfg.providers as f64).round() as usize;
    let mut next_ident: u32 = 0;
    let mut providers: Vec<Provider> = (0..cfg.providers)
        .map(|i| {
            let adv = i < n_adv;
            let q = if adv { cfg.q_adv } else { rng.uniform(0.80, 0.98) };
            let ident = next_ident;
            next_ident += 1;
            Provider { q, adv, kp: Keypair::from_seed(rng.bytes32()), ident, hint: 0.0 }
        })
        .collect();
    // Issuer indices: 0..R honest requesters, then Sybils, then colluding
    // providers acting as requesters.
    let requesters: Vec<Keypair> = (0..cfg.requesters).map(|_| Keypair::from_seed(rng.bytes32())).collect();
    let uses_sybils = cfg.attack.farming() || cfg.attack.slander() || cfg.attack == Attack::TargetedSlander;
    let sybils: Vec<Keypair> = if uses_sybils {
        (0..n_adv * cfg.sybils_per_adv).map(|_| Keypair::from_seed(rng.bytes32())).collect()
    } else {
        Vec::new()
    };
    // Discovery flooding (A11). Created after every pre-registered identity,
    // so with no flood nothing here draws from the RNG.
    let n_flood = (cfg.flood_per_honest * (cfg.providers - n_adv) as f64).round() as usize;
    for _ in 0..n_flood {
        let ident = next_ident;
        next_ident += 1;
        providers.push(Provider { q: cfg.q_adv, adv: true, kp: Keypair::from_seed(rng.bytes32()), ident, hint: 0.0 });
    }
    // A13 visibility salt, drawn only when visibility < 1, so the default
    // RNG stream is unchanged.
    let vis_salt: u64 = if cfg.visibility < 1.0 {
        let b = rng.bytes32();
        u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
    } else {
        0
    };
    // Whether observer `o` sees issuer `i`'s receipts (SplitMix64 of the pair).
    let visible = |o: u32, i: u32| -> bool {
        if cfg.visibility >= 1.0 || i == o || i as usize >= cfg.requesters {
            return true;
        }
        let mut z = vis_salt.wrapping_add(((o as u64) << 32) | i as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        ((z >> 11) as f64 / (1u64 << 53) as f64) < cfg.visibility
    };
    let sybil_base = cfg.requesters as u32;
    let colluder_base = sybil_base + sybils.len() as u32;
    let mut pool = Pool::new(cfg.decay, cfg.issuer_cap);
    let mut receipt_n: u64 = 0;
    let evidence_arm = !matches!(cfg.arm, Arm::B0 | Arm::B2);
    let canary = matches!(cfg.arm, Arm::B2 | Arm::B2L | Arm::OursC);

    let mut execs = vec![0u64; cfg.rounds];
    let mut adv_execs = vec![0u64; cfg.rounds];
    // A12 slander targets: the first `slander_targets` honest providers
    // (honest qualities are i.i.d., so this is a random choice). No RNG.
    let targets: Vec<usize> = (n_adv..cfg.providers).take(cfg.slander_targets).collect();
    let mut is_target = vec![false; providers.len()];
    for &i in &targets {
        is_target[i] = true;
    }
    let mut tgt_execs = vec![0u64; cfg.rounds];
    let mut tasks = vec![0u64; cfg.rounds];
    let mut successes = vec![0u64; cfg.rounds];
    // Per-observer issuer weights for W1/W2 (issuer -> weight, self excluded).
    let mut wmap: Vec<HashMap<u32, f64>> = vec![HashMap::new(); cfg.requesters];
    // Global arms score the same for every observer; evidence changes only
    // at round boundaries, so cache per round.
    let global = matches!(cfg.arm, Arm::B1 | Arm::B2L);
    let mut gcache: HashMap<u32, f64> = HashMap::new();

    for r in 0..cfg.rounds {
        let now_ms = r as u64 * DAY_MS;
        // Adversary behavior this round.
        if cfg.attack == Attack::OnOff {
            let on = (r / cfg.onoff_period) % 2 == 0;
            for pv in providers.iter_mut().filter(|p| p.adv) {
                pv.q = if on { cfg.q_adv_on } else { cfg.q_adv };
            }
        }
        if cfg.attack == Attack::Whitewash && r > 0 && r % cfg.whitewash_every == 0 {
            for pv in providers.iter_mut().filter(|p| p.adv) {
                pv.kp = Keypair::from_seed(rng.bytes32());
                pv.ident = next_ident;
                next_ident += 1;
            }
        }
        for pv in providers.iter_mut() {
            pv.hint = if pv.adv { rng.uniform(0.88, 1.0) } else { rng.uniform(0.40, 0.90) };
        }
        // Issuer weights from evidence through the previous round.
        if evidence_arm && matches!(cfg.arm, Arm::Ours | Arm::OursC | Arm::APrior | Arm::AUnilateral) {
            match cfg.weighting {
                Weighting::W0 => {}
                Weighting::W1 => wmap = weights::w1(&pool, cfg.requesters),
                Weighting::W2 => wmap = weights::w2(&pool, cfg.requesters),
            }
            if cfg.visibility < 1.0 {
                // A13: an observer weights only issuers whose receipts it sees.
                // Exact for W1 (agreement is pairwise); W2 is not evaluated.
                for (o, m) in wmap.iter_mut().enumerate() {
                    m.retain(|i, _| visible(o as u32, *i));
                }
            }
        }
        gcache.clear();
        // Identity -> provider index, for the A11 shortlist-memory policy.
        let ident_ix: HashMap<u32, usize> = if cfg.shortlist_memory > 0 {
            providers.iter().enumerate().map(|(i, p)| (p.ident, i)).collect()
        } else {
            HashMap::new()
        };
        let mut pending: Vec<Pending> = Vec::new();
        let ended_ms = now_ms + DAY_MS / 2;
        // Honest tasks.
        for o in 0..cfg.requesters {
            let k = rng.poisson(cfg.tasks_per_round);
            for _ in 0..k {
                tasks[r] += 1;
                let mut order: Vec<usize> = rng.sample_indices(providers.len(), cfg.shortlist.min(providers.len()));
                if cfg.shortlist_memory > 0 && evidence_arm {
                    // A11 local policy: keep the requester's best-scored
                    // known-good providers next to what discovery returned.
                    // Sorted by (score, index): HashMap order is not stable.
                    let mut known: Vec<(f64, usize)> = pool
                        .cells
                        .get(o)
                        .map(|m| {
                            m.iter()
                                .filter(|(_, c)| c.a > 0.0)
                                .filter_map(|(id, _)| ident_ix.get(id).map(|&i| (score_for(cfg, &pool, o as u32, *id, now_ms, &wmap).0, i)))
                                .collect()
                        })
                        .unwrap_or_default();
                    known.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
                    let keep: Vec<usize> = known.into_iter().take(cfg.shortlist_memory).map(|(_, i)| i).collect();
                    order.retain(|i| !keep.contains(i));
                    order.truncate(cfg.shortlist.saturating_sub(keep.len()));
                    order.extend(keep);
                }
                if !rng.bernoulli(cfg.epsilon) && cfg.arm != Arm::B0 {
                    // Pre-shuffled, so the stable sort breaks score ties at random.
                    let mut keyed: Vec<((f64, f64), usize)> = Vec::with_capacity(order.len());
                    for &i in &order {
                        let pv = &providers[i];
                        let hint = if matches!(cfg.arm, Arm::B2 | Arm::B2L) { pv.hint } else { 0.0 };
                        let s = if cfg.arm == Arm::B2 {
                            0.5
                        } else if global && cfg.visibility < 1.0 {
                            // A13: a global ledger scores from the receipts this
                            // observer can see, so it is no longer global.
                            pool.score(pv.ident, now_ms, |iss| if visible(o as u32, iss) { 1.0 } else { 0.0 }).0
                        } else if global {
                            *gcache
                                .entry(pv.ident)
                                .or_insert_with(|| score_for(cfg, &pool, o as u32, pv.ident, now_ms, &wmap).0)
                        } else {
                            score_for(cfg, &pool, o as u32, pv.ident, now_ms, &wmap).0
                        };
                        keyed.push(((s, hint), i));
                    }
                    keyed.sort_by(|a, b| b.0 .0.total_cmp(&a.0 .0).then(b.0 .1.total_cmp(&a.0 .1)));
                    order = keyed.into_iter().map(|(_, i)| i).collect();
                }
                let mut attempts = 0;
                for i in order {
                    let pv = &providers[i];
                    if canary {
                        let pass = if pv.adv { cfg.pass_adv } else { cfg.pass_honest };
                        if !rng.bernoulli(pass) {
                            continue;
                        }
                    }
                    attempts += 1;
                    execs[r] += 1;
                    if pv.adv {
                        adv_execs[r] += 1;
                    }
                    if is_target[i] {
                        tgt_execs[r] += 1;
                    }
                    let ok = rng.bernoulli(pv.q);
                    if evidence_arm {
                        receipt_n += 1;
                        let outcome = if ok { Outcome::Ok } else { Outcome::Failed };
                        let provider_signs = ok || !pv.adv;
                        let v = receipt(
                            mode,
                            &ReceiptSpec {
                                n: receipt_n,
                                requester: &requesters[o],
                                provider: &pv.kp,
                                capability: CAP,
                                started_ms: ended_ms - 1000,
                                ended_ms,
                                outcome,
                                usage: Usage { tokens_in: 1, tokens_out: 1, ttft_ms: 0, wall_ms: 1000 },
                                requester_signs: true,
                                provider_signs,
                            },
                        );
                        pending.push(Pending { issuer: o as u32, provider: pv.ident, ended_ms, v });
                    }
                    if ok {
                        successes[r] += 1;
                        break;
                    }
                    if attempts >= cfg.fallback {
                        break;
                    }
                }
            }
        }
        // Attack evidence published this round (identical for every arm).
        if evidence_arm && n_adv > 0 {
            let attack_receipt = |issuer_kp: &Keypair, issuer: u32, pv: &Provider, outcome: Outcome, prov_signs: bool, rn: &mut u64, pending: &mut Vec<Pending>| {
                *rn += 1;
                let v = receipt(
                    mode,
                    &ReceiptSpec {
                        n: *rn,
                        requester: issuer_kp,
                        provider: &pv.kp,
                        capability: CAP,
                        started_ms: ended_ms - 1000,
                        ended_ms,
                        outcome,
                        usage: Usage { tokens_in: 1, tokens_out: 1, ttft_ms: 0, wall_ms: 1000 },
                        requester_signs: true,
                        provider_signs: prov_signs,
                    },
                );
                pending.push(Pending { issuer, provider: pv.ident, ended_ms, v });
            };
            let honest: Vec<usize> = (0..providers.len()).filter(|i| !providers[*i].adv).collect();
            if cfg.attack == Attack::TargetedSlander && !targets.is_empty() {
                // Camouflage on honest non-targets (truthful, dual-signed), and
                // unilateral negatives against the targets only.
                let cover: Vec<usize> = honest.iter().copied().filter(|i| !is_target[*i]).collect();
                for si in 0..sybils.len() {
                    if !cover.is_empty() {
                        for _ in 0..cfg.camouflage_tasks {
                            let h = cover[rng.below(cover.len())];
                            let ok = rng.bernoulli(providers[h].q);
                            let out = if ok { Outcome::Ok } else { Outcome::Failed };
                            attack_receipt(&sybils[si], sybil_base + si as u32, &providers[h], out, true, &mut receipt_n, &mut pending);
                        }
                    }
                    for _ in 0..cfg.slander_receipts {
                        let h = targets[rng.below(targets.len())];
                        attack_receipt(&sybils[si], sybil_base + si as u32, &providers[h], Outcome::Failed, false, &mut receipt_n, &mut pending);
                    }
                }
            }
            for a in 0..n_adv {
                if cfg.attack.farming() {
                    for s in 0..cfg.sybils_per_adv {
                        let si = a * cfg.sybils_per_adv + s;
                        for _ in 0..cfg.farm_receipts {
                            attack_receipt(&sybils[si], sybil_base + si as u32, &providers[a], Outcome::Ok, true, &mut receipt_n, &mut pending);
                        }
                        if cfg.attack == Attack::Camouflage && !honest.is_empty() {
                            for _ in 0..cfg.camouflage_tasks {
                                let h = honest[rng.below(honest.len())];
                                let ok = rng.bernoulli(providers[h].q);
                                let out = if ok { Outcome::Ok } else { Outcome::Failed };
                                attack_receipt(&sybils[si], sybil_base + si as u32, &providers[h], out, true, &mut receipt_n, &mut pending);
                            }
                        }
                    }
                }
                if cfg.attack.slander() && !honest.is_empty() {
                    for s in 0..cfg.sybils_per_adv {
                        let si = a * cfg.sybils_per_adv + s;
                        for _ in 0..cfg.slander_receipts {
                            let h = honest[rng.below(honest.len())];
                            attack_receipt(&sybils[si], sybil_base + si as u32, &providers[h], Outcome::Failed, false, &mut receipt_n, &mut pending);
                        }
                    }
                }
                if cfg.attack == Attack::Collusion && n_adv > 1 {
                    for _ in 0..cfg.collusion_receipts {
                        let mut b = rng.below(n_adv - 1);
                        if b >= a {
                            b += 1;
                        }
                        let kp = providers[a].kp.clone();
                        attack_receipt(&kp, colluder_base + a as u32, &providers[b], Outcome::Ok, true, &mut receipt_n, &mut pending);
                    }
                }
            }
        }
        // Ingest the round's receipts under the arm's admission rule.
        for pd in pending {
            if admit(cfg.arm, &pd.v, pd.ended_ms) {
                let (aw, bw) = match pd.v.core.outcome {
                    Outcome::Ok => (1.0, 0.0),
                    Outcome::Disputed => (0.0, 2.0),
                    _ => (0.0, 1.0),
                };
                pool.add(pd.issuer, pd.provider, pd.ended_ms, aw, bw);
                if let Some(d) = dump.as_deref_mut() {
                    d.push((pd.issuer, pd.provider, pd.ended_ms, pd.v));
                }
            }
        }
    }
    if let Some(out) = target_out {
        let share = |lo: usize, hi: usize| -> f64 {
            let (a, b): (u64, u64) = (tgt_execs[lo..hi].iter().sum(), execs[lo..hi].iter().sum());
            if b == 0 { 0.0 } else { a as f64 / b as f64 }
        };
        *out = [share(cfg.rounds / 2, cfg.rounds), share(0, cfg.rounds)];
    }
    metrics(cfg, n_adv, &execs, &adv_execs, &tasks, &successes, receipt_n)
}

/// The arm's admission rule. MAP arms and B1 use the real `map-trust` gate
/// (weight is applied later, so T-3 is checked with weight 1 here).
fn admit(arm: Arm, v: &Verified, now_ms: u64) -> bool {
    match arm {
        Arm::B0 | Arm::B2 => false,
        Arm::B2L => v.provider_signed && v.core.requester != v.core.provider,
        Arm::AUnilateral => {
            admissible(v, now_ms, SKEW_MS, 1.0).is_ok()
                || (v.requester_signed && !v.provider_signed && v.core.outcome == Outcome::Ok && v.core.requester != v.core.provider)
        }
        _ => admissible(v, now_ms, SKEW_MS, 1.0).is_ok(),
    }
}

/// The arm's score for (observer, provider identity).
pub fn score_for(cfg: &Cfg, pool: &Pool, o: u32, prov: u32, now_ms: u64, wmap: &[HashMap<u32, f64>]) -> (f64, bool) {
    match cfg.arm {
        Arm::B0 | Arm::B2 => (0.5, false),
        Arm::B1 | Arm::B2L => pool.score(prov, now_ms, |_| 1.0),
        Arm::Ours | Arm::OursC | Arm::AUnilateral if cfg.weighting == Weighting::W0 => {
            // Only the observer's own cell carries weight.
            let (a, b) = pool.contribution(o, prov, now_ms, 1.0);
            ((a + 1.0) / (a + b + 2.0), a + b > 0.0)
        }
        Arm::APrior => pool.score(prov, now_ms, |i| {
            if i == o {
                1.0
            } else {
                let w = issuer_weight(cfg.weighting, o, i, wmap);
                if w > 0.0 { w } else { 0.5 }
            }
        }),
        _ => pool.score(prov, now_ms, |i| issuer_weight(cfg.weighting, o, i, wmap)),
    }
}

fn issuer_weight(w: Weighting, o: u32, i: u32, wmap: &[HashMap<u32, f64>]) -> f64 {
    if i == o {
        return 1.0;
    }
    match w {
        Weighting::W0 => 0.0,
        Weighting::W1 | Weighting::W2 => wmap[o as usize].get(&i).copied().unwrap_or(0.0),
    }
}

fn metrics(cfg: &Cfg, n_adv: usize, execs: &[u64], adv: &[u64], tasks: &[u64], succ: &[u64], receipts: u64) -> Vec<f64> {
    let ratio = |num: &[u64], den: &[u64], lo: usize, hi: usize| -> f64 {
        let (a, b): (u64, u64) = (num[lo..hi].iter().sum(), den[lo..hi].iter().sum());
        if b == 0 { 0.0 } else { a as f64 / b as f64 }
    };
    let t = cfg.rounds;
    let half = t / 2;
    let share: Vec<f64> = (0..t).map(|r| if execs[r] == 0 { 0.0 } else { adv[r] as f64 / execs[r] as f64 }).collect();
    let target = cfg.alpha / 4.0;
    let ttd = if n_adv == 0 {
        0.0
    } else {
        (0..t)
            .find(|&r| {
                let hi = (r + 5).min(t);
                share[r..hi].iter().sum::<f64>() / (hi - r) as f64 <= target
            })
            .map(|r| r as f64)
            .unwrap_or(t as f64)
    };
    let off_start = if cfg.attack == Attack::OnOff {
        let rounds: Vec<usize> = (0..t).filter(|r| (r / cfg.onoff_period) % 2 == 1 && r % cfg.onoff_period < 3).collect();
        let (a, b) = rounds.iter().fold((0u64, 0u64), |(a, b), r| (a + adv[*r], b + execs[*r]));
        if b == 0 { f64::NAN } else { a as f64 / b as f64 }
    } else {
        f64::NAN
    };
    vec![
        ratio(adv, execs, half, t),
        ratio(adv, execs, 0, t),
        ratio(succ, tasks, half, t),
        ratio(succ, tasks, 0, t),
        ttd,
        ratio(adv, execs, 0, 5.min(t)),
        off_start,
        receipts as f64,
    ]
}
