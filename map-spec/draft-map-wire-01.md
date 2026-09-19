---
title: "MAP Wire Format and Registry Index"
docname: draft-map-wire-01
category: info
submissiontype: independent
author: Jesse Larsen
email: jesselarsen@gmail.com
ipr: trust200902
---

# MAP Wire Format and Registry Index

**Status of This Memo:** This document is an Internet-Draft-style preprint
published for open-source community review. Two implementations by one
author, under a source-isolation rule, pass the full live interop matrix (ops §6.1). It has not been
submitted to the IETF (intended independent stream; see ops §3.5). Distribution
is unlimited. It expires six months after
publication and MUST NOT be cited except as "work in progress" pending human/
expert review.


This document is subject to BCP 78 and BCP 79. The source working tree is distributed under Apache-2.0 (see repository LICENSE); contributors SHOULD disclose relevant IPR per BCP 79. On publication the RFC is copyrighted by the IETF Trust and licensed per the IETF Trust's terms (see draft-map-ops-01 sect. 3.5).

## Abstract

This document is the wire-level reference for the MAP (Manolito Agent Protocol)
family. It presents the protocol data unit layering — transport frame, signed
COSE_Sign1 envelope over deterministic CBOR, protected and body headers, and
message-type-selected content — as a single diagram with the field-level
detail each layer's owning document specifies, so that an implementer can see
the whole encoding in one place. It then consolidates every code point the
family assigns: message types, error codes, body and protected-header keys,
cryptographic suites, capability-ontology terms, delegation caveats, chunk
kinds, settlement methods, pricing models, dispute claim codes, and the
remaining registries, each with its registration policy and a pointer to the
document that owns it.

This document is an index, not a normative source. Where it disagrees with an
owning document, the owning document governs. It defines no protocol behavior
of its own and requests no IANA action of its own.

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## 1. Purpose and how to read this document

This document is the **consolidated, non-normative index** of the MAP wire
format and of every assigned registry in the family. It exists so a reader
can see the whole on-the-wire picture and the complete code-point space in
one place, without opening all eight normative drafts.

It is an **index, not a normative source**. The normative definitions — CDDL,
validation rules, MUST/SHOULD requirements, and the exact bytes — remain in
the owning documents listed below. Where this index and an owning document
disagree, **the owning document wins** (engineering policy: one named source
of truth per fact, ops §2 rule 1).

## 2. The MAP Protocol Data Unit (PDU) layering

Every MAP message is a signed CBOR/COSE envelope. The layering, top to
bottom:

```
AgentID / key material                      (identity §3)
        │
        ▼
Envelope = COSE_Sign1                       (core §4)
   [ protected(ProtHdr), payload(Body), signature ]
        │
        ▼
Body = {                                     (core §4.1)
   version, msg-type, msg-id, timestamp,
   [recipient], [ttl-ms], [session-id], [seq],
   [crit], [ext], content }
        │
        ▼
content := selected by msg-type             (core §5 registry → owning doc)
   HELLO / HELLO_ACK / PING / PONG / CLOSE /
   ERROR / FLOW_CREDIT                        (core §6, §8, §9)
   DESCRIPTOR / STATUS_QUERY / STATUS_REPORT /
   CAP_QUERY / CAP_RESPONSE                   (discovery §2, §3, §7)
   OFFER / QUOTE / ASSIGN / ACCEPT / REJECT /
   CHUNK / TOOL_CALL / TOOL_GRANT / TOOL_RESULT /
   CHECKPOINT / CHECKPOINT_ACK / CANCEL / COMPLETE /
   RESUME / RESERVE / LEASE / LEASE_RELEASE   (tasks §2–§6, §8, §11)
   RECEIPT / RECEIPT_ACK / EVIDENCE_QUERY /
   EVIDENCE_SET / CHALLENGE / CHALLENGE_RESPONSE  (trust §2, §4, §8)
   PAY_QUOTE / PAY_AGREE / PAY_INVOICE /
   PAY_PROOF / PAY_RECEIPT / PAY_DISPUTE /
   PAY_DISPUTE_EVIDENCE                       (pay §2–§6)
        │
        ▼
Task transcript commitment                   (tasks §8.1)
   Merkle root over output/tool CHUNKs (kinds 0, 1, 4)
```

Two transmission contexts (core §7):

- **In-session**: the envelope's COSE `external_aad` is the channel-binding
  token `CB` (core §7.3); `recipient`, `session-id`, and `seq` are REQUIRED.
- **Out-of-session** (descriptors, receipts, revocations, provider records):
  `external_aad` is empty; `ttl-ms` is REQUIRED; freshness/supersession rules
  are per object class (core §7.4).

Four design invariants bind every layer (core §1):

- **I1** — signatures cover the exact transmitted bytes; verifiers MUST NOT
  re-serialize before verifying.
- **I2** — every envelope is independently verifiable from the sender's key
  material alone.
- **I3** — unknown non-critical fields are ignored *and preserved verbatim*
  when stored or forwarded.
- **I4** — all normative structures are CDDL (RFC 8610) over the deterministic
  CBOR profile (core §4.2; RFC 8949 §4.2.1).

## 3. Master Registry Index

Each subsection lists the assigned values (copied from the owning document)
and a "Defined in" pointer to the normative definition. Registration policy
for each registry is in ops §3 (summary in §4 below).

### 3.1 Cryptographic Suites (core §2)

| ID | Name | Signature | KEM/DH | AEAD | Hash | KDF |
|----|------|-----------|--------|------|------|-----|
| 1 | MAP-SUITE-1 (MTI) | Ed25519 | X25519 | ChaCha20-Poly1305 | SHA-256 | HKDF-SHA-256 |
| 2 | MAP-SUITE-2 | Ed25519 | X25519 | ChaCha20-Poly1305 | BLAKE3 | BLAKE3-derive-key |
| 3 | (reserved, post-quantum) | Ed25519+ML-DSA-44 | X25519+ML-KEM-768 | ChaCha20-Poly1305 | SHA-256 | HKDF-SHA-256 |

Suite 3 is a placeholder; its code point MUST NOT be used until a companion
specification defines it. Policy: **Standards Action**.

### 3.2 Message Types (core §5)

| Range | Family | Defined in |
|-------|--------|-----------|
| 0x0001–0x00FF | Core: HELLO 0x0001, HELLO_ACK 0x0002, PING 0x0003, PONG 0x0004, CLOSE 0x0005, ERROR 0x0006, FLOW_CREDIT 0x0007 | core |
| 0x0100–0x01FF | Discovery: DESCRIPTOR 0x0100, STATUS_QUERY 0x0101, STATUS_REPORT 0x0102, CAP_QUERY 0x0103, CAP_RESPONSE 0x0104 | discovery |
| 0x0200–0x02FF | Tasks: OFFER 0x0200, QUOTE 0x0201, ASSIGN 0x0202, ACCEPT 0x0203, REJECT 0x0204, CHUNK 0x0205, TOOL_CALL 0x0207, TOOL_GRANT 0x0208, TOOL_RESULT 0x0209, CHECKPOINT 0x020A, CHECKPOINT_ACK 0x020B, CANCEL 0x020C, COMPLETE 0x020D, RESUME 0x020E, RESERVE 0x020F, LEASE 0x0210, LEASE_RELEASE 0x0211 | tasks |
| 0x0300–0x03FF | Trust: RECEIPT 0x0300, RECEIPT_ACK 0x0301, EVIDENCE_QUERY 0x0302, EVIDENCE_SET 0x0303, CHALLENGE 0x0304, CHALLENGE_RESPONSE 0x0305 | trust |
| 0x0400–0x04FF | Pay: PAY_QUOTE 0x0400, PAY_AGREE 0x0401, PAY_INVOICE 0x0402, PAY_PROOF 0x0403, PAY_RECEIPT 0x0404, PAY_DISPUTE 0x0405, PAY_DISPUTE_EVIDENCE 0x0406 | pay |
| 0x0500–0x05FF | Identity: ROTATE 0x0500, REVOKE 0x0501, DELEGATE 0x0502 | identity |
| 0xF000–0xFFFF | Private use | — |

`0x0206` is deliberately unassigned (never reused; ops §3.3). Registration
policy: **Specification Required**.

### 3.3 Body Keys (core §4.1)

`0` version, `1` msg-type, `2` msg-id, `3` timestamp, `4` recipient,
`5` ttl-ms, `6` session-id, `7` seq, `8` crit, `9` ext, `10` content. Key 33
(delegation chain) resides in `ProtHdr` (core §4.1). Registration policy:
**Specification Required**.

### 3.4 Extension Keys (core §5.1)

| Key | Name | Defined in | Status |
|-----|------|-----------|--------|
| 1 | ext-bid | tasks §2.2 | complete |
| 2 | ext-kvx | tasks §8.4 | reserved (stub) |
| 3 | ext-git | tasks §9 | reserved (stub) |

Points ≥ 0xF000 are private use. Policy: **Specification Required**.

### 3.5 Error Codes (core §9)

| Space | Codes |
|-------|-------|
| 1xxx protocol | 1000 malformed, 1001 unsupported version/suite, 1002 unknown critical extension, 1003 replay/out-of-window/bad-seq, 1004 message too large, 1005 rate limited / no credit, 1006 state violation |
| 2xxx auth/authz | 2000 bad signature/identity misbind, 2001 expired object, 2002 revoked key, 2003 invalid delegation chain, 2004 zone violation, 2005 not authorized for message type, 2006 countersignature refused |
| 3xxx capability | 3000 unsupported message/capability, 3001 model unavailable, 3002 context too large, 3003 ontology term unknown |
| 4xxx resource | 4000 overloaded, 4001 queue full, 4002 insufficient memory/VRAM class, 4003 reservation expired, 4004 storage exhausted |
| 5xxx execution | 5000 task failed, 5001 deadline/TTFT exceeded, 5002 cancelled, 5003 checkpoint invalid, 5004 tool denied, 5005 output policy violation, 5006 resumption impossible |
| 6xxx payment | 6000 quote expired, 6001 payment required, 6002 settlement failed (generic), 6003 dispute opened, 6004 method unsupported, 6005 authorized cap exceeded, 6006 agreement digest mismatch, 6007 task-binding mismatch, 6008 invalid payment proof, 6009 invoice-watermark violation, 6010 escrow violation |

Policy: **Specification Required**.

### 3.6 Transport Bindings (core §10)

libp2p protocol ID `/map/1.0.0` (RECOMMENDED); Raw QUIC ALPN `map/1`;
TCP+TLS 1.3 ALPN `map/1`; WebSocket subprotocol `map.v1`; gRPC binding
(optional, non-normative). Policy: **Specification Required**.

### 3.7 AgentID Key Types (identity §11)

`0xED01` = Ed25519 public key (suite 1/2). Future suites register their
multicodec here. Policy: **Standards Action**.

### 3.8 Delegation Caveats (identity §4, §11)

| Key | Name | Value |
|-----|------|-------|
| 1 | task-pinning | 32-byte TaskID — chain valid only for envelopes referencing that task |
| 2 | audience | 34-byte AgentID — chain valid only when validated by that party |

An unknown caveat key MUST invalidate the chain (fail-closed). Policy:
**Expert Review**.

### 3.9 Revocation Reasons (identity §11)

`0` unspecified, `1` compromise, `2` superseded, `3` cessation. Policy:
**Expert Review**.

### 3.10 Capability Ontology (discovery §4)

Hierarchical URIs under `map:cap:` — initial branches:
`inference/text-generation[/code|/chat]`, `inference/embedding`,
`inference/vision | /speech-to-text | /text-to-speech | /image-gen`,
`tool/git | /shell | /compiler/<lang> | /browser | /sql`,
`lang/<bcp47-or-programming-language-tag>`, `memory/<domain>`,
`relay`, `gateway/<namespace>`, `schedule/reservation`. Matching is by exact
segment prefix. Policy: **Expert Review**.

### 3.11 CapEntry Parameter Keys (discovery §4, §12)

| Key | Name | Value |
|-----|------|-------|
| 1 | gateway-of | tstr namespace — the fronted capability is re-advertised through this gateway namespace |

Policy: **Expert Review** (immutable once assigned, ops §3.3).

### 3.12 Hardware / Sandbox Classes (discovery §12)

HardwareClass: `vram-class` (0 n/a … 9 >128 GB), `ram-class` (same scale),
`accel` (0 none … 4 cpu-only), `bandwidth-class` (1 ≤100Mb … 3 >1Gb).
SecurityProfile sandbox: `0` none, `1` container, `2` microVM, `3` wasm,
`4` tee. Policy: **Expert Review**.

### 3.13 Chunk Kinds (tasks §3)

`0` output-text, `1` output-bytes, `2` log, `3` usage-update, `4` tool-io.
Policy: **Expert Review**.

### 3.14 Reservation / Lease Fields (tasks §6)

Message types RESERVE 0x020F, LEASE 0x0210, LEASE_RELEASE 0x0211; rsv-id is
the idempotency key. Policy: **Expert Review**.

### 3.15 Task Caveat Keys (tasks §16)

`delegable`, `budget`, `args-digest` (tool-grant caveats). Policy:
**Expert Review**.

### 3.16 Receipt Outcome Codes (trust §3)

`0` ok, `1` failed, `2` timeout, `3` cancelled, `4` disputed. Policy:
**Expert Review**.

### 3.17 Trust Evidence Query Params / Vouch Caveat Keys / Challenge Modes (trust §11)

EvidenceQuery fields (subject, scope, window, limit, role filter); vouch
records are DelegationCert-shaped; CHALLENGE/CHALLENGE_RESPONSE (0x0304/
0x0305) for disclosed canary audits. Policy: **Expert Review**.

### 3.18 Settlement Methods (pay §5.1)

| URI | Rail | Policy |
|-----|------|--------|
| `pay:lightning-bolt12` | Lightning | Specification Required |
| `pay:stripe-intent` | card processors | Specification Required |
| `pay:erc20-stablecoin` | EVM stablecoins | Specification Required |
| `pay:ach` | ACH | Specification Required |
| `pay:interledger` | Interledger / Web Monetization | Specification Required |
| `pay:credit-ledger` | private mesh ledger | Specification Required |
| `pay:x402` | x402 (HTTP-native stablecoin) | Specification Required |

Each entry's defining document specifies params, proof, verification, and
finality/reversal model. Policy: **Specification Required**.

### 3.19 Pricing Models / Terms / Rate-table Unit Codes (pay §3, §8)

Pricing models: `0` flat, `1` per-token (in/out rates), `2` per-second,
`3` per-invocation. Terms: `0` postpaid, `1` prepaid, `2` escrow,
`3` metered-incremental. Unit codes: `0` flat total, `1` per token in,
`2` per token out, `3` per millisecond, `4` per invocation. Policy:
**Specification Required**.

### 3.20 Dispute Claim Codes / Evidence-item Kinds (pay §6.1, §8)

Claim codes: `0` non-delivery, `1` output-mismatch, `2` non-payment,
`3` overbilling, `4` terms breach. Evidence-item kinds: `0` PAY_QUOTE
envelope, `1` PAY_AGREE, `2` PAY_INVOICE, `3` PAY_PROOF, `4` PAY_RECEIPT,
`5` countersigned checkpoint, `6` COMPLETE, `7` ExecutionReceipt,
`8` method artifact, `9` CHUNK envelope. Policy: **Specification Required**.

## 4. Registration policies and change control

Each registry's IANA policy (from ops §3):

| Registry | Policy |
|----------|--------|
| Cryptographic Suites, AgentID Key Types | Standards Action |
| Message Types, Body Keys, Error Codes, Extension Keys, Transport Bindings, Settlement Methods, Pricing Models, Terms, Unit Codes, Dispute codes/kinds | Specification Required |
| Capability Ontology, Delegation Caveats, Chunk Kinds, HW/Sandbox Classes, Revocation Reasons, Task Caveat Keys, CapEntry Parameter Keys, Trust Evidence/Vouch/Challenge | Expert Review |

Immutability (ops §3.3): code points are never reused or redefined;
deprecation marks them reserved forever. Deprecation process (ops §3.4):
mark deprecated → ≥2 minor versions dual support → remove only at a major
version. Private ranges (core §5, ext keys ≥ 0xF000) need no registration
and MUST NOT appear in public-mesh descriptors.

## 5. Conformance classes

MAP-Core (REQUIRED: identity + core); MAP-Discovery; MAP-Tasks; MAP-Trust;
MAP-Pay; MAP-Fed (arch §10). A conforming provider/requester implements
Core + Discovery + Tasks; Trust and Pay are RECOMMENDED on public meshes.

## 6. Security Considerations

This document is a wire-format layering summary and a consolidated registry
index; it introduces no mechanism beyond those in the protocol documents,
whose security considerations apply unchanged. In particular, all code-point
and key registrations indexed here are consumed under the deterministic-CBOR
and signed-envelope rules of `draft-map-core-01` §4, whose Security
Considerations govern malleability, downgrade, and replay.

## 7. IANA Considerations

This document defines no new registry of its own. It provides a consolidated
index (§3) of the registries defined by the MAP protocol documents; each
registry's contents, assignment policy, and change control are normative in
its defining document, and the registration policies are summarized in §4.
No IANA action is requested by this document beyond those already requested
by the protocol documents it indexes.

## 8. References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels. [RFC8610] CDDL.
[RFC8949] CBOR. [RFC9052] COSE. Owning documents: `draft-map-core-01`,
`draft-map-identity-01`, `draft-map-discovery-01`, `draft-map-tasks-01`,
`draft-map-trust-01`, `draft-map-pay-01`, `draft-map-ops-01`.
