# MAP — Mutual Attestation Protocol

MAP is an **accountability layer for delegated agent work**. When one agent
hands a task to another, MAP adds:

- **Dual-signed execution receipts.** Requester and provider both sign the
  exact same bytes about what happened.
- **Local trust.** Each observer derives trust only from evidence it can
  verify, and can always rebuild its scores from its own receipt store. There
  are no self-reported scores and no global ledger.
- **Requester-countersigned checkpoints.** A task can resume on a *different*
  provider after a failure, and the failed provider is paid only for work the
  requester acknowledged.
- **Metering bound to evidence.** The payment-rail voucher is linked to the
  checkpoint or receipt it pays for.
- **Data-sensitivity zones** as dual-signed, attributable labels.
- **Two-plane discovery.** Signed descriptors that embed the provider's A2A
  Agent Card are stored in a sharded DHT; volatile status is pulled on demand.

MAP runs **over** existing ecosystems instead of replacing them:
[A2A](https://a2a-protocol.org/) carries the task, an ADS-compatible Kademlia
DHT carries discovery records, and [MPP](https://mpp.dev/) sessions or
[x402 V2](https://x402.org/) move the money.

> **Status: 2.0-draft, under active rework.** The v1 family (11 IETF-style
> drafts, a full protocol stack, and a 2026-09-18 external review) is frozen
> at git tag [`map-v1-ietf`](https://github.com/jesselarsen/map-rfc/tree/map-v1-ietf). Why it changed:
> [`docs/ADR-0001-thin-accountability-layer.md`](docs/ADR-0001-thin-accountability-layer.md).
> The goal is an arXiv systems paper with a quantitative evaluation.

## Specification

| Document | What it is |
|---|---|
| [`spec/layer.md`](spec/layer.md) | The layer: identity, encoding, transcripts, receipts, checkpoints and resumption, local trust, metering, zones, discovery records, threat model (normative) |
| [`spec/binding-a2a.md`](spec/binding-a2a.md) | MAP as an A2A v1.0 extension: artifact carriage, TaskID derivation, transcript mapping, state mapping, resumption (normative) |
| [`spec/binding-libp2p-discovery.md`](spec/binding-libp2p-discovery.md) | Discovery and status over libp2p Kademlia, ADS relationship (normative) |
| [`spec/mapping-payment-rails.md`](spec/mapping-payment-rails.md) | Binding metering evidence to MPP sessions and x402 V2 (informative) |
| [`spec/test-vectors/`](spec/test-vectors/) | Frozen byte-exact vectors and the Python regenerator |

## Implementations

- `map-rs/`: Rust reference implementation (Cargo workspace).
- `map-go/`: Go second implementation. Written from the spec and vectors
  only, never from the Rust source; its protocol core is stdlib-only.

The Rust workspace has been pruned to the kept layers (P2). The Go module
still carries its v1 task/pay packages until P5. See `memory.md` §0 for the
phase plan and current state.

```powershell
# Rust (Windows: mingw on PATH + gnu toolchain)
$env:PATH = "C:\msys64\mingw64\bin;" + $env:PATH
cd map-rs; cargo +stable-x86_64-pc-windows-gnu test --workspace
# Go
cd map-go; gofmt -l . ; go vet ./... ; go test ./...
# Live cross-implementation matrix
pwsh interop/run-matrix.ps1
```

## Security findings and regression tests

The 2026-09-18 review demonstrated exploits against v1. They are preserved
and reproducible in [`attic/probes/`](attic/probes/). Those in layers MAP
keeps are ported as `#[ignore]` regression tests that assert the secure
behaviour: receipt farming, shard flooding, replay-cache cost, and node-ID
grinding (a measurement). They flip to passing as the fixes land:

```powershell
cd map-rs; cargo +stable-x86_64-pc-windows-gnu test --workspace -- --ignored
```

## Repository layout

- `spec/`: the current specification and vectors
- `docs/`: ADRs; `docs/history/` holds the v1 errata, changelog, design
  critique and review process
- `attic/probes/`: the reviewers' exploit crates (standalone)
- `map-rs/`, `map-go/`: implementations
- `interop/`: the live Rust↔Go matrix
- `formal/`: ProVerif models
- `tasks/`: v1 implementation packages (historical)
- `MAP Review Results/`: the 2026-09-18 reviews

License: Apache-2.0.
