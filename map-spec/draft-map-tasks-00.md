---
title: "MAP Tasks: Negotiation, Streaming Inference, Tools, Checkpoints, and Delegation"
docname: draft-map-tasks-00
category: std
---

# MAP Task Execution and Secure Messaging

**Status:** preprint, work in progress. BCP 14 language applies (see
Requirements Language, below).

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## Abstract

Defines the task layer of MAP: offer/quote/assign negotiation (with an
optional bidding extension), streaming inference with credit-based flow
control, tool invocation gated by requester-issued capability tokens,
Merkle-committed transcripts with countersigned checkpoints, failure
recovery by transcript replay, task delegation with budget and zone
propagation, distributed memory/context retrieval, cancellation, and the
task state machines for both roles.

## 1. Model

A **task** is a unit of remote work bound to one (requester, provider)
pair, one capability, and one TaskID. Orchestration — decomposing a goal
into a DAG of tasks, merging results — is application logic; MAP sees only
tasks and (optionally) their parent/child delegation links. This keeps the
protocol small and leaves merge strategy, scoring, and scheduling as local
policy (critique C5), while giving orchestrators everything they need:
verifiable per-task evidence, deterministic error semantics, and
resumability.

TaskID = 32 random bytes chosen by the requester; it is the idempotency
key: a provider receiving ASSIGN for a known TaskID MUST NOT start a second
execution and SHOULD respond with current state.

Data ownership (normative): the requester is the source of truth for task
*terms* (constraints, grants, budget); the provider is the source of truth
for *execution state*; the countersigned transcript commitment (§8) is the
only *shared* durable fact, and every cross-party claim — completion,
usage, dispute — MUST be expressible against it. Neither party's local
view is authoritative for the other; there is no distributed transaction
between them, only signed messages, idempotent retries, and compensations
(cancel-and-pay-to-boundary, §11). Ordering guarantees are scoped, never
global: `seq` per session direction, `chunk-seq` per task.

## 2. Negotiation

```
Requester                              Provider
   | -- OFFER {task summary} ----------→ |    (may be skipped: direct ASSIGN
   | ←-------------- QUOTE {terms} ----- |     under a standing agreement)
   | -- ASSIGN {full task, terms ref} -→ |
   | ←------------------- ACCEPT ------- |    (or REJECT{code})
```

```cddl
Offer = {
  1 => TaskID,
  2 => CapURI,
  3 => TaskConstraints,
  ? 4 => PayRef,            ; pricing expectation (pay §3.1)
  ? 5 => Timestamp,         ; offer expiry
  ? 6 => ExtMap             ; negotiated extensions (e.g. ext-bid, §2.2)
}
TaskConstraints = {
  ? 1 => uint,              ; max-context-tokens (hard)
  ? 2 => uint,              ; max-output-tokens
  ? 3 => uint,              ; deadline: wall-clock ms budget (hard)
  ? 4 => uint,              ; ttft-ms guarantee requested (hard)
  ? 5 => ZoneLabel,         ; task zone label (hard; §9)
  ? 6 => uint,              ; min sandbox class (hard)
  ? 7 => Multihash,         ; required model weights digest (hard if set)
  ? 8 => ExtMap
}
Quote = { 1 => TaskID, ? 2 => PayQuoteRef /*pay §3.1; absent = gratis or
          settlement out of band — payment is optional at the protocol
          layer (ARCHITECT RULING 2026-07-12, was required; ERRATA)*/,
          3 => uint /*guaranteed ttft-ms*/,
          4 => Timestamp /*expiry*/, ? 5 => ExtMap }
Accept = { }                                 ; empty content map
Reject = { 1 => uint /*reason-code, §14*/ }
```

(Accept/Reject content maps assigned 2026-07-06 from TASK-007 FR-1; the
negotiation diagram above showed them only inline.)

Fields marked *hard* MUST be satisfiable or the provider MUST REJECT
(3xxx/4xxx); accepting then violating them is receipt-recordable breach.
The quoted `ttft-ms` adopts design3's guarantee discipline: if the first
CHUNK does not arrive within guarantee × 1.1, the requester SHOULD CANCEL
(5001, retryable at other peers), fail over, and record the miss.

### 2.1 ASSIGN and payload transport

```cddl
Assign = {
  1 => TaskID, 2 => CapURI, 3 => TaskConstraints,
  4 => InputSpec, 5 => FlowInit, ? 6 => PayAgreeRef /*pay §3.1*/, ? 7 => ExtMap }
InputSpec = {
  ? 1 => bstr,              ; inline input (<= negotiated size)
  ? 2 => [+ Multihash],     ; content-addressed blocks to fetch (Zone<=1
  ? 3 => [+ ContextRef]     ;   for proprietary payloads; §9)
}
FlowInit = { 1 => uint /*msg credits*/, 2 => uint /*byte credits*/ }
```

### 2.2 Bidding extension (OPTIONAL, extension key 1 = `ext-bid`)

Where markets are wanted, OFFER MAY be broadcast to k candidates; QUOTEs
compete; the requester's selection function is local policy. `ext-bid`
(extension key 1, core §5.1) is negotiated in HELLO (core §6.1 key 3)
before use; peers that did not negotiate it MUST ignore its ExtMap entry.

The extension adds a single ExtMap entry (Offer key 6 / Quote key 5) keyed
by the extension code point:

```cddl
BidExt = {                            ; ExtMap value under ext key 1
  1 => Timestamp,                     ; bid-deadline (QUOTEs after are late)
  ? 2 => uint                         ; queue-wait-ms (advertised backlog)
}
```

`bid-deadline` (in an OFFER) bounds how long the requester collects QUOTEs;
a provider's QUOTE received after it MAY be dropped. `queue-wait-ms` (in a
QUOTE) is the provider's advertised current backlog, an input to the
requester's local selection function only.

No auction clearing rules are standardized: selection stays local policy
(they were dimensionally incoherent in the source designs — critique C5),
so this extension defines fields, never a mechanism.

Security Considerations. `queue-wait-ms` is self-reported and therefore
untrusted (critique C1): a provider can under-report backlog to win bids
("bid-sniping"). The requester's only defense is the trust projection —
persistent under-delivery against advertised wait surfaces as poorer
outcomes in dual-signed ExecutionReceipts (trust §8), so the lie is
checkable ex post and repricing is local policy. `bid-deadline` is not a
commitment device; it does not bind the provider.

## 3. Streaming: CHUNK

```cddl
Chunk = {
  1 => TaskID,
  2 => uint,                ; chunk-seq, from 0, dense
  3 => uint,                ; kind: 0 output-text, 1 output-bytes,
                            ;  2 log, 3 usage-update, 4 tool-io
  4 => bstr,                ; content
  ? 5 => bool               ; fin — last chunk of this kind
}
```

Chunks flow under FLOW_CREDIT (core §8.1). `usage-update` chunks carry
running metered usage `{tokens-in, tokens-out, ms}` enabling pay-as-you-go
settlement (pay §4). Ordering is per `chunk-seq`; the transcript (§8.1) is
defined over output-kind chunks in seq order, making audits unambiguous.

## 4. COMPLETE and output commitment

```cddl
Complete = {
  1 => TaskID, 2 => uint /*outcome: 0 ok, 1 partial, 2 failed*/,
  3 => Multihash,           ; transcript-root (§8.1) — output commitment
  4 => { 1 => uint, 2 => uint, 3 => uint, 4 => uint },
                            ; tokens-in/out, ttft-ms, wall-ms (claimed)
  ? 5 => ExtMap }
```

The requester verifies the transcript root against received chunks; match
⇒ it SHOULD issue/countersign the RECEIPT (trust §3) and settle (pay §5).
Mismatch ⇒ 5000 + dispute path (pay §6). The signed COMPLETE is the
provider's non-repudiable claim of what it produced (TH6, TH9).

## 5. Ranking guidance (informative)

Requesters typically shortlist via stable-plane constraints, refresh with
STATUS, then rank by a local utility over: own-measured RTT/TTFT history,
evidence-based trust per capability (trust §7), quoted price, and current
queue. Weights are yours; publishing them invites gaming. Hard constraints
are protocol; utility is not.

## 6. Reservations (capability `map:cap:schedule/reservation`)

Optional lease primitive for scarce resources: a requester sends RESERVE
for a capability during a time window; the provider answers with a LEASE
(granted / rejected / revoked). A granted lease is referenced by an
ASSIGN's PayAgreeRef-adjacent flow (the lease id) and obliges the provider
as below. Providers MAY require reservations by advertising `accepting = 2`
in STATUS. Leases are soft state with expiry; no distributed scheduler is
defined (Kubernetes-style global scheduling is a non-goal — the mesh
converges via prices and queues).

Message types: RESERVE `0x020F`, LEASE `0x0210`, LEASE_RELEASE `0x0211`
(core §5).

```cddl
Reserve = {                           ; msg-type 0x020F
  1 => bstr .size 16,                 ; rsv-id (requester-chosen, unique)
  2 => CapURI,                        ; reserved capability
  3 => Timestamp,                     ; window start (from)
  4 => Timestamp,                     ; window end (until)
  ? 5 => TaskConstraints,             ; the work the lease will admit
  ? 6 => PayRef,                      ; holding-cost expectation (pay §3.1)
  ? 7 => ExtMap
}
Lease = {                             ; msg-type 0x0210
  1 => bstr .size 16,                 ; rsv-id (echoes the Reserve)
  2 => uint,                          ; status: 0 granted, 1 rejected, 2 revoked
  ? 3 => [Timestamp, Timestamp],      ; granted window [from, until]
  ? 4 => PayQuoteRef,                 ; price of the hold (pay §3.1)
  ? 5 => ExtMap
}
LeaseRelease = {                      ; msg-type 0x0211
  1 => bstr .size 16,                 ; rsv-id
  ? 2 => uint                         ; reason code (informational)
}
```

State machine (per rsv-id): **Requested** (RESERVE sent) → **Granted**
(LEASE status 0) → **Active** (during `[from, until]`) →
**Released** (LEASE_RELEASE) / **Expired** (past `until`) /
**Revoked** (LEASE status 2). A RESERVE MAY instead be answered with LEASE
status 1 (**Rejected**), a terminal state.

Normative rules:

- The rsv-id is the idempotency key: a repeated RESERVE with the same
  rsv-id and an identical window MUST be treated as a retransmission (the
  same LEASE re-sent), and a RESERVE reusing an rsv-id with a *different*
  window MUST be rejected (LEASE status 1) — silently re-scoping a live
  lease is forbidden.
- A granted LEASE's window (key 3, when present) MUST fall within the
  RESERVE's requested `[from, until]`; a grant outside it is invalid and
  the requester MUST treat it as a rejection.
- While a lease is Active, the provider MUST ACCEPT a conforming ASSIGN
  that references it (a task within the reserved capability and window and
  the RESERVE's constraints). A provider that fails to admit conforming
  work inside the window has missed the lease; the miss is
  receipt-recordable as a Failed outcome (outcome 1, trust §3) so the
  broken commitment is scored.
- A LEASE for an rsv-id whose requested window already lies entirely in
  the past MUST be status 1 (rejected).
- LEASE_RELEASE is idempotent by rsv-id and voluntary; a released or
  expired lease confers no obligation.

Overlapping reservations for the same resource are provider policy, not
protocol — a provider MAY reject (status 1) or admit them at its own risk.

Security Considerations. The abuse is **reservation squatting**: cheaply
holding scarce capacity to deny it to others. The mitigation is economic,
not protocol-structural — RESERVE carries a `PayRef` (key 6) so the
provider MAY price the hold (a non-refundable or escrowed holding cost via
the pay layer, pay §3), making squatting cost money. A provider under
sustained lease-DoS SHOULD fall back to shorter windows and higher holding
costs; both are local policy. Because a missed lease is receipt-recordable,
a provider that grants leases it cannot honor accrues negative trust
(trust §8), which bounds over-committing.

## 7. Tool invocation

Direction inversion is the crux: during execution the *provider* asks the
*requester* for permission to run a tool. Authorization is a capability
token — a DelegationCert (identity §4) issued by the requester, pinned to
this TaskID, naming allowed tool URIs and caveats:

```
Provider                              Requester
   | -- TOOL_CALL {task, tool-uri,  --→ |  policy check (allowlist, zone,
   |               args-digest}         |  sandbox class, budget)
   | ←- TOOL_GRANT {DelegationCert  --- |  caveats: task-id, tool-uri,
   |               w/ caveats} -------- |  args-digest, expiry (<=60s),
   | -- TOOL_RESULT {output, digest} -→ |  max-invocations
```

Rules: providers MUST NOT execute tools absent a valid unexpired grant
whose caveats cover the exact call (args-digest binds the arguments);
grants are single-task, non-redelegable by default; Zone-2 execution
additionally requires SecurityProfile sandbox ≥ container and the tool on
the provider's advertised allowlist (TH7, TH15). Tool I/O appears in the
transcript as kind-4 chunks — auditable end to end.

## 8. Checkpoints, transcripts, and resumption

### 8.1 Transcript commitment

`transcript-root` = the Merkle root (suite hash H) over the output/tool
chunks (kinds 0, 1, 4) in `chunk-seq` order, constructed as follows:

```
leaf(c)        = H(0x00 || enc([c.chunk-seq, c.kind, c.content]))
                 ; enc = deterministic CBOR (core §4.2) of the triple —
                 ; bare concatenation would be ambiguous across
                 ; variable-length fields
parent(l, r)   = H(0x01 || l || r)      ; domain-separated from leaves
odd node       = promoted unchanged to the next level
empty root     = H(0x00 || enc([]))     ; zero output chunks
```

Incremental, streaming-friendly, and shared by checkpoints, COMPLETE,
receipts, and disputes — one commitment structure everywhere. The 0x00/
0x01 domain separation prevents leaf/interior confusion attacks; the CBOR
leaf encoding makes leaves injective in (seq, kind, content).

### 8.2 CHECKPOINT / CHECKPOINT_ACK

```cddl
Checkpoint = {
  1 => TaskID, 2 => uint /*through chunk-seq*/,
  3 => Multihash /*transcript-root so far*/,
  ? 4 => bstr /*opaque engine state — Zone<=1 or attested only*/,
  ? 5 => { usage as in Complete.4 } }
```

Providers SHOULD checkpoint every N output tokens (default 512) or at tool
boundaries. The requester replies CHECKPOINT_ACK containing its *own
signature over (TaskID, chunk-seq, root)* — the countersignature that makes
a checkpoint a valid resumption point (critique C11: never resume from
state attested only by the party that just failed or may be malicious).

### 8.3 Resumption

On provider loss, the requester selects a successor and sends RESUME:
{TaskID, last countersigned checkpoint, the output transcript so far
(inline or content-addressed), original ASSIGN terms}. The successor
re-warms by **transcript replay** — reprocessing prompt + prior output as
context — and continues from chunk-seq+1. Determinism across heterogeneous
engines is not assumed; the seam at the resumption point is permitted and
visible in the transcript (marked by a kind-2 log chunk).

### 8.4 Opaque state / KV-cache (extension key 2 = `ext-kvx`)

Engine-state blobs (Checkpoint.4) MAY shortcut re-warm **only** between
peers running the identical engine+model+quantization, and only within
Zone ≤1 or with TEE attestation — normative resumption remains replay
(critique C10). `ext-kvx` (extension key 2, core §5.1) is HELLO-negotiated
(core §6.1 key 3) and never required for conformance.

Payload container (the only structure this revision fixes):

```cddl
KvxExt = {                            ; ExtMap value under ext key 2
  1 => uint,                          ; container version
  2 => bstr                           ; opaque engine-state blob
}
```

A receiver MUST reject a `KvxExt` outside Zone ≤1 unless the sender is
TEE-attested (zone enforcement, §9). Engine-fingerprint matching (which
engine+model+quantization tuples are interchangeable) and the blob's
internal format are **deferred**: this section reserves the code point and
the container; full semantics are future work. An implementation that does
not understand a received `KvxExt` MUST fall back to replay resumption.

## 9. Zone enforcement (normative)

The task zone label (Constraints.5) governs everything attached to the
task: ASSIGN MUST NOT go to a peer whose zone level exceeds the label
(arch Z1); InputSpec content-addressed fetches of proprietary blocks are
Zone ≤1; TOOL_GRANTs inherit the label; DELEGATE (§10) propagates it and
MAY only tighten it (Z2). Violations: 2004, non-retryable, receiptable.
The Git-workspace extension (`ext-git`, extension key 3, core §5.1, from
design3) — shallow, path-scoped, token-gated clone/push over a dedicated
stream — is defined for Zone ≤1 only and MUST be rejected in Zone ≥2.
`ext-git` is HELLO-negotiated (core §6.1 key 3). Payload container:

```cddl
GitExt = {                            ; ExtMap value under ext key 3
  1 => uint,                          ; container version
  2 => bstr                           ; opaque git-workspace parameters
}
```

A receiver MUST reject a `GitExt` in Zone ≥2 (2004, non-retryable,
receiptable). The clone/push protocol details (shallow depth, path scoping,
token gating, stream framing) are **deferred**: this reserves the code
point, the container, and the zone restriction; full semantics are future
work. Peers that did not negotiate `ext-git` MUST ignore its ExtMap entry.

## 10. Delegation of tasks

A provider MAY subcontract via DELEGATE-linked child tasks when the ASSIGN
carries caveat `delegable ≥ 1`. The child ASSIGN embeds the parent chain
(delegation depth decrements; budget caveats split; zone tightens-only).
The root requester's receipt evidence names the whole chain, so
accountability survives subcontracting. Default is non-delegable.

## 11. Cancellation

CANCEL content `{ 1 => TaskID, 2 => uint /*reason-code, §14*/ }` (field
numbers assigned 2026-07-06 from TASK-007 FR-1) from either side; provider
stops within one chunk boundary, emits a final CHECKPOINT and
COMPLETE(outcome=partial). Usage to the countersigned boundary is payable
(pay §4); this makes cancellation cheap to honor and removes the incentive to
ignore it.

## 12. Memory / context retrieval

Peers advertising MemoryTopics (discovery §2) serve:

```cddl
CtxQuery = { 1 => tstr /*domain*/, 2 => tstr /*embedding-model id*/,
             3 => [+ float16/float32 as bstr] /*query vector(s)*/,
             4 => uint /*top-k*/, ? 5 => float-as-decimal /*min-sim*/ }
CtxResult = { 1 => [+ { 1 => bstr /*text*/, 2 => int /*sim milli*/ ,
             ? 3 => Multihash /*provenance*/ } ] }
```

The embedding-model id MUST match an advertised MemoryTopic (similarity
across embedding spaces is undefined — critique). Thresholds are query
parameters, not protocol constants. Retrieved context enters InputSpec as
ContextRefs with provenance, so poisoned context is attributable (TH6/TH7).

## 13. Task state machines

Requester view:

```
CREATED -OFFER→ QUOTED -ASSIGN→ PENDING -ACCEPT→ ACTIVE
   |               |               |‐REJECT→ FAILED(f)      ACTIVE -CHUNK*→ ACTIVE
   |               └-expiry→ FAILED(f)                      ACTIVE -CHECKPOINT/ACK→ ACTIVE
   └(direct ASSIGN)→ PENDING                                ACTIVE -provider loss→ SUSPENDED
SUSPENDED -RESUME(new peer)→ ACTIVE        ACTIVE -COMPLETE(ok)→ DELIVERED
ACTIVE|SUSPENDED -CANCEL→ CANCELLED        DELIVERED -verify root→ SETTLING -pay/receipt→ CLOSED
any -unrecoverable 2xxx/5xxx→ FAILED       DELIVERED -root mismatch→ DISPUTED (pay §6)
```

Provider view: IDLE → QUOTING → COMMITTED (on ACCEPT; resources reserved)
→ EXECUTING ⇄ CHECKPOINTING → DELIVERED → SETTLED, with CANCELLED and
FAILED exits mirroring the above. Providers MUST bound COMMITTED-without-
ASSIGN-traffic by the quote expiry.

## 14. Error handling and retries

Retryable-at-same-peer: 4000/4001 (backoff per `retry-after-ms`), 1005.
Retry-elsewhere: 5001 TTFT miss, 4002, 3001. Non-retryable without change:
2xxx, 3002, 5004, 5005. Idempotent re-ASSIGN with the same TaskID is always
safe (§1). Orchestrator fallback chains (local → trusted mesh → gateway)
are thereby deterministic — the property design3 wanted from "aggressive
fallbacks", now grounded in error semantics.

## 15. Security considerations

Tool gating (§7) and zone rules (§9) are the primary injection/exfiltration
controls; transcripts+countersigned checkpoints give attributable execution
and safe resumption; flow credit bounds provider exposure; TaskID
idempotency prevents replayed ASSIGN double-execution (with core §7).
Providers remain exposed to semantically malicious *inputs* — sandboxing
requirements are the floor, not a guarantee; meshes SHOULD gate Zone-2 tool
execution behind stricter sandbox classes than the default.

## 16. IANA-style considerations

Registers: chunk kinds; reservation/lease fields; caveat keys `delegable`,
`budget`, `args-digest`; extensions TBD-BID, TBD-KVX, TBD-GIT.

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels.
