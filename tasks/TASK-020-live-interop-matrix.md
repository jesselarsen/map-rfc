# TASK-020 — live Rust↔Go interop matrix (the ops §6.1 gate)

Status: BLOCKED on TASK-019 · Priority: P1 · Targets: `map-rs` (map-session
TCP transport + session-id fix + map-cli subcommand), `map-go`
(cmd/map-interop), new top-level `interop/` (driver + MATRIX.md record)

Authored by the 2026-07-12 architect session. This is the LAST gate: ops
§6.1 requires two independent implementations passing the full matrix
against each other before any document leaves draft. Vector parity
(TASK-015/018) proved byte-level agreement; this package proves LIVE
protocol agreement over a real socket, in both orientations.

---

## 1. Problem Statement

No MAP session has ever crossed a language boundary. Build the minimal
harness that runs the matrix: a Rust peer binary, a Go peer binary, a
driver that spawns them pairwise over TCP, and a written record
(`interop/MATRIX.md`). Also fix the one known reference-impl gap so both
sides enforce the same rules (session-id 1006 — see FR-2).

## 2. Functional Requirements

### FR-1 Rust TCP test transport (map-session)

`TcpTransport` implementing `Transport` over `std::net::TcpStream`: 4-byte
BE length-prefixed frames, size-first read against a configured max,
exporter = `initiator-AgentID || responder-AgentID` (the InMemoryDuplex
construction). std::net only — no new dependencies. Documented as
test-harness-only (same caveat as TASK-019 FR-4).

### FR-2 Rust session-id enforcement (map-session; reference-impl bug fix)

core §6.1: every in-session envelope carries the session's SessionID and a
mismatch is a state violation (1006). `Session::recv` currently never
checks body key 6 after the handshake. Add, in the FR-5/TASK-019 position
(after sender check, before replay): key 6 present and equal → else send
ERROR 1006, close, surface an error. Unit test: craft a valid frame with a
wrong sid → 1006 + Closed. (Go already enforces this per TASK-019 —
symmetry is required for the matrix.)

### FR-3 peer binaries

- `map-cli interop --role {dial|listen} --addr HOST:PORT --seed HEX32
  --peer-seed HEX32 --scenario NAME [--delegate]` (extend map-cli; reuse
  its existing arg style).
- `map-go/cmd/map-interop` with the same flags.

Identical contract: run ONE scenario, print `OK <scenario>` or
`FAIL <scenario>: <detail>` as the LAST stdout line, exit 0/1. Keys derive
from seeds (the harness uses the frozen vector keys A/B and the DK tier
for --delegate). `--peer-seed` gives the expected peer identity (both
sides pin each other — the matrix never runs unauthenticated).

### FR-4 scenarios (each a fresh TCP connection; listener side mirrors)

| id | check | expected |
|---|---|---|
| S1 echo | handshake; dialer sends 0xF000 {1:"ping-map"}; listener echoes content back; dialer verifies; then PING→auto-PONG each way; CLOSE(0,"done") | clean close both ends |
| S2 replay | dialer sends an echo frame, then re-sends the IDENTICAL raw frame bytes | peer answers ERROR 1003; session survives |
| S3 seq-gap | dialer seals a valid envelope with seq skipped by one (scenario code drives envelope primitives directly on the socket) | ERROR 1003 |
| S4 tamper | dialer flips one payload byte of a sealed frame | ERROR 2000 |
| S5 cross-session relay | dialer completes session A, captures an echo frame, closes; completes session B on a NEW connection and replays the captured frame | ERROR 2000 (CB differs — the §7.3 anti-relay property, live) |
| S6 version-disjoint | dialer offers only version 0x0200 | handshake ERROR 1001, connection closed |
| S7 wrong-sid | dialer sends a valid CB-bound envelope whose body key 6 is a different sid | ERROR 1006, session closed (needs FR-2) |
| S8 delegated | dialer runs with --delegate: HELLO signed by DK carrying chain [RIK→DK cert]; listener validates and reports accountable root == RIK AgentID in its OK line | clean close |

### FR-5 the matrix

Every scenario runs in BOTH orientations: (Rust dials → Go listens) and
(Go dials → Rust listens). 8 scenarios × 2 = 16 cells. Driver:
`interop/run-matrix.ps1` (PowerShell; this machine) — builds both binaries
(`cargo build -p map-cli` with the documented toolchain/PATH, `go build
./cmd/map-interop`), allocates ports, spawns listener then dialer per
cell, collects OK/FAIL, prints the 16-cell table, exits non-zero on any
FAIL. Also `interop/README.md` (how to run) and `interop/MATRIX.md` — the
ops §6.1 RECORD: date, both commit hashes, toolchain versions, the table.

### FR-6 spec status update (only on a fully green matrix)

ops §6.1 status note: "live pairwise matrix delivered (16/16), Rust↔Go,
2026-07-XX — see interop/MATRIX.md". If any cell fails, FIX THE
IMPLEMENTATION THAT IS WRONG AGAINST THE DRAFTS (or ERRATA a genuine spec
ambiguity) — never adjust a scenario to pass.

## 3. Non-Functional Requirements

No new Rust deps (std::net; map-cli already has its CLI plumbing). No new
Go deps. Scenario code may use envelope primitives directly for the
crafted frames (S3/S4/S5/S7) — that is the point of a conformance harness.
Timeouts on every socket op (5 s) so a wedged cell fails, never hangs.

## 4. System Architecture

Peer binaries are thin: scenario table → session layer. The driver is
orchestration only. Nothing here ships to production; `interop/` is a
top-level sibling of `formal/` (spec-adjacent artifacts).

## 5–9. Design / Data / Algorithms / Edge Cases

Ports: driver picks a free port per cell (bind port 0, read it back, pass
to both). Process lifetime: listener starts, prints `LISTENING <port>`
when bound (driver waits for it), dialer runs, both must exit within 15 s.
S2/S5 keep the session usable after the peer's ERROR reply (1003/2000 are
notify-not-close in the reference behavior; S7's 1006 closes).
Edge case: S5's capture must happen AFTER session A's handshake so the
frame is CB-bound to A.

## 10. Error Handling

A scenario failure prints the offending frame hex (≤ first 64 bytes) in
the FAIL detail — first-diff discipline, live edition.

## 11. Security Considerations

S5 is the live proof of the §7.3 anti-relay property that TASK-014 proved
formally (P2) and the vectors can only show statically — the matrix closes
the model-to-wire gap. S8 proves cross-language delegation-chain
validation (identity §4/§6) end to end.

## 12–14. Performance / IO

Irrelevant; correctness only.

## 15. Acceptance Criteria

- ✓ FR-2 landed with its unit test; whole Rust workspace green, zero
  warnings; map-go suite green.
- ✓ `interop/run-matrix.ps1` runs 16/16 green on this machine.
- ✓ `interop/MATRIX.md` records the run (date, commits, versions, table).
- ✓ ops §6.1 note updated per FR-6.
- ✓ Both peer binaries buildable from a clean checkout with the
  documented PATH prep (mingw for Rust, Go bin for Go).

## 16. Comprehensive Test Plan

The matrix IS the test plan; unit tests only for FR-1 (framing round-trip,
oversize reject) and FR-2 (wrong-sid → 1006).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-020 §FR-1..6, in order. Read FIRST: this package,
> TASK-019 (its FR-5 defines the session behavior both binaries rely on),
> core §6/§7/§10, and `interop/`'s absence (you create it). Rust side:
> `TcpTransport` in map-session (std::net, no new deps), the FR-2
> session-id 1006 fix + test, and the `map-cli interop` subcommand. Go
> side: `cmd/map-interop`. Then the driver script and a full matrix run on
> this machine (toolchain PATHs: memory.md §5 for Rust mingw, §7 step 13
> for Go). Record the result in interop/MATRIX.md and update ops §6.1 ONLY
> if 16/16. If a cell fails: diagnose which implementation violates the
> drafts, fix THAT side, rerun; if the drafts are genuinely ambiguous,
> STOP on that scenario and file a PENDING-ARCHITECT entry in
> map-spec/ERRATA.md with both implementations' readings. Report every
> cross-language mismatch you find even after fixing — those findings are
> this package's highest-value output.

## 18. Validation Checklist (reviewer)

```
pwsh interop/run-matrix.ps1          # 16/16, exit 0
git log --oneline -2                  # commits recorded in MATRIX.md match
cargo +stable-x86_64-pc-windows-gnu test --workspace   # (mingw on PATH)
cd map-go && go test ./...
```

## 19. Known Risks

R1: Windows firewall prompts on first listen — bind 127.0.0.1 only.
R2: flaky cells from port races — bind-0 allocation + LISTENING handshake
in the driver removes them. R3: the two sides disagree on a rule the
drafts underspecify — that is a FINDING (§17), budget time for it.
R4: map-cli may need arg-parsing extension — keep it dependency-free
(match its existing style, however minimal).

## 20. Handoff Notes

On green: memory.md §5/§7/§8 + tasks/README.md row 020; ops §6.1 note.
This completes every precondition ops §6.1 names for leaving -00 except
human review: the natural next spec action is the rev-01 editorial cut
(ops §3.4/§4), folding ERRATA resolutions into normative text.
