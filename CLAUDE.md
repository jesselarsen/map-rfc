# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@memory.md

## Current direction (2026-09 pivot) — read this first

The IETF submission track is **abandoned**. The v1 family (11 drafts + both
implementations + the 2026-09-18 external review) is frozen at tag
`map-v1-ietf`; `main` stays at that snapshot. Active work is on branch
`accountability-layer`. It reworks MAP into a thin **accountability layer for
delegated agent work** running over A2A (tasks), an ADS-style Kademlia DHT
(discovery) and MPP/x402 (payment rails), and it will be published as an
**arXiv systems paper with a quantitative evaluation**.

- Rationale, the keep/drop table and positioning:
  `docs/ADR-0001-thin-accountability-layer.md`.
- Phase plan and open decisions (D1–D7): memory.md §0.
- Reviewer exploit probes and their ported `#[ignore]` regression tests:
  `attic/probes/README.md`.

The sections below describe the v1 layout. They stay accurate until the P1
(spec) and P2 (prune) phases land, and then they are rewritten. Where this
section and the ones below conflict, this section wins.

## What this project is

MAP — the Manolito Agent Protocol — is a standards-track-style specification
family (written like IETF Internet-Drafts) for a decentralized, permissionless
network of autonomous AI agents. It evolves the Manolito/MP2P capability-routing
lineage onto a modern stack: self-certifying Ed25519 identities, deterministic
CBOR + COSE signed messages, libp2p/QUIC transports, two-plane discovery over a
sharded Kademlia DHT, evidence-based local trust, and settlement-agnostic
payments.

This repository holds **both the specification and two reference
implementations**. The specs are `map-spec/draft-map-*.md` plus the test-vector
and critique documents; `map-rs/` is a Cargo workspace, `map-go/` a Go module,
`build/` the I-D render pipeline, and `interop/` a live cross-implementation
matrix. Build and test commands are in `AGENTS.md` — a spec change that touches
the wire is not done until both implementations and the matrix agree with it.

Read [README.md](README.md) first — it has the full reading order, conformance
classes, and the "five decisions to know before reading anything." Do not
duplicate that content here; consult it directly.

## Repository layout

- `map-spec/` — the specification family itself. All normative and
  informational drafts, plus the critique that justifies them, live here:
  - `01-design-critique.md` — critical evaluation of the original source
    designs (findings C1–C15) and the disposition of every idea into the
    current family. This is the record of *why* the spec looks the way it
    does — check it before proposing something that resembles a rejected
    approach.
  - `draft-map-arch-01.md` — goals/non-goals, system & trust model, threat
    catalog TH1–TH15, zones & privacy model, scalability analysis,
    conformance classes. (Informational)
  - `draft-map-identity-01.md` — AgentID (≡ did:key ≡ libp2p PeerID), key
    hierarchy, delegation certificates, rotation/revocation, ephemeral & org
    identities, WebAuthn. (Standards)
  - `draft-map-core-01.md` — signed envelope (COSE_Sign1 over deterministic
    CBOR), negotiation & downgrade resistance, channel binding, replay
    protection, flow control, error taxonomy, state machine, transport
    bindings, extension framework. (Standards)
  - `draft-map-discovery-01.md` — descriptors, governed capability ontology,
    two-plane discovery (stable DHT / volatile status), hot-key sharding, NAT
    profile, query privacy. (Standards)
  - `draft-map-tasks-01.md` — offer/quote/assign, streaming with credit flow
    control, tool grants, Merkle transcripts + countersigned checkpoints,
    resumption by replay, delegation, memory retrieval, task state machines.
    (Standards)
  - `draft-map-trust-01.md` — dual-signed ExecutionReceipts, selective
    evidence disclosure, admission costs, graduated bootstrap, local trust
    computation, challenges, gaming analysis. (Standards)
  - `draft-map-pay-01.md` — payment negotiation & evidence, settlement-method
    registry, metered streaming settlement, escrow abstraction, disputes.
    (Standards)
  - `draft-map-ops-01.md` — engineering policy (context/ownership map),
    registries & change control, federation gateways, conformance &
    formal-analysis program, reference implementation, migration (incl.
    HEP-0.1 → MAP), compliance notes. (BCP)
- `map-spec/test-vectors/` — `map-test-vectors.md` (TV-001 signed HELLO
  envelope, TV-002 dual-signed receipt: real Ed25519 signatures over
  deterministic CBOR, reproducible from listed seeds) plus `gen_vectors.py`,
  the Python regenerator (needs pynacl, cbor2, base58). Normative examples —
  any reference implementation must reproduce these byte-for-byte.
- `map-rs/` — the Rust reference implementation (Cargo workspace, impl #1
  for the two-implementation rule in ops §6.1; the Go impl comes later as
  impl #2). See `memory.md` §5 for crate status and known caveats.
- `tasks/` — implementation-specification packages (`TASK-00N-<slug>.md`)
  authored by the architect role and handed to downstream coding LLMs.
  See `tasks/README.md` for the role contract and execution order.
- `memory.md` — cross-session handoff: non-negotiable protocol decisions,
  current state, roadmap. Loaded via the `@memory.md` include above; keep it
  current when state changes materially.
- `input-designs/` — the original design-goals and design-prompt documents
  that seeded this project in an earlier session. Treat as historical input,
  not a source of truth to reconcile against —
  `map-spec/01-design-critique.md` already did that reconciliation.
- `.claude/rules/` — reference material on data-intensive application design
  and domain-driven design; background reading, not repo-specific policy.

## Role model for implementation work

Claude in this repo acts as **Principal Software Architect / Senior Staff
Engineer**: it reviews the specs, does the design analysis, and produces
complete implementation-specification packages under `tasks/` for downstream
coding LLMs to execute. It does **not** write the production Rust/Go
implementation itself unless explicitly instructed. Each package must be
self-contained (embedded CDDL, wire constants, invariants, vector hex) so the
coding model needs no clarifying questions. When an implementation exposes a
spec ambiguity, the spec is patched in the same change (spec-feedback loop —
precedent: discovery §6 shard fix, tasks §8.1 Merkle fix).

**If you are a NEW architect session: check `map-spec/ERRATA.md` FIRST**
(if it exists) — it queues PENDING-ARCHITECT items recorded while the
architect role was unstaffed, per the protocol in `tasks/README.md`
("Operating without the architect"). Resolve that queue before authoring
new work. TASK-001..016 cover the entire memory.md §8 roadmap;
`tasks/README.md` has the dependency graph and the post-roadmap frontier.

## Working conventions

- **Naming**: draft files follow `draft-map-<area>-<revision>.md`
  (e.g. `draft-map-core-01.md`). New areas get a new `<area>` slug; revisions
  of an existing document bump the two-digit `<revision>` suffix per the
  versioning/deprecation process defined in `draft-map-ops-01.md`.
- **Requirements language**: BCP 14 (RFC 2119 / RFC 8174), unweakened — use
  MUST / SHOULD / MAY precisely, and don't introduce softer synonyms.
- **RFC structure**: each Standards-track document follows Internet-Draft
  conventions — Abstract, Status of this Memo, Terminology, Requirements
  Language, Architecture/Protocol Overview, Message Formats, State Machines,
  Error Handling, Security Considerations, Privacy Considerations, IANA
  Considerations, Examples, Normative/Informative References — as established
  in the existing drafts. Match the section style of the document you're
  editing rather than inventing a new structure.
- **One named source of truth per fact** (engineering policy in
  `draft-map-ops-01.md` §2): don't restate a normative definition in more than
  one document — cross-reference the owning document instead.
- **No self-reported trust, ever**: trust/reputation content must derive from
  dual-signed receipts and local projections, never self-asserted scores.
  This was Critical Finding C1 in `01-design-critique.md` — do not
  reintroduce it.
- **Signatures cover received bytes** of deterministic CBOR (COSE_Sign1),
  never a re-serialization. Protobuf is only an optional bulk carrier, not
  the canonical wire format.
- Changes that touch a cross-cutting decision (the "five decisions to know"
  in the README, or anything in the engineering policy table in
  `draft-map-ops-01.md`) should be checked against every document that
  depends on it, not just the one being edited.
- No document leaves draft status without two independent interoperating
  implementations (per `draft-map-ops-01.md` §6.1) — don't mark a draft as
  final/stable unilaterally.
