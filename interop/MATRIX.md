# MAP live interop matrix — the ops §6.1 record

Ops §6.1 requires **two independent implementations passing the full matrix
against each other** before any document leaves draft. Vector parity
(TASK-015/018) proved byte-level agreement statically; this is the LIVE
proof: real MAP sessions completed over TCP, in both orientations, across
the scenario set below.

## Latest run

- **Date:** 2026-07-17
- **Result:** **16 / 16 PASS**
- **Implementations:** `map-rs` (Rust, impl #1) ↔ `map-go` (Go, impl #2)
- **Commit:** `ac9d4ce` (spec review Items 1–9; documentation-only — no wire/code change)
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
| delegated | PASS | PASS | delegated HELLO (chain in ProtHdr key 33) validates cross-impl; accountable identity resolves to the chain root (identity §4/§6) |

Each scenario runs in both orientations (dialer/listener swapped), so the
table's 8 rows × 2 columns = the 16 cells.

## Scenario contract

Both peers (`map-cli interop` and `map-go/cmd/map-interop`) implement the
same contract: one scenario per invocation, `--role dial|listen`, keys
derived from 32-byte `--seed`/`--peer-seed` (and `--issuer-seed` for the
delegation root under `delegated`), a listener prints `LISTENING <addr>`
when bound, and every run's last stdout line is `OK <scenario>` or
`FAIL <scenario>: <detail>`. The adversarial frames (seqgap, tamper,
wrongsid, relay) are crafted at the envelope layer using each session's
public channel-binding accessor plus the `send_raw` harness hook.

## Notes / findings

- The per-frame **SessionID → 1006** check (core §6.1) was implemented in
  Go first (TASK-019) and added to the Rust reference in this package
  (TASK-020 FR-2, map-session `recv`) — the `wrongsid` cells are its live
  cross-impl proof. Before the fix, Rust silently accepted a foreign
  SessionID post-handshake.
- The interop is intentionally scoped to the core session envelope
  (handshake + framed in-session messages + delegation). Task/receipt/
  payment flows over a live cross-language session remain future work; they
  layer on this session without changing it (I2).

## Reproducing

```
pwsh interop/run-matrix.ps1
```

Needs Go on PATH (`C:\Program Files\Go\bin`) and the GNU Rust toolchain
with mingw on PATH (`C:\msys64\mingw64\bin`) — see `memory.md` §5/§7.
