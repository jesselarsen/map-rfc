# Red-team review 2: paper draft at c67ad76

**Date:** 2026-09-27.
**Scope:** `paper/` as committed in c67ad76 (P9a), checked against
`eval/results/5bc7702/` (REPORT.md, MINIMAX.md, per-seed CSVs, bench),
`eval/HYPOTHESES.md`, `spec/layer.md`, the vectors, `interop/MATRIX.md` and
git history.
**Reviewer:** a fresh-context session. It had not seen the drafting session
and read `REVIEW-redteam-1.md` only as preliminary input.

**Process:** `docs/history/REVIEW-PROCESS.md`, Rule 0. This register
records findings and a *proposed* disposition only. Nothing in `paper/`,
`spec/` or `eval/` was edited, NITs included. The author rules on each
finding before any fix session starts.

**Roles and severity:** the same as review 1 (1 hostile PC, 2 protocol red
team, 3 methodology and statistics, 4 claims auditor, 5 presentation;
MAJOR / MINOR / NIT). New IDs are `R2-xx`. Where a new finding extends a
review-1 finding, both IDs are given.

---

## Part A: review-1 findings, re-checked

Every review-1 finding still holds on independent reading, and I would
rule none of them REJECT. Four need amending:

| ID | Re-check | Change to the proposal |
|---|---|---|
| RT-01 | Confirmed, and it goes further. C-6's default c = 8 was picked with κ ∈ {0.01, 0.1} Parts per signature, which is a CPU-only price (126 µs / 20 ms ≈ 0.006). On the INPUT_REQUIRED fallback path a checkpoint costs a round trip, so κ ≈ 2.5–5 Parts. REPORT.md's own κ = 1 column already moves argmin c to 32–128 in 8 of 9 settings | The fix must reach the **C-6 default**, not only the overhead wording: give a per-path c, or justify c = 8 on exposure grounds |
| RT-05 | Confirmed, and it goes further. A requester-only negative receipt needs no evidence that any interaction happened. The prereg's slander Sybils file 10 per round against providers they never used. TaskID mixes in the provider-chosen `a2aTaskId`, and that value can be invented when the provider never signs | Add to the DOCUMENT text: negative evidence is attributable to an issuer but is **not bound to a real task** |
| RT-08 | Confirmed, and it goes further. The "28 settings" are α ∈ {0.05..0.3}; the prereg excluded α = 0.5 from H1. At α = 0.5 under camouflage, W2's share is 2.2× B2's (0.550 against 0.255). So the "27 of 28" in the abstract and conclusion leaves out exactly the cell where the method loses to the baseline | The abstract should say "α ≤ 0.3" if it keeps the number |
| RT-13 | Confirmed, but it understates the problem. Superseded by **R2-03**: capturing *every* shard costs about 1.6 s of one core, not only a target shard | Merge into R2-03 |

Two more points on review-1 findings, with evidence:
- **RT-12** (no common random numbers) is confirmed by the E2 CSV. The
  restart arm was re-run at every c sweep point with an identical
  configuration. At L = 10³, λL = 1, f = 0 its mean waste is 845, 771,
  705, 655 and 769 across those points. That spread is independent-stream
  noise.
- **RT-02:** switching to W1 is not uniformly worse. Point estimates only:
  W1/B2 under camouflage at α = 0.3 is 0.072/0.165 = 0.44, which passes,
  where W2 fails (0.69). Against B1 under on-off, W1 is worse. The W1
  table will show both.

**Review 1's "Not findings" section is partly wrong.** It says every E1–E4
figure matches REPORT.md/MINIMAX.md. Three do not; see R2-07 and R2-11.

---

## Part B: new findings

| ID | Sev | Role | Location | Finding (one line) | Proposed |
|---|---|---|---|---|---|
| R2-01 | MAJOR | 3,4 | eval E1 "ablations confirm both admissibility rules"; intro; abstract | The A-unilateral ablation shows **no effect in any cell**. R-4, the paper's headline mechanism, has no measured effect in E1, yet the text says the ablation confirms it | ACCEPT-EDITORIAL (+ optional isolating run) |
| R2-02 | MAJOR | 1,3,4 | eval E1 Arms/Results | B2L, Wang et al.'s own prose design, is run but never reported. It **beats Ours-L** under whitewash, slander and on-off at every α. Ours-L+C, in turn, ties or beats B2L everywhere, which is a stronger and fairer result than the one the paper reports | ACCEPT |
| R2-03 | MAJOR | 2,3 | E1 world; E3a/E3b; security (reviewer findings); spec §11.5 | E1 assumes **honest discovery**; the prereg says so, the paper does not. E3 shows that capturing *all 256* shards costs about 1.6 s of one core. The paper's claim that a captured shard is routed "only through exploration" assumes honest candidates remain in the shortlist. The composition is never evaluated | ACCEPT (state it + composed run) or DOCUMENT prominently |
| R2-04 | MAJOR | 2,3 | threat model; E1; security G2; Fig. 4 caption | Identities are "free", yet E1 fixes 20 Sybils per adversarial provider. Under W1, aggregate Sybil weight grows linearly with the number of camouflaging Sybils, because the cap is per issuer. "W1 never does worse than random" and the minimax win hold only at this ratio. The attack's cost in real tasks, which is the paper's whole defence against camouflage, is never reported | ACCEPT (Sybil-count sweep + cost) or DOCUMENT |
| R2-05 | MAJOR | 1 | related work; security G2 | The closest prior art is missing. Credence (NSDI'06) weights peers by vote correlation and adds flow-based trust, the direct analogue of W1 and W2. PeerTrust's PSM is similarity-weighted credibility. DSybil (S&P'09) bounds loss against unlimited Sybils using the observer's own feedback. TrustChain/NetFlow combines bilateral interaction records with subjective Sybil-resistant trust | ACCEPT |
| R2-06 | MAJOR | 3,4 | eval (whole); limitations | The paper drops things the pre-registration says the paper will report. The most important is the byte overhead of checkpoints per Part, which the prereg says "the paper states … honestly" and which is unfavourable. Also missing: B1's zero-feedback-cost limitation, the time-to-detect and all-rounds metrics, and the 16-core E3b run. Meanwhile the paper says it reports everything | ACCEPT-EDITORIAL |
| R2-07 | MAJOR | 4 | eval H5; impl benchmarks; spec layer §7.5 C-6; spec/CHANGELOG | "0.086%" and "0.69%" overhead, and "1.4 µs" per transcript append, come from the **superseded run 9a4a968**. The declared source run 5bc7702 gives 0.0794%, 0.6306% and 1.33 µs. This falsifies the README's provenance statement, and the stale figures are also in the spec | ACCEPT-EDITORIAL + spec |
| R2-08 | MAJOR | 1,2,3 | intro Q3; design §checkpoints; E2; abstract "0.42%" | Resuming from an output prefix assumes all task state is in the output Parts. An agent with tool calls, hidden reasoning or external side effects cannot be continued that way, and a resumed task may repeat side effects. E2's waste also leaves out what the successor spends to fetch, verify and ingest (prefill) the prefix. That cost is O(prefix), not O(c), and at L = 10³ it dwarfs 3.2 Parts | ACCEPT-EDITORIAL (+ DOCUMENT scope) |
| R2-09 | MAJOR | 1,4 | limitations "One author"; front matter; design T-2 anecdote | 81 of 90 commits carry `Co-Authored-By: Claude`, and review 1 was produced by the model that drafted the paper. The paper has no generative-AI disclosure and says "the same person wrote MAP, both implementations, the baselines and the simulator". Also, "an external review" (T-2 and §security) does not say who or what reviewed | ACCEPT |
| R2-10 | MAJOR | 1 | intro Contributions item 4; conclusion | The "three negative results that we believe generalize" are all known, and the paper itself calls one of them "the classic weakness". A PC will read contribution 4 as padding | ACCEPT-EDITORIAL |
| R2-11 | MINOR | 4 | eval E2 bullets | Provider-only "53.0 Parts overbilled, 0.45 integrity failures" is measured at **λL = 3**, but it sits under the H2 bullet whose operating point is λL = 1; at λL = 1 overbilling is 22–31 Parts. The restart figure 768.6 is not in REPORT.md (it is the c = 8 row of the CSV) | ACCEPT-EDITORIAL |
| R2-12 | MINOR | 3 | eval E3c | Under attack, single-path lookup success *rises* with churn (N = 10⁵, f = 0.3: 0.755 at 0%, 0.906 at 10%, 0.988 at 30%). That is counter-intuitive and unexplained, and suggests a churn-model artefact. The paper quotes the 10%-churn figure as ordinary | ACCEPT (explain or fix model) |
| R2-13 | MINOR | 3,4 | eval E1 whitewash bullet | "Halving that floor is not possible" is false in general. The floor comes from ε = 0.1 exploration that includes zero-evidence identities, and spec §8.5 already recommends limiting those. B2L and Ours-L+C reach 0.002 under whitewash (extends RT-11) | ACCEPT-EDITORIAL |
| R2-14 | MINOR | 4 | related work, ERC-8004 paragraph | "That is the outcome E1 predicts for any global ledger": the ERC-8004 study measures coordinated positive reviewers (farming). E1's B1 under farming is 0.094, half of random, and does not collapse. B1 also has R-4, which ERC-8004 lacks | ACCEPT-EDITORIAL |
| R2-15 | MINOR | 1,4 | related Table 3 vs text | Table 3 lists Wang et al.'s trust as "local + federated", while the text says "we differ in … where trust is computed". The real difference is admissibility and issuer weighting, not locality | ACCEPT-EDITORIAL |
| R2-16 | MINOR | 2 | background; design §discovery; E3d/H4 | §2 criticises the baseline for ranking on self-reported hints, then the volatile plane ranks on a self-estimated StatusReport (spec §11.4). H4 assumes honest reports | ACCEPT-EDITORIAL |
| R2-17 | MINOR | 3 | eval F1 | F1-b P1 ("adds nothing") is tested by "the CI contains 0": absence of evidence, not an equivalence test. At α = 0, **both** F1-a CIs exclude the published value, a systematic shortfall of about 2 points hidden by the ±0.05 tolerance | ACCEPT-EDITORIAL |
| R2-18 | MINOR | 5 | eval E5 formal | The ProVerif result concerns the HELLO/channel-binding handshake of `/map/status`, a v1 mechanism the paper never describes. `binding-libp2p` DHT-14 says it does not protect A2A. It is evidence for none of G1–G5 | ACCEPT-EDITORIAL |
| R2-19 | MINOR | 4 | eval "Reproducibility" | "We ran the whole evaluation twice", followed directly by "the main trust grid ran only at the second commit", and A8/A9 ran once | ACCEPT-EDITORIAL |
| R2-20 | MINOR | 5 | eval.tex:336 "A4 in §security" | "A4" here means *reviewer finding* A4 (grinding, spec §11.5). The paper uses A1–A9 for amendments, where A4 is the D5 repair, and the §security list carries no labels | ACCEPT-EDITORIAL |
| R2-21 | MINOR | 2 | security G3 | Under C-4 an older checkpoint cannot carry higher usage, so "C-3/C-4 stop splicing an older, higher-usage checkpoint" argues a non-threat. The live threats go unargued: a requester presenting a stale checkpoint to underpay, or a provider forking the chain. G3 also implies a requester may sign a wider zone, while Z-3 says a zone "never widens" | ACCEPT-EDITORIAL |
| R2-22 | MINOR | 4 | abstract; contributions item 2 | "Interoperate … with an unmodified A2A SDK client": the SDK cell verifies the card and completes a *plain* task. It exercises no MAP artifact. That is G5 compatibility, not MAP interop | ACCEPT-EDITORIAL |
| R2-23 | MINOR | 4 | security "Reviewer findings" | "Receipt farming … Fixed." With camouflage, W1 still yields 0.200 at α = 0.5 and 0.072 at α = 0.3. Say "mitigated; camouflage remains" | ACCEPT-EDITORIAL |
| R2-24 | MINOR | 1,2 | intro Q1/Q3; design C-2 | A countersignature checks **integrity** (received = committed), not correctness. The intro's "did the work happen as claimed?" and "a transcript nobody verified" imply more. A garbage stream is billable up to the last checkpoint | ACCEPT-EDITORIAL |
| R2-25 | MINOR | 1,3 | E2; related work | E2's c trade-off is Young's first-order optimum (c* ≈ √(2κ/λ); Young 1974, Daly 2006), and nothing on checkpoint-interval theory is cited. Per-checkpoint countersigning is also gradual fair exchange, where c − 1 is the classic exposure bound (Asokan et al.) | ACCEPT-EDITORIAL |
| R2-26 | NIT | 4 | impl §Rust | "A 2,535-line A2A adapter" includes `ollama-provider` (363 lines, v1); the `a2a` adapter alone is 2,172 lines. The Rust count leaves out conformance and adapters, while Go's 9,274 includes its conformance suite (878) and `cmd` (614), so the two are not comparable | fix |
| R2-27 | NIT | 4 | eval L1 bullet | "Up to 0.217 at α = 0.5": the maximum is 0.220 (on-off) | fix |

---

## Part C: details and evidence

**R2-01 (R-4 unmeasured).** REPORT.md, all-arms table: in every one of the
35 attack cells, `a_unilateral` equals `ours` to within 0.003. Examples:
0.025/0.025 at α = 0.2 farming, 0.116/0.114 at α = 0.3 camouflage, and
0.551/0.550 at α = 0.5 camouflage.

The mechanism is in the E1 world. The only unilateral positive receipts
come from adversarial providers acting as issuers, and T-3 already gives
those issuers weight 0. So in this world R-4 is redundant given T-3. That is
a fine result, but the text says the opposite ("confirm both admissibility
rules"). R-4's value is argued (G1) and not measured.

Options:
- say so plainly;
- or add an arm with T-3 off and R-4 off (B1 + A-unilateral) to isolate
  what R-4 contributes on its own.

**R2-02 (B2L).** Steady-state share at α = 0.2, from the REPORT all-arms
table:

| attack | B2L | Ours-L | Ours-L+C |
|---|---|---|---|
| whitewash | 0.002 | 0.025 | 0.002 |
| slander | 0.002 | 0.025 | 0.002 |
| on-off | 0.018 | 0.033 | 0.002 |
| farming | 0.123 | 0.025 | 0.002 |

The pattern is the same at every α.

H1 as pre-registered compares a no-canary MAP with a canary that has no
memory. The comparison a reader wants, "Wang's full design against MAP's
full design", is B2L against Ours-L+C. MAP wins it in farming,
camouflage, collusion, combined and on-off, and ties in whitewash and
slander. It should be reported as exploratory, since it was not a
hypothesis. Leaving B2L out is currently worse for the paper than
including it.

**R2-03 (discovery × trust).** HYPOTHESES.md E1: "Discovery is honest in E1,
so this isolates trust." The paper's E1 World paragraph leaves this out.

The cost of capture:
- a shard holds about n/256 honest providers (390 at n = 10⁵);
- every key pair lands in *some* shard, so one core adds about 238
  identities per second to **every** shard at once;
- all 256 shards reach an attacker majority after
  256 × 390 / 60,901 ≈ 1.6 s.

Consequences:
- H3's "share equals a" has no force, because a is whatever the attacker
  wants.
- A new requester's shortlist (m = 10, drawn from r = 3 shards) is then
  almost entirely zero-evidence Sybils. Ranking cannot pick an honest
  provider it was never shown.
- Spec §11.5 admits "Capture can hide honest providers, an availability
  loss". The paper does not.

Proposed:
- state the assumption in E1;
- run a composed exploratory case: shortlists drawn from discovery with
  flooding at x identities per honest provider, for new and established
  requesters;
- or DOCUMENT this as the main open problem, above grinding.

**R2-04 (Sybil count and cost).** The prereg fixes 20 Sybil requesters per
adversarial provider: 800 against 500 honest at α = 0.2, and 2,000 against
500 at α = 0.5. A8 itself attributes W2's collapse to that 4:1 ratio.

Under W1 each camouflaging Sybil gets roughly the weight of an honest
peer, and the per-issuer cap of 50 limits each issuer, not the number of
issuers. So W1 plausibly collapses at a higher ratio. The claim that "W1
bounds what that buys to each observer's direct agreement" is true per
Sybil, not in aggregate.

The paper's economic defence is to "make evidence cost real tasks", but it
never reports that cost: 2 real tasks per Sybil per round, which comes to
about 1,600 purchased tasks per round at α = 0.2.

Proposed: an exploratory sweep of Sybils per provider ∈ {5, 20, 80} under
camouflage for W1 at α = 0.2, reporting the attacker share against the
tasks purchased.

**R2-05 (prior art), sources checked:**
- Walsh & Sirer, *Experience with an Object Reputation System for P2P
  Filesharing* (Credence), NSDI 2006. It compares peers' voting histories
  with a correlation coefficient and adds a flow-based trust computation.
- Yu, Shi, Kaminsky, Gibbons & Xiao, *DSybil: Optimal Sybil-Resistance for
  Recommendation Systems*, IEEE S&P 2009. It gives provable, optimal
  guarantees against unlimited Sybils.
- Otte, de Vos & Pouwelse, *TrustChain: A Sybil-resistant scalable
  blockchain*, FGCS 107 (2020; online 2017). It keeps per-agent chains of
  interaction records and ranks with NetFlow.
- Xiong & Liu, *PeerTrust*, IEEE TKDE 2004. It weights feedback by a
  personalized similarity measure.

The security section says that "no evidence-based scheme can tell them
from honest raters without an external cost". It has to engage DSybil,
which bounds the *loss* rather than distinguishing Sybils. That framing
may be the better one for MAP.

This finding is distinct from RT-21 (attestation alternatives). The
title's "Attestation" will also make readers expect TEE attestation;
answer that under RT-21.

**R2-06 (prereg promises):** quotes are from HYPOTHESES.md.
- E4: "Reported without hypothesis: byte overhead relative to Part
  payload … The paper states this honestly." E2 gives 38,486 checkpoint
  bytes per 1,000-Part task at c = 8, about 38 B per Part. Base64 in A2A
  metadata adds another third. For token-sized Parts that is several
  times the payload.
- B1: "a Sybil cost we set to zero; this is stated as a limitation." The
  paper never states it.
- The E1 metrics "time to detect" and "attacker share … over all rounds"
  are pre-registered, and neither REPORT.md nor the paper reports them.
- A5: "the paper reports both" (1 core and 16 cores). The paper reports
  only 1 core.
- The E3d message-rate crossover is RT-14.

**R2-07 (stale run):**

| Figure | 9a4a968 | 5bc7702 (declared source) | Where the stale value appears |
|---|---|---|---|
| Overhead at c = 8 | 0.0864% | 0.0794% | paper eval:377; `spec/layer.md` ~l.725; `spec/CHANGELOG.md` l.90 |
| Overhead at c = 1 | ≈ 0.69% | 0.6306% | paper eval:378; `spec/layer.md` l.727 |
| Transcript append | 1.378 µs | 1.33 µs (median 1.35) | paper impl:83 |
| Checkpoint cycle | 137 µs | 126 µs | — (the paper's 126 is correct) |

The receipt figure (127 µs), key generation (16.4 µs) and the checkpoint
cycle are correct. The stale values err conservative, but `paper/README.md`
states that every evaluation number comes from 5bc7702.

**R2-08 (resumption scope).** The design's resumption contract is "the
transcript prefix". Examples that do not fit:
- a coding agent with a workspace;
- a research agent with a browsing history;
- any provider that made an external call (payment, write, e-mail): a
  successor cannot see it, and may repeat it.

Say which task class MAP resumption targets (streamed generation whose
state is its output), and put the rest in Limitations.

E2 should also report the *successor's* prefix cost. At L = 10³ with
failure midway, ingesting about 500 Parts is 1–2 orders of magnitude above
the 3.2 Parts of waste, even at a prefill discount. It is still far below
restart's 768.6, so H2 survives, but "0.42%" does not.

**R2-09 (AI involvement).** From `git log`: 81 of 90 commits carry
`Co-Authored-By: Claude Opus 5.5`. ACM's authorship policy requires the use
of generative-AI tools to be disclosed in the work, and arXiv holds authors
responsible for, and expects disclosure of, significant use; the author
should check the current wording of both.

The "One author" threat to validity also changes shape. The same model
lineage implemented the system, the baselines and the simulator, and
red-teamed the result. That correlated-error risk is exactly what
pre-registration is meant to offset, and a reader should be told about it.

**R2-10 (negative results are known):**
1. A global ledger collapses under free Sybil slander. This follows from
   the threat model: 800–2,000 free unilateral issuers against 500 honest
   ones. B1 does *not* collapse under farming.
2. Propagation fails once Sybils buy attack edges. The paper itself calls
   this "the classic weakness", citing SybilGuard and Cheng–Friedman.
3. Grinding with free identities follows from Douceur (2002) and is why
   S/Kademlia uses crypto puzzles.

Recast them as "measured confirmations in the agent setting", and move the
genuinely new result into item 4: minimax selection reversing the
pre-registered choice, which is a methodological point about
pre-registering *where* to test.

**R2-12 (churn).** REPORT E3c, 1-path success at f = 0.3:

| N | churn 0% | 10% | 30% |
|---|---|---|---|
| 10⁴ | 0.963 | 0.966 | 0.994 |
| 10⁵ | 0.755 | 0.906 | 0.988 |

Likely causes: churn takes malicious routers offline too, so the
effective f falls; or churn forces table refreshes that sample honest
nodes. Check `e3c.rs` before quoting any churn number.

---

## Part D: suggested order (REVIEW-PROCESS Stage 3)

1. **Spec:** R2-07 (C-6 rationale numbers), plus RT-01 extended to the C-6
   default itself.
2. **Analysis from existing CSVs, no new runs:**
   - R2-02 (B2L, and Ours-L+C against B2L);
   - R2-06 (byte overhead, time to detect, 16-core);
   - R2-11;
   - RT-02.
3. **New exploratory runs,** each labelled as such. Ranked by what a PC
   would ask first:
   - R2-03 (composed discovery and trust);
   - R2-04 (Sybil count);
   - RT-04 (targeted slander);
   - RT-03 (partial visibility);
   - R2-12 (churn check);
   - R2-01 optional (isolate R-4).
4. **Framing and related work:** R2-05, R2-10, R2-08, R2-09, RT-07, RT-08,
   RT-21, RT-24.
5. **Editorial:** the remaining MINORs and NITs.

**If the author can do only three things before posting:** R2-09
(disclosure), R2-05 (prior art) and R2-02/R2-01 (report B2L, correct the
ablation claim). Those are what a reviewer would reject on, and none needs
a new run.
