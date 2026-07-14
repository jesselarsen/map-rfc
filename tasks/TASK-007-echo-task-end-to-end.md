# TASK-007 — Echo Task End-to-End

Status: READY (after TASK-006) · Priority: P1 · Target: new crate `map-rs/crates/map-taskrun`
Depends on: TASK-005, TASK-006 · Blocks: TASK-008 (receipts), TASK-009 (payments)

---

## 1. Problem Statement

The tasks crate has the pieces (TaskId, TranscriptBuilder, FlowAccount,
requester FSM) but nothing executes a task over a live session:
OFFER → QUOTE → ASSIGN → ACCEPT → CHUNK stream under credit →
COMPLETE with a verified transcript root. This task builds the runtime
for both roles and proves it with an echo capability
(`map:cap:inference/text-generation` semantics are irrelevant — the
provider echoes input back), the "hello world" of the protocol
(memory.md §8 step 3).

## 2. Functional Requirements

- FR-1 Message codecs (in `map-tasks`, pure): to_value/from_value for
  Offer, Quote, Assign, Accept (empty map), Reject {1 code}, Chunk,
  Complete, Cancel {1 TaskID, 2 code}, FlowCredit — exactly per the CDDL
  in tasks §2/§2.1/§3/§4 and core §8.1:
  - `Offer  = {1 TaskID, 2 CapURI, 3 TaskConstraints, ?4 PayRef, ?5 expiry}`
  - `Quote  = {1 TaskID, 2 PayQuoteRef, 3 ttft-ms-guarantee, 4 expiry, ?5 ext}`
  - `Assign = {1 TaskID, 2 CapURI, 3 TaskConstraints, 4 InputSpec,
     5 FlowInit, ?6 PayAgreeRef, ?7 ext}`
  - `Chunk  = {1 TaskID, 2 chunk-seq, 3 kind, 4 content, ?5 fin}`
  - `Complete = {1 TaskID, 2 outcome, 3 transcript-root(multihash-sha256
     of the tasks §8.1 Merkle root), 4 usage{1..4}, ?5 ext}`
  - `FlowCredit = {1 TaskID-bytes, 2 msg-credits, 3 byte-credits}`
  Pay refs are OPTIONAL and omitted in this package (TASK-009 wires them).
- FR-2 Provider runtime `ProviderTask`: given an Incoming ASSIGN and an
  `Executor` trait (`fn execute(&InputSpec-inline-bytes) ->
  Vec<(kind, content)>`), reply ACCEPT, stream CHUNKs (respecting
  FlowAccount credits granted by FlowInit + subsequent FLOW_CREDIT;
  block/park when out of credit — expose `Starved` state rather than
  busy-wait), build the transcript with `TranscriptBuilder` (kinds 0/1/4
  only, per decision 10), then send COMPLETE with root + usage.
- FR-3 Requester runtime `RequesterTask`: drive the existing
  `map_tasks::ReqState`/`step` FSM; send OFFER, evaluate QUOTE (accept
  iff quoted ttft ≤ constraint if set), ASSIGN with inline InputSpec and
  FlowInit; receive CHUNKs (feeding its own TranscriptBuilder and
  consuming credits — grant more via FLOW_CREDIT when < 25% remain);
  on COMPLETE verify `root == own transcript root` → task Verified;
  mismatch → error 5000 + state Failed (dispute path is future).
- FR-4 TTFT ×1.1 rule (tasks §2): if the first CHUNK does not arrive
  within `quoted-ttft × 1.1` (measured with the injected Clock against a
  deadline the caller polls — no timers/threads inside the runtime),
  the requester sends CANCEL (5001) and transitions Cancelled.
- FR-5 Direct-ASSIGN path (tasks §2: OFFER may be skipped) — supported;
  test covers it.
- FR-6 The echo Executor: returns exactly one kind-0 chunk with the
  input bytes + one kind-3 usage chunk (excluded from transcript,
  kind 3 ∉ {0,1,4}); deterministic.

## 3. Non-Functional Requirements

Pinned deps only. No threads inside runtimes (caller pumps; runtimes are
state machines with `on_incoming(&mut self, Incoming) -> Vec<Outgoing>`
+ `poll_deadline(now_ms)`). Zero warnings. Injected clocks everywhere.

## 4. System Architecture

New crate `crates/map-taskrun` (deps: identity, core, session, tasks).
It must NOT depend on mesh/trust/pay (receipts and payment hook in via
TASK-008/009 through returned artifacts, not dependencies): on Verified
completion the requester runtime EXPOSES `(ReceiptInputs { task, cap,
started, ended, usage, outcome }, transcript_root)` for callers.

## 5. Component Design

```
crates/map-taskrun/src/
  codec.rs     the FR-1 to_value/from_value impls (or in map-tasks —
               DECISION: put codecs in map-tasks (pure data, owned by the
               tasks context); runtimes in map-taskrun)
  provider.rs  ProviderTask (states: AwaitAssign→Streaming{Starved?}→
               Completed/Cancelled/Failed), Executor trait, EchoExecutor
  requester.rs RequesterTask wrapping ReqState/step + transcript + credit
               grant policy + ttft deadline
  lib.rs
```

## 6. Interface Specifications

`on_incoming` never blocks and never touches a transport — it returns
messages to send (msg_type, Value); the caller owns session I/O. This
makes every state transition unit-testable without threads.
Preconditions/postconditions: chunk-seq dense from 0 (receiver rejects
gaps → Failed 1003-equivalent local error); FlowAccount `try_consume`
before every CHUNK send; COMPLETE only after all fin chunks.

## 7. Data Structures

`ReceiptInputs` as in §4 — field-for-field what trust §3 ReceiptCore
needs minus the identities (caller adds those). InputSpec: inline bytes
(key 1) only; content-addressed fetch (keys 2/3) out of scope.

## 8. Algorithms

Transcript on both sides: push kinds 0/1/4 in chunk-seq order into
`TranscriptBuilder`; root compared on COMPLETE. Credit grant policy
(requester): initial FlowInit {64 msgs, 1 MiB}; re-grant same amounts
when remaining < 25%. Usage accounting: provider sums content bytes and
reports wall-ms from injected clock deltas.

## 9. Edge Cases

QUOTE expired before ASSIGN (clock past expiry) → requester Rejected
locally, no ASSIGN. REJECT after OFFER → Failed with code. Chunk after
COMPLETE → protocol violation, Failed. Zero-credit FlowInit → provider
immediately Starved, streams nothing until credit. Empty input → still
one kind-0 chunk (empty content) + usage; root over one leaf. CANCEL
crossing COMPLETE in flight: whichever the peer processes first wins;
both runtimes must accept the other's terminal without panicking
(idempotent terminals).

## 10. Error Handling

Every runtime error carries the wire code it maps to (REJECT codes 3xxx,
CANCEL 5001, transcript mismatch 5000). Terminal states are sticky;
`on_incoming` in a terminal state returns empty (idempotent, ops §2
rule 3).

## 11. Security Considerations

Transcript verification is the point: a test MUST tamper one chunk byte
in flight (between runtimes) and show COMPLETE verification fails with
5000. Credits enforce receiver sovereignty (TH resource-exhaustion):
provider MUST NOT send beyond granted credit — test asserts Starved
rather than overrun. Zone checks are out of scope here (constraints
carry ZoneLabel but enforcement is TASK-010+ territory) — document that.

## 12. Performance Considerations

None; echo payloads are tiny. Note in docs: chunk size must respect the
session max_message_size minus envelope overhead (use 16 KiB content
max — well under the floor 64 KiB).

## 13. Input Specifications

Runtimes: Incoming messages + clock polls. Echo executor: inline bytes.

## 14. Output Specifications

Outgoing (msg_type, Value) lists; terminal artifacts (`ReceiptInputs`,
root) on the requester; counters (chunks sent/received, credits granted).

## 15. Acceptance Criteria

- ✓ Workspace green, zero warnings, no new deps; codecs live in
  map-tasks, runtimes in map-taskrun.
- ✓ Integration test: full OFFER→QUOTE→ASSIGN→ACCEPT→CHUNKs→COMPLETE
  over two live sessions (threads pumping), echo verified, roots match,
  ReceiptInputs exposed.
- ✓ Direct-ASSIGN test passes.
- ✓ TTFT ×1.1 CANCEL test passes (fixed clock stepped past deadline).
- ✓ Tamper test (§11) fails verification with 5000.
- ✓ Starved-not-overrun credit test passes.

## 16. Comprehensive Test Plan

Unit (no threads): every FSM transition on both runtimes; codec
roundtrips for all FR-1 messages incl. optional-field absence; gap in
chunk-seq; chunk-after-complete; quote-expiry; zero credit.
Integration (threads + InMemoryDuplex): happy path; direct-ASSIGN; ttft
cancel; tamper; credit starvation/regrant; empty input.
Property: echo of seeded-LCG random payloads (≤16 KiB) roundtrips and
roots match.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, add the FR-1 message codecs to `crates/map-tasks`
> (pure to_value/from_value, field numbers exactly as §FR-1 quotes from
> tasks §2–§4 — verify against `map-spec/draft-map-tasks-01.md` before
> coding; if any field number is absent from the draft, STOP on that item
> and report) and create workspace member `crates/map-taskrun` per
> §4–§14. Runtimes are pump-driven state machines: no threads, no
> transport calls, no wall-clock inside them. Reuse ReqState/step,
> TranscriptBuilder, FlowAccount — do not fork them; if ReqState lacks a
> transition you need, extend it in map-tasks with a test and note it.
> Build with `cargo +stable-x86_64-pc-windows-gnu` from PowerShell; full
> suite after every fix. Deliver: code, tests per §16, report.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo +stable-x86_64-pc-windows-gnu test --workspace
cargo test -p map-taskrun -- --list | wc -l    # >= 18
cargo tree -p map-taskrun | grep -E "map-(trust|pay|mesh)"  # no hits
```

## 19. Known Risks

R1: tasks §2 Quote/Assign optional-field numbering may have gaps the
codecs expose (the PayRef family was one; there may be more) — report,
don't invent. R2: CANCEL/COMPLETE race — the idempotent-terminal rule
(§9) is the resolution; don't add locking.

## 20. Handoff Notes

Update memory.md §5/§8 (step 3 done). TASK-008 consumes `ReceiptInputs`;
TASK-009 threads PayQuoteRef/PayAgreeRef through the same codecs (fields
already present as options).
