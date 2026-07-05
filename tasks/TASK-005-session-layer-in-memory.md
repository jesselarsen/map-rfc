# TASK-005 — Session Layer over an In-Memory Duplex

Status: READY · Priority: P1 · Target: new crate `map-rs/crates/map-session`
Depends on: TASK-001 (green workspace), TASK-003 (conformance guards) ·
Blocks: descriptor publish/fetch, echo task end-to-end, libp2p binding
(memory.md §8 steps 1–3)

---

## 1. Problem Statement

Every crate so far is pure data-plane: envelopes, negotiation math, replay
state. Nothing yet *drives a session*: there is no component that performs
the HELLO/HELLO_ACK handshake over a byte pipe, derives the transcript
hash and channel binding, enforces the core §12 state machine, and gives
callers a `send/recv` for in-session messages with seq, replay, and
size-limit enforcement. This task builds that layer over a deterministic
in-memory duplex transport. The libp2p/QUIC binding (real exporter, real
network) is explicitly a LATER package — this one proves the protocol
logic end-to-end in-process, fully testable and clock-free.

## 2. Functional Requirements

- FR-1 `Transport` abstraction (blocking, byte-oriented):
  `send(&[u8])`, `recv() -> Vec<u8>` (framed: one envelope per call),
  `exporter() -> Vec<u8>` (transport keying material for CB, core §7.3).
- FR-2 `InMemoryDuplex::pair(agent_a, agent_b) -> (Transport, Transport)`
  built on `std::sync::mpsc` channels. Its exporter, per core §7.3's rule
  for transports lacking a native exporter, is the concatenation of both
  peers' static public keys **as authenticated by the transport**; for
  the in-memory pair this is defined as
  `initiator-AgentID-bytes || responder-AgentID-bytes` (34+34 = 68 bytes;
  both endpoints MUST derive the identical value).
- FR-3 Handshake, initiator side: build HELLO (core §6.1 Hello shape;
  reuse `map_core::hello::Hello`), seal with **empty** external_aad
  (session not yet bound), send; receive HELLO_ACK; run
  `hello::negotiate`; compute
  `TH = transcript_hash(hello_env_bytes, hello_ack_env_bytes)` over the
  exact transmitted bytes; `CB = channel_binding(&TH, &exporter)`; enter
  ESTABLISHED.
- FR-4 Handshake, responder side: receive HELLO, verify (open with empty
  aad), check the sender's AgentID equals the transport-authenticated
  peer key (mismatch = error 2000 + CLOSE, core §7.3), negotiate, send
  HELLO_ACK (also empty aad), compute the same TH/CB, enter ESTABLISHED.
- FR-5 State machine per core §12: `Idle → HelloSent → Established →
  Closed` (initiator), `Idle → Established → Closed` (responder answers
  HELLO directly). Any protocol violation → send ERROR where specified,
  transition to Closed. API calls invalid in the current state return
  `SessionError::State` (maps to error 1006).
- FR-6 In-session send: caller provides msg_type + content `Value`;
  the session fills Body: negotiated version, msg-id (from an injected
  `MsgIdSource`), timestamp (from an injected `Clock`), recipient =
  peer AgentID (**required in-session**, core Body key 4), session-id
  (16 bytes, initiator-chosen at handshake, carried in HELLO's envelope
  Body key 6 and echoed by the responder), seq from a per-direction
  `SeqTracker` (key 7), content (key 10); seals with `external_aad = CB`.
- FR-7 In-session recv: open with `external_aad = CB` (wrong session →
  signature failure, the anti-relay property), then enforce in order:
  sender == peer (2000), size ≤ negotiated max_message_size (1004, check
  BEFORE signature work on the raw frame length), replay/timestamp window
  via `ReplayCache` (1003), seq monotonic via `SeqTracker::accept` (1003),
  state == Established (1006). Violations: return typed error AND send an
  ERROR envelope with the named code (except when the frame is unverifiable
  — bad signature/oversize — where the session MUST NOT echo content back,
  core §14; send ERROR with detail omitted).
- FR-8 PING/PONG (0x0003/0x0004): `Session::ping()` sends PING with empty
  map content; the recv loop auto-replies PONG carrying the PING's msg-id
  bytes as content key 1. Three consecutive unanswered pings is the
  caller's policy, not the session's (expose counters, don't decide).
- FR-9 CLOSE (0x0005): `close(code, reason)` sends CLOSE with
  ErrorMsg-shaped content (core §9/§12) and transitions to Closed;
  receiving CLOSE transitions to Closed and surfaces the reason. After
  Closed, send/recv return `SessionError::State`.
- FR-10 Determinism for tests: `Clock` and `MsgIdSource` are traits;
  production impls exist (`SystemClock` behind getrandom/SystemTime —
  allowed, it is not consensus-relevant; `RandomMsgIds`), but ALL tests
  use fixed implementations. No test may read the wall clock.

## 3. Non-Functional Requirements

Pinned workspace deps only; std::sync::mpsc for the duplex (no crossbeam,
no tokio, no async). Zero warnings. Blocking API. Frame limit checks must
be O(1). New crate added to workspace `Cargo.toml` members (architect-
approved member addition; still no new external dependencies).

## 4. System Architecture

`map-session` depends on `map-identity` and `map-core` ONLY (session layer
is a core-context component; it must not know about tasks/trust/pay —
callers route content by msg_type themselves). Nothing depends on it yet;
the future echo-task package will.

```
caller ──> Session (state machine, seq, replay, limits, CB)
                │ sealed envelope bytes
                ▼
           Transport trait  ←── InMemoryDuplex (mpsc pair)   [this task]
                                libp2p /map/1.0.0            [future]
```

## 5. Component Design

```
crates/map-session/src/
  transport.rs   Transport trait + InMemoryDuplex + TransportError
  clock.rs       Clock, MsgIdSource traits + system + fixed impls
  session.rs     SessionConfig, Session, SessionState, SessionError,
                 handshake_initiate(), handshake_respond()
  lib.rs         re-exports
```

Key types:

```
pub struct SessionConfig {
    pub keypair: map_identity::Keypair,
    pub expected_peer: map_identity::AgentId,   // transport-authenticated
    pub hello: map_core::hello::Hello,          // local offer/answer basis
}
pub enum SessionState { Idle, HelloSent, Established, Closed }
pub struct Session<T: Transport, C: Clock, M: MsgIdSource> { .. }
impl Session {
    pub fn initiate(cfg, transport, clock, ids) -> Result<Session, SessionError>
    pub fn respond(cfg, transport, clock, ids) -> Result<Session, SessionError>
    pub fn send(&mut self, msg_type: u16, content: Value) -> Result<(), SessionError>
    pub fn recv(&mut self) -> Result<Incoming, SessionError>
    pub fn ping(&mut self) -> Result<(), SessionError>
    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), SessionError>
    pub fn state(&self) -> SessionState
    pub fn negotiated(&self) -> &map_core::hello::Negotiated
    pub fn channel_binding(&self) -> &[u8; 32]
    pub fn transcript_hash(&self) -> &[u8; 32]
}
pub struct Incoming { pub msg_type: u16, pub content: Value,
                      pub msg_id: MsgId, pub seq: u64 }
```

`recv` handles PING (auto-PONG, returns the next real message — i.e.
loops internally), PONG (returns it; caller correlates), CLOSE (state →
Closed, returns `Err(SessionError::PeerClosed{code, reason})`), ERROR
(returns `Err(SessionError::PeerError{code, retryable, text})`).

## 6. Interface Specifications

- `initiate`/`respond` perform the FULL handshake before returning (block
  on the transport). Preconditions: state Idle. Postconditions on Ok:
  state Established; TH, CB, Negotiated populated; seq trackers at 0 both
  directions. Errors: `Transport`, `Handshake(NegotiateError)`,
  `PeerMismatch` (kid ≠ expected_peer → also send ERROR 2000 + CLOSE),
  `Envelope(EnvelopeError)`.
- `send`: precondition Established; increments the outbound seq exactly
  once per successful transport write (on transport error, the session
  transitions to Closed — no seq reuse ambiguity).
- `recv`: order of checks is NORMATIVE as listed in FR-7; each rejection
  maps to exactly one `SessionError` variant carrying the core §9 code.
- All methods: no panics on any input; no I/O besides the Transport.

## 7. Data Structures

Session-id: 16 bytes from `MsgIdSource` at initiate() (reuse the 16-byte
generator; it is an opaque token, core Body key 6). HELLO/HELLO_ACK
envelopes carry Body keys {0,1,2,3,6,10} (no recipient pre-authentication,
no seq — the handshake is out-of-session by definition; seq starts with
the first in-session envelope). In-session envelopes carry keys
{0,1,2,3,4,6,7,10}.

## 8. Algorithms

Handshake sequence (initiator): build HELLO body (version = highest local,
msg_type 0x0001) → seal(empty aad) → send → recv → open(empty aad) →
require msg_type 0x0002 and sender == expected_peer → negotiate(local,
remote-as-answer) → TH over (sent-bytes, received-bytes) exactly as
transmitted → CB = HKDF(salt=TH, ikm=exporter, info="map cb", L=32) (this
is `map_core::hello::channel_binding` — do not reimplement) → Established.
Responder mirrors with roles swapped and MUST echo the initiator's
session-id. Timestamp validation during handshake uses the same
ReplayCache window (the HELLO msg-id enters the cache too — a replayed
HELLO within 10 min MUST be rejected 1003).

## 9. Edge Cases

- Responder receives a non-HELLO first envelope → ERROR 1006 + CLOSE.
- HELLO with no mutual version/suite → `negotiate` errs → responder sends
  ERROR 1001 and closes; initiator surfaces `Handshake`.
- max_message_size below the 65 536 floor in a received Hello → treat as
  malformed (1000) per core §6.1.
- Frame larger than negotiated limit arriving mid-session → 1004; the
  check uses the RAW frame length before any decode.
- Peer sends seq 0 twice (duplicate after success) → 1003, session stays
  Established (msg-level, not fatal).
- `send` after `close` → `SessionError::State`, nothing written.
- Zero-length frame from transport → EnvelopeError (malformed), 1000.

## 10. Error Handling

`SessionError` variants each carry the wire error code they correspond to
(1000/1001/1003/1004/1006/2000 per core §9) plus context. Rule: every
path that rejects a PEER's message both returns the error AND (state
permitting) notifies the peer with an ERROR envelope — EXCEPT
unverifiable frames (bad signature, oversize), which get a minimal ERROR
without echoing any received content (core §14 leakage rule).

## 11. Security Considerations

- Empty aad for handshake envelopes, CB for everything after — this
  boundary IS the anti-relay property; tests must prove an Established-
  session envelope fails verification on a second session (different CB).
- TH is computed over transmitted envelope BYTES (I1); never re-seal or
  re-encode to recompute it.
- kid == transport-authenticated peer check (FR-4/FR-7) is TH4's defense;
  skipping it on ANY path is a critical bug.
- Downgrade test required: tamper the versions list inside the received
  HELLO bytes before the responder processes them → both sides compute
  different TH → first in-session envelope fails (core §6.2 property).
- The in-memory exporter is honest by construction; document loudly that
  a real transport binding MUST use the TLS exporter/Noise hash, and that
  `InMemoryDuplex` is test infrastructure, not a deployable transport.

## 12. Performance Considerations

Blocking mpsc is fine; no buffering beyond the channel. Per-message cost
is one seal/open (one Ed25519 op each) — no optimization work in scope.

## 13. Input Specifications

Constructor inputs per §5/§6. Transport frames are complete envelopes
(the duplex preserves message boundaries; no length-prefixing needed —
document that stream transports WILL need framing, future package).

## 14. Output Specifications

Public API per §5. No files, no stdout (library). Counters exposed:
`pings_unanswered()`, `msgs_sent()`, `msgs_received()`.

## 15. Acceptance Criteria

- ✓ Workspace builds + tests green, zero warnings, no new external deps.
- ✓ Full handshake between two threads over `InMemoryDuplex`; both sides
  Established with identical TH, CB, Negotiated values.
- ✓ Echo exchange: A sends PING, gets PONG; A sends an arbitrary content
  map (msg_type 0xF000 private-use), B receives it intact with seq 0.
- ✓ All §16 negative tests pass with the exact error-code mapping.
- ✓ Anti-relay test (§11) passes.
- ✓ Downgrade-tamper test (§11) passes.
- ✓ `map-session` depends only on map-identity + map-core (+ pinned
  crypto deps); `cargo tree -p map-session` shows no tasks/trust/pay.

## 16. Comprehensive Test Plan

Unit: state-transition table (every method × every state → expected
Ok/State error); exporter symmetry (both duplex ends identical);
session-id echo; seq increments; replayed-HELLO rejection.
Integration (two threads, fixed clock/ids): happy-path handshake +
message + ping + close (both directions); peer-mismatch (wrong expected_peer)
→ PeerMismatch + ERROR 2000 observed by the other side; no-mutual-version
→ 1001; oversize frame → 1004; duplicate msg-id → 1003; non-monotonic seq
→ 1003; send-after-close → State; CLOSE reason roundtrip.
Security: anti-relay (two sessions, replay envelope across) and
downgrade-tamper per §11.
Concurrency: one test with sender/receiver in separate threads pumping
100 messages each way (fixed clock stepping +1 ms per send) — ordering
and seq integrity hold.
Property (no new deps): for random-ish content maps built from a seeded
LCG in the test, send→recv roundtrips `Value` equality.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, create the new workspace member `crates/map-session`
> (add it to the root `Cargo.toml` members) implementing §5–§14 of this
> document exactly. Reuse `map_core::hello::{Hello, negotiate,
> transcript_hash, channel_binding}`, `map_core::envelope::{seal, open,
> Body, MsgId}`, `map_core::replay::{ReplayCache, SeqTracker}`, and
> `map_core::{msg_type, errors::codes}` — do not reimplement any of them
> and do not modify any existing crate. Handshake envelopes use empty
> external_aad; in-session envelopes use the channel binding; the FR-7
> check order is normative. All tests use fixed Clock/MsgIdSource
> implementations — no wall-clock or RNG reads in tests. Build with
> `cargo +stable-x86_64-pc-windows-gnu` from PowerShell (this machine has
> no MSVC linker; Git-Bash shadows `link`). Run the FULL workspace suite
> after every fix. Deliver: source, tests per §16, and a report (design
> notes, test summary, any spec ambiguity found — quote + proposed
> wording — especially anything under-specified about HELLO_ACK's exact
> content shape in core §6.1, which this task will exercise for the
> first time beyond TV-003).

## 18. Validation Checklist (reviewer)

```
cd map-rs
cargo +stable-x86_64-pc-windows-gnu build --workspace 2>&1 | grep -c warning  # 0
cargo +stable-x86_64-pc-windows-gnu test --workspace                          # green
cargo tree -p map-session | grep -E "map-(tasks|trust|pay|discovery)"         # no hits
cargo test -p map-session -- --list | wc -l                                   # >= 20 tests
```

## 19. Known Risks

- R1: core §6.1 specifies Hello but is thinner on HELLO_ACK's answer
  shape (the scaffold reuses the Hello struct with singleton lists, as
  TV-003 does). If the implementation needs a distinction the spec
  doesn't make, that is a spec-feedback item — report, don't invent.
- R2: blocking `recv()` with internal PING auto-reply can deadlock if
  both sides block on recv simultaneously with no traffic; document that
  callers own the pump loop; the 100-message concurrency test must be
  structured to avoid symmetric blocking (alternate send/recv).
- R3: session-id echo is asserted by this design (Body key 6 on both
  handshake envelopes); core §6 does not currently say the responder MUST
  echo it — expected spec-feedback item; flag it.

## 20. Handoff Notes

On green: update memory.md §5 (crate table) and §7/§8 (step 1 done);
record any spec patches (HELLO_ACK shape, session-id echo) in memory §4.
Next packages after this: descriptor publish/fetch (needs only
map-session + map-discovery over the duplex), then the echo task
end-to-end (ASSIGN → CHUNK stream under FlowAccount → COMPLETE with
transcript root, driving the tasks-crate state machine over a live
session), then the libp2p transport binding (real exporter).
