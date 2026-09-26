# Pre-registered evaluation plan (map-sim)

**Status: PRE-REGISTERED, 2026-09-26, before any simulation code or result
exists.** This file is committed and tagged `prereg-v1` before `map-sim`
is written. Git history is the evidence of ordering. After the tag, this
file changes only by appending to *Amendments* (dated, with the reason);
the text above that section is never edited. Every hypothesis is
reported whatever its outcome, including the regimes where we expect MAP
to lose.

Author note: the simulator, the baselines and MAP are all written by the
same author. That is the main threat to validity. The mitigations are this
pre-registration, the replication of a third party's published curve
before comparing against their method (F1-a), and publishing all code,
configs and raw per-seed CSVs.

## 0. Common method

- **Engine.** A deterministic discrete-event simulator: virtual time,
  binary-heap event queue, one seeded ChaCha8 stream per run. The same
  (config, seed) produces a byte-identical CSV.
- **Real code.** Receipt admissibility (R-2..R-4), checkpoint
  construction and verification, resumption, shard functions and metering
  arithmetic come from the protocol crates (`map-trust`,
  `map-transcript`, `map-discovery`, `map-meter`). Signing and
  verification use real Ed25519 unless `--fast-crypto` is set. That flag
  is allowed only for runs with more than 10⁴ nodes. A fixed-seed test
  must show that fast and real modes produce identical metric CSVs.
- **Seeds and statistics.** 20 seeds per sweep point unless a scenario
  says otherwise (F1 uses 60, matching the source). We report the mean
  and a 95% percentile-bootstrap CI (10,000 resamples over per-seed
  values). A comparative hypothesis "holds at a point" only if its
  bootstrap CI (of a difference or ratio of per-seed means, resampling
  seeds jointly) lies entirely on the claimed side. We report every tested
  point and the count of points where each hypothesis holds. There is no
  multiple-comparison correction; the full grid is published.
- **Outputs.** `eval/results/<git-sha>/<scenario>.csv` (one row per seed ×
  sweep point × arm) plus `env.json` (toolchain, CPU, config hashes).
  Figures are produced only from those CSVs by `eval/analysis/`.

## F1. Fairness replication and extension in 2603.03753's own scenario

The source is arXiv:2603.03753, simulator `plan-lab-szu/agentic_p2p_sim`
v0.3.10, as read on 2026-09-26. Its model is registry discovery with
self-reported descriptor hints. Sybils (fraction α of N = 100) overclaim
the hints: semantic 0.92–1.0, trust 0.88–1.0, cheap and fast. They
execute successfully with probability 0.05. Ranking is 0.45·semantic +
0.20·trust + 0.20·(1−latency) + 0.15·(1−cost), shortlist K = 25, and each
step allows up to 3 execution attempts. A workflow is two steps (image
generator, then publisher) arriving at 0.5/s over 60 s. The image step
fails on quality with probability 0.5. "Risk-Aware" adds a one-round
canary: honest peers pass with 0.97, Sybils with 0.08 (α-sweep configs).
It is applied to both steps, with 30 extra uniformly random probes.
Their published result (`results/fig5_a_robustness.csv`):

| α | No Verify | Risk-Aware |
|---|---|---|
| 0.0 | 0.878 | 0.871 |
| 0.1 | 0.035 | 0.772 |
| 0.2 | 0.017 | 0.638 |
| 0.3 | 0.013 | 0.511 |
| 0.4 | 0.012 | 0.440 |
| 0.5 | 0.016 | 0.407 |

We re-implement this model in `map-sim` (we do not run their code) with
their α-sweep parameters.

- **F1-a (replication, gate for everything below).** For each α ∈ {0,
  0.1, …, 0.5} and both policies, our mean workflow success over 60 seeds
  lies within ±0.05 absolute of the table. If any point misses, we fix
  our re-implementation only when we can name the modeling difference,
  and record that in *Amendments*. We do not proceed to F1-b with an
  unexplained mismatch.
- **F1-b (MAP evidence in their world).** New arms:
  - **MAP-L**: each requester ranks candidates by its own local
    projection (dual-signed success receipts, unilateral failure
    receipts). Unknown candidates keep their self-reported rank as the
    tiebreak, with ε = 0.1 exploration.
  - **MAP-L+RA**: MAP-L ranking followed by the Risk-Aware canary and
    fallback.

  Horizons: 60 s (theirs) and 3600 s.
  - *Prediction 1 (a losing regime for MAP).* At 60 s, each requester has
    well under one workflow of its own history. MAP-L+RA does not beat
    Risk-Aware: the CI of the difference contains 0 at every α.
  - *Prediction 2.* At 3600 s, MAP-L+RA − Risk-Aware ≥ 0.10 at α ∈ {0.2,
    0.3, 0.4, 0.5}.
  - *Prediction 3.* Sybils rotate identity every 60 s (whitewashing).
    MAP-L+RA − Risk-Aware stays ≥ 0.05 at α ∈ {0.2, 0.3}. Fresh
    identities carry no evidence, so honest providers with positive
    history keep winning. The margin shrinks relative to Prediction 2.

## E1. Trust under attack (H1, and decision D5)

**World.** One capability. P = 200 providers, R = 500 honest requesters,
T = 60 rounds (1 round = 1 day of decay). Each honest requester issues
Poisson(2) tasks per round. Each task gets a shortlist of m = 10 providers
sampled uniformly from live providers. Discovery is honest in E1, so this
isolates trust; F1 and E3 cover discovery attacks.
- Honest provider success probability: q ~ U[0.80, 0.98].
- A fraction α ∈ {0, 0.05, 0.1, 0.2, 0.3, 0.5} of providers are
  adversarial, with q = 0.20 unless the attack says otherwise.

**Evidence.**
- A success yields a dual-signed receipt, outcome 0.
- An honest provider countersigns its failures (dual-signed, outcome 1).
- An adversarial provider refuses to countersign failures, so the
  requester files a unilateral negative receipt (requester-only,
  outcome 1). This is "selective countersigning", layer §6.6.
- All receipts go to one public pool that every observer can read (worst
  case: adversaries publish without limit). R-1 is checked once, at pool
  ingestion.

**Attacks** (each run separately, plus "combined" = farming + slander +
selective countersigning):
1. *Farming.* Each adversarial provider controls 20 Sybil requester
   identities, and each Sybil pool-publishes 10 dual-signed outcome-0
   receipts per round with its provider.
2. *Farming with camouflage.* As farming, but each Sybil also runs 2 real
   tasks per round against random honest providers and reports them
   truthfully, so it builds agreement with honest observers.
3. *Collusion ring.* Adversarial providers act as requesters for each
   other: 10 dual-signed outcome-0 receipts per provider per round.
4. *Whitewashing.* An adversarial provider re-enters under a fresh
   AgentID every 5 rounds.
5. *On-off.* Adversarial providers have q = 0.95 for 10 rounds, then
   q = 0.20 for 10 rounds, alternating.
6. *Slander.* 20 Sybil requesters per adversarial provider each file 10
   unilateral negative receipts per round against random honest
   providers.

**Arms.** All use the same selection rule: ε = 0.1 uniform exploration
within the shortlist (the §8.5 graduated bootstrap), otherwise argmax of
the arm's score, with ties broken uniformly.
- **B0** random selection.
- **B1** global ledger (ERC-8004-style): one global Beta projection over
  every admissible receipt from every issuer, issuer weight 1. This is
  simultaneously the "no issuer weighting" ablation of MAP. It does not
  model ERC-8004's on-chain feedback cost, a Sybil cost we set to zero;
  this is stated as a limitation.
- **B2** 2603.03753 Risk-Aware: per-task canary (honest pass 0.97,
  adversarial 0.08) with fallback (≤ 3 attempts) over a self-reported
  ranking in which adversaries overclaim. No persistent reputation.
- **B2L** B2 plus their Tier-1 "federated community ledger" (their
  prose): a global Beta over provider-signed receipts, where a provider
  signature alone suffices.
- **Ours-L** MAP local projection: R-2..R-4 via `map-trust`, §8.2 Beta,
  decay 0.98/day, per-issuer cap 50 per cell, with issuer weighting by
  one of the D5 arms:
  - **W0 self-only**: the observer's own receipts weigh 1, all others 0.
  - **W1 agreement**: issuer i weighs max(0, 1 − 2·MAD). MAD is the mean
    absolute difference between i's and the observer's per-provider
    success rates, over providers both have ≥ 3 receipts for. It needs an
    overlap of ≥ 3 providers, else the weight is 0. Recomputed once per
    round.
  - **W2 personalized EigenTrust**: power iteration (10 steps, restart
    probability 0.15 to the observer) over the row-normalized W1
    agreement graph.
- **Ablations of Ours-L (with the D5 winner):**
  - *A-prior*: unknown issuers weigh 0.5, the v1 behavior behind A1.
  - *A-unilateral*: unilateral positive receipts are admitted.
- **Ours-L+C** the D5 winner with B2's canary in front, to test
  complementarity.

**Metrics.**
- *Attacker share*: the fraction of honest requesters' tasks executed by
  adversarial providers, over rounds 31–60 (steady state) and over all
  rounds.
- *Honest success*: the fraction of honest tasks that succeed.
- *Time to detect*: the first round r at which the attacker share
  averaged over rounds r..r+4 is ≤ α/4, right-censored at 60.

**D5 decision rule (pre-registered).** Choose the W arm with the lowest
steady-state attacker share averaged over all attacks at α = 0.2. If two
arms are within each other's CIs, or differ by < 2 percentage points of
honest success, choose the simpler one, in the order W0 < W1 < W2. The
winner becomes the recommended default in `spec/layer.md` §8.4 (P8).

**H1.** At every α ∈ {0.05, 0.1, 0.2, 0.3} and every attack, Ours-L
(the D5 winner)'s steady-state attacker share ≤ 0.5 × that of B1 and
≤ 0.5 × that of B2. Upper CI bound of the ratio ≤ 0.5.

**Predicted losing regimes** (reported whether or not they occur):
- *L1 cold start.* In rounds 1–5, Ours-L without the canary has a higher
  attacker share than B2.
- *L2 sparse evidence.* With 0.1 tasks per requester per round, W0's
  attacker share is not significantly below B0's.
- *L3 on-off.* During the first 3 rounds of each "off" phase, Ours-L's
  attacker share exceeds its own steady state. Decay lags behavior.
- *L4 camouflage.* W1 and W2 are worse than W0 under farming with
  camouflage.

## E2. Checkpointed handoff (H2)

**World.** A task of L Parts, L ∈ {10², 10³, 10⁴}, one Part per time
unit. The serving provider fails with hazard λ per Part, λL ∈ {0.1, 1, 3}
(expected failures per task), and is then replaced.
- Checkpoint interval c ∈ {1, 8, 32, 128, ∞}.
- Dishonest-provider fraction f ∈ {0, 0.2}. A dishonest provider that
  fails claims 50% more progress than it delivered.

**Arms.**
- *Restart*: the successor starts at Part 0.
- *Countersigned resume* (MAP §7): the successor replays from the last
  dual-signed checkpoint. Real `CheckpointArtifact` propose, countersign,
  verify and resume with prefix-root verification in real mode.
- *Provider-only resume* (v1-like): the successor trusts the failed
  provider's own claimed progress.

**Metrics.**
- *Wasted work*: Parts executed that are not in the final transcript.
- *Signatures* and *checkpoint bytes* (real encodings) per task.
- *Overbilling*: usage billed to failed providers minus usage the
  requester acknowledged.
- *Transcript integrity*: whether the final transcript root verifies.

**H2.** At λL = 1, L = 10³, c = 8: countersigned resume wastes ≥ 60% less
work than restart (upper CI bound of the ratio ≤ 0.4).

**H2-bound (asserted in code, not tested statistically).** Under
countersigned resume, every failed provider's billed usage exceeds the
acknowledged usage by at most one checkpoint interval (C-5), in 100% of
runs. A violation aborts the run.

We also report, without a hypothesis:
- the waste-versus-signature trade-off curve, and the c that minimizes
  (waste + κ·signatures) for κ ∈ {0.01, 0.1, 1} Part-equivalents per
  signature;
- provider-only resume's overbilling and integrity failures at f = 0.2.
  We expect them to be non-zero, since no countersignature bounds the
  claim.

## E3. Discovery (H3, H4)

- **E3a Shard-flood poisoning (A3).** N providers (10³, 10⁴) publish to
  their own shard of 256. The attacker controls a fraction a ∈ {0.01,
  0.05, 0.1} of identities and mis-publishes each of its records into all
  of the r = 3 shards a querier samples (the flood). We measure the
  attacker fraction among candidates from r = 3 random shards, without
  and with the D-6 querier filter (`map_discovery::shard`).
  - **H3.** With D-6, the attacker's candidate share is ≤ its fair share
    a: the CI of (share − a) has upper bound ≤ 0.01.
  - Without D-6 we expect a share far above a (reported).
- **E3b Grinding (A4).** The attacker grinds identities into a target
  shard. Its budget is the measured keygen+shard rate (re-measured by a
  criterion bench in this repo) × a time window. We measure the fraction
  of the target shard it controls, with a fixed `shard()` versus the
  epoch-salted `shard_e()` (D-8) and epochs of {1 h, 24 h} (the grind must
  restart each epoch). No hypothesis; D-8 is experimental and the result
  decides its requirement level.
- **E3c Lookup under malicious routers (D-9).** Kademlia with k = 20 and
  α = 3, N ∈ {10³, 10⁴, 10⁵}, churn of {0, 10%, 30%} offline. A fraction
  f ∈ {0, 0.1, 0.2, 0.3} of nodes are malicious routers that answer
  lookups only with other malicious nodes. We compare 1 path against
  d = 3 disjoint paths and measure lookup success, hops, messages and
  latency (RTTs drawn from F1's mixture). No hypothesis: this replicates
  the known S/Kademlia result as supporting evidence for D-9.
- **E3d Staleness of the volatile plane (H4).** Provider queue length
  follows an M/M/1 process at utilization ρ ∈ {0.5, 0.8}. The querier
  ranks 10 candidates by queue length. We compare
  - *pull*: a StatusQuery at selection time (one RTT per candidate);
  - *descriptor-carried*: the value as of the last descriptor refresh,
    TTL ∈ {10, 60, 600} s, refresh at 0.8·TTL.

  Metrics are the mean absolute queue-length error at selection, the
  selection regret (chosen queue minus minimum queue), and messages per
  query.
  - **H4.** Pull has lower regret than descriptor-carried at TTL ≥ 60 s
    at both ρ (upper CI bound of the difference < 0).
  - We also report the message-rate crossover, where 2·K·query-rate
    equals N/(0.8·TTL). No hypothesis for TTL = 10 s.

## E4. Metering on vouchers (H5)

Built on E2's countersigned-resume runs, with per-token pricing (layer
§9.5 rates), c ∈ {1, 8, 32, 128}, and generation time 20 ms per token.

- **H5-a (asserted in code).**
  - Cumulative vouchers are monotone per provider.
  - Re-delivering a voucher digest is idempotent.
  - No amount is negative (`map-meter`).
  - A provider's maximum exposure (work delivered but not yet covered by
    a voucher) is ≤ price(c Parts), in 100% of runs.
- **H5-b.** The time overhead of MAP signing and verification per task,
  measured with a criterion bench on this machine (sign + verify ×
  2 parties per checkpoint, plus the receipt) and expressed relative to
  generation time, is < 1% at c ≥ 8.
- **Reported without hypothesis:** byte overhead relative to Part
  payload. For token-sized Parts we expect it to be large (a 312-byte
  checkpoint per c Parts). The paper states this honestly; the
  mitigation is a larger c.

## Amendments

Appended after `prereg-v1`. The text above this section is unchanged.

**A1 (2026-09-26, E1 uses `--fast-crypto`).**
- *Rule change:* §0 permits fast mode only above 10⁴ nodes. E1 has about
  2 700 identities but signs about 1.2 M receipts per run under farming at
  α = 0.5. The real-mode grid would take about 14 CPU-hours (measured: 48
  realcheck jobs took 357 s).
- *Evidence:* `configs/e1_realcheck.toml` covers all eight arms under
  farming and combined attacks at α = 0.2. It produces the identical CSV in
  real mode (every receipt Ed25519-signed and verified) and fast mode:
  SHA-256 `61b82527…c063` both ways.
- *Ongoing check:* `run_all.ps1` repeats this comparison on every run.

**A2 (2026-09-26, clarification: fallback for every E1 arm).**
- The text gave B2 "fallback (≤ 3 attempts)". So that the arms differ only
  in ranking and filtering, every E1 arm uses the same fallback: up to 3
  execution attempts per task in the arm's order.
- Canary arms (B2, B2L, Ours-L+C) skip a candidate that fails the canary
  without spending an attempt, as in 2603.03753.

**A3 (2026-09-26, clarification: what B1 contains).**
- B1 is "Ours-L without T-3 issuer weighting". It therefore keeps decay
  (0.98/day) and the per-issuer cap (50 per cell).
- A pure ERC-8004-style unweighted sum would be a *weaker* baseline. We
  keep the stronger one, which makes H1 harder to satisfy.
- B2 and B2L break ties with the self-reported hint. Ledger and MAP scores
  treat an unknown provider as the Beta prior mean, 0.5.

**Implementation note (no hypothesis change).**
- E1 computes scores through a factorized evidence pool. That it equals a
  rebuilt `map_trust::TrustProjection` over the same receipts is asserted
  by `map-rs/sim/tests/e1_equiv.rs`.
- That test caught one bug before any E1 output was examined: the
  per-issuer cap was applied before the issuer weight instead of after,
  as `TrustProjection` does. The fix came before the first E1 run.
