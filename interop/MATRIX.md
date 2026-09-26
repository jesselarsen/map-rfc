# MAP live interop matrix

The live cross-implementation record between `map-rs` (Rust) and `map-go`
(Go). Both implementations are by one author. The v1 session layer in Go
was written from the spec and vectors only; the v2 Go packages (receipts,
trust, checkpoints, metering, the A2A binding) were written with the Rust
source open (see `map-go/README.md`). The checks below therefore lean on
artifacts neither codebase produced: frozen vectors from the Python
generator, and the unmodified A2A Python SDK.

## Latest run

- **Date:** 2026-09-26
- **Result:** **28 / 28 PASS**
- **Commit:** the accountability-layer P5c commit
- **Toolchains:** Go 1.26.5 (windows/amd64); rustc 1.96.0
  (stable-x86_64-pc-windows-gnu); a2a-sdk 1.0.3 (CPython 3.12)
- **Driver:** [`run-matrix.ps1`](run-matrix.ps1): builds every peer, runs
  all three parts, exits non-zero on any failure. The SDK cells run when
  `MAP_A2A_SDK_PY` names a Python with `a2a-sdk/requirements.txt`
  installed.

| part | cells | result |
|---|---|---|
| (a) frozen vectors, per implementation | 2 | 2/2 |
| (b) A2A failover, requester × provider A × provider B | 8 | 8/8 |
| (b) plain A2A client across implementations | 2 | 2/2 |
| (b) unmodified a2a-sdk against each provider | 2 | 2/2 |
| (c) core session over TCP, both orientations | 14 | 14/14 |

### (a) Frozen vectors

Each implementation's conformance suite runs against
`spec/test-vectors/map-test-vectors.md`: the core vectors (TV-001..005,
NV-001..005, derived constants) and the v2 vectors TV-021 (dual-signed
CheckpointArtifact), NV-013/NV-014 (checkpoint rejects), TV-022 (signed
Agent Card), TV-023 (receipt admissibility, 13 cases), TV-024 (v2
descriptor envelope) and TV-025 (A2A TaskID and Part leaves).

### (b) A2A binding over HTTP

Every combination of requester, provider A and provider B in {rust, go}
runs one task. Provider A crashes after producing 10 Parts; the requester
resumes on provider B from A's last countersigned checkpoint (through
Part 8). A cell passes only if all of these hold:

- A exited with the crash code;
- B's receipt is dual-signed and carries resumed-from and a voucher digest;
- the receipt's transcript root covers all 24 Parts;
- the voucher watermarks sum to 24;
- A was paid for exactly the 8 Parts it had countersigned.

| requester | provider A | provider B | result |
|---|---|---|---|
| rust | rust | rust | PASS |
| rust | rust | go | PASS |
| rust | go | rust | PASS |
| rust | go | go | PASS |
| go | rust | rust | PASS |
| go | rust | go | PASS |
| go | go | rust | PASS |
| go | go | go | PASS |

Plain client (no `A2A-Extensions` header) → provider in the other
implementation: an ordinary `TASK_STATE_COMPLETED` task with no MAP
metadata and no pauses (A2A-28). rust → go PASS, go → rust PASS.

a2a-sdk 1.0.3 → each provider (`a2a-sdk/check_sdk.py`). The SDK:
- resolves the card and verifies its EdDSA JWS with its own
  canonicalizer;
- rejects an altered card;
- completes a task, and `GetTask` agrees;
- verifies TV-022 offline.

rust PASS, go PASS.

**Harness validation.** A planted Go-only divergence (the TaskID domain
tag changed from `MAPTaskID` to `MAPTaskId`) produced:
- (b): 2/8. Exactly the six mixed-implementation cells failed, while
  rust/rust/rust and go/go/go passed.
- (a): Go's vectors failed at TV-025.

When the TaskID disagrees, provider A rejects the countersignature. The
requester then has no resumption point and restarts on B, so the cell
flags that nothing was resumed and that A was not paid for 8 Parts.

### (c) Core session (TCP)

| scenario | rust→go | go→rust | what it proves |
|---|---|---|---|
| echo | PASS | PASS | handshake, negotiation, CB-bound in-session exchange, PING auto-PONG, CLOSE |
| replay | PASS | PASS | duplicate msg-id rejected (1003), session survives (core §7.1) |
| seqgap | PASS | PASS | non-dense sequence rejected (1003) (core §7.2) |
| tamper | PASS | PASS | flipped signature byte rejected (2000) over received bytes (I1) |
| relay | PASS | PASS | a frame CB-bound to session A fails in session B (2000), the live §7.3 anti-relay property |
| versiondisjoint | PASS | PASS | no mutual version → handshake ERROR 1001, both sides |
| wrongsid | PASS | PASS | in-session frame with a foreign SessionID → 1006, session closed (core §6.1) |

Transport: framed TCP loopback, 4-byte big-endian length prefix,
channel-binding exporter = initiator-AgentID ‖ responder-AgentID. This is a
test-harness transport only: raw TCP does not authenticate the static keys.

## Peer contracts

- **A2A (part b):** `map-a2a-provider --listen H:P --seed N [--parts 24]
  [--checkpoint-every 4] [--zones 1,2] [--fail-after-parts N]` and
  `map-a2a-requester --providers URL,URL [--seed N] [--zone Z] [--prompt S]
  [--plain]`. Both implementations take the same flags. The requester's
  last stdout line is `MAP-DEMO-RESULT {json}`. Rust bins:
  `cargo build -p map-a2a --features http`. Go:
  `map-go/cmd/map-a2a-{provider,requester}`.
- **Core (part c):** `map-cli interop` and `map-go/cmd/map-interop`,
  `--role dial|listen`, keys from `--seed`/`--peer-seed`. A listener prints
  `LISTENING <addr>`; the last line is `OK <scenario>` or
  `FAIL <scenario>: <detail>`.

## History

- 2026-09-25: 14/14 core after the v2 prune (P2/P3).
- v1 record: 26/26 including libp2p task/pay cells, at `ed26742` / tag
  `map-v1-ietf`. Those cells retired with the v1 task and settlement
  protocols.
- The per-frame SessionID → 1006 check (core §6.1) was implemented in Go
  first (TASK-019) and added to Rust in TASK-020. The `wrongsid` cells are
  its live proof.

## Reproducing

```
pwsh interop/run-matrix.ps1
```

Needs Go on PATH (`C:\Program Files\Go\bin`) and the GNU Rust toolchain
with mingw on PATH (`C:\msys64\mingw64\bin`); optionally
`MAP_A2A_SDK_PY` for the SDK cells.
