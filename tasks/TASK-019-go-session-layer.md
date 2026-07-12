# TASK-019 — Go session layer (map-go/session): the wire twin of map-session

Status: READY · Priority: P1 (sole prerequisite of the ops §6.1 live matrix)
Target: `map-go` (new packages `session`; extensions to `envelope`)
Depends on: TASK-015 · Blocks: TASK-020 (live Rust↔Go interop matrix)

Authored by the 2026-07-12 architect session after a line-level read of the
Rust wire surfaces (`map-session/session.rs`, `map-core/hello.rs`,
`map-core/replay.rs`, `map-session/transport.rs`, `map-net/framing.rs`).
Every constant and rule below is extracted from that code + core §4/§6/§7 —
the Go implementation is written from THIS SPEC and the drafts, not from
reading the Rust source (independence rule, TASK-015 §1; the extraction
here is the architect's, made so the implementer never needs the Rust tree).

---

## 1. Problem Statement

map-go stops at vector parity: it can seal/open envelopes but cannot run a
session (no HELLO negotiation, no channel binding enforcement, no replay/
seq, no framing, no transport). The ops §6.1 matrix is LIVE PAIRWISE — a Go
peer must complete a MAP handshake with the Rust peer over a real socket
and exchange CB-bound envelopes. This package builds that layer. It ends at
a Go↔Go TCP loopback test green; the cross-language harness is TASK-020.

## 2. Functional Requirements

### FR-1 envelope package extensions (map-go/envelope)

- `Body` gains the in-session keys (core §4.1): `Recipient *identity.AgentID`
  (4), `TTLMs *uint64` (5), `SessionID []byte` (6, 16 bytes), `Seq *uint64`
  (7). `parseBody` parses them when present (today keys 4–9 are skipped);
  encoding emits them in bytewise key order 0,1,2,3,4,5,6,7,10 (the
  canonical map sort produces this automatically).
- New `SealBody(priv, body, externalAAD)` sealing a fully-specified Body
  (the existing `Seal` keeps its signature and delegates to it).
- `Open` is unchanged (it already takes `externalAAD` — pass CB in-session).

### FR-2 hello codec + negotiate (map-go/session)

- `Hello` struct: versions []uint16 (key 1, LIST), suites []uint16 (2,
  LIST), extensions [](k,v) (3, map — ALWAYS emitted, even empty),
  maxMessageSize uint32 (4; decode rejects < 65536), maxTasks *uint64 (?5),
  zones []uint8 (?7, omitted when empty, values ≤ 3).
- `HelloAck` struct: version (1, SINGLETON), suite (2, SINGLETON),
  extensions (3, always emitted), maxMessageSize (4 = the RESPONDER'S OWN
  max, not the negotiated min — the initiator computes min itself),
  maxTasks (?5). Key 8 (transcript-hash) is NOT modeled.
- `Negotiate(initiator, local Hello) (Negotiated, error)`: highest mutual
  version; FIRST mutual suite in the INITIATOR'S preference order;
  extension intersection with min(offered, local) version; effective
  max-message-size = min of both. No mutual version/suite → error (wire
  1001).
- `AckAsHello(ack)` → one-element Hello view, so the initiator re-runs
  Negotiate to validate the responder's choice sits within its own offer.

### FR-3 replay + sequence (map-go/session)

- ReplayCache: per-(sender, msg-id) uniqueness, ±120 000 ms timestamp skew
  window, 600 000 ms cache expiry, check-and-record semantics, three
  outcomes (fresh / replayed / out-of-window; both rejects are wire 1003).
- SeqTracker: dense monotonic from 0 per direction; anything else rejected
  (1003).

### FR-4 framing + transport (map-go/session)

- Frame = 4-byte big-endian length prefix + envelope bytes. Reads are
  size-first: reject a prefix > max-message-size BEFORE allocating/reading
  the body (core §6.3).
- `Transport` interface: `Send(frame []byte) error`, `Recv() ([]byte,
  error)`, `Exporter() []byte`.
- `TCPTransport` over `net.Conn` with the framing above and exporter =
  `initiator-AgentID || responder-AgentID` (68 bytes — the identical
  construction the Rust InMemoryDuplex and the core §10 libp2p binding
  use). Document: TEST-HARNESS transport — raw TCP does not authenticate
  the static keys, so this binding is for the conformance matrix only,
  never deployment (deployment bindings are core §10's).
- `InMemoryDuplex` equivalent (channel-based pair) for deterministic unit
  tests.

### FR-5 session state machine (map-go/session)

`Initiate(cfg, transport, clock, ids)` / `Respond(...)` → `*Session`, then
`Send(msgType, content) ([]byte, error)` (returns the SEALED frame — the
TASK-017 I1 seam, day one), `Recv() (*Incoming, error)` where `Incoming`
carries msg-type, content, msg-id, seq, and `Raw` (exact received frame).

Handshake, initiator: fresh 16-byte session-id; HELLO body
{0: max(offered versions), 1: 0x0001, 2: msg-id, 3: now-ms,
6: session-id, 10: Hello}, external_aad EMPTY. Then: receive; an ERROR
surfaces code/retryable; require msg-type HELLO_ACK; require sender ==
expected peer; require the session-id ECHOED (mismatch → malformed; this
is the reference implementation's R3 requirement, matched exactly);
replay-record the ack's msg-id; parse HelloAck; re-run Negotiate against
AckAsHello. TH = SHA-256("MAPv1-transcript" || hello-env || ack-env);
CB = HKDF-SHA256(salt=TH, ikm=exporter, info="map cb", L=32) — both
already exist in map-go/hello; reuse, do not duplicate.

Handshake, responder: receive; require HELLO; require sender == expected
peer; if ProtHdr key 33 present, validate the chain (envelope.OpenDelegated
at now-ms) and record its ROOT as the accountable peer (a bad chain →
handshake ERROR 2003); replay-record; extract session-id (required);
Negotiate (failure → handshake ERROR 1001 and close); HELLO_ACK body
{0: negotiated version, 1: 0x0002, 2: msg-id, 3: now-ms, 6: SAME
session-id, 10: HelloAck singletons}, empty aad. Handshake-time ERROR
envelopes: out-of-session shape, empty aad, fixed msg-id 0xEE×16.

Established I/O: every sent envelope carries {0: negotiated version,
1: mt, 2: fresh msg-id, 3: now-ms, 4: peer AgentID, 6: session-id,
7: out-seq++, 10: content}, external_aad = CB. Receive pipeline, IN THIS
ORDER (core §4.3, cheapest-first): (1) raw frame size vs negotiated max →
ERROR 1004; (2) envelope.Open with CB (strict decode + strict verify) —
identity failure → ERROR 2000, other → 1000; (3) sender == expected peer →
2000; (4) SESSION-ID CHECK: body key 6 present and equal to the session's
→ else ERROR 1006 STATE_VIOLATION and close (core §6.1 — see §11 note);
(5) replay check → 1003; (6) seq present and dense → 1003 / 1000 if
absent; (7) dispatch: PING (0x0003) → auto-reply PONG (0x0004) with
content {1: ping's msg-id} and continue the loop; PONG → deliver; CLOSE
(0x0005) → state Closed, surface the peer's ErrorMsg-shaped reason; ERROR
(0x0006) → surface code/retryable/text; anything else → deliver Incoming.
`Close(code, reason)` sends CLOSE with an ErrorMsg-shaped body and
transitions to Closed. Sending/receiving outside Established errors.

### FR-6 clocks and msg-ids

Injectable `Clock` (ms) and `MsgIDSource` interfaces; production defaults
`time.Now().UnixMilli()` and `crypto/rand`. Unit tests use fixed clocks and
counting id sources — no wall-clock, no randomness in tests (the live
matrix in TASK-020 uses the production defaults; that is where determinism
legitimately ends).

### FR-7 error codes

An `ErrorMsg` codec in map-go (core §9 shape: {1 code, 2 retryable,
?3 text, ?4 msg-id, ?5 detail}) — needed to emit/parse ERROR and CLOSE.
Codes used by this layer: 1000, 1001, 1003, 1004, 1006, 2000, 2003.

## 3. Non-Functional Requirements

Zero deps beyond the Go stdlib (net, crypto/rand, crypto/hkdf, time).
gofmt-clean, `go vet` clean. Errors as values; no panics on input.

## 4. System Architecture

`session` sits above `envelope`/`hello`/`identity` exactly as map-session
sits above map-core. Nothing in `conformance` changes (vector tests remain
deterministic and session-free).

## 5–9. Design / Data / Algorithms / Edge Cases

Covered in FR-2..5; the FRs are deliberately at wire level so no Rust
reading is needed. Edge cases the tests MUST pin: HELLO with
max-message-size < 65536 rejected at decode; extension intersection takes
min version; suite tie broken by initiator order (offer [2,1] vs local
[1,2] → suite 2); ack echoing a version OUTSIDE the initiator's offer →
initiator rejects (the re-negotiate validation); PING auto-answer does not
consume a delivered message slot; seq gap/duplicate → 1003; frame with
session-id of ANOTHER session → 1006.

## 10. Error Handling

Every rejection path names its core §9 code (FR-5 order and FR-7 list);
the session closes on 1004/1006/2000, stays open on 1003 (rate-limited
notify, matching the reference: replay/seq errors reply and surface but do
not transition state).

## 11. Security Considerations

The CB-as-external_aad is the whole point: an envelope recorded on one
session MUST NOT verify on another (TASK-020 proves it live). NOTE — the
reference Rust implementation does NOT currently enforce the per-frame
session-id check (core §6.1 "mismatch is 1006"); Go MUST implement it
(FR-5 step 4), and TASK-020 fixes the Rust side to match. This is a
known reference-impl gap, pre-decided here — do not "fix" Go to match
Rust's omission.

## 12–14. Performance / IO

One allocation per frame; nothing else matters at this scale.

## 15. Acceptance Criteria

- ✓ `go vet ./... && gofmt -l .` clean; `go test ./...` green on a clean
  checkout, still zero external deps.
- ✓ Go↔Go loopback over BOTH InMemoryDuplex and real TCP (127.0.0.1,
  ephemeral port): handshake, echo both directions, PING auto-PONG,
  CLOSE with reason.
- ✓ Negative unit tests: replayed frame → 1003; seq gap → 1003; tampered
  frame → 2000; wrong session-id → 1006; version-disjoint HELLO →
  handshake ERROR 1001; oversized frame prefix rejected before read.
- ✓ Delegated handshake unit test: initiator with chain [RIK→DK cert]
  (vector keys) → responder resolves accountable root = RIK.
- ✓ Existing conformance suite untouched and green (25 vectors).

## 16. Comprehensive Test Plan

Unit tests per package section with fixed clock/ids (cite the core clause
per test); one TCP loopback integration test (two goroutines). The FR-5
receive-order is itself a test: a frame that is BOTH oversized and
tampered must yield 1004 (size first).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-go`, implement TASK-019 §FR-1..7 exactly. Read FIRST:
> draft-map-core-00 §4, §6, §7, §9, §10 and this package. Do NOT read any
> `map-rs` source — every wire rule you need is in the FRs; where the
> drafts and the FRs differ, the FRs win and you report the difference
> (they encode reference-implementation behavior the drafts underspecify:
> the session-id echo, the ACK max-message-size semantics, the handshake
> ERROR msg-id 0xEE×16, and the session-id 1006 check Go implements ahead
> of Rust). Reuse `hello.TranscriptHash`/`hello.ChannelBinding` and
> `envelope.Open/OpenDelegated`; extend `envelope.Body` per FR-1 without
> disturbing the frozen-vector produce paths (the conformance suite MUST
> stay green — run it). Keep zero external dependencies. Deliver: code,
> tests per §15/§16, a README section in map-go/README.md ("session
> layer"), and an ambiguity report (spec clauses you found underspecified
> while implementing).

## 18. Validation Checklist (reviewer)

```
cd map-go && go vet ./... && gofmt -l . && go test ./...
go list -m all            # still just the module itself
go test ./conformance -run TestProduceParity -v   # 25 vectors, unchanged
```

## 19. Known Risks

R1: accidental divergence in HELLO/ACK byte layout → the Go↔Go tests
cannot catch a shared misreading; TASK-020's live matrix is the real
check, and FR-2's field-shape rules are written from the frozen TV-001/
TV-003 bytes to anchor them. R2: the session-id 1006 check will make
Go stricter than Rust until TASK-020 lands — acceptable, sequenced.
R3: blocking net I/O + goroutine tests can hang on bugs; every test takes
a deadline (`conn.SetDeadline` / test timeout).

## 20. Handoff Notes

Update memory.md (§8 map-go state: session layer) + tasks/README.md row
019. Then TASK-020 (harness + matrix) is unblocked — it needs exactly:
`session.Initiate/Respond/Send/Recv/Close`, `TCPTransport`, and the FR-7
codes.
