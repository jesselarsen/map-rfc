# Issue (design proposal): countersigned checkpoints and resumption on another worker

**Labels to suggest:** spec, design

> This is the largest of the proposals. File it as a design discussion and
> don't write code unless the maintainer wants the feature.

---

## Problem

An INAM receipt records one finished interaction. That fits
request/response work, and §15 says plainly that a streamed response
"cannot [be hashed] until the stream ends, so this binding fits
request/response calls, not long-lived streams". For long or streamed
jobs (generation, multi-step agent work), three things go unrecorded:

1. **Partial delivery.** If the worker fails halfway, there is no signed
   record of how much it delivered, so `partial` has nothing to point to.
2. **Billing on failure.** Nothing bounds what a failed worker may claim
   for the part it did.
3. **Handover.** Another worker can't continue from a state the requester
   has verified. It starts again, or it trusts the failed worker's own
   account.

## Proposed change (sketch)

A **Checkpoint**: a small JSON document signed by both parties, in the
same style as a receipt, issued every c parts during a job. It is held by
the parties and doesn't have to go to the registry.

```json
{
  "checkpointVersion": "1.0",
  "checkpointId": "sha256:<hex of canonical content>",
  "jobId": "job_...",
  "agentA": "did:key:...",
  "agentB": "did:key:...",
  "throughSeq": 63,
  "transcriptRoot": "sha256:<hex>",
  "usage": { "tokensOut": 4096 },
  "createdAt": "...",
  "resumedFrom": "sha256:<checkpointId>",
  "signatures": { "agentB": "...", "agentA": "..." }
}
```

`resumedFrom` is optional and names the predecessor's checkpoint when the
job was handed over.

Rules, adapted from MAP's layer §7 (`<MAP_REPO_URL>/spec/layer.md`):

- **Transcript root.** The parts delivered so far are the leaves of an
  RFC 6962 tree, with entry bytes `canonical({seq, part})`, computed by the
  same code as §13.2 (`sdk-js/src/core/merkleLog.ts`). No new tree code is
  needed, and the root is checkable with functions the SDKs already
  export. (MAP's own tree promotes odd nodes instead of splitting the RFC
  6962 way. INAM doesn't need to match MAP's bytes, so reuse is the better
  fit.)
- **Order.** The worker signs first. The requester countersigns only after
  recomputing `transcriptRoot` over the parts it actually received.
- **Monotone.** `throughSeq` strictly increases, and each `usage` counter
  never decreases.
- **Billing bound.** A worker that fails is owed only for the `usage` in
  its last checkpoint signed by both parties. Its receipt ends `partial`
  with that usage.
- **Resume.** The requester gives a new worker the last dual-signed
  checkpoint and the transcript prefix. The new worker recomputes the
  root and refuses on a mismatch. Its receipt carries
  `result.resumedFrom = <checkpointId>`. Placing it inside `result`
  means §4.2's list of hashed fields doesn't change.

Reputation impact: none required. A relying party can treat a `partial`
receipt backed by a dual-signed checkpoint as stronger evidence than a
bare `failed` one.

## Evidence it's worth the cost

From MAP's evaluation (simulated, reference implementation; not INAM):
- At 1 expected failure per 1,000-part task, resuming from a checkpoint
  every 8 parts wasted 3.2 parts per task, 0.42% of restarting from
  scratch.
- Signing and verifying added 0.079% CPU at one token per 20 ms.
- The cost that matters is the round trip if the worker pauses for each
  countersignature, which is why the interval should be negotiable.

## Scope limits (state these in the spec)

Resuming from a transcript is complete only when the job's state *is* its
delivered output, as with streamed generation. It doesn't carry a
worker's hidden state, and a successor may repeat external side effects
(payments, writes) unless the application makes them idempotent.

## What it breaks

New optional resources and fields; existing receipts are unchanged. It's
a wire and signature change, so the full GOVERNANCE.md process applies.
A §15-style HTTP or A2A carriage for checkpoints would be a follow-up.
MAP has an A2A carriage (`binding-a2a.md`, A2A-20/21) that could serve as
the model.
