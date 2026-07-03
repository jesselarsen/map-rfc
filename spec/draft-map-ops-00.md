---
title: "MAP Operations: Governance, Registries, Federation, Interoperability, and Migration"
docname: draft-map-ops-00
category: bcp
---

# MAP Operations, Governance, and Interoperability

**Status:** preprint, work in progress. BCP 14 language applies.

## 1. Scope

Everything that keeps the MAP family coherent over time: the engineering
policy binding the documents together, code-point registries and their
change policies, the deprecation process, federation between meshes,
conformance and interoperability testing, the formal-analysis program,
reference-implementation guidance, migration from legacy designs, and
regulatory notes.

## 2. Engineering policy: bounded contexts and data semantics

The document family is a deliberate bounded-context decomposition. Each
document owns a vocabulary, a small set of aggregates, and explicit data
semantics; cross-context references are **by identity only** (AgentID,
TaskID, quote-id, invoice-id, descriptor-cid, transcript-root), never by
embedded foreign structures. Context integration happens through signed
messages and translation (gateways, §5), never shared internal models.

| Context (doc) | Aggregate roots (identity) | Durable facts / events | Derived data (rebuildable) | Ordering scope |
|---|---|---|---|---|
| Identity | Key lineage (AgentID) | DelegationCert, ROTATE, REVOKE records | resolved chain caches, revocation views | per AgentID (rotation seq/time) |
| Core | Session (SessionID) | negotiation transcript hash | replay caches, credit counters | `seq` per session direction |
| Discovery | Descriptor (AgentID + seq) | signed descriptors | DHT provider records, peer caches | `seq` per subject (single writer, LWW) |
| Tasks | Task (TaskID) | ASSIGN terms, countersigned checkpoints, COMPLETE | provider execution state, requester progress view | `chunk-seq` per task |
| Trust | Receipt (content address) | ExecutionReceipts, vouches, challenge outcomes | trust scores/tiers (projections) | timestamps per (peer, capability) |
| Pay | Agreement (quote digest), Invoice (invoice-id) | PAY_AGREE, PAY_PROOF, PAY_RECEIPT | balance/exposure views | per invoice-id |

Binding rules across the family (restating what each document enforces
locally, so reviewers can check any change against one list):

1. **One source of truth per fact**, named explicitly; everything else is
   a derived copy with a repair path (republish, re-fetch, recompute).
2. **Events are past-tense facts** (RECEIPT, PAY_RECEIPT, REVOKE);
   commands are requests (ASSIGN, CANCEL, PAY_AGREE); the two are never
   conflated in naming or semantics.
3. **Idempotency everywhere delivery is uncertain**: TaskID, invoice-id,
   msg-id, content addresses. No handler may have unrepeatable side
   effects; no design may assume exactly-once delivery.
4. **Ordering is scoped, never global** — and each scope is written down.
5. **Distinguish accepted / durable / applied** (visible in pay §1's
   agreed/submitted/settled and tasks' countersigned-checkpoint boundary).
6. **Schema evolution is additive within a major version**; contract
   changes ride registries and version negotiation; fields are never
   reused with new meaning (registry policy forbids re-assignment, §3.3).
7. **Projections must be rebuildable** (trust scores from receipts, DHT
   index from descriptors) and their staleness observable.

Changes to any MAP document that violate a row or rule above MUST be
rejected in review or accompanied by an explicit amendment to this table.

## 3. Registries and change control

Registries are documents plus process — never runtime services (arch G1).

| Registry | Policy | Notes |
|---|---|---|
| Cryptographic Suites | Standards Action | security-critical |
| Message Types, Body Keys, Error Codes | Specification Required | ranges per core §5; 0xF000+ private |
| Extension Keys | Specification Required | HELLO-negotiated before use |
| Capability Ontology | Expert Review | additive; aliases at majors only |
| Settlement Methods | Specification Required | must define proof verification & reversal model |
| Delegation Caveats, Chunk Kinds, HW/Sandbox classes | Expert Review | |

### 3.1 Designated experts
Two or more per registry, appointed by the project's steering process;
review criteria: no semantic overlap with existing points, complete
specification, security considerations present.

### 3.2 Private/experimental use
Private ranges (core §5, ext keys ≥ 0xF000) require no registration and
MUST NOT appear in public-mesh descriptors.

### 3.3 Immutability of assignments
Code points are never reused or re-defined; deprecation marks them
reserved forever (rule 6 above).

### 3.4 Deprecation process
Mark deprecated in registry with rationale and replacement → minimum two
minor versions of dual support → removal only at a major version, with the
major's migration notes. HELLO negotiation makes the sunset observable in
the field (peers advertising deprecated versions are countable).

## 4. Versioning discipline

Semantics per core §6.4. Additionally: every published draft revision
carries a changelog distinguishing editorial / additive / breaking;
breaking items MUST name the major version that will carry them. Reference
implementations MUST keep a compatibility test pinning the previous
minor's golden vectors.

## 5. Federation

A **mesh** is a deployment sharing bootstrap and policy (arch §3). Meshes
federate through **gateways** — ordinary agents advertising
`map:cap:gateway/<namespace>` — which are, precisely, anti-corruption
layers: they translate a foreign context (another mesh, or an external
service such as a hosted-model API) into local MAP vocabulary without
letting the foreign model leak through.

Gateway rules:
- G-1: A gateway signs everything **as itself**. It MUST NOT fabricate
  descriptors, receipts, or proofs purporting to originate from the
  external party; provenance (`gateway-of: <namespace>` ext field) is
  REQUIRED on re-advertised capabilities.
- G-2: Evidence about work done through a gateway accrues to the gateway's
  AgentID — it is accountable for its upstream, which is exactly the
  incentive alignment wanted from the "OpenRouter gateway peer" pattern
  (design3, adopted).
- G-3: Zone labels never launder through a gateway (arch Z2); a gateway
  bridging to an external API is Zone ≥2 by definition unless the
  deployment allowlists it into Zone 1.
- G-4: Namespaces are DNS-anchored (`gateway/openrouter.example.org`) or
  mesh-registered; collisions resolve in favor of the DNS anchor.

Cross-mesh identity, receipts, and payments need no translation — they
are already self-certifying — so federation cost is limited to discovery
bridging and policy reconciliation.

## 6. Conformance and interoperability testing

### 6.1 Structure
Per conformance class (arch §10): golden test vectors (encode/decode/
verify — TV-001/002 seed the set), negative vectors, state-machine
conformance suites, and live pairwise interop matrices. Advancement rule:
no document leaves draft status without **two independent
implementations** passing the full matrix against each other.

### 6.2 Required negative tests (minimum)
Duplicate delivery of every idempotent message (ASSIGN, RECEIPT,
PAY_INVOICE, PAY_PROOF) — observable effect exactly once. Replayed
envelopes within and across sessions (must fail 1003; cross-session must
fail channel binding). Out-of-order and gapped `seq`/`chunk-seq`.
Truncated/duplicate-key/indefinite-length CBOR (must fail 1000). Stale
descriptor `seq` (must not regress). Wrong-shard provider records (must be
discarded). Delegation chains that widen scope/zone/depth (must fail
2003). Signature over re-serialized rather than received bytes (vector
constructed to differ — must fail). **Projection rebuild**: delete trust
scores, recompute from receipt store, byte-compare decisions; same for
DHT index from descriptors.

### 6.3 Formal analysis program
Model in Tamarin or ProVerif before beta: HELLO negotiation + channel
binding (properties: mutual authentication, downgrade resistance, session
binding of envelopes), and delegation-chain validation (property: scope
attenuation is monotone). Publish models beside the spec; the state
machines in core §12 and tasks §13 are written to be mechanically
translatable. Claims not covered by models or tests are labeled
assumptions (arch §7).

### 6.4 Fuzzing and load
Continuous structure-aware CBOR fuzzing of every decoder; churn simulation
(30 %/hour peer turnover) against discovery; hot-key load tests validating
the §6-discovery sharding math at 10^5 synthetic providers per capability.

## 7. Migration

### 7.1 From legacy Manolito/MP2P (conceptual)
No wire compatibility (arch non-goal). Concept map: file-hash search →
capability-key lookup; flat UDP peer exchange → Kademlia + AutoNAT/relay;
unsigned cleartext → signed CBOR over encrypted transports; "who has the
file" results → descriptors + status. The retained virtues are
architectural: no supernodes, tolerance of churn, small messages.

### 7.2 From HEP-0.1 drafts (the attached preprint)

| HEP-0.1 | MAP | Notes |
|---|---|---|
| NodeAdvertisement | Descriptor (discovery §2) | trust/load/latency fields **dropped** (critique C1/C3/C6) |
| trust: TrustScore | ExecutionReceipts + local projection | trust §1–3 |
| CapabilityQuery | CAP_QUERY hard constraints + local ranking | critique C5 |
| routing score formula | informative guidance only | tasks §5 |
| TaskCheckpoint | CHECKPOINT + requester countersign | critique C11 |
| kv_cache_delta | transcript replay; TBD-KVX extension | critique C10 |
| did:hep | AgentID ≡ did:key ≡ libp2p PeerID | critique C13 |
| credits/escrow prose | PAY_* + settlement registry | critique C9 |
| protobuf-signed ads | deterministic CBOR + COSE, verify received bytes | critique C2 |

### 7.3 Coexistence
A HEP-0.1 prototype bridges via a gateway (G-1..G-4) during transition;
dual-stack nodes advertise both and prefer MAP sessions.

## 8. Reference implementation guidance

Rust first (rust-libp2p maturity), Go second; workspace mirrors the
context map — one crate per bounded context, dependencies only along the
document dependency graph, no shared "common domain" crate beyond
identifier newtypes:

```
map/            crates: map-identity, map-core, map-discovery,
                map-tasks, map-trust, map-pay
adapters/       ollama-provider, gateway (hosted-API bridge), ide-rpc
storage/        append-only receipt log + rebuildable projections;
                descriptor cache keyed (AgentID, seq)
conformance/    golden vectors, negative vectors, matrix harness
```

Implementation musts drawn from §2: value types for AgentID/TaskID/Amount
(no bare byte-strings crossing APIs); receipt ingestion idempotent;
projection rebuild as a first-class CLI operation; observability for lag
(descriptor staleness, credit exhaustion, retry counts, dispute rates).
MVP ordering (adapted from design3's scope, which was sound): core +
identity handshake → descriptor publish/fetch → echo task with streaming
and flow credit → receipts → payments last.

## 9. Compliance and regulatory notes (informative)

Receipts and descriptors are pseudonymous personal data under GDPR-like
regimes: holders are controllers of their local stores; the protocol
compels no publication, and erasure is satisfiable because disclosure is
pull-based and local deletion is always possible (trust §4). Payment
compliance (KYC/AML/sanctions) attaches to settlement methods and their
operators (pay §7). Model-output liability and content policy are
application-layer; SecurityProfile and zones give deployments the
enforcement hooks. Export-control considerations for cryptography follow
the chosen suite's primitives, all of which are widely published
standards.

## 10. Security considerations

Governance capture is the meta-risk: registries use open policies with
appealable expert review; no registry is a runtime dependency, so capture
degrades coordination, not operation. Federation gateways concentrate
trust by construction — G-1/G-2 make that concentration visible and
priced rather than hidden.
