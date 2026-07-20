---
title: "MAP Introduction and Motivation: Autonomous Agent Interoperability"
docname: draft-map-intro-01
category: info
submissiontype: independent
author: Jesse Larsen
email: jesselarsen@gmail.com
ipr: trust200902
---

# MAP Introduction and Motivation

**Status of This Memo:** This document is an Internet-Draft-style preprint
published for open-source community review. Two independent interoperating
implementations pass the full live interop matrix (ops §6.1). It has not been
submitted to the IETF (intended independent stream; see ops §3.5). Distribution
is unlimited. It expires six months after
publication and MUST NOT be cited except as "work in progress" pending human/
expert review.


This document is subject to BCP 78 and BCP 79. The source working tree is distributed under Apache-2.0 (see repository LICENSE); contributors SHOULD disclose relevant IPR per BCP 79. On publication the RFC is copyrighted by the IETF Trust and licensed per the IETF Trust's terms (see draft-map-ops-01 sect. 3.5).

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## Abstract

The Manolito Agent Protocol (MAP) is a decentralized, permissionless protocol
for autonomous AI agents to discover one another by capability, negotiate
work, execute it with verifiable evidence, and settle payment — without a
central authority, global reputation service, or native token. This document
states the problem MAP addresses, why prior designs failed, the design
responses MAP adopts, and a reading guide for the rest of the specification
family. The substantive specifications are the companion `draft-map-*-01`
documents; the critical evaluation that motivates them is
`01-design-critique.md`.

## 1. The Problem

Autonomous AI agents increasingly need to cooperate: one agent commissions
another to run inference, call a tool, retrieve memory, or transform data.
That cooperation should be **verifiable, permissionless, and attriburable** —
any agent should be able to find a capable counterparty, agree on terms,
confirm the work was done, and pay for it, without trusting a central
broker with the identity graph, the reputation ledger, or the money supply.

The historical Manolito/MP2P file-sharing network (c. 2001) showed that a
flat, serverless, high-churn peer network can route by *what is wanted*
rather than *where it lives*. MAP evolves that instinct from "who has this
file?" to "who can do this task?": the network becomes a **capability graph**.
Several recent design sketches and a preprint (HEP v0.1) attempted exactly
this, but — as `01-design-critique.md` documents in detail — they contained
defects that would be fatal in standards review: self-referential signature
definitions, self-asserted trust scores, volatile data pushed through a
global DHT, no replay protection, normative magic-weight routing, and a
serialization choice that cannot support the signature semantics the
documents rely on.

## 2. Why Prior Designs Failed (high level)

The critique identifies fifteen findings (C1–C15). The most consequential:

- **C1 — Self-asserted trust.** Advertised trust/reputation scores are
  signed by the subject, proving only self-praise; a rational attacker
  advertises a perfect score. Any routing formula weighted by such a score is
  steered by attacker-controlled input.
- **C2 — Non-canonical signatures.** Signing Protobuf structures (which have
  no canonical encoding) makes "serialize and verify" fail across
  implementations and enables re-serialization malleability.
- **C3 — Volatile state in the DHT.** Republishing load/latency every few
  seconds does not scale and routes on stale fiction.
- **C4 — No replay protection.** Signed requests can be replayed verbatim
  (re-executing paid work, double-spending receipts).
- **C5 — Normative magic-weight routing.** Fixed score weights freeze bad
  constants into the standard and mix attacker-controlled inputs with honest
  ones; ranking should be local policy.
- **C6 — Latency as a global field.** Latency is a property of a path
  between two peers, not of a node.
- **C7 — Privacy leakage.** Exact hardware fingerprints and broadcast query
  intent link a stable identity to high-entropy metadata.
- **C8 — Transport lock-in.** Hard-wiring gRPC/QUIC with two parallel
  identities contradicts transport-agnosticism.
- **C9 — Central-bank economics.** "Credits minted by the orchestrator"
  reintroduces the central authority the goals forbid.
- **C10 — Impractical resumption.** KV-cache transfer assumes identical
  engines; normative resumption must be engine-independent.
- **C11 — Checkpoint trust.** A checkpoint signed only by the party that may
  have failed can poison its successor.
- **C12 — Taxonomy anarchy.** Free-text capability strings fragment the
  keyspace.
- **C13 — Identifier fragmentation.** Three incompatible ID spaces for the
  same key.
- **C14 — Weakened requirements language.** "RFC 2119, adapted" guts
  normative force.
- **C15 — Sybil hand-waving.** "Reputation decay fixes Sybil" is an assertion,
  not a mechanism.

The full findings, dispositions, and per-document assessment are in
`01-design-critique.md`.

## 3. MAP's Design Responses

Each cluster of failures maps to a concrete MAP mechanism:

| Problem (critique) | MAP mechanism | Where |
|---|---|---|
| C1 self-asserted trust | No trust fields in descriptors; trust is a local projection over dual-signed `ExecutionReceipt`s | trust §1–3 |
| C2 non-canonical sigs | Deterministic CBOR (RFC 8949 §4.2.1) + COSE_Sign1; verify received bytes, never re-serialize | core §4 |
| C3 volatile DHT | Two-plane discovery: stable signed descriptors in a sharded DHT; volatile status pulled on demand | discovery §1, §5 |
| C4 no replay | `msg-id` + timestamp window + session sequence numbers + channel binding | core §7 |
| C5 magic routing | Standardize verifiable *inputs* and hard *constraints*; ranking is local policy | tasks §5, trust §7 |
| C6 latency field | Removed; RTT measured by the querier | discovery §3 |
| C7 privacy | Coarse hardware buckets; selective receipt disclosure; zones; relayed/decoy queries | arch §8, discovery §9 |
| C8 transport lock-in | One identity, one address book; normative bindings for libp2p/QUIC/TCP+TLS/WebSocket | core §10 |
| C9 central bank | Payment *negotiation* standardized; *settlement* is a pluggable registry (Lightning, stablecoins, ACH, Interledger, credit-ledger, x402) | pay §3, §5 |
| C10 resumption | Transcript replay (Merkle-rooted); KV-cache is an optional same-engine extension | tasks §8 |
| C11 checkpoint trust | Requester countersigns each checkpoint's transcript root | tasks §8.2 |
| C12 taxonomy | Governed capability ontology (URI) + IANA-style registry, Expert Review | discovery §4, ops §3 |
| C13 ID fragmentation | One `AgentID` ≡ did:key ≡ libp2p Ed25519 PeerID | identity §3 |
| C14 weak language | Proper RFC 8174 boilerplate; example domains from `example.org` | every draft |
| C15 Sybil | Layered admission costs (PoW/VDF/stake/vouch) + per-observer evidence-based trust | trust §5 |

## 4. What MAP Is Not

To bound scope, MAP explicitly is **not** (see arch §4.1):

- A model-training or fine-tuning coordinator.
- A general-purpose file-sharing network (content transfer exists only in
  service of tasks).
- A native token, blockchain, or consensus layer.
- A global, objective reputation service.
- A guarantor of the *semantic quality* of AI outputs — MAP makes behavior
  *attributable and evidence-bearing*, not correct.
- Wire-compatible with legacy Manolito clients (a conceptual migration guide
  is in ops §7).

MAP is a **protocol, not a product**. "Hermes" and similar orchestrators are
implementations that speak MAP; nothing in these documents privileges any
implementation.

## 5. Document Map and Reading Order

The family comprises the documents listed in the README "Contents" table and
the arch §11 document map. For orientation, read in this order:

1. This introduction (`draft-map-intro-01`) — motivation and reading guide.
2. `01-design-critique.md` — the critical evaluation that justifies every
   design choice (findings C1–C15).
3. `draft-map-arch-01` — goals/non-goals, threat model TH1–TH15, zones,
   scalability, conformance classes.
4. `draft-map-identity-01` then `draft-map-core-01` for the foundation.
5. Then `draft-map-discovery-01` and `draft-map-tasks-01`.
6. Finally `draft-map-trust-01` and `draft-map-pay-01`; `draft-map-ops-01`
   governs the family.

For the complete wire format and the full code-point space in one place,
consult `draft-map-wire-01` (the master registry index).

Implement in the order: identity, core, discovery, tasks, then trust/pay.

## 6. Status and Maturity

Two independent, separately-authored implementations — `map-rs` (Rust) and
`map-go` (Go) — reproduce every frozen test vector byte-for-byte and complete
a live pairwise interop matrix (16/16 scenarios, `interop/MATRIX.md`) over a
real transport, in both orientations. The core security properties
(downgrade resistance, channel-binding session separation, delegation
attenuation) are machine-checked (ops §6.3). Remaining formal work and the
deferred extension semantics are listed honestly in the README "Status"
section and in `ops §6.3` / `ops §8`. The documents remain in draft pending
human/expert review.

## 7. Security Considerations

This document is a non-normative motivational and design-rationale overview
and defines no protocol mechanisms; it introduces no new security
considerations. The security considerations of MAP are normative in the
protocol documents — principally `draft-map-core-01` (envelope, sessions,
channel binding) — and the family threat model is in `draft-map-arch-01`.

## 8. IANA Considerations

This document requests no IANA actions.

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels. [RFC8949] CBOR.
[RFC9052] COSE. `01-design-critique.md` (critical evaluation, findings C1–C15).
`README.md` (contents and reading order). `draft-map-arch-01` (goals, threat
model, zones).
