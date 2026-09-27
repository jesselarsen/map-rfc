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

**A4 (2026-09-27, repair of the D5 decision rule; requested by the author).**
- *Flaw:* the rule's tie clause ("… or differ by < 2 percentage points of
  honest success, choose the simpler one") is satisfied almost always in
  E1. Every arm has the same fallback (A2), so honest success sits near
  0.99 for all three weightings. The clause therefore returns W0
  regardless of attacker share, which defeats the rule's stated purpose
  ("the arm with the lowest steady-state attacker share").
- *What was seen before this amendment:* a 2-seed plumbing run
  (`run_all.ps1 -Quick`, seeds 0–1, not a result). Its mean steady-state
  attacker share over the seven attacks at α = 0.2 was W0 0.081, W1
  0.031, W2 0.029; honest success was 0.993, 0.998, 0.998. The literal
  rule picked W0. We disclose these numbers because the amendment was
  written after seeing them.
- *Repaired rule:* take the W arm with the lowest steady-state attacker
  share (mean over the seven attacks at α = 0.2, 20 seeds, bootstrap CI).
  Choose a simpler arm (W0 < W1 < W2) instead only if **both** hold:
  - its share CI overlaps the best arm's share CI;
  - its honest success is not more than 2 percentage points below the
    best arm's.

  Honest success thus becomes a guard against a harmful choice, not a
  reason to prefer simplicity.
- `analyze.py` applies the repaired rule and also prints the winner under
  the original wording, so both are on record. The paper reports the
  repaired rule as a post-hoc amendment.

**A5 (2026-09-27, E3b grinding rate).**
- As planned in the e3b config, `grind_rate_per_s` is set to this
  machine's criterion measurement: `keygen_shard` at 16.42 µs per attempt
  on one core, so 60 901 attempts/s. The reviewer measured 59 778.
- *Exploratory addition, not a hypothesis:*
  `configs/e3b_multicore.toml` repeats E3b at 16 × that rate, one
  commodity 16-core machine. The pre-registered single-core figure
  understates a realistic attacker; the paper reports both.

**A6 (2026-09-27, the D5 decision).**
- Run at commit 9a4a968: `eval/results/9a4a968/e1-d5.csv`, 20 seeds, α =
  0.2, all seven attacks. Mean steady-state attacker share: W0 0.0796
  [0.0793, 0.0800], W1 0.0312 [0.0310, 0.0313], W2 0.0290 [0.0288,
  0.0292]. Honest success: 0.9929, 0.9975, 0.9979.
- The repaired rule (A4) selects **W2**: the W1 and W2 share CIs do not
  overlap. The original wording would have selected W0.
- `configs/e1_main.toml` therefore uses `weights = "w2"`, and W2 becomes
  the recommended default for `spec/layer.md` §8.4 (P8).
- *Implementation note:* after this run, W1/W2 were re-implemented in
  `map-sim/src/scenarios/weights.rs`, 3.2× faster.
  - The old code summed agreement terms while iterating a
    randomly-ordered `HashMap`. That is a latent nondeterminism, although
    three repeated processes gave identical CSVs and an old-versus-new
    W2 run matched byte-for-byte (SHA-256 `4a8d08…`).
  - The new code accumulates in fixed order and equals the direct
    definitions exactly (`tests/weights_equiv.rs`, validated by two
    planted bugs).
  - The full evaluation is re-run at the new commit; if D5 there differs
    from the above, a further amendment records it.

**A7 (2026-09-27, implementation only; no rule, config or hypothesis
changes).**
- The full re-run at 9a15e99 was interrupted during `e1_main` when the
  host session exited. Its CSVs up to that point (f1a, f1b, e2, e1-d5,
  e1-sparse) had the same SHA-256 as run 1 (9a4a968). `e1_main` had
  produced nothing.
- The run had used 135k CPU-seconds without finishing `e1_main`. The
  cause was W2's power iteration, which is memory-bound: each edge
  streamed two 4 KB rows (500 observers × 8 B).
- `w2` now processes observers in cache blocks of 32. Blocking only
  reorders work across accumulators. Each accumulator still receives
  the restart term first, then its contributions in ascending source
  order, so the result is bit-identical:
  - `tests/weights_equiv.rs` passes;
  - a planted restart-index bug makes it fail;
  - the α = 0.2 / combined / W2 probe CSV is byte-identical to the old
    binary's (SHA-256 `4a8d08…`).
  - One job at α = 0.2 went from 366 s to 41 s; the worst cell, α = 0.5
    combined, takes 245 s.
- The full evaluation is re-run at the commit that carries this
  amendment.

**A8 (2026-09-27, EXPLORATORY; not a hypothesis test).**
- *Trigger:* in `e1_main` at 5bc7702, Ours-L (W2) under farming with
  camouflage had a steady-state attacker share of 0.550 at α = 0.5. That
  is worse than random selection (B0 0.500). D5 compared the weightings
  only at α = 0.2.
- `configs/e1_camo_explore.toml` repeats the D5 comparison under
  camouflage at α ∈ {0.3, 0.5}: arm `ours`, W0/W1/W2, 20 seeds, binary
  at 5bc7702. Output: `results/5bc7702/e1-camo-explore.csv`. Steady-state
  attacker share [95% CI]:
  - α = 0.3: W0 0.118 [0.117, 0.119], W1 0.072 [0.071, 0.074], W2 0.116
    [0.114, 0.118].
  - α = 0.5: W0 0.237 [0.236, 0.238], W1 0.206 [0.201, 0.212], W2 0.551
    [0.545, 0.558]. Honest success is 0.961 / 0.978 / 0.866.
- *Reading:* at α = 0.5 each adversarial provider's 20 camouflaging
  Sybils outnumber the honest requesters 4 : 1 (2,000 against 500).
  Their truthful reports about honest providers earn them agreement
  edges, and W2's propagation concentrates mass in that cluster. W1 uses
  only direct agreement with the observer and does not collapse.
- This does not change D5 as pre-registered: W2 won at α = 0.2 under the
  A4 rule. Whether the spec's recommended default follows D5 or this
  exploratory result is a P8 decision, recorded separately.

**A9 (2026-09-27, pre-registered before the run it describes; the
author's decision after A8).** The P8 recommended default for layer §8.4
is chosen by a minimax rule over the whole E1 grid.
- *Disclosure:* this rule is written after two sets of results:
  - W2 on every E1 cell (`e1_main`, arm `ours`);
  - W0, W1 and W2 on two camouflage cells (A8).

  The new information it waits for is W0 and W1 on the remaining cells.
- *Data:*
  - **W2:** `results/5bc7702/e1-main.csv`, rows with `arm = ours`, whose
    weighting is W2 (A6).
  - **W0 and W1:** a new run, `configs/e1_minimax.toml`: arm `ours`,
    20 seeds, binary at 5bc7702, output
    `results/5bc7702/e1-minimax.csv`.
  - **Cells:** α ∈ {0.05, 0.1, 0.2, 0.3, 0.5} × the seven attacks (35
    cells). α = 0 has no attacker and is excluded.
- *Rule:*
  1. For each arm and cell, take the mean steady-state attacker share
     over seeds, with a 95% percentile bootstrap CI (`stats.mean_ci`).
  2. `worst(arm)` is the cell with that arm's highest mean.
  3. The winner is the arm with the lowest worst-case mean.
  4. If the winner's CI at its worst cell overlaps another arm's CI at
     that arm's worst cell, those arms are tied. The simplest of the tied
     arms wins, in the order W0 < W1 < W2.
- *Reported with it:* the full 35-cell table per arm, and honest success
  at each arm's worst cell.
- *Script:* `analysis/minimax.py`, committed with this amendment.
- The winner becomes the layer §8.4 recommended default. D5 (A6) stays
  reported as run.

**A9 result (2026-09-27).** `e1-minimax` at the 5bc7702 binary: 1,400 jobs,
CSV SHA-256 `4dd3aeb4…`. `analysis/minimax.py` gives these worst cells:
- W0: 0.348 [0.346, 0.350] (α = 0.5, on-off);
- W1: 0.200 [0.196, 0.204] (α = 0.5, camouflage);
- W2: 0.550 [0.543, 0.557] (α = 0.5, camouflage).

The CIs do not overlap, so **W1 wins** with no tie and becomes the layer
§8.4 recommended default. Full table: `results/5bc7702/MINIMAX.md`. H1 and
the other E1 verdicts stay as pre-registered, computed with the D5 winner
(W2).

**A10 (2026-09-27, EXPLORATORY; not a hypothesis test; written before the
run it describes).**
- *Trigger:* review finding R2-04 (`paper/REVIEW-redteam-2.md`). The threat
  model says identities are free, but E1 fixed 20 Sybil requesters per
  adversarial provider (800 against 500 honest requesters at α = 0.2).
  Under W1 each camouflaging Sybil earns roughly an honest peer's weight.
  The per-issuer cap bounds each issuer, not the number of issuers. So W1's
  robustness, and the A9 minimax result, may hold only at that ratio.
- *Disclosure:* before this amendment was written, a one-seed timing probe
  was run to size the grid (α = 0.2, camouflage, seed 0; not a result).
  Its steady-state attacker shares were W1 0.070 at 80 Sybils and 0.157 at
  320, and W2 0.204 and 0.530. The grid below was chosen after seeing them.
- *Config:* `configs/e1_sybil_sweep.toml`.
  - Arm `ours`, α = 0.2, 20 seeds, `--fast-crypto` (A1).
  - Attack ∈ {camouflage, farming}. Farming, without camouflage, is the
    control: its Sybils share no providers with honest observers.
  - Weights ∈ {W0, W1, W2}; `sybils_per_adv` ∈ {5, 20, 80, 320}.
  - The binary is built from the current tree. Its simulator is unchanged
    since 5bc7702, apart from the additive `ProjParams::recommended()`,
    which map-sim does not call.
  - Output: `results/5bc7702/e1-sybil-sweep.csv`, alongside A8/A9.
    `env.json` records the actual commit.
- *Reported, whatever the outcome:*
  - steady-state attacker share and honest success per cell, mean
    [95% CI];
  - the Sybil-to-honest-requester ratio (n_adv · s / 500);
  - the camouflage tasks the attacker buys per round, n_adv · s · 2,
    against the honest workload of about 1,000 tasks per round.
- *Reading, fixed now:*
  - W1 *degrades* at a Sybil count if its CI lies entirely above its CI
    at 20;
  - it *loses to self-only* if its CI lies entirely above W0's in the same
    cell;
  - it is *worse than random* if its CI lies entirely above α = 0.2.
- If W1 loses to self-only at any count, the layer §8.4 recommendation
  gets a stated Sybil-ratio caveat. That is a P8-style spec decision,
  recorded separately.
- *Script:* `analysis/sybil_sweep.py` writes `results/5bc7702/SYBIL-SWEEP.md`.
