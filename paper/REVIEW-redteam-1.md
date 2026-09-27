# Red-team review 1: paper draft at c67ad76

**Date:** 2026-09-27.
**Scope:** `paper/` as committed in c67ad76 (P9a).
**Reviewer:** the same model that drafted the paper.

That reviewer is the most anchored one possible, so treat an empty area as
"not looked at hard enough", not as "clean". A fresh-context reviewer,
someone who has not seen the drafting session, is still worth running.

**Process:** `docs/history/REVIEW-PROCESS.md`. Under **Rule 0** this
register records findings and a *proposed* disposition only. The author
rules on each finding (ACCEPT / ACCEPT-EDITORIAL / DOCUMENT / DEFER /
REJECT), and fixes start only after that.

**Roles used:**
1. hostile program-committee reviewer (novelty and framing);
2. protocol red team;
3. methodology and statistics;
4. claims auditor (every number against its source);
5. presentation.

**Severity:**
- **MAJOR:** a competent reviewer would reject on it, or the text states
  something false.
- **MINOR:** weakens a claim, or is imprecise.
- **NIT:** mechanical.

Several findings also apply to `spec/layer.md`; each one says so.

## Register

| ID | Sev | Role | Location | Finding (one line) | Proposed |
|---|---|---|---|---|---|
| RT-01 | MAJOR | 2,3 | eval §E4/H5; design C-6; spec C-6 | The "<1% overhead" counts CPU signing only, not the countersignature round trip. The **mandatory** A2A fallback pauses the task in INPUT_REQUIRED at every checkpoint | ACCEPT + spec |
| RT-02 | MAJOR | 1,3 | abstract; Fig. 3; H1 | Every headline trust number is for W2, which the paper no longer recommends. W1's results are not shown | ACCEPT |
| RT-03 | MAJOR | 2,3 | eval E1 "World"; related (SCITT para); spec §14.2 | E1 assumes one public receipt pool (called "the worst case"). The design never says how an observer obtains third-party receipts, and elsewhere says receipts are disclosed at the holder's discretion | ACCEPT (state + sensitivity) or DOCUMENT |
| RT-04 | MAJOR | 2 | threat model; E1 attack set | The strongest attack on W1 is not evaluated: **camouflaged targeted slander** | ACCEPT (exploratory run) or DOCUMENT |
| RT-05 | MAJOR | 2 | threat model; security; E2 | Dishonest *requesters* are not modelled: withheld countersignatures, free-riding up to c Parts per task, genuine-customer slander. E2's `unpaid_delivered_parts` is unreported | ACCEPT |
| RT-06 | MAJOR | 3,4 | design "Billing bound"; eval E2; spec §7.5 | "Overbilling bounded by c" is misstated: under C-5 overbilling is 0. The simulator defines billed = acknowledged, so the "asserted, no violations" claim is true by construction | ACCEPT-EDITORIAL + spec |
| RT-07 | MAJOR | 1,3 | eval E2; abstract | H2 measures *checkpointing*, not *countersigning*: provider-only resume also wastes ~0. Countersigning's value is safety, and that result follows by construction from the adversary model | ACCEPT-EDITORIAL |
| RT-08 | MAJOR | 1,3 | abstract | Selective reporting. The abstract quotes the favourable half of H1 (against memoryless B2, 27/28), not the pre-registered verdict (PARTIAL 18/28) | ACCEPT-EDITORIAL |
| RT-09 | MAJOR | 1,4 | abstract | "trust layers … use single-signer receipts" contradicts §Related Work (Signet co-signs) | ACCEPT-EDITORIAL |
| RT-10 | MINOR | 3 | E1 world | No sensitivity to adversary quality: q = 0.2 against honest q ≥ 0.8 is easy to separate. Subtle degraders are untested | DOCUMENT or sweep |
| RT-11 | MINOR | 3 | E1 results | Ours-L's steady-state share (about 0.125·α) sits at the ε = 0.1 exploration-plus-fallback floor, so ratios against B1 are largely a function of ε | DOCUMENT (+ ε sweep) |
| RT-12 | MINOR | 3 | eval "Statistics" | "Seeds are resampled jointly for paired arms" is nominal. Arms are separate sweep points with independent RNG streams, so there are no common random numbers | ACCEPT-EDITORIAL |
| RT-13 | MINOR | 1,3 | E3b; abstract | E3b is arithmetic dressed as simulation, and the *harm* of shard capture is not measured. "Unmitigated" appears in the abstract without an impact figure | ACCEPT-EDITORIAL (+ optional impact run) |
| RT-14 | MINOR | 3 | E3d/H4 | Pull costs 20 msgs/query against 0.02, a factor of 1000. The pre-registered message-rate crossover is not reported | ACCEPT-EDITORIAL |
| RT-15 | MINOR | 3 | eval | CIs about 0.001 wide make "holds" a sign test. Several verdicts turn on small margins (W1 camouflage α = 0.3: 0.53 against a bar of 0.5); effect sizes should lead | ACCEPT-EDITORIAL |
| RT-16 | MINOR | 4 | impl §Go | "it found real ambiguities in the specification": unsupported, with no example given | give examples or cut |
| RT-17 | MINOR | 4 | impl §Interop | "Seventeen live test vectors pin every artifact" overclaims: StatusQuery/StatusReport and voucher digests are not vectored | ACCEPT-EDITORIAL |
| RT-18 | MINOR | 4 | impl | "A receipt is 327 bytes" is TV-002, which has no keys 12/13. A v2 receipt with zone/resumed-from is larger | ACCEPT-EDITORIAL |
| RT-19 | MINOR | 2 | threat G4 | "Neither side is exposed beyond one interval" is false for the requester: its exposure is acknowledged usage plus credit it *chooses* to extend (M-6) | ACCEPT-EDITORIAL |
| RT-20 | MINOR | 2 | security G2 | "nothing, including an implementation bug … can raise a score" overclaims. T-1 makes a divergence detectable; it cannot prevent a bug | ACCEPT-EDITORIAL |
| RT-21 | MINOR | 1 | related work | Execution-attestation alternatives are missing: TEE remote attestation, verifiable computation/zkML, redundant execution (all named by ERC-8004's validation registry), plus classic accountability primitives (TrInc, A2M, AVM). "Why receipts, not attestation?" will be asked | ACCEPT |
| RT-22 | MINOR | 4 | Table 3 | "---" is defined as "out of scope" but used under Evaluation to mean "none reported" | ACCEPT-EDITORIAL |
| RT-23 | MINOR | 4 | E3c | "replicates the S/Kademlia result" overstates; the replication is qualitative | ACCEPT-EDITORIAL |
| RT-24 | MINOR | 1 | intro; related | The Signet comparison is the crux of novelty but is one sentence. It should say exactly what MAP adds over BilateralReceipt | ACCEPT |
| RT-25 | MINOR | 3 | abstract | "wastes 0.42% of the work restarting does" generalizes one operating point (L = 10³, λL = 1, c = 8) | ACCEPT-EDITORIAL |
| RT-26 | MINOR | 3 | E4; C-6 | Part and token are conflated (E4: 1 Part = 1 token). A2A Parts can be large, which changes what c = 8 means and costs | DOCUMENT |
| RT-29 | MINOR | 1,3 | eval "Pre-registration" | The paper says hypotheses were tagged "*before* writing the simulator", but git only proves commit order. `prereg-v1` (2185eac, 11:22) has no simulator files, yet the first simulator commit (65aa368, a large engine plus F1/E2) lands 20 min later. Say "before committing", or cite other evidence (session logs) | ACCEPT-EDITORIAL |
| RT-27 | NIT | 5 | main.tex | `\usepackage{multirow}` and `\hyp` are unused | fix (Rule 0 exception) |
| RT-28 | NIT | 5 | intro l.33–35 | "Their system uses … a local plus federated reputation ledger" vs §2: the ledger is prose-only. Say "proposes" | fix (Rule 0 exception) |

## Details and evidence

**RT-01 (overhead).** H5-b is computed as (signature operations ×
criterion cost) / (L × 20 ms/token); see `eval/analysis/analyze.py:344` and
`map-rs/sim/src/scenarios/e4.rs:24`. That formula has no network term. The
binding's A2A-21 fallback is MUST-support, and it pauses the task in
INPUT_REQUIRED for one requester round trip per checkpoint. At c = 8 tokens ×
20 ms = 160 ms of generation per checkpoint, a 50–100 ms round trip adds
roughly 30–60% wall time on the fallback path. The primary path (A2A-20) is
asynchronous and adds no pause. The C-6 rationale in the spec quotes the same
0.086%.

Proposed fix:
- call the figure "CPU overhead";
- add an RTT-based estimate for both paths;
- recommend the async path, or a larger c when only the fallback is
  available.

**RT-02 (W1 not shown).** The data already exists: W1 across the α × attack
grid is in `e1-minimax.csv`, and B1/B2 are in `e1-main.csv`. A point-estimate
check with no CIs, done for sizing only, gives W1 19/28 against W2 18/28 on
the H1 criterion. W1 is **worse against B1 under on-off** (ratio about 1.9,
against 1.4 for W2), and narrowly fails camouflage at α = 0.3 (0.53). The
arms are not paired (different RNG streams), so any W1 table must be labelled
exploratory, with unpaired CIs.

**RT-03 (receipt dissemination).** W1 and W2 beat W0 (0.031 against 0.080 at
α = 0.2) only because each observer reads other honest issuers' receipts. The
"one public pool" is the worst case for *publication flooding* but the best
case for *evidence availability*. The pre-registration's parenthesis
("adversaries publish without limit") was dropped in the paper.

The design specifies no gossip or dissemination mechanism. §Related and spec
§14.2 say disclosure is the holder's choice. Publishing receipts also leaks
the delegation graph, which is a privacy cost.

Proposed:
- state the assumption plainly;
- add a partial-visibility sensitivity run (each observer sees a fraction of
  third-party receipts), or DOCUMENT this as the main external-validity
  limit.

**RT-04 (targeted slander).** Under W1 an issuer's weight is its agreement
averaged over the providers it shares with the observer. A camouflaging
Sybil that reports truthfully on 30 providers and lies about 2 targets keeps
nearly full weight: the MAD dilution means lies cost almost nothing. That
weight then lands on the targets. The E1 "combined" attack is
farming + slander + selective countersigning, *without* camouflage, so this
attack was never run.

**RT-05 (dishonest requesters).** Four gaps:
- A requester can stop countersigning and still receive up to c − 1
  further Parts per checkpoint window. At task end it can refuse the
  receipt, which denies the provider positive evidence.
- MAP has no provider-side trust in requesters.
- A genuine, non-Sybil customer's unilateral negative receipt is exactly
  the "can dispute but cannot disprove" claim that the intro criticises in
  provider-signed receipts.
- E2 already records `unpaid_delivered_parts`, but the paper does not
  report it.

**RT-06 (billing bound).** `e2.rs:187-199`: `billed = provider_acked - start`,
which *is* C-5. The check `billed > delivered` therefore cannot fire. The
check `unpaid_here >= c` tests the simulator's own checkpoint schedule. So
"asserted in every run, no violation" is an invariant check, not evidence.
The quantity bounded by c is the provider's *unpaid* work, not overbilling.
Spec §7.5 uses the same "bounds overbilling" wording. **Spec finding.**

**RT-07 (what H2 measures).** Restart is the wrong comparison for MAP's
contribution. Provider-only resume wastes about 0 when everyone is honest. The
contribution of countersigning is that dishonest providers cannot overbill
(53 Parts per task) or corrupt the transcript (0.45 failures per task). That
follows from the adversary model by construction; it illustrates the
property rather than measuring it.

**RT-08/09 (abstract).** The abstract should state H1 = PARTIAL 18/28, name
the weighting, and drop "single-signer" (or say "mostly single-signer").

**RT-11 (floor effect).** At ε = 0.1 uniform exploration within a 10-provider
shortlist, plus fallback, Ours-L's steady-state share tracks about 0.12–0.13 α
in most non-camouflage cells (for example 0.025 at α = 0.2 and 0.080 at
α = 0.5). The paper explains the whitewash failure by this floor but not the
wins.

**RT-13 (E3b).** 60,901 attempts/s ÷ 256 ≈ 238 per second into any shard,
against n/256 honest nodes, is a one-line calculation. The paper should say
so, and optionally measure the candidate share a querier sees with
r = 3 random shards when one shard (or all 256) is captured.

## Not findings (checked, no issue)

- Every bibliography entry matches its primary source; checked while
  drafting.
- Every E1–E4 figure quoted in the paper matches `REPORT.md`/`MINIMAX.md`:
  spot-checked about 40 numbers, including all table cells.
- Byte-identical reruns, *commit* ordering of the pre-registration (the
  tag has 0 simulator files and is an ancestor of the first simulator
  commit; but see RT-29), and A9 pushed before its run: all verified
  against git.
- The formal-model claim is scoped correctly: discovery-plane anti-relay
  only, with non-repudiation stated as unmodelled.

## Suggested fix order (per REVIEW-PROCESS Stage 3, largest blast radius first)

1. **Spec wording.** RT-06 (§7.5) and RT-01 (C-6 rationale).
2. **New or extra analysis, each labelled exploratory:**
   - RT-02 (W1 table and figure from existing CSVs);
   - RT-03 (partial-visibility run);
   - RT-04 (targeted slander);
   - RT-05 (report unpaid Parts; model a withholding requester);
   - optionally RT-10, RT-11 and RT-13.
3. **Framing.** RT-07, RT-08, RT-09, RT-24, RT-21, RT-15.
4. **Editorial.** The remaining MINORs and NITs.
