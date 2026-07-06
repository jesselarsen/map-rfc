# TASK-015 — Go Implementation (impl #2, scoped to Core Conformance)

Status: READY (after TASK-011 freezes vectors) · Priority: P3 · Target: new top-level `map-go/`
Depends on: TASK-011 (frozen vector set) · Blocks: any draft leaving -0x status (ops §6.1 two-impl rule)

---

## 1. Problem Statement

Ops §6.1: no document leaves draft without two independent
implementations passing the matrix. `map-rs` is impl #1. This task
starts impl #2 in Go, scoped deliberately: **byte-exact conformance to
the full frozen vector set** (produce + consume + reject), NOT feature
parity. Independence rule: the Go code MUST be written from the SPECS
and VECTORS, not translated from the Rust source — translation would
silently copy impl #1's interpretation bugs and destroy the
two-implementation rule's value.

## 2. Functional Requirements

- FR-1 Layout: `map-go/` at repo root; Go 1.22+; module
  `github.com/jesselarsen/map-rfc/map-go`; packages `identity`, `cbor`
  (deterministic encoder + strict decoder — hand-rolled like the Rust
  one; do NOT import a CBOR library, the encoding profile IS the thing
  under test), `envelope`, `hello`, `receipt`, `discovery` (cap URIs,
  shard, keys), `conformance`.
- FR-2 Stdlib + `golang.org/x/crypto` only (ed25519 is stdlib;
  blake2 NOT needed — hardcode the TV-002 task-id hex as in Rust).
  Base58: implement in-package (~40 lines) rather than importing.
- FR-3 Conformance: reproduce EVERY frozen positive vector byte-for-
  byte from seeds (TV-001..the latest at execution time — read
  `map-spec/test-vectors/map-test-vectors.md` as the single source);
  reject every NV with the required error class. Same first-diff-offset
  reporting rule as the Rust crate.
- FR-4 Ed25519 verify semantics: Go stdlib `ed25519.Verify` differs
  from dalek `verify_strict` on edge inputs (small-order/non-canonical
  points). Implement the strict checks explicitly (reject non-canonical
  A and R, reject small-order A) — document each check against RFC 8032
  and test with at least one known non-canonical-point rejection case.
- FR-5 Cross-impl live handshake is OUT of scope here (needs TASK-012's
  network layer + a Go session layer — future package); this task ends
  at vector parity + a `go test ./...` green tree.
- FR-6 CI-ready: `map-go/README.md` with build/test instructions;
  everything runs with plain `go test` (no network, no clocks —
  same determinism rules as the Rust side).

## 3. Non-Functional Requirements

Idiomatic Go (errors as values, no panics on input); `gofmt`-clean;
value types for AgentID/TaskID (ops §8 rule applies to impl #2 too);
zero external deps beyond x/crypto.

## 4. System Architecture

Mirrors the bounded contexts, not the Rust files: identity ← cbor ←
envelope ← {hello, receipt, discovery} ← conformance. No session/task
runtime in this package.

## 5. Component Design

Per FR-1. The cbor package is the critical path: RFC 8949 §4.2.1
deterministic encoding (minimal-length integers, definite lengths,
bytewise-sorted map keys) + strict decoder rejecting all deviations
(the NV-003-class vectors are its acceptance test).

## 6. Interface Specifications

`envelope.Seal(kp, body, externalAAD) ([]byte, error)`;
`envelope.Open(env, externalAAD) (Opened, error)` where `Opened.Payload`
preserves received bytes (I1 — same pattern as Rust: never re-encode to
verify). `receipt.Sign(kp, role, coreBytes)`; `receipt.Verify(r)`
requiring both sigs for trust use, either for structural validity
(trust §3). Errors carry the core §9 class for NV assertions.

## 7. Data Structures

AgentID `[34]byte` newtype with multibase/did:key formatting matching
the vector file's strings exactly (test against them).

## 8. Algorithms

Same normative algorithms, written from spec text: Sig_structure
construction (core §4), TH/CB (core §6.2/§7.3 — HKDF-SHA256 via
x/crypto/hkdf), shard = digest first byte (discovery §6), Merkle
transcript rules (tasks §8.1) if the frozen vector set includes TV-009
(it does — implement transcript leaf/parent/empty).

## 9. Edge Cases

The NV catalog IS the edge-case list; additionally: map key sort order
for mixed uint/text keys (RFC 8949 bytewise — the PayQuote rate-table
and MethodOffer vectors exercise it), tag 4 (decimal) and tag 18
encoding, 64-bit timestamps.

## 10. Error Handling

Typed errors per package; conformance reports aggregate (never stop at
first failure).

## 11. Security Considerations

FR-4 is the substantive one — a lax verifier would pass every vector
yet diverge from impl #1 on adversarial inputs. The independence rule
(§1) is process-security: reviewers should spot-check that Go names/
structure do NOT mirror the Rust internals.

## 12. Performance Considerations

None.

## 13/14. Input & Output Specifications

`go test ./...`; conformance failures print vector id + first-diff
offset.

## 15. Acceptance Criteria

- ✓ `go test ./...` green on a clean checkout, no deps beyond x/crypto.
- ✓ Every frozen TV reproduced byte-exact (produce) and accepted
  (consume); every NV rejected with the required class.
- ✓ FR-4 strict-verify tests present incl. a non-canonical-point
  rejection.
- ✓ AgentID multibase/did:key strings match the vector file exactly.
- ✓ ops §6.1 status note updated: impl #2 at vector parity (the interop
  MATRIX — live pairwise — remains future; do not overclaim).

## 16. Comprehensive Test Plan

Per-package unit tests written from spec clauses (cite the clause in
the test comment); the conformance run as the integration layer;
fuzz the cbor strict decoder with `go test -fuzz` seed corpus from the
vectors (accept/reject stability, no panics).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Create `map-go/` per §FR-1..6. HARD RULE: write from
> `map-spec/draft-map-*.md` and `map-spec/test-vectors/
> map-test-vectors.md` only — do NOT read `map-rs/` source except the
> two files that are registry mirrors by design (`msg_type.rs` code
> points, `errors.rs` codes), and say so in the report. Hand-roll the
> deterministic CBOR codec; implement strict Ed25519 verification per
> §FR-4 with RFC 8032 citations. Preserve received bytes for all
> verification (I1). Report any spec clause you found ambiguous while
> implementing WITHOUT Rust source to lean on — those reports are this
> task's highest-value output (they are what a fresh implementer hits).
> Deliver: code, tests, README, ops §6.1 note, ambiguity report.

## 18. Validation Checklist (reviewer)

```
cd map-go && go vet ./... && gofmt -l . && go test ./...
go list -m all | wc -l        # tiny (stdlib + x/crypto)
```

## 19. Known Risks

R1: subtle CBOR sort-order or length-encoding divergence — the vectors
catch byte-level issues; the fuzz corpus catches decoder laxity. R2:
scope creep toward feature parity — resist; vector parity is the
deliverable. R3: ambiguities found here may require spec patches after
the architect is gone — follow the tasks/README.md handoff protocol
(record, propose, mark PENDING-ARCHITECT; do not change normative text
for anything security-relevant).

## 20. Handoff Notes

Update memory.md (§8: impl #2 started; add a map-go state section).
Future: Go session layer + cross-impl live handshake against map-net
(the real ops §6.1 matrix), then Go task/receipt flows.
