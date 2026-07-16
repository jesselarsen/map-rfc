# MAP — The Manolito Agent Protocol (specification family, rev 01)

A standards-track-style specification family for a decentralized,
permissionless network of autonomous AI agents, evolving the
capability-routing spirit of the Manolito (MP2P) network onto a modern
stack: self-certifying Ed25519 identities, deterministic CBOR + COSE
signed messages, libp2p/QUIC transports, two-plane discovery over a
sharded Kademlia DHT, evidence-based local trust, and settlement-agnostic
payments.

Status: preprint for community review — work in progress. Requirements
language: BCP 14 (RFC 2119 / RFC 8174), unweakened. **Every ops §6.1
precondition for leaving draft is met** (two independent implementations,
byte-exact vector parity, formal analysis of the core security properties,
and a live pairwise interop matrix) short of human review.

## Contents and reading order

| # | File | Scope | Class |
|---|------|-------|-------|
| 0 | `map-spec/01-design-critique.md` | Critical evaluation of the source designs (findings C1–C15) and disposition of every idea into the family | — |
| 1 | `map-spec/draft-map-arch-01.md` | Goals/non-goals, system & trust model, threat catalog TH1–TH15, zones & privacy model, scalability analysis, conformance classes | Informational |
| 2 | `map-spec/draft-map-identity-01.md` | AgentID (≡ did:key ≡ libp2p PeerID), key hierarchy, delegation certificates, rotation/revocation, ephemeral & org identities, WebAuthn | Standards |
| 3 | `map-spec/draft-map-core-01.md` | Signed envelope (COSE_Sign1 over deterministic CBOR), negotiation & downgrade resistance, channel binding, replay protection, flow control, error taxonomy, state machine, transport bindings, extension framework | Standards |
| 4 | `map-spec/draft-map-discovery-01.md` | Descriptors, governed capability ontology, two-plane discovery (stable DHT / volatile status), hot-key sharding, NAT profile, query privacy | Standards |
| 5 | `map-spec/draft-map-tasks-01.md` | Offer/quote/assign, streaming with credit flow control, tool grants, Merkle transcripts + countersigned checkpoints, resumption by replay, delegation, memory retrieval, task state machines | Standards |
| 6 | `map-spec/draft-map-trust-01.md` | Dual-signed ExecutionReceipts, selective evidence disclosure, admission costs, graduated bootstrap, local trust computation, challenges, gaming analysis | Standards |
| 7 | `map-spec/draft-map-pay-01.md` | Payment negotiation & evidence, settlement-method registry (credit-ledger, x402, Lightning, stablecoin, ACH, Interledger), metered streaming settlement with exact watermark billing, pause/resume/cancel, escrow abstraction, disputes | Standards |
| 8 | `map-spec/draft-map-ops-01.md` | Engineering policy (context/ownership map), registries & change control, federation gateways, conformance & formal-analysis program, reference implementation, migration (incl. HEP-0.1 → MAP), compliance notes | BCP |
| 9 | `map-spec/test-vectors/map-test-vectors.md` | TV-001…016 signed envelopes + NV-001…010 MUST-reject vectors — real Ed25519 signatures over deterministic CBOR, reproducible from listed seeds (`gen_vectors.py`) | Normative examples |
| — | `map-spec/CHANGELOG.md` | Per-revision changelog (editorial / additive / breaking) required by ops §4 | — |
| — | `map-spec/ERRATA.md` | Historical record of every architect ruling, with rationale | — |

The Rust reference implementation lives in `map-rs/` (impl #1); a Go
reference implementation lives in `map-go/` (impl #2, independently
written from the specs and vectors). Implementation-specification packages
for downstream coding agents live in `tasks/` (see `tasks/README.md`, 23
packages authored to date); the live interop harness lives in `interop/`.
`memory.md` is the cross-session project handoff file. Historical input
designs are under `input-designs/`.

Read 0 → 1 → 3 for orientation; implement in the order 2, 3, 4, 5, then 6/7.

## Reference implementations

### `map-rs/` (Rust, impl #1)

A Cargo workspace (Rust 2021) that builds and tests green with zero
warnings. The protocol runs **end-to-end**: HELLO handshake + channel
binding → descriptor publish/fetch and capability query over a libp2p
transport → an echo task (offer/quote/assign → credit-metered CHUNK
stream → COMPLETE with a Merkle transcript root) → dual-signed
ExecutionReceipt and pull-based evidence → a local trust score →
settlement, either postpaid-flat or metered-incremental with exact
per-token/per-second billing, over `pay:credit-ledger` (with `pay:x402`
registered and its adapter in progress) → a federation gateway fronting
external backends under the G-rules. Delegation chains, real Ollama
inference, and formal verification of the handshake/delegation security
properties are all implemented and tested.

| Crate | Role |
|-------|------|
| `map-identity` | AgentID, Ed25519 keypairs, strict verify |
| `map-core` | deterministic CBOR, COSE_Sign1 envelope, HELLO negotiation, replay cache, error taxonomy |
| `map-discovery` | descriptors, capability keys, digest-byte sharding, seq-LWW cache |
| `map-tasks` | TaskID, Merkle transcript, flow credit, requester FSM, task message codecs |
| `map-trust` | ExecutionReceipts, append-only store, rebuildable Beta projection, countersign tolerance |
| `map-pay` | decimal amounts/assets, settlement ledger, payment + metered-watermark codecs, exact billing arithmetic |
| `map-identity`/`map-core::delegation` | DelegationCert issuance/validation, tighten-only attenuation, caveats |
| `map-session` | session layer over pluggable transports (in-memory duplex, TCP), state machine, seq/replay/size enforcement, session-id binding |
| `map-net` | libp2p `/map/1.0.0` transport, kad-backed descriptor index |
| `map-mesh` | descriptor publish/ingest, CAP/STATUS flows over sessions |
| `map-taskrun` | pump-driven provider/requester runtimes: task execution, receipts, evidence, postpaid + metered-incremental payment flows |
| `map-cli` | `keygen`, projection-rebuild demo, `serve-ollama` (feature-gated), `interop` (the live-matrix peer) |
| `map-conformance` | byte-exact TV-001…016 + the full NV-001…010 MUST-reject catalog |
| `adapters/ollama-provider`, `adapters/gateway` | real inference backend (feature-gated); federation provenance + admission gate |
| `formal/` | ProVerif models proving downgrade resistance and delegation attenuation (spec-adjacent, not a crate) |

Build and test:

```
cd map-rs
$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"   # mingw needed once map-net (libp2p) is in the tree
cargo +stable-x86_64-pc-windows-gnu build --workspace      # zero warnings
cargo +stable-x86_64-pc-windows-gnu test  --workspace      # unit + integration + conformance
cargo run -p map-cli -- keygen
```

### `map-go/` (Go, impl #2)

A zero-external-dependency Go module, written from the specs and frozen
vectors under the two-implementation independence rule (no `map-rs`
source consulted). Implements the deterministic CBOR codec, envelope
seal/open with strict Ed25519 verification, delegation-chain validation,
and a full session layer (HELLO negotiation, channel binding, replay/seq
enforcement, TCP framing) — everything needed to complete a live MAP
session against the Rust peer.

```
cd map-go
go vet ./... && gofmt -l . && go test ./...
```

## Live interop matrix

`interop/` runs the actual ops §6.1 gate: `map-rs` and `map-go` complete
real MAP sessions against each other over TCP, in both orientations,
across 8 scenarios (echo, replay, sequence-gap, tamper, cross-session
relay, version-disjoint handshake, foreign-session-id rejection, and a
delegated handshake) — **16/16 passing**. See `interop/MATRIX.md` for the
recorded run and `interop/README.md` to reproduce it:

```
pwsh interop/run-matrix.ps1
```

Invariants enforced in code (both implementations): signatures verify
over received bytes (never a re-serialization); no single-signed or
unverified receipt can reach a trust projection; derived state (trust
scores, the mesh index) is rebuildable from its evidence; payments —
postpaid or metered — move money at most once per invoice, and metered
billing arithmetic is exact with no rounding rule anywhere in the family.
Both conformance suites reproduce every published test vector byte-for-byte
and reject every negative vector with its required error class.

## Conformance classes

MAP-Core (required) · MAP-Discovery · MAP-Tasks · MAP-Trust · MAP-Pay ·
MAP-Fed. A conforming provider or requester implements
Core + Discovery + Tasks; Trust and Pay are RECOMMENDED on public meshes.
No document leaves draft status without two independent interoperating
implementations passing the full matrix (ops §6.1) — **met**, see above.

## Engineering policy

The family is bound by an explicit data-semantics and bounded-context
policy (ops §2): one named source of truth per fact; events are past-tense
facts, commands are requests; idempotency wherever delivery is uncertain
(TaskID, invoice-id, msg-id, content addresses); ordering guarantees
scoped and written down; derived state (trust scores, DHT index, caches)
rebuildable by construction; schema evolution additive within a major
version with immutable registry assignments. Changes violating the policy
table are rejected in review. `map-spec/CHANGELOG.md` classifies every
change to the specs as editorial, additive, or breaking; there have been
no breaking changes since rev 00.

## Five decisions to know before reading anything

1. Signatures cover received bytes of deterministic CBOR (COSE_Sign1) —
   never a re-serialization; Protobuf survives only as an optional bulk
   carrier.
2. No self-reported trust anywhere; trust is a local projection over
   dual-signed receipts.
3. Two-plane discovery: stable signed descriptors in a sharded DHT;
   volatile load/TTFT pulled on demand. Latency is never advertised.
4. Hard constraints are protocol; ranking is local policy.
5. Zones 0–3 gate where payloads may flow, normatively.

## Status and what's left

The core protocol — identity, envelope, negotiation, discovery, tasks,
trust, and payments (both postpaid and metered-incremental) — is
specified, implemented twice, and proven to interoperate live. Deliberately
out of scope for this revision: prepaid/escrow settlement reordering
(pay §5.2), the dispute protocol (pay §6), and a Go task/session runtime
beyond the interop harness. The one open implementation package is
`tasks/TASK-023-x402-settlement-adapter.md` (a feature-gated adapter for
the `pay:x402` settlement method, kept isolated so its EVM-signing
dependency never touches the default build).

License: Apache-2.0. Naming: "MAP" honors the MP2P
lineage; "Hermes" remains free as an implementation name.
