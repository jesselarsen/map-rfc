# MAP live interop matrix — the ops §6.1 record

Ops §6.1 requires **two independent implementations passing the full matrix
against each other** before any document leaves draft. Vector parity
(TASK-015/018) proved byte-level agreement statically; this is the LIVE
proof: real MAP sessions completed over TCP, in both orientations, across
the scenario set below.

## Latest run

- **Date:** 2026-07-17
- **Result:** **16 / 16 core (TCP) + 6 / 6 task + 4 / 4 pay (libp2p/QUIC) PASS**
- **Implementations:** `map-rs` (Rust, impl #1) ↔ `map-go` (Go, impl #2)
- **Commit:** `a1c45c5` (post-review hardening: CB-exporter security note +
  `map-cli` `libp2p-interop` feature gate — implementation/doc only, no wire
  change; re-verified at the default `map-cli` build, feature ON)
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

## Task matrix (libp2p/QUIC, both orientations)

In addition to the core-session matrix above, the driver now runs the **task
protocol (tasks §1–§13) over a real libp2p/QUIC session** in both orientations
to prove the full task runtime interoperates across the two implementations
over the libp2p transport binding (core §10 / net). The wire format is
identical by construction — the transcript codec is cross-impl-verified in
`map-go/tasks/transcript_test.go` (TV-008) — and the transcript roots printed
by each side match byte-for-byte.

- **Date added:** 2026-07-17
- **Result:** **6 / 6 PASS** (task-stream, task-multichunk, task-transcript ×
  rust→go and go→rust)
- **Transport:** libp2p/QUIC loopback (`/ip4/127.0.0.1/udp/0/quic-v1`),
  `/map/1.0.0` substream protocol, 4-byte big-endian framing, channel-binding
  exporter = initiator-AgentID ‖ responder-AgentID (FR-4); Noise-authenticated
  PeerID ≡ AgentID (FR-3).
- **Capability:** `map:cap:demo/excuse` (Go demo "MAP-Excuse-As-A-Service" /
  Rust `map-cli interop` libp2p task-mode).
- **Channel-binding caveat:** the `/map/1.0.0` exporter above is the
  identity-pair stand-in of core §10 (rust-libp2p exposes no portable Noise
  handshake-hash / TLS-exporter on its stream API), which is **weaker than a
  true key-schedule exporter** — it binds the identity pair, not the specific
  connection instance. It is sufficient for this matrix (single connection per
  pair; msg-id replay cache + monotonic seq carry instance separation) but is
  a known pre-submission maturity limitation, analyzed in core §7.3 / §10 /
  §14. The CB derivation is transport-agnostic, so a stronger exporter drops
  in without a wire change.

| scenario | rust→go | go→rust | what it proves |
|---|---|---|---|
| task-stream | PASS | PASS | OFFER→QUOTE→ASSIGN→ACCEPT + 1 CHUNK→COMPLETE, transcript root verified on requester |
| task-multichunk | PASS | PASS | 20 CHUNK transcript, flow-credit streaming + requester re-grant, root verified |
| task-transcript | PASS | PASS | mixed-kind (text/byte/text) transcript root verified end-to-end |

## Pay matrix (libp2p/QUIC, both orientations)

The pay family (negotiation + `pay:credit-ledger` settlement, pay §2–§5) runs
live over the same libp2p/QUIC session in both orientations, after the task
transcript verifies. This closes the ops §6.1 gate for the pay family — the
codec is cross-impl byte-verified by the frozen pay vectors (TV-010b / TV-013 /
TV-016 / TV-017 / TV-018), and both implementations settle the same flows.

- **Date added:** 2026-07-18
- **Result:** **4 / 4 PASS** (pay-postpaid, pay-metered × rust→go and go→rust)
- **Runtimes:** Rust `map-taskrun::{payments,metered}` ↔ Go `map-go/pay`
  (pure-stdlib port, TASK-029). Requester hosts the in-process credit-ledger
  operator (`--operator-seed`), pre-credited.

| scenario | rust→go | go→rust | what it proves |
|---|---|---|---|
| pay-postpaid | PASS | PASS | PAY_QUOTE→AGREE→INVOICE→PROOF(credit-ledger entry)→RECEIPT; exactly one settlement, cross-impl `verify_entry` |
| pay-metered | PASS | PASS | 2 incremental watermark invoices (kind-3 usage CHUNK + PAY_INVOICE), six §4.1 checks, monotonic watermarks, delta settlement |

## Notes / findings

- The per-frame **SessionID → 1006** check (core §6.1) was implemented in
  Go first (TASK-019) and added to the Rust reference in this package
  (TASK-020 FR-2, map-session `recv`) — the `wrongsid` cells are its live
  cross-impl proof. Before the fix, Rust silently accepted a foreign
  SessionID post-handshake.
- The TCP core-session matrix (above) confirms the two implementations
  interoperate on the envelope. The libp2p task matrix (above) confirms the
  full task protocol interoperates across both implementations over the
  libp2p transport in both orientations. The task wire format is proven
  cross-impl via the transcript test vectors (TV-008) — the roots printed by
  each side match byte-for-byte.

## Reproducing

```
pwsh interop/run-matrix.ps1
```

Needs Go on PATH (`C:\Program Files\Go\bin`) and the GNU Rust toolchain
with mingw on PATH (`C:\msys64\mingw64\bin`) — see `memory.md` §5/§7.
