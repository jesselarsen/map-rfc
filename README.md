# MAP — The Manolito Agent Protocol (specification family, rev 00)

A standards-track-style specification family for a decentralized,
permissionless network of autonomous AI agents, evolving the
capability-routing spirit of the Manolito (MP2P) network onto a modern
stack: self-certifying Ed25519 identities, deterministic CBOR + COSE
signed messages, libp2p/QUIC transports, two-plane discovery over a
sharded Kademlia DHT, evidence-based local trust, and settlement-agnostic
payments.

Status: preprint for community review — work in progress. Requirements
language: BCP 14 (RFC 2119 / RFC 8174), unweakened.

## Contents and reading order

| # | File | Scope | Class |
|---|------|-------|-------|
| 0 | `map-spec/01-design-critique.md` | Critical evaluation of the source designs (findings C1–C15) and disposition of every idea into the family | — |
| 1 | `map-spec/draft-map-arch-00.md` | Goals/non-goals, system & trust model, threat catalog TH1–TH15, zones & privacy model, scalability analysis, conformance classes | Informational |
| 2 | `map-spec/draft-map-identity-00.md` | AgentID (≡ did:key ≡ libp2p PeerID), key hierarchy, delegation certificates, rotation/revocation, ephemeral & org identities, WebAuthn | Standards |
| 3 | `map-spec/draft-map-core-00.md` | Signed envelope (COSE_Sign1 over deterministic CBOR), negotiation & downgrade resistance, channel binding, replay protection, flow control, error taxonomy, state machine, transport bindings, extension framework | Standards |
| 4 | `map-spec/draft-map-discovery-00.md` | Descriptors, governed capability ontology, two-plane discovery (stable DHT / volatile status), hot-key sharding, NAT profile, query privacy | Standards |
| 5 | `map-spec/draft-map-tasks-00.md` | Offer/quote/assign, streaming with credit flow control, tool grants, Merkle transcripts + countersigned checkpoints, resumption by replay, delegation, memory retrieval, task state machines | Standards |
| 6 | `map-spec/draft-map-trust-00.md` | Dual-signed ExecutionReceipts, selective evidence disclosure, admission costs, graduated bootstrap, local trust computation, challenges, gaming analysis | Standards |
| 7 | `map-spec/draft-map-pay-00.md` | Payment negotiation & evidence, settlement-method registry, metered streaming settlement, escrow abstraction, disputes | Standards |
| 8 | `map-spec/draft-map-ops-00.md` | Engineering policy (context/ownership map), registries & change control, federation gateways, conformance & formal-analysis program, reference implementation, migration (incl. HEP-0.1 → MAP), compliance notes | BCP |
| 9 | `map-spec/test-vectors/map-test-vectors.md` | TV-001 signed HELLO envelope, TV-002 dual-signed receipt — real Ed25519 signatures over deterministic CBOR; reproducible from listed seeds (`gen_vectors.py`) | Normative examples |

The Rust reference implementation lives in `map-rs/` (Cargo workspace,
impl #1 for the two-implementation rule). Implementation-specification
packages for downstream coding agents live in `tasks/` (see
`tasks/README.md`). `memory.md` is the cross-session project handoff
file. Historical input designs are under `input-designs/`.

Read 0 → 1 → 3 for orientation; implement in the order 2, 3, 4, 5, then 6/7.

## Conformance classes

MAP-Core (required) · MAP-Discovery · MAP-Tasks · MAP-Trust · MAP-Pay ·
MAP-Fed. A conforming provider or requester implements
Core + Discovery + Tasks; Trust and Pay are RECOMMENDED on public meshes.
No document leaves draft status without two independent interoperating
implementations (ops §6.1).

## Engineering policy

The family is bound by an explicit data-semantics and bounded-context
policy (ops §2): one named source of truth per fact; events are past-tense
facts, commands are requests; idempotency wherever delivery is uncertain
(TaskID, invoice-id, msg-id, content addresses); ordering guarantees
scoped and written down; derived state (trust scores, DHT index, caches)
rebuildable by construction; schema evolution additive within a major
version with immutable registry assignments. Changes violating the policy
table are rejected in review.

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

License: MIT OR Apache-2.0 (proposed). Naming: "MAP" honors the MP2P
lineage; "Hermes" remains free as an implementation name.
