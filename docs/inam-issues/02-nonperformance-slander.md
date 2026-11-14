# Issue: non-performance reports weighted by earned trust are open to camouflaged slander

**Labels to suggest:** reputation, threat-model

---

## Problem

THREAT-MODEL.md lists the residual risk of smearing a worker with
non-performance reports (§3.3): "The poster's trust is the only limit."
This issue adds measured evidence that this limit gives way once an
attacker can *earn* the trust that weights its reports, and proposes a
cheap test.

The mechanism: a report against a worker counts as a zero outcome
weighted by the reporting poster's own base trust, deduplicated per
poster. A fresh identity weighs about 0, as §3.3 says. But base trust
can be earned through ordinary, honest work. An attacker who runs a few
real, truthful jobs with outside agents from each of many identities
gives each identity real weight, and then spends that weight on reports
against chosen targets.

## Evidence (from a different system, so it's an analogy, not a result about INAM)

MAP ([spec](https://github.com/jesselarsen/map-rfc/blob/6cc324e/spec/layer.md)) ran a pre-registered
simulation of the same pattern: one-sided negative evidence weighted by
the complainer's earned standing. In its targeted-slander experiment
([exploratory amendment A12](https://github.com/jesselarsen/map-rfc/blob/6cc324e/eval/HYPOTHESES.md), [results](https://github.com/jesselarsen/map-rfc/blob/6cc324e/eval/results/5bc7702/TARGETED-SLANDER.md); α = 0.2, 20 Sybils per adversarial provider,
5 fixed honest targets, 20 seeds), each Sybil runs 2 truthful tasks per
round against honest non-target providers and files negatives only
against the targets. Relative harm is the drop in the targets' share of
honest requesters' tasks:

| How complaints are weighted | No camouflage | 2 truthful tasks per round |
|---|---|---|
| Unweighted global ledger | 0.93 | 0.91 |
| By agreement with the observer's own experience (W1) | -0.17 (no harm) | 0.68 |
| By agreement, propagated (W2, EigenTrust-like) | 0.03 (no harm) | 0.88 |
| Observer's own evidence only (W0) | 0.01 | 0.01 |

Weighting by earned standing stopped the naive attack completely and
stopped very little of the camouflaged one. The attack didn't win the
attackers any tasks: their own task share was unchanged. It is pure
sabotage, which is the case non-performance reports enable.

INAM differs in ways that may help, and the test should measure them,
not assume them:
- a report needs the worker's own signed offer on a real job;
- the 72-hour grace period;
- one counted report per poster per worker;
- the poster's weight is base trust (stake, volume, success), not
  agreement.

## Proposed change

1. **Test first.** Add a simulation or property test: k identities each
   earn base trust through m honest receipts with anchored outside
   counterparties, then each files one report against a target worker
   with a clean history. Report how the target's `trustScore` changes as
   k and m grow. That turns the threat-model line into a number.
2. **Depending on the result, one or more of:**
   - cap the total weight of non-performance reports against one worker
     relative to the weight of its finalized receipts (the same idea as
     the §5.2 pair cap, applied to the negative side);
   - expose `nonPerformanceReports` with the reporters' weights in
     `components`, so a relying party can discount them;
   - recommend in §5.4 that a relying party weigh reports by its *own*
     relationship with each reporter rather than by the registry's score
     (see #173, relying-party appraisal).

## What it breaks

Nothing for step 1. Step 2's cap changes how reputation is computed, so
it needs the 7-day comment period and a SPEC bump under GOVERNANCE.md.

---

*Context: this comes from MAP, the Mutual Attestation Protocol ([spec](https://github.com/jesselarsen/map-rfc/blob/6cc324e/spec/layer.md), [pre-registered evaluation](https://github.com/jesselarsen/map-rfc/blob/6cc324e/eval/HYPOTHESES.md), [paper draft](https://github.com/jesselarsen/map-rfc/tree/6cc324e/paper)), an Apache-2.0 accountability layer over A2A. I've suspended its development and won't be implementing this. I'm filing it so the design and the evidence are on record here for whoever wants them.*
