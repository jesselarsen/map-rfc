# MAP demo agents & task interop

> **Retired (v1).** The Rust demos below (`rfc-title-o-matic`, the libp2p
> task/pay interop modes) were removed with the v1 task and settlement
> protocols in P2 of the accountability-layer rework; they run from tag
> `map-v1-ietf`. The Go demos still build until the Go prune (P5). The A2A
> provider/requester demo replaces them in P4.

Two real task-answerers prove the MAP task protocol (tasks §1–§13) over a
live session, in both implementations:

| agent | impl | capability | transport | payment |
|---|---|---|---|---|
| `MAP-Excuse-As-A-Service` (`map-go/cmd/map-excuse`) | Go (impl #2) | `map:cap:demo/excuse` | libp2p/QUIC | free |
| `rfc-title-o-matic` (`map-rs`, bin of `map-cli`) | Rust (impl #1) | `map:cap:demo/rfc-title` | libp2p/QUIC | free, or **terms-1 prepaid** (`--price`, `pay:credit-ledger`) |
| `map-cli interop --transport libp2p` (Rust `map-rs`) | Rust (impl #1) | `map:cap:demo/excuse` | libp2p/QUIC | free |

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

## Run the paid demo (cross-impl): "no pay, no work"

`rfc-title-o-matic --price <cents>` turns on the **terms-1 prepaid gate**
(pay §5.2.2): the provider quotes up front and does *zero* task work until a
`PAY_RECEIPT` settles over `pay:credit-ledger`. The Go `map-requester --pay`
is the payer and hosts the in-process ledger operator (pre-credited to
`--budget`). This is a real cross-implementation payment: Rust payee ↔ Go
payer, the same pay codec both sides byte-verify (TASK-029).

The gate is TASK-024's `PrepaidProviderPayment`, unchanged. Sequencing is
**pay-then-task** (sequential): no task message is exchanged before
settlement — strictly stronger than a deferred-ASSIGN interleave, and it
composes with the Go requester's task-only `RunRequester`.

Seeds below are arbitrary 32-byte hex; the operator seed is shared by both
peers (the provider needs only the operator's AgentID to verify the ledger
entry). Amounts are integer cents (`iso4217:USD`, exponent −2).

```powershell
$PROV = "map-rs\target\debug\rfc-title-o-matic.exe"   # cargo build --bin rfc-title-o-matic
$REQ  = "$env:TEMP\map-requester.exe"                  # go build -o ... ./cmd/map-requester
$SP = "0a"*32; $SR = "0b"*32; $OP = "04"*32           # provider / requester / operator

# Priced provider: 500 cents (=$5.00) per task.
Start-Process $PROV -ArgumentList `
  "--seed",$SP,"--listen","/ip4/127.0.0.1/udp/0/quic-v1","--cap","map:cap:demo/rfc-title",`
  "--price","500","--operator-seed",$OP `
  -RedirectStandardOutput prov.txt -NoNewWindow
$addr = (Select-String prov.txt -Pattern 'LISTENING\s+(\S+)').Matches[0].Groups[1].Value
```

**(a) Paid run — settles, then delivers the title.** The requester's budget
(1000) covers the price (500):

```powershell
& $REQ --seed $SR --addr $addr --cap map:cap:demo/rfc-title --input paid-agents `
       --pay --operator-seed $OP --budget 1000
# requester prints:
#   OK paid: 500 cents (iso4217:USD)
#   OK receipt: 60606060606060606060606060606060
#   OK result: map:cap:demo/rfc-title
#   OK transcript-root: 7bd63860f000814f09af736e658d94d7e8c1d3a2624db145c16f8f09ee615137
#   OK chunks: 1 credits-granted: 0
# provider prints:
#   SETTLED 500 (iso4217:USD, -2)
#   RECEIPT 60606060606060606060606060606060
```

**(b) Over-budget run — clean refusal, no work, no money moved.** Budget
(300) is below the price (500):

```powershell
& $REQ --seed $SR --addr $addr --cap map:cap:demo/rfc-title --input paid-agents `
       --pay --operator-seed $OP --budget 300
# requester prints (exit 1):
#   FAIL request: OVER-BUDGET: quote 500 cents > budget 300 cents - refusing, no work, no payment
# provider prints only LISTENING — no SETTLED, no task ran.
```

**(c) Unpaid run — the gate holds.** A plain `map-requester` (no `--pay`)
against a priced provider gets no work:

```powershell
& $REQ --seed $SR --addr $addr --cap map:cap:demo/rfc-title --input paid-agents
# requester prints (exit 1):
#   FAIL request: session: peer_closed [0] unpaid
# provider prints (stderr):
#   GATE-HELD (unpaid): first message was 0x0200, not PAY_AGREE - no work done
# No output chunks, no transcript, no money moved.
```

**Free path unchanged.** With no `--price`, `rfc-title-o-matic` constructs no
payment object and sends no `0x04xx` frame; a plain `map-requester` gets the
title exactly as before (byte-identical transcript root `7bd6…5137` for the
same input) — the paid path is purely additive.

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
(see `spec/test-vectors/map-test-vectors.md` TV-008). Because the
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
