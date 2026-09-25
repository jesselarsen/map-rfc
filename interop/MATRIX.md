# MAP live interop matrix

The live cross-implementation record: real MAP sessions between `map-rs` and
`map-go`, in both orientations, across the scenario set below. (Both
implementations are by one author under a source-isolation rule; the Go side
is written from the spec and vectors, never from Rust source.)

## Latest run

- **Date:** 2026-09-25
- **Result:** **14 / 14 core (TCP) PASS** (the `delegated` cell was retired
  with delegation in P3, layer §3.3)
- **Implementations:** `map-rs` (Rust) ↔ `map-go` (Go)
- **Commit:** the accountability-layer P2 commit (Rust prune: v1 task and
  pay runtimes removed; the Rust interop peer now serves only the core
  cells). The previous record, 26/26 including libp2p task/pay cells, is at
  `ed26742` / tag `map-v1-ietf`.
- **Toolchains:** Go 1.26.5 (windows/amd64); rustc 1.96.0
  (stable-x86_64-pc-windows-gnu)
- **Transport:** framed TCP loopback (127.0.0.1), 4-byte big-endian length
  prefix, channel-binding exporter = initiator-AgentID ‖ responder-AgentID
  (the core §10 identity-pair binding). Test-harness transport only — raw
  TCP does not authenticate the static keys; deployment bindings are core
  §10's TLS/QUIC/libp2p.
- **Driver:** [`run-matrix.ps1`](run-matrix.ps1) — builds both peers, runs
  every scenario × orientation, exits non-zero on any failure.

| scenario | rust→go | go→rust | what it proves |
|---|---|---|---|
| echo | PASS | PASS | handshake, negotiation, CB-bound in-session exchange, PING auto-PONG, CLOSE |
| replay | PASS | PASS | duplicate msg-id rejected (1003), session survives (core §7.1) |
| seqgap | PASS | PASS | non-dense sequence rejected (1003) (core §7.2) |
| tamper | PASS | PASS | flipped signature byte rejected (2000) over received bytes (I1) |
| relay | PASS | PASS | a frame CB-bound to session A fails in session B (2000) — the live §7.3 anti-relay property |
| versiondisjoint | PASS | PASS | no mutual version → handshake ERROR 1001, both sides |
| wrongsid | PASS | PASS | in-session frame with a foreign SessionID → 1006, session closed (core §6.1) |

Each scenario runs in both orientations (dialer/listener swapped), so the
table's 7 rows × 2 columns = the 14 cells.

## Scenario contract

Both peers (`map-cli interop` and `map-go/cmd/map-interop`) implement the
same contract: one scenario per invocation, `--role dial|listen`, keys
derived from 32-byte `--seed`/`--peer-seed`, a listener prints `LISTENING <addr>`
when bound, and every run's last stdout line is `OK <scenario>` or
`FAIL <scenario>: <detail>`. The adversarial frames (seqgap, tamper,
wrongsid, relay) are crafted at the envelope layer using each session's
public channel-binding accessor plus the `send_raw` harness hook.

## Retired v1 cells

The v1 libp2p/QUIC **task** matrix (task-stream, task-multichunk,
task-transcript; 6 cells) and **pay** matrix (pay-postpaid, pay-metered;
4 cells) were retired in P2 of the accountability-layer rework, together
with the v1 task and settlement protocols they exercised. Their record
(26/26 at `ed26742`) is preserved at tag `map-v1-ietf`. Cross-implementation
cells for the A2A binding (receipts, countersigned checkpoints, resumption)
are added in P5.

## Notes / findings

- The per-frame **SessionID → 1006** check (core §6.1) was implemented in
  Go first (TASK-019) and added to the Rust reference in this package
  (TASK-020 FR-2, map-session `recv`) — the `wrongsid` cells are its live
  cross-impl proof. Before the fix, Rust silently accepted a foreign
  SessionID post-handshake.
- The TCP core-session matrix (above) confirms the two implementations
  interoperate on the envelope and session layer, which the v2 libp2p
  discovery binding still uses for STATUS traffic.

## Reproducing

```
pwsh interop/run-matrix.ps1
```

Needs Go on PATH (`C:\Program Files\Go\bin`) and the GNU Rust toolchain
with mingw on PATH (`C:\msys64\mingw64\bin`) — see `memory.md` §5/§7.
