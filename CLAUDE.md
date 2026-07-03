# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

MAP — the Manolito Agent Protocol — is a standards-track-style specification
family (written like IETF Internet-Drafts) for a decentralized, permissionless
network of autonomous AI agents. It evolves the Manolito/MP2P capability-routing
lineage onto a modern stack: self-certifying Ed25519 identities, deterministic
CBOR + COSE signed messages, libp2p/QUIC transports, two-plane discovery over a
sharded Kademlia DHT, evidence-based local trust, and settlement-agnostic
payments.

This is a **documentation/spec repository**, not a software project — there is
no build, lint, or test command. The "product" is the set of `draft-map-*.md`
files plus supporting critique and test-vector documents. Work here means
editing, extending, and keeping these documents internally consistent.

Read [README.md](README.md) first — it has the full reading order, conformance
classes, and the "five decisions to know before reading anything." Do not
duplicate that content here; consult it directly.

## Repository layout

- `spec/` — the specification family itself. All normative and informational
  drafts, plus the critique that justifies them, live here:
  - `01-design-critique.md` — critical evaluation of the original source
    designs (findings C1–C15) and the disposition of every idea into the
    current family. This is the record of *why* the spec looks the way it
    does — check it before proposing something that resembles a rejected
    approach.
  - `draft-map-arch-00.md` — goals/non-goals, system & trust model, threat
    catalog TH1–TH15, zones & privacy model, scalability analysis,
    conformance classes. (Informational)
  - `draft-map-identity-00.md` — AgentID (≡ did:key ≡ libp2p PeerID), key
    hierarchy, delegation certificates, rotation/revocation, ephemeral & org
    identities, WebAuthn. (Standards)
  - `draft-map-core-00.md` — signed envelope (COSE_Sign1 over deterministic
    CBOR), negotiation & downgrade resistance, channel binding, replay
    protection, flow control, error taxonomy, state machine, transport
    bindings, extension framework. (Standards)
  - `draft-map-discovery-00.md` — descriptors, governed capability ontology,
    two-plane discovery (stable DHT / volatile status), hot-key sharding, NAT
    profile, query privacy. (Standards)
  - `draft-map-tasks-00.md` — offer/quote/assign, streaming with credit flow
    control, tool grants, Merkle transcripts + countersigned checkpoints,
    resumption by replay, delegation, memory retrieval, task state machines.
    (Standards)
  - `draft-map-trust-00.md` — dual-signed ExecutionReceipts, selective
    evidence disclosure, admission costs, graduated bootstrap, local trust
    computation, challenges, gaming analysis. (Standards)
  - `draft-map-pay-00.md` — payment negotiation & evidence, settlement-method
    registry, metered streaming settlement, escrow abstraction, disputes.
    (Standards)
  - `draft-map-ops-00.md` — engineering policy (context/ownership map),
    registries & change control, federation gateways, conformance &
    formal-analysis program, reference implementation, migration (incl.
    HEP-0.1 → MAP), compliance notes. (BCP)
- `test-vectors/map-test-vectors.md` — TV-001 signed HELLO envelope, TV-002
  dual-signed receipt: real Ed25519 signatures over deterministic CBOR,
  reproducible from listed seeds. (Normative examples). Any reference
  implementation must be validated against these vectors.
- `implementations/` — reference implementations as they get built (`rust/`
  primary, `go/` secondary — see `draft-map-ops-00.md` for the
  reference-implementation and conformance program). Currently empty
  scaffolding; see `implementations/README.md`. A conforming implementation
  should record which spec revision (`-00`, `-01`, ...) it targets, since
  `spec/` and `implementations/` version independently.
- `input-designs/` — the original design-goals and design-prompt documents
  that seeded this project in an earlier session. Treat as historical input,
  not a source of truth to reconcile against — `spec/01-design-critique.md`
  already did that reconciliation.
- `.claude/rules/` — reference material on data-intensive application design
  and domain-driven design; background reading, not repo-specific policy.

## Working conventions

- **Naming**: draft files follow `draft-map-<area>-<revision>.md`
  (e.g. `draft-map-core-00.md`). New areas get a new `<area>` slug; revisions
  of an existing document bump the two-digit `<revision>` suffix per the
  versioning/deprecation process defined in `draft-map-ops-00.md`.
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
  `draft-map-ops-00.md` §2): don't restate a normative definition in more than
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
  `draft-map-ops-00.md`) should be checked against every document that
  depends on it, not just the one being edited.
- No document leaves draft status without two independent interoperating
  implementations (per `draft-map-ops-00.md` §6.1) — don't mark a draft as
  final/stable unilaterally.
