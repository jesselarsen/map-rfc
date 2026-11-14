# Issue: a relying-party appraisal (re-derive and weigh the evidence locally)

**Labels to suggest:** sdk, reputation, spec

---

## Problem

SPEC §5.4 says the registry's `trustScore` and `evidence` are a hint, and
that "a relying party that needs a finding re-derives the evidence itself
from signed records". The SDKs export the primitives for that (signature,
content-hash, and Merkle-proof checks), but nothing turns the re-derived
receipts into a decision. `decideTrust()` in `sdk-js/src/trust.ts` takes
the registry's `ReputationResult` as given. A relying party that re-derives
the evidence is left to invent its own weighting, and most will just fall
back to the registry's number.

## Proposed change

An informative SPEC section plus one SDK function in each SDK:

```ts
appraiseLocally(
  receipts: Receipt[],          // fetched, then each one checked: receiptId, both signatures
  observer: string,             // the relying party's own INAM ID
  policy?: AppraisalPolicy
): ReputationResult             // same shape as the registry's, so it feeds decideTrust()
```

with these rules, adapted from MAP's local trust projection
(`<MAP_REPO_URL>/spec/layer.md` §8):

1. **Admissibility.** Use only finalized receipts whose ID and both
   signatures verify locally. A receipt that fails a check is dropped and
   counted, never trusted.
2. **Issuer weight.** Each receipt is weighted by its *requester*, from the
   observer's point of view:
   - the observer's own receipts weigh 1;
   - a requester with no evidence relationship to the observer weighs
     **0**, not a neutral prior;
   - any other requester weighs by **agreement**: 1 − 2 × the mean
     absolute difference between its success rates and the observer's on
     workers both have at least 3 receipts with, clamped at 0, and 0 if
     they share fewer than 3 such workers.
3. **A per-requester cap** on total contribution to one worker, so one
   prolific requester can't dominate.
4. **Decay** from `result.completedAt`, never from fetch time, so the
   result is a pure function of (receipts, observer, evaluation time) and
   any two clients with the same inputs agree.

The output keeps the registry's response shape, with `evidenceLevel`
computed the same way, so existing `decideTrust()` policies work
unchanged on a locally derived result.

## Why these rules

In MAP's pre-registered evaluation (seven attacks, α from 0.05 to 0.5, 20
seeds), these rules (W1) had the best worst case of the three weightings
tested. Worst-case attacker task share: W1 0.200, self-only 0.348,
propagated agreement 0.550, against 0.500 for random selection. The
propagated, EigenTrust-like variant failed once camouflaging Sybils
outnumbered honest requesters. That matters for the full EigenTrust solve
that README lists as the next step. These numbers come from MAP's
simulator, not from INAM's formula.

Known limits, to state in the section:
- weighting by agreement can be bought with real work (camouflage);
- with sparse first-hand evidence, local appraisal is barely better
  than random;
- a new observer has no history, so it should combine this with
  `evidenceLevel` floors or an escrow decision, not use it alone.

## What it breaks

Nothing: it's a new, optional client-side function and an informative
section. No wire, signature, or registry change.

## Offer

I can write the TypeScript and Python versions with shared test vectors:
fixed receipt sets with an expected appraisal for a given observer.
