# MAP demo agents & task interop

Two real task-answerers prove the MAP task protocol (tasks §1–§13) over a
live session, in both implementations:

| agent | impl | capability | transport |
|---|---|---|---|
| `MAP-Excuse-As-A-Service` (`map-go/cmd/map-excuse`) | Go (impl #2) | `map:cap:demo/excuse` | libp2p/QUIC |
| `rfc-title-o-matic` (`map-rs`, bin of `map-cli`) | Rust (impl #1) | `map:cap:demo/rfc-title` | libp2p/QUIC |
| `map-cli interop --transport libp2p` (Rust `map-rs`) | Rust (impl #1) | `map:cap:demo/excuse` | libp2p/QUIC |

Both peers serve the same capability (`map:cap:demo/excuse`) with a
deterministic executor, so the live interop matrix (`run-matrix.ps1`) exercises
the task protocol over libp2p/QUIC in **both orientations** (rust→go and
go→rust) across three scenarios — `task-stream`, `task-multichunk`,
`task-transcript` — each verifying the Merkle transcript root on the requester.
The printed roots match byte-for-byte across implementations (cross-impl
parity, TV-008).

`rfc-title-o-matic` is the standalone Rust provider binary (Phase D): a real
task-answerer for `map:cap:demo/rfc-title` (a deterministic RFC-title
generator), wire-compatible with the Go `map-excuse` task protocol. It listens
on libp2p/QUIC, accepts sessions, runs one `ProviderTask` per session, and
publishes a discovery descriptor (best-effort; skipped without a DHT bootstrap).
The capability-agnostic Go `map-requester` dials it directly by multiaddr.

## Run the Go demo manually

Two 32-byte hex seeds (dialer, listener). Pick any; keys derive
deterministically (identity §3).

```powershell
cd map-go
go build -o $env:TEMP/map-excuse.exe ./cmd/map-excuse
go build -o $env:TEMP/map-requester.exe ./cmd/map-requester

# listener (provider) — prints LISTENING <multiaddr>/p2p/<peerid>
Start-Process $env:TEMP/map-excuse.exe `
  -ArgumentList "--seed","<LISTENER_SEED>","--listen","/ip4/0.0.0.0/udp/0/quic-v1","--cap","map:cap:demo/excuse" `
  -RedirectStandardOutput listener.txt

# read the LISTENING line, then dial it
$addr = (Select-String -Path listener.txt -Pattern 'LISTENING\s+(\S+)').Matches[0].Groups[1].Value
& $env:TEMP/map-requester.exe --seed <DIALER_SEED> --addr $addr --cap map:cap:demo/excuse --input "traffic"
# prints: OK result: map:cap:demo/excuse
#         OK transcript-root: <hex>
```

## Run the Rust provider manually (cross-impl)

`rfc-title-o-matic` is a Rust binary of the `map-cli` crate. It answers
`map:cap:demo/rfc-title`; the Go `map-requester` dials it with no change
(the requester is capability-agnostic).

```powershell
cd map-rs
cargo +stable-x86_64-pc-windows-gnu build -p map-cli --bin rfc-title-o-matic
$bin = "target/debug/rfc-title-o-matic.exe"

# listener (Rust provider) - prints LISTENING <multiaddr>/p2p/<peerid>
Start-Process $bin `
  -ArgumentList "--seed","<LISTENER_SEED>","--listen","/ip4/0.0.0.0/udp/0/quic-v1","--cap","map:cap:demo/rfc-title" `
  -RedirectStandardOutput rust-listener.txt

$addr = (Select-String -Path rust-listener.txt -Pattern 'LISTENING\s+(\S+)').Matches[0].Groups[1].Value
& $env:TEMP/map-requester.exe --seed <DIALER_SEED> --addr $addr --cap map:cap:demo/rfc-title --input "mesh congestion"
# prints: OK result: map:cap:demo/rfc-title
#         OK transcript-root: f1752ebae4594c2f0415694143bf881f53230d7669b4ebbb17545ab58218b7d4
```

## Run the full interop matrix

```powershell
pwsh interop/run-matrix.ps1
```

Builds both peers, runs the 8 core-session scenarios × 2 orientations over
TCP (16 cells) **plus** the 3 task scenarios × 2 orientations (rust↔go and
go↔rust) over libp2p/QUIC (6 cells), and exits 0 only if all pass. Latest
result: **16/16 core + 6/6 task PASS**.

## Wire contract (cross-impl)

The transcript codec is cross-impl-verified by
`map-go/tasks/transcript_test.go` against the canonical Python oracle roots
(see `map-spec/test-vectors/map-test-vectors.md` TV-008). Because the
session framing, envelope, and transcript bytes are identical by
construction, the same task scenarios run Rust ↔ Go over the shared libp2p
task-mode.

**Channel-binding caveat.** The `/map/1.0.0` sessions in these demos use the
identity-pair exporter stand-in (`initiator-AgentID ‖ responder-AgentID`) of
core §10 — rust-libp2p exposes no portable Noise handshake-hash / TLS
exporter on its stream API. This is weaker than a true key-schedule exporter
(binds the identity pair, not the connection instance) and is a known
pre-submission maturity limitation; see core §7.3 / §10 / §14. A deployment
needing instance-level channel binding should run MAP over an
exporter-bearing transport (raw QUIC / TLS 1.3) — the CB derivation is
transport-agnostic, so no wire change is required.
