---
title: "MAP: Family Index and Submission Overview"
docname: draft-map-index-01
category: info
submissiontype: independent
---

# MAP: Family Index and Submission Overview

**Status of This Memo:** This document is an Internet-Draft-style preprint
published for open-source community review. Two independent interoperating
implementations pass the full live interop matrix (ops §6.1). It has not been
submitted to the IETF (intended independent stream; see ops §3.5). Distribution
is unlimited. It expires six months after publication and MUST NOT be cited
except as "work in progress" pending human/expert review.

This document is subject to BCP 78 and BCP 79. The source working tree is
distributed under Apache-2.0 (see repository LICENSE); contributors SHOULD
disclose relevant IPR per BCP 79. On publication the RFC is copyrighted by the
IETF Trust and licensed per the IETF Trust's terms (see draft-map-ops-01
§3.5).

## Abstract

The Manolito Agent Protocol (MAP) is a family of specifications for a
decentralized, permissionless network of autonomous AI agents. MAP evolves the
capability-oriented spirit of the historical Manolito (MP2P) file-sharing
network into a modern agent fabric in which participants advertise *what they
can do* — models, tools, memory domains, compute — and requesters route work by
semantic capability, verifiable evidence of past behavior, price, and locally
measured performance. This document is the **cover/index** for the MAP family:
it lists the companion documents, states the reading order, and records the
submission status. Each companion document carries its own normative text;
this index defines no wire format, code point, or protocol behavior.

## 1. Family overview

MAP is written as a set of focused Internet-Drafts rather than one monolith, so
each concern (identity, transport, discovery, tasks, trust, payments,
operations) can advance and be reviewed independently while sharing one
envelope, one identity model, and one registry index. The shared technical
foundations are:

- a self-certifying Ed25519 identity (AgentID ≡ did:key ≡ libp2p PeerID);
- a deterministic CBOR + COSE_Sign1 signed envelope with downgrade-resistant
  negotiation and channel binding;
- a family-wide registry index (`draft-map-wire-01`) that is the single
  source of truth for every assigned code point;
- evidence-based local trust (dual-signed ExecutionReceipts) rather than
  self-asserted reputation.

The motivation, problem statement, and design critique are in
`draft-map-intro-01` and `01-design-critique.md`; the architecture, trust
model, and threat catalog are in `draft-map-arch-01`.

## 2. Document index

This is a set of **10 Internet-Drafts** submitted as a group to the IETF
Independent Stream. The table below is the authoritative scope/dependency map
(also carried in `draft-map-arch-01` §11).

| Document | Class | Scope | Depends on |
|---|---|---|---|
| draft-map-index-01 | Informational | this cover/index document | — |
| draft-map-intro-01 | Informational | introduction & motivation; problem statement, critique thesis, reading guide | — |
| draft-map-arch-01 | Informational | architecture, system & trust model, threat catalog, zones & privacy, scalability, conformance classes | — |
| draft-map-wire-01 | Informational (reference) | wire PDU layering diagram & master registry index (all assigned code points) | all |
| draft-map-core-01 | Standards | envelope, negotiation, sessions, errors, flow control, extension framework, transport bindings | identity |
| draft-map-identity-01 | Standards | AgentID, key hierarchy, delegation, rotation, revocation, DID/WebAuthn interop | — |
| draft-map-discovery-01 | Standards | descriptors, capability ontology, two-plane discovery, NAT traversal profile | core |
| draft-map-tasks-01 | Standards | negotiation, inference & streaming, tools, checkpoints, delegation, memory/context | core, discovery |
| draft-map-trust-01 | Standards | receipts, evidence exchange, admission costs, trust computation guidance | core |
| draft-map-pay-01 | Standards | quotes, agreements, proofs, receipts, settlement-method registry, disputes | core, tasks |
| draft-map-ops-01 | BCP | registries & governance, federation, interop/testing, reference implementation, migration | all |

## 3. Reading order

1. `draft-map-intro-01` — why, and what was rejected (read first).
2. `draft-map-arch-01` — goals, non-goals, trust model, threats.
3. `draft-map-core-01` — the envelope and session model everything else builds on.
4. Per-area: `draft-map-identity-01`, `draft-map-discovery-01`,
   `draft-map-tasks-01`, `draft-map-trust-01`, `draft-map-pay-01`.
5. `draft-map-wire-01` — the registry index; consult whenever a code point is
   in question (it is the single source of truth for assignments).
6. `draft-map-ops-01` — governance, registries, federation, and the
   formal-analysis / interop program that gates publication.

## 4. Submission status

The family is intended for the IETF Independent Stream (each draft's
`submissiontype: independent` front-matter records this). Registry ownership
and the pre/post-publication authority split are defined in `draft-map-ops-01`
§3.5. Submission is gated on the two-independent-implementation criterion
(`draft-map-ops-01` §6.1): both reference implementations (`map-rs`, `map-go`)
pass the full test-vector set and the live pairwise interop matrix (16/16).
The documents remain in draft pending human/expert review.

## 5. References (selected)

[BCP14] BCP 14 requirement levels (RFC 2119 / RFC 8174).
[RFC2026] BCP 78 (copyright). [RFC8179] BCP 79 (IPR).
The companion MAP documents listed in §2.
