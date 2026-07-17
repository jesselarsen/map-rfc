---
title: "MAP Core: Message Envelope, Sessions, Negotiation, and Error Handling"
docname: draft-map-core-01
category: std
submissiontype: independent
---

# MAP Core Protocol

**Status of This Memo:** This document is an Internet-Draft-style preprint
published for open-source community review. Two independent interoperating
implementations pass the full live interop matrix (ops §6.1). It has not been
submitted to the IETF (intended independent stream; see ops §3.5). Distribution
is unlimited. It expires six months after publication and MUST NOT be cited
except as "work in progress" pending human/expert review.


This document is subject to BCP 78 and BCP 79. The source working tree is distributed under Apache-2.0 (see repository LICENSE); contributors SHOULD disclose relevant IPR per BCP 79. On publication the RFC is copyrighted by the IETF Trust and licensed per the IETF Trust's terms (see draft-map-ops-01 sect. 3.5).

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## Abstract

This document defines the mandatory core of the Manolito Agent Protocol
(MAP): a signed CBOR/COSE message envelope with deterministic encoding, a
version/suite/extension negotiation handshake, session establishment with
transport channel binding, replay protection, credit-based flow control, a
uniform error taxonomy, the extension framework, and normative bindings to
libp2p, QUIC, TCP+TLS, and WebSocket transports. Every other MAP document
layers message *contents* on the envelope defined here.

## 1. Overview

A MAP interaction has two phases. First, peers establish a **session**: an
encrypted, mutually authenticated transport connection (or logical stream)
over which they exchange HELLO messages to negotiate a protocol version,
cryptographic suite, limits, and extensions. Second, they exchange
**envelopes** — individually signed CBOR messages — whose `content` field is
defined by the companion specifications. Some envelopes (descriptors,
receipts, revocations) also travel *out of session* via DHT storage or
gossip; the envelope format is identical, but channel binding is absent and
replay rules differ (§7.4).

Design invariants (rationale in 01-design-critique.md C2, C4):

- I1: Signatures cover the exact transmitted bytes. Verifiers MUST NOT
  re-serialize before verifying.
- I2: Every envelope is independently verifiable given only the sender's
  key material (self-contained authentication).
- I3: Unknown non-critical fields are ignored *and preserved verbatim* when
  a message is stored or forwarded.
- I4: All normative structures are defined in CDDL [RFC8610] over the
  deterministic CBOR profile of §4.2.

## 2. Cryptographic Suites

A suite fixes every primitive used by core. Registry: draft-map-ops-01 §3.

| ID | Signature | KEM/DH | AEAD | Hash | KDF |
|----|-----------|--------|------|------|-----|
| 1 (MAP-SUITE-1, MTI) | Ed25519 [RFC8032] | X25519 [RFC7748] | ChaCha20-Poly1305 [RFC8439] | SHA-256 | HKDF-SHA-256 [RFC5869] |
| 2 (MAP-SUITE-2) | Ed25519 | X25519 | ChaCha20-Poly1305 | BLAKE3 | BLAKE3-derive-key |
| 3 (reserved) | hybrid Ed25519+ML-DSA-44 | X25519+ML-KEM-768 | ChaCha20-Poly1305 | SHA-256 | HKDF-SHA-256 |

Suite 1 is mandatory-to-implement (maximum ecosystem overlap: identical
primitives to Noise/TLS1.3/libp2p defaults). Suite 2 is RECOMMENDED where
BLAKE3 content addressing dominates. Suite 3 is a placeholder establishing
the post-quantum migration path (arch G7); its code point MUST NOT be used
until a companion specification defines it. Hash *references* inside
messages use multihash, so mixed-suite meshes interoperate on content
addressing.

## 3. Identifiers and Common Types (CDDL)

```cddl
AgentID     = bstr .size (34..66)   ; multicodec-prefixed public key
                                    ; (draft-map-identity-01 §3)
MsgID       = bstr .size 16         ; cryptographically random per message
SessionID   = bstr .size 16
TaskID      = bstr .size 32         ; requester-chosen; see tasks §3
Multihash   = bstr                  ; per multiformats/multihash
CapURI      = tstr .regexp "map:cap:[a-z0-9][a-z0-9/._-]{0,126}"
Timestamp   = uint                  ; milliseconds since Unix epoch, UTC
Version     = uint                  ; (major << 8) | minor; 0x0100 = 1.0
ZoneLabel   = uint .le 3            ; arch §8.1
ExtMap      = { * int => any }      ; extension key registry: ops §3
```

## 4. The Envelope

### 4.1 Structure

Every MAP message is a COSE_Sign1 [RFC9052] object, CBOR tag 18:

```cddl
Envelope = #6.18([
  protected:   bstr .cbor ProtHdr,
  unprotected: {},                  ; MUST be empty map; receivers ignore
  payload:     bstr .cbor Body,
  signature:   bstr .size 64        ; per negotiated suite (Ed25519 here)
])

ProtHdr = {
  1 => -8,                          ; COSE alg: EdDSA
  4 => AgentID,                     ; COSE kid: SENDER AgentID
  ? 33 => [+ bstr]                  ; x5chain-slot reused: delegation chain
}                                   ; (identity §6); order: leaf..root

Body = {
  0  => Version,                    ; protocol-version (negotiated, §6)
  1  => uint,                       ; msg-type (registry §11)
  2  => MsgID,
  3  => Timestamp,
  ? 4  => AgentID,                  ; recipient (REQUIRED in-session)
  ? 5  => uint,                     ; ttl-ms (store-and-forward objects)
  ? 6  => SessionID,
  ? 7  => uint,                     ; seq (REQUIRED in-session, §7.2)
  ? 8  => [* int],                  ; crit: ext keys receiver MUST grok
  ? 9  => ExtMap,                   ; extensions
  10 => any                         ; content, per msg-type
}
```

The signature is computed per COSE over
`Sig_structure = ["Signature1", protected, external_aad, payload]`
encoded with §4.2. `external_aad` is the channel-binding token (§7.3) for
in-session messages and the empty byte string for out-of-session objects.

### 4.2 Deterministic encoding profile

All CBOR that is signed, hashed, or content-addressed MUST use RFC 8949
§4.2.1 core deterministic encoding: shortest-form integers, definite-length
strings/arrays/maps only, map keys sorted bytewise on their encoded form,
no duplicate keys. Floating-point values MUST NOT appear in signed
structures; quantities that are conceptually fractional use integer
milli-/micro-units or CBOR decimal fractions (tag 4) with integer mantissa
and exponent. Per invariant I1, this profile governs *producers*; verifiers
operate on received bytes and additionally MUST reject envelopes violating
the profile (malleability defense).

### 4.3 Validation rules (every receiver, every envelope)

1. Well-formed CBOR; tag 18; 4-element array; sizes within negotiated
   limits (§6.3) — else error 1000/1004.
2. `protected.alg` matches negotiated suite — else 1001.
3. `Body[0]` equals the negotiated version (in-session) or a supported
   version (out-of-session) — else 1001.
4. All keys listed in `crit` are understood — else 1002.
5. Timestamp within ±120 s of local clock (in-session) or within object
   class freshness rules (out-of-session) — else 1003.
6. Replay checks pass (§7) — else 1003.
7. Signature verifies against `kid`, or against the leaf key of a valid
   delegation chain terminating at `kid`'s root (identity §6) — else
   2000/2003.
8. Sender authorization for msg-type in current state (per companion spec)
   — else 2005.
Failing envelopes MUST NOT cause state transitions; receivers SHOULD
respond with ERROR (in-session) and MUST rate-limit error responses.

## 5. Message Types (initial registry)

| Range | Family | Defined in |
|---|---|---|
| 0x0001–0x00FF | Core: HELLO 0x0001, HELLO_ACK 0x0002, PING 0x0003, PONG 0x0004, CLOSE 0x0005, ERROR 0x0006, FLOW_CREDIT 0x0007 | this doc |
| 0x0100–0x01FF | Discovery: DESCRIPTOR 0x0100, STATUS_QUERY 0x0101, STATUS_REPORT 0x0102, CAP_QUERY 0x0103, CAP_RESPONSE 0x0104 | discovery |
| 0x0200–0x02FF | Tasks: OFFER 0x0200, QUOTE 0x0201, ASSIGN 0x0202, ACCEPT 0x0203, REJECT 0x0204, CHUNK 0x0205, TOOL_CALL 0x0207, TOOL_GRANT 0x0208, TOOL_RESULT 0x0209, CHECKPOINT 0x020A, CHECKPOINT_ACK 0x020B, CANCEL 0x020C, COMPLETE 0x020D, RESUME 0x020E, RESERVE 0x020F, LEASE 0x0210, LEASE_RELEASE 0x0211 | tasks |
| 0x0300–0x03FF | Trust: RECEIPT 0x0300, RECEIPT_ACK 0x0301, EVIDENCE_QUERY 0x0302, EVIDENCE_SET 0x0303, CHALLENGE 0x0304, CHALLENGE_RESPONSE 0x0305 | trust |
| 0x0400–0x04FF | Pay: PAY_QUOTE 0x0400, PAY_AGREE 0x0401, PAY_INVOICE 0x0402, PAY_PROOF 0x0403, PAY_RECEIPT 0x0404, PAY_DISPUTE 0x0405, PAY_DISPUTE_EVIDENCE 0x0406 | pay |
| 0x0500–0x05FF | Identity: ROTATE 0x0500, REVOKE 0x0501, DELEGATE 0x0502 | identity |
| 0xF000–0xFFFF | Private use | — |

`0x0206` is deliberately unassigned (a hole between CHUNK 0x0205 and
TOOL_CALL 0x0207; never to be reused — ops §3.3).

Registration policy for new code points: Specification Required
(draft-map-ops-01 §3).

### 5.1 Extension Keys (initial registry)

Extensions are negotiated in HELLO (§6.1 key 3) before use and keyed by
these code points; each is fully specified in its owning document. Points
are immutable once assigned (ops §3.3); ≥ 0xF000 is private-use.

| Key | Name | Defined in | Status |
|---|---|---|---|
| 1 | ext-bid | tasks §2.2 | complete |
| 2 | ext-kvx | tasks §8.4 | reserved (stub) |
| 3 | ext-git | tasks §9 | reserved (stub) |

## 6. Session Establishment and Negotiation

### 6.1 HELLO / HELLO_ACK

The connection initiator sends HELLO as its first envelope:

```cddl
Hello = {
  1 => [+ Version],        ; supported protocol versions, descending pref
  2 => [+ uint],           ; supported suites, descending pref
  3 => { * int => uint },  ; offered extensions: ext-key => max ext-version
  4 => uint,               ; max-message-size accepted (bytes; >= 65536)
  ? 5 => uint,             ; max concurrent tasks accepted
  ? 6 => bstr .size 32,    ; cookie (retry/DoS defense, §6.5)
  ? 7 => [* ZoneLabel]     ; zones the sender is willing to operate in
}
```

The responder selects the highest mutually supported version, the first
mutually supported suite in the *initiator's* preference order, and the
intersection of extensions, replying HELLO_ACK:

```cddl
HelloAck = {
  1 => Version,            ; chosen
  2 => uint,               ; chosen suite
  3 => { * int => uint },  ; accepted extensions with chosen versions
  4 => uint,               ; responder's max-message-size
  ? 5 => uint,
  ? 8 => bstr              ; transcript-hash: H(HELLO bytes || this map
}                          ;  fields 1..5 deterministically encoded)
```

Note the field-shape asymmetry under identical field numbers: Hello
fields 1/2 are *lists* (offered sets) while HelloAck fields 1/2 are
*singletons* (the chosen values) — implementers regularly miss this;
TV-001/TV-003 are the byte-level guard.

If no version or suite is mutual, the responder sends ERROR 1001 with the
supported sets and closes. Both peers MUST verify each envelope's signer
matches the transport-authenticated identity (§7.3) — the HELLO exchange is
therefore itself authenticated and MITM-resistant.

The initiator's HELLO envelope carries a fresh SessionID (Body key 6);
the responder MUST echo the same SessionID in its HELLO_ACK envelope,
and every subsequent in-session envelope carries it. A SessionID
mismatch is a state violation (1006) and the session MUST be closed.

### 6.2 Downgrade resistance

Both peers compute the negotiation transcript hash
`TH = H("MAPv1-transcript" || HELLO-envelope-bytes || HELLO_ACK-envelope-bytes)`
with the chosen suite's hash. TH is mixed into the channel-binding token
(§7.3). An attacker who strips versions/suites from either HELLO alters TH
and every subsequent signature check fails — closing threat TH14.

### 6.3 Limits

Default/floor limits: max-message-size 256 KiB (floor 64 KiB); larger
payloads MUST use chunked CHUNK streams or content-addressed fetch. Peers
MUST enforce their advertised limits and MUST NOT send beyond the peer's.
Receivers SHOULD enforce the size limit on the raw received frame before
any decoding or signature work (cheapest check first; DoS posture §6.5).

### 6.4 Version policy

`Version` is (major, minor). Minor revisions are strictly additive
(new optional fields, new msg-types); implementations MUST accept unknown
optional Body keys and unknown msg-types within a session by replying ERROR
3000 without closing. Major revisions may break anything and never
interoperate implicitly. Deprecation process: draft-map-ops-01 §3.4.

### 6.5 Denial-of-service posture at handshake

Responders under load MAY reply to HELLO with ERROR 1005 carrying a
`cookie`; the initiator retries HELLO including the cookie, proving
reachability before the responder allocates session state (cf. DTLS/QUIC
retry). Signature verification precedes all allocation-heavy processing.

## 7. Anti-Replay and Channel Binding

### 7.1 Message-level

Receivers maintain, per sender, a cache of MsgIDs seen within the last
10 minutes and MUST reject duplicates (1003) and out-of-window timestamps.
The cache bound is enforceable because of rate limits (§8.3).

### 7.2 Session sequence numbers

In-session envelopes carry `seq`, starting at 0 per direction and
incrementing by 1. Receivers MUST reject non-monotonic seq (1003). seq
gives cheap ordering across multiplexed transports and makes transcript
audits (tasks §8) unambiguous.

### 7.3 Channel binding

On session establishment each peer derives
`CB = HKDF(TH, transport-exporter, "map cb", 32)`
where `transport-exporter` is: the TLS 1.3 exporter
(`EXPORTER-map-binding`) for TLS/QUIC transports; the Noise handshake hash
for Noise transports; for transports lacking an exporter, the
concatenation of both peers' static public keys as authenticated by the
transport. CB is the COSE `external_aad` for every in-session envelope.
Consequences: an envelope recorded on one session cannot verify on another
(anti-relay), and the signer is cryptographically bound to the
transport-authenticated endpoint (TH4). The transport identity MUST equal
the envelope `kid`'s key or a delegated key chained to it (identity §6);
mismatch is error 2000 and immediate CLOSE.

### 7.4 Out-of-session objects

Descriptors, revocations, receipts, and other store-and-forward objects use
empty `external_aad`, MUST carry `ttl-ms`, and are deduplicated by content
address; freshness/supersession rules are per object class (e.g.,
descriptor `seq`, discovery §5.3).

## 8. Flow Control and Rate Limiting

### 8.1 Credit-based streaming

Any message family that streams (task CHUNKs, evidence sets) is governed by
receiver-granted credit. FLOW_CREDIT content:

```cddl
FlowCredit = {
  1 => TaskID / SessionID,   ; scope
  2 => uint,                 ; additional message credits
  3 => uint                  ; additional byte credits
}
```

Senders MUST NOT exceed outstanding credit in either unit; violations are
1005 and MAY be receipt-recorded. Initial credits are granted in
ASSIGN/ACCEPT (tasks §4). Rationale: transport backpressure alone lets a
slow or malicious reader pin provider resources (VRAM, queue slots)
indefinitely — critique "no flow control". Credit lets providers bound
commitment and bill for reserved-but-unconsumed output.

### 8.2 Keepalive

PING/PONG carry an opaque 8-byte payload echoed verbatim; RECOMMENDED
interval 30 s on idle sessions; 3 missed PONGs ⇒ session failure handling.

### 8.3 Rate limits

Implementations MUST enforce configurable per-peer ceilings on: envelopes/s,
HELLOs/min from an address, STATUS_QUERYs/min, and DHT-record ingestion/s,
with 1005 + exponential backoff hints (`retry-after-ms` in ERROR detail).

## 9. Error Handling

ERROR content:

```cddl
ErrorMsg = {
  1 => uint,            ; code
  2 => bool,            ; retryable
  ? 3 => tstr,          ; human-readable (English; not for machines)
  ? 4 => MsgID,         ; message being answered
  ? 5 => { * int => any } ; machine-readable detail (per-code schema)
}
```

Code space (registry; Specification Required):

- **1xxx protocol**: 1000 malformed, 1001 unsupported version/suite,
  1002 unknown critical extension, 1003 replay/out-of-window/bad-seq,
  1004 message too large, 1005 rate limited / no credit, 1006 state
  violation.
- **2xxx auth/authz**: 2000 bad signature/identity misbind, 2001 expired
  object, 2002 revoked key, 2003 invalid delegation chain, 2004 zone
  violation, 2005 not authorized for message type, 2006 countersignature
  refused (the receiver declines to attest the sender's claimed
  facts — e.g. receipt usage/timestamps outside countersignature
  tolerance, trust §3).
- **3xxx capability**: 3000 unsupported message/capability, 3001 model
  unavailable, 3002 context too large, 3003 ontology term unknown.
- **4xxx resource**: 4000 overloaded, 4001 queue full, 4002 insufficient
  memory/VRAM class, 4003 reservation expired, 4004 storage exhausted.
- **5xxx execution**: 5000 task failed, 5001 deadline/TTFT exceeded,
  5002 cancelled, 5003 checkpoint invalid, 5004 tool denied, 5005 output
  policy violation, 5006 resumption impossible.
- **6xxx payment**: 6000 quote expired, 6001 payment required, 6002
  settlement failed, 6003 dispute opened, 6004 method unsupported,
  6005 authorized cap exceeded (PayAgree.4), 6006 agreement digest
  mismatch (quote/agreement envelope digest does not match the
  referenced bytes — price-switch defense, pay §3.1), 6007 task
  binding mismatch (a pay message's task binding ≠ the bound task:
  PayQuote.9 vs the OFFER's TaskID, pay §3; UsageWatermark.1 vs the
  agreement's task, pay §4.1), 6008 invalid payment proof (forged or
  mismatched settlement proof, pay §5), 6009 invoice-watermark violation
  (a metered invoice's UsageWatermark fails a pay §4.1 check: presence,
  monotonicity, meter consistency, or billing arithmetic), 6010 escrow
  violation (a terms-1/2 invoice or receipt fails a pay §5.2 check:
  phase presence, prepaid/hold amount ≠ quoted total, premature or
  oversized release, duplicate hold, or an unrecognized receipt kind).
  6002 remains the generic settlement failure for causes not covered by
  a specific code.

Retryability semantics: `retryable = true` means the identical request MAY
succeed later at the same peer; false means the requester MUST change
something (peer, parameters, payment) — this drives orchestrator fallback
logic deterministically (adopting design3's TTFT/fallback discipline).

## 10. Transport Bindings

Common requirements: mutual authentication with keys verifiable against
AgentIDs; confidentiality and integrity; ordered byte streams;
multiplexing (or one connection per session). Envelope framing on raw
streams: 4-byte big-endian length prefix, then envelope bytes; length MUST
respect negotiated max-message-size.

- **libp2p (RECOMMENDED)**: protocol ID `/map/1.0.0`; security Noise or
  TLS; muxer yamux; transports QUIC-v1 preferred, TCP, WebSocket,
  WebTransport. PeerID MUST equal the AgentID key (identity §3.2). NAT
  traversal profile — AutoNAT v2, Circuit Relay v2, DCUtR hole punching —
  is REQUIRED for public-mesh providers (discovery §10). One MAP session
  runs per `/map/1.0.0` substream, length-prefixed as above.

  *Channel binding for `/map/1.0.0`* (§7.3 third clause — transports lacking
  an exporter): rust-libp2p does not portably expose a Noise
  handshake-hash / TLS-exporter equivalent on its stream API, so the
  exporter for this binding is `initiator-AgentID || responder-AgentID`
  (the two PeerIDs the connection authenticated, in stream-initiator-first
  order). This binds the session to the authenticated identities of both
  endpoints. Residual (acknowledged, weaker than a true exporter): it binds
  the identity *pair*, not the specific connection instance, so it does not
  by itself separate two concurrent connections between the same pair; the
  per-message msg-id replay cache and the monotonic in-session sequence
  numbers (§7.1) prevent cross-connection replay within the window
  regardless. An implementation whose libp2p version exposes Noise
  handshake-hash material SHOULD use it instead and record that choice.
- **Raw QUIC**: ALPN `map/1`; TLS 1.3 with RawPublicKey or self-signed
  X.509 carrying the Ed25519 key; exporter per §7.3.
- **TCP+TLS 1.3**: ALPN `map/1`; same certificate rules.
- **WebSocket**: subprotocol `map.v1`; binary frames = framed envelopes;
  over TLS; where the WS layer terminates TLS at an intermediary, CB falls
  back to static-key binding and the session MUST be treated as Zone ≥2.
- **gRPC binding (OPTIONAL, non-normative appendix in tasks)**: bulk CHUNK
  streaming MAY be carried over gRPC for ecosystem convenience; every gRPC
  message remains a MAP envelope (I2 holds regardless of carrier).

## 11. Extension Framework

Extension keys are integers from the MAP Extension Registry (ops §3).
Rules: (a) unknown keys absent from `crit` MUST be ignored and, for stored/
forwarded objects, preserved byte-exactly (I3); (b) keys present in `crit`
MUST be understood or the envelope rejected with 1002; (c) extensions are
version-negotiated in HELLO field 3 before use in-session; (d) extensions
MUST NOT redefine core field semantics. This mirrors the proven
HTTP/TLS/COSE evolution model and is the mechanism by which bidding,
KV-cache transfer, Git workspace sync, etc. attach without core changes.

## 12. Session State Machine

```
                    send HELLO                    recv HELLO_ACK(ok)
        +------+  ------------->  +------------+  ---------------→ +-------------+
        | IDLE |                  | HELLO_SENT |                   |             |
        +------+  ------------->  +------------+                   |             |
            |       recv HELLO           | recv ERROR/timeout      | ESTABLISHED |
            |                            v                         |             |
            |                        +--------+   CLOSE sent/recv, |             |
            +--- (responder) ------> | CLOSED | <---- auth fail ---+-------------+
                 send HELLO_ACK,     +--------+        or 3×PING loss    |
                 enter ESTABLISHED        ^                              |
                                          +------------------------------
```

ESTABLISHED admits all in-session message families per negotiated
conformance classes. CLOSE content carries an ErrorMsg-shaped reason.
Half-close is not modeled; either CLOSE tears down the session after
flushing acknowledged state (task-level survival across sessions is
handled by tasks §8 resumption, not by the session layer).

## 13. Examples and Test Vectors

TV-001 (signed HELLO envelope, 166 bytes) and TV-002 (dual-signed
ExecutionReceipt) in `test-vectors/map-test-vectors.md` are normative
encoding examples generated with real Ed25519 keys; all signatures verify
per §4. Implementations MUST reproduce byte-identical Sig_structures from
the listed inputs.

## 14. Security Considerations

Covered throughout: bytes-not-objects verification (§4, I1), deterministic
encoding as malleability defense (§4.2), downgrade resistance (§6.2),
handshake DoS (§6.5), replay and relay (§7), resource pinning (§8),
information leakage via error verbosity (ERROR `detail` MUST NOT echo
payload contents to unauthenticated peers). Residual: timestamp windows
assume ±120 s clock sanity; deployments with worse clocks MUST widen
windows and accept the enlarged replay cache, or deploy roughtime.

## 15. IANA-Style Considerations

This document establishes (initial contents above; policies in ops §3):
MAP Cryptographic Suites; MAP Message Types; MAP Body Keys; MAP Extension
Keys; MAP Error Codes; MAP Transport Bindings (ALPN/protocol-ID strings). The
assigned values and the full family-wide index are in `draft-map-wire-01.md`
§3.1, §3.2, §3.3, §3.4, §3.5, and §3.6 respectively; in case of divergence
the normative tables above govern.

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels.
