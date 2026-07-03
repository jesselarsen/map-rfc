---
title: "The Manolito Agent Protocol (MAP): Architecture, Requirements, and Threat Model"
docname: draft-map-arch-00
category: info
submissiontype: independent
---

# The Manolito Agent Protocol (MAP): Architecture

**Status of This Memo:** This document is an Internet-Draft-style preprint
published for open-source community review. It has not been submitted to the
IETF. Distribution is unlimited. It expires six months after publication and
MUST NOT be cited except as "work in progress".

## Abstract

The Manolito Agent Protocol (MAP) is a family of specifications for a
decentralized, permissionless network of autonomous AI agents. MAP evolves
the capability-oriented spirit of the historical Manolito (MP2P) file-sharing
network into a modern agent fabric: participants advertise *what they can
do* — models, tools, memory domains, compute — and requesters route work by
semantic capability, verifiable evidence of past behavior, price, and
locally measured performance. This document defines the architecture, design
goals and non-goals, the system and trust models, the threat model, the
privacy model (including data-sensitivity zones), scalability targets, and
the map of companion documents. It contains no wire formats; those are
defined in draft-map-core-00 and its siblings.

## 1. Introduction and Heritage

Manolito/MP2P (c. 2001) demonstrated that a flat, serverless, UDP-centric
network of consumer nodes could sustain search and exchange at scale with
minimal per-node state. MAP retains three MP2P instincts — decentralization
without supernodes, lightweight signed messages, tolerance of extreme
churn — and discards its obsolete mechanics (cleartext UDP, file-hash
indexing, no identity layer). The essential inversion, taken from the
attached design work: MP2P asked "who has this file?"; MAP asks "who can do
this task?". The network is a **capability graph**.

MAP is a *protocol*, not a product. "Hermes" and similar orchestrators are
implementations that speak MAP; nothing in these documents privileges any
implementation.

## 2. Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## 3. Terminology

- **Agent**: a protocol participant identified by an AgentID. May act as
  Requester, Provider, Relay, Gateway, or several at once.
- **AgentID**: a self-certifying identifier derived from an Ed25519 public
  key (draft-map-identity-00).
- **Requester / Provider**: the party commissioning a task / the party
  executing it. Roles are per-task, not per-node.
- **Orchestrator**: application logic on a requester that decomposes,
  routes, and merges tasks. Out of protocol scope except where it emits
  MAP messages.
- **Descriptor**: a signed, content-addressed advertisement of an agent's
  stable properties (draft-map-discovery-00).
- **Capability**: a registered hierarchical URI naming a service class,
  e.g. `map:cap:inference/text-generation` (draft-map-discovery-00 §4).
- **Envelope**: the signed CBOR/COSE container for every MAP message
  (draft-map-core-00).
- **Receipt**: dual-signed evidence of a completed interaction
  (draft-map-trust-00).
- **Zone**: a data-sensitivity class constraining where payloads may flow
  (§8).
- **Mesh**: a connected deployment of MAP agents sharing bootstrap and
  policy; meshes MAY federate (draft-map-ops-00).

## 4. Design Goals

G1. **Decentralized and permissionless.** No central authority, registry
server, or global PKI is required to join, advertise, discover, or
transact. (Governed *namespaces* — the capability ontology, code-point
registries — are documents and processes, not runtime services.)

G2. **Self-certifying identity.** Every artifact that matters — descriptor,
task assignment, checkpoint, receipt, payment message — is signed and
verifiable offline against the sender's AgentID.

G3. **Capability-first semantics.** Discovery, negotiation, and routing
operate on capabilities and constraints, never on addresses as such.

G4. **Local trust.** No global reputation exists. Each agent computes its
own trust from evidence it can verify. Identity does not imply reputation;
anonymous participation is permitted and simply starts with none.

G5. **Transport agnosticism.** MAP runs over any ordered, bidirectional,
mutually authenticated, encrypted, multiplexed byte stream. libp2p over
QUIC is the RECOMMENDED substrate; TCP+TLS/Noise and WebSocket bindings are
normative; future transports slot in without protocol change.

G6. **Secure and private by default.** All traffic encrypted; all control
messages signed; replay-protected; metadata minimized; data-sensitivity
zones enforced by protocol rule, not convention.

G7. **Cryptographic agility.** Algorithms are named by negotiated suite
identifiers; the protocol survives replacement of any primitive, including
migration to post-quantum signatures/KEMs.

G8. **Extensible without forking.** Unknown non-critical extensions are
ignored and preserved; critical extensions are negotiated; all code points
live in registries with defined policies.

G9. **Economically open.** Payment *negotiation* is standardized; payment
*settlement* is pluggable (Lightning, card processors, stablecoins, ACH,
Interledger, private ledgers). The protocol never mandates a currency,
chain, or processor.

G10. **Operable at scale and at the edge.** Target: 10^9 identities, 10^7
concurrently reachable providers per mesh, individual nodes as small as a
single-board computer. Mechanisms whose cost grows super-logarithmically
with network size are excluded from the mandatory core (§9).

G11. **Honest about churn and failure.** Mobile, NATed, intermittently
connected agents are first-class; every long-lived operation defines
timeout, resumption, and compensation behavior.

### 4.1 Non-Goals

- Model training or fine-tuning coordination.
- General-purpose file sharing (content transfer exists only in service of
  tasks).
- A native token, blockchain, or consensus layer.
- Global, objective reputation scores.
- Guaranteeing the *semantic quality* of AI outputs — MAP makes behavior
  *attributable and evidence-bearing*, not correct.
- Backward wire compatibility with legacy Manolito clients (a conceptual
  migration guide is provided in draft-map-ops-00 §7).

## 5. System Model

```
+---------------------------------------------------------------+
| Application     Orchestrators, IDE plugins, agent runtimes    |
+---------------------------------------------------------------+
| MAP Semantics   Tasks & streaming (tasks) | Trust (trust)     |
|                 Payments (pay)            | Federation (ops)  |
+---------------------------------------------------------------+
| MAP Discovery   Descriptors, ontology, DHT records, status    |
+---------------------------------------------------------------+
| MAP Core        Envelope, negotiation, sessions, errors,      |
|                 flow control, extension framework             |
+---------------------------------------------------------------+
| MAP Identity    AgentIDs, delegation, rotation, revocation    |
+---------------------------------------------------------------+
| Substrate       libp2p (Kademlia, gossipsub, AutoNAT, relay,  |
|                 hole punching) / QUIC / TCP+TLS / WebSocket   |
+---------------------------------------------------------------+
```

Roles: **Relays** forward encrypted traffic for unreachable peers (they see
ciphertext and coarse metadata only). **Gateways** re-advertise external
services (e.g., a hosted-API bridge) into a mesh, with mandatory provenance
labeling. **Bootstrap peers** are ordinary agents listed out-of-band; they
hold no authority.

## 6. Trust Model (Assumptions)

T1. No participant is trusted by default; the network is assumed to contain
actively malicious, colluding, well-resourced adversaries.
T2. Every protocol-relevant claim MUST be independently verifiable or
explicitly marked as an unverifiable assertion (e.g., a rating).
T3. Identity is cheap unless a mesh applies admission costs; therefore no
mechanism may assume identities are scarce (Sybil-aware design).
T4. Trust decisions are local. Two honest agents may legitimately disagree
about a third.
T5. Reputation derives only from evidence signed by counterparties, weighted
by the evaluator's trust in those counterparties.
T6. Transports provide confidentiality and integrity between endpoints once
established; endpoint *identity binding* is MAP's responsibility (channel
binding, draft-map-core-00 §7.3).
T7. The cryptographic primitives of the negotiated suite are sound; clocks
are loosely synchronized (±120 s); no global time authority exists.
T8. Availability of any single peer, relay, or bootstrap node is never
assumed.

## 7. Threat Model

Adversary capabilities considered: create unlimited identities; observe and
inject on any public network path; participate as provider, requester,
relay, and gateway simultaneously; collude; replay recorded traffic; run
modified implementations; control substantial compute. Out of scope: global
passive adversaries deanonymizing via traffic timing across the entire
Internet (mitigated only partially, §8.3); compromise of a victim's own
endpoint.

Threat catalog with primary mitigations (normative details in the cited
documents):

| # | Threat | Primary mitigations | Spec |
|---|--------|--------------------|------|
| TH1 | Sybil flooding of discovery/trust | Admission cost options (PoW/VDF ticket, stake, vouching); per-observer evidence-based trust; rate limits | trust §5, discovery §7 |
| TH2 | Eclipse of a victim's routing view | Disjoint-path Kademlia lookups; bucket diversity by IP prefix; multiple bootstrap sources; signed descriptor CIDs verified end-to-end | discovery §8 |
| TH3 | Replay of signed messages | msg-id uniqueness cache + timestamp window; session sequence numbers; channel binding | core §7 |
| TH4 | MITM / identity misbinding | Transport handshake keys MUST equal/chain to AgentID; channel-bound envelopes | core §7.3, identity §5 |
| TH5 | Spam / resource exhaustion | Credit-based flow control; per-peer rate limits; puzzle-gated expensive queries; quotas before reservation | core §8, tasks §6 |
| TH6 | Poisoned models / model substitution | Model content hashes (multihash) in descriptors; output commitments; redundant execution & canary audits | discovery §5, trust §8 |
| TH7 | Prompt injection via delegated content | Sandbox class advertisement; tool calls require requester-signed capability tokens with caveats; zone limits on raw data | tasks §7, arch §8 |
| TH8 | Malicious checkpoint injection | Requester-countersigned checkpoints; opaque state restricted to Zone ≤1 or attested executors | tasks §8.3 |
| TH9 | Payment fraud (non-delivery / non-payment) | Signed quotes & agreements; metered receipts; escrow-capable methods; dispute evidence = transcript roots | pay §5–6 |
| TH10 | Reputation manipulation (self-praise, ballot stuffing, badmouthing) | No self-reported scores; dual-signed receipts; issuer-trust weighting; collusion analysis | trust §7–8 |
| TH11 | Routing-layer attacks (query bias, record suppression) | Query multiple shards/replicas; signed records with seq numbers; client-side cross-checks | discovery §8 |
| TH12 | Metadata leakage (fingerprints, intent, social graph) | Bucketed hardware fields; selective receipt disclosure; relayed/decoy queries; zone gating | §8.3, discovery §9 |
| TH13 | Identity theft / key compromise | Key hierarchy with offline roots; rotation & revocation records; short-lived session keys | identity §7 |
| TH14 | Downgrade of version/suite | Negotiation transcript bound into session; minimum-version policy | core §6.4 |
| TH15 | Malicious tool execution on providers | Sandbox mandatory for Zone-2 execution; tool allowlists; authorization round-trip | tasks §7 |

Formal analysis plan: the session-establishment, channel-binding, and
delegation subsystems SHOULD be modeled in Tamarin or ProVerif before the
family advances maturity; the state machines in core/tasks are specified to
be directly translatable (draft-map-ops-00 §6.3).

## 8. Privacy Model

### 8.1 Zones (normative)

Every payload-bearing object carries a zone label; every peer relationship
has a zone level assigned by local policy.

- **Zone 0 — Local**: this device only. Never leaves the host.
- **Zone 1 — Trusted**: explicitly allowlisted peers (own fleet, partners).
  Raw proprietary data (source code, documents, embeddings of them) MAY
  flow here.
- **Zone 2 — Community**: authenticated but untrusted mesh peers. Only
  abstracted or non-sensitive payloads.
- **Zone 3 — Open**: anonymous/ephemeral counterparties and public
  gateways.

Rule Z1: An agent MUST NOT transmit an object to a peer whose zone level
exceeds the object's zone label. Rule Z2: A provider MUST NOT forward
received task content to a higher-numbered zone than it was received under
(no laundering); delegation carries the label. Rule Z3: Zone violations are
protocol errors (code 2004) and receipt-recordable events.

### 8.2 Data minimization requirements

- Descriptors: hardware in coarse buckets (e.g., VRAM class ∈ {≤8, 16, 24,
  32, 48, 64, 96, 128, >128} GB); exact figures OPTIONAL and zone-gated; no
  serials, hostnames, or OS fingerprints.
- No latency, load, or trust figures in long-lived records (they are either
  meaningless or volatile — see critique C3/C6).
- Receipts disclose to chosen audiences on request; global broadcast of
  receipts is NOT RECOMMENDED.
- Intermediate task data MUST be deleted after settlement unless retention
  is negotiated.

### 8.3 Residual risks (honest statement)

DHT queries reveal interest in a capability to the queried nodes; status
queries reveal requester–candidate pairs; relays learn traffic timing.
Mitigations offered (shard sampling, relayed queries, decoy traffic,
ephemeral identities per draft-map-identity-00 §8) reduce but do not
eliminate these. Deployments with strong anonymity needs SHOULD run MAP
over an anonymity network and use Zone-3 ephemeral identities.

## 9. Scalability Analysis and Targets

Costs in the mandatory core are bounded as follows (N = mesh size, P =
providers of a capability):

- Discovery lookup: O(log N) DHT hops; hot capabilities sharded so any
  single key holds O(P / S) records for S shards (discovery §6).
- Stable-record maintenance: one descriptor republish per agent per hour —
  ~278 puts/s per million agents mesh-wide, absorbed by the DHT.
- Volatile state: pulled on demand from a shortlist (typically ≤ 10 status
  queries per routing decision); optional presence gossip is topic-scoped
  and rate-limited to ≥30 s intervals.
- Per-connection state: O(active sessions), independent of N.
- Trust: evidence storage is local and prunable; no global convergence
  required (EigenTrust-style global iteration is explicitly OPTIONAL and
  off the critical path).

Anything violating these bounds (global broadcast, per-second global
republish, global consensus) is excluded from mandatory conformance — this
is the structural fix for critique C3.

## 10. Conformance Classes

- **MAP-Core** (REQUIRED): identity + core envelope/sessions/errors.
- **MAP-Discovery**: descriptors, ontology, DHT/status planes.
- **MAP-Tasks**: task lifecycle, streaming, tools, checkpoints.
- **MAP-Trust**: receipts and evidence exchange.
- **MAP-Pay**: payment negotiation (≥1 settlement method).
- **MAP-Fed**: federation gateways.

A conforming *provider* implements Core+Discovery+Tasks; a conforming
*requester* Core+Discovery+Tasks; Trust and Pay are RECOMMENDED for public
meshes. Test-suite structure: draft-map-ops-00 §6.

## 11. Document Map

| Document | Scope | Depends on |
|---|---|---|
| draft-map-arch-00 | this document | — |
| draft-map-core-00 | envelope, negotiation, sessions, errors, flow control, extension framework, transport bindings | identity |
| draft-map-identity-00 | AgentID, key hierarchy, delegation, rotation, revocation, DID/WebAuthn interop | — |
| draft-map-discovery-00 | descriptors, capability ontology, two-plane discovery, NAT traversal profile | core |
| draft-map-tasks-00 | negotiation, inference & streaming, tools, checkpoints, delegation, memory/context | core, discovery |
| draft-map-trust-00 | receipts, evidence exchange, admission costs, trust computation guidance | core |
| draft-map-pay-00 | quotes, agreements, proofs, receipts, settlement-method registry, disputes | core, tasks |
| draft-map-ops-00 | registries & governance, federation, interop/testing, reference implementation, migration | all |

## 12. Security Considerations

This entire document is a security consideration; see §6–§8 and the
per-document Security Considerations sections.

## 13. References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels. [RFC8032] EdDSA.
[RFC7748] X25519. [RFC8439] ChaCha20-Poly1305. [RFC5869] HKDF.
[RFC8949] CBOR. [RFC8610] CDDL. [RFC9052]/[RFC9053] COSE.
[RFC9000] QUIC. [RFC6762] mDNS (.local reservation).
libp2p specifications (Kademlia, gossipsub v1.1, AutoNAT, Circuit Relay v2,
DCUtR). W3C DID Core; did:key method. UCAN spec (informative).
Manolito/MP2P historical documentation (informative).
