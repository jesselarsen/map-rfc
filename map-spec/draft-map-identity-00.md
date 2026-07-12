---
title: "MAP Identity: Agent Identifiers, Key Hierarchy, Delegation, and Lifecycle"
docname: draft-map-identity-00
category: std
---

# MAP Identity

**Status:** preprint, work in progress. BCP 14 language applies (see
Requirements Language, below).

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## Abstract

Defines the self-certifying MAP agent identifier (AgentID), a three-tier
key hierarchy (root, device, session), delegation certificates with
attenuating caveats, key rotation and revocation records, ephemeral and
organizational identities, hardware-backed keys, and interoperability with
W3C DIDs (did:key, did:web), libp2p PeerIDs, and WebAuthn.

## 1. Design position

Identity in MAP is a keypair, not a name, and confers nothing but
attributability (arch T3–T5). There is no global PKI and no certificate
authority; all authority statements are peer-issued delegations verifiable
offline. The identifier is deliberately byte-compatible with two deployed
ecosystems so that no bespoke resolver is needed (fixes critique C13):

- **libp2p**: an Ed25519 PeerID is `multihash(identity, protobuf-pubkey)`;
  MAP mandates Ed25519 identities on libp2p transports, so the transport
  peer identity and AgentID derive from the same key (§3.2).
- **did:key**: `did:key:z...` for an Ed25519 key is exactly
  multibase-base58btc(multicodec(0xED01) || key) — the AgentID's canonical
  text form (§3.1).

## 2. Key hierarchy

```
Root Identity Key (RIK)  — Ed25519; SHOULD be offline or hardware-backed;
      |                    signs only: delegations, rotation, revocation.
      v  DelegationCert (scope: all or subset; long-lived)
Device Key (DK)          — per device/process; signs descriptors,
      |                    delegations to session keys, receipts.
      v  DelegationCert (scope: narrow; short-lived, hours)
Session Key (SK)         — per session or per counterparty; used as the
                           transport static key and envelope signer.
```

Single-key operation (RIK = DK = SK) is permitted for constrained or
low-value agents; the hierarchy is the RECOMMENDED posture because it
bounds blast radius: SK compromise burns hours, DK compromise burns one
device until revoked, RIK compromise is identity loss (§7).

## 3. AgentID

### 3.1 Format

```
AgentID(bytes) = multicodec-varint(key-type) || raw-public-key
  ed25519-pub  = 0xED 0x01 || 32 bytes            (suite 1/2)
AgentID(text)  = multibase(base58btc, AgentID(bytes))   ; "z6Mk..."
DID form       = "did:key:" || AgentID(text)
```

Future suites register their multicodec here; verifiers dispatch on the
prefix — this is the identity leg of cryptographic agility.

### 3.2 Binding rules

B1: On libp2p, the connection's PeerID key MUST equal the peer's SK, and
the envelope `kid` MUST be an AgentID whose key is that SK — either
directly, or as the LEAF subject of a delegation chain carried in ProtHdr
key 33. B2: On TLS transports, the certificate/raw public key MUST satisfy
the same rule. B3: `kid` names the *signing* key; the **accountable**
identity is the chain root's issuer when a chain is present, and `kid`
itself otherwise. Verifiers resolve and report the accountable identity;
receipts and trust records accrue to it. (B1/B3 rewritten 2026-07-12 with
the §4 kid ruling; see ERRATA.)

## 4. Delegation certificates

```cddl
DelegationCert = Envelope             ; msg-type 0x0502, out-of-session
DelegateBody = {                      ; carried in Body key 10
  1 => AgentID,                       ; issuer (parent key)
  2 => AgentID,                       ; subject (child key)
  3 => Timestamp,                     ; not-before
  4 => Timestamp,                     ; not-after
  ? 5 => [+ CapURI],                  ; capability scope (absent = all)
  ? 6 => [+ ZoneLabel],               ; zone scope
  ? 7 => uint,                        ; max further delegation depth
  ? 8 => { * int => any }             ; caveats (registry; e.g. budget,
}                                     ;  audience, task-id pinning)
```

Chain validation: each cert's issuer key signs its envelope; each child is
the next cert's issuer; scopes/zones/depth only ever *attenuate* (a child
scope MUST be a subset of the parent's; violations ⇒ 2003); all validity
windows MUST cover the verification time; the envelope `kid` MUST equal
the chain LEAF's subject (the leaf key produces the envelope signature;
violations ⇒ 2003), and the verifier resolves the chain root's issuer as
the accountable identity (§3.2 B3). `max-depth` (key 7) bounds further
delegation below the declaring certificate: a chain is invalid (2003)
when any certificate has more certificates between itself and the leaf
than its declared max-depth — a cert with max-depth 0 must itself be the
leaf cert. (kid sentence corrected and max-depth counting pinned
2026-07-12, ARCHITECT RULING; wire behavior was already frozen by
TV-012 — see ERRATA.) Semantics deliberately parallel UCAN (informative
reference) so existing tooling and audit intuition transfer. Tool-grant
tokens in tasks §7 are DelegationCerts with task-pinning caveats — one
mechanism, two uses.

Chains are carried in ProtHdr key 33, ordered leaf..root (core §6). A chain
MUST NOT exceed **8** certificates (a denial-of-service bound: chain
validation is one signature verification per link). An **unknown caveat key
MUST invalidate the entire chain** (fail-closed: a caveat is a restriction,
so silently ignoring one would widen scope). Registered caveat keys (§11):
`1` = **task-pinning** (value: a 32-byte TaskID, tasks §1 — the chain is
valid only for envelopes whose Body references that task; the referencing
check is performed by the consumer); `2` = **audience** (value: a 34-byte
AgentID — the chain is valid only when the validating party's own AgentID
equals it).

[RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-08): the former
clause "the chain root MUST equal the envelope `kid`" was corrected in
§4 and §3.2 above — `kid` equals the chain leaf's subject; the root is
the accountable identity. Byte-level behavior unchanged (frozen by
TV-012). Full record in `map-spec/ERRATA.md`.]

## 5. Descriptor binding and proof of possession

An agent proves control of its AgentID implicitly: every envelope is
signed, and sessions are channel-bound (core §7.3). No separate
challenge-response registration exists or is needed; a DHT record or
descriptor is trusted exactly as far as its signature chain.

## 6. Organizational and ephemeral identities

**Organizational**: an org RIK delegates to member DKs with scoped caveats;
counterparties see receipts accrue to the org AgentID while members remain
individually revocable. Orgs MAY additionally publish `did:web` documents
listing their AgentID(s) to anchor a human-meaningful name; MAP itself
never requires the DNS anchor (G1) and treats it as evidence, not
authority.

**Ephemeral (Zone 3 / anonymous)**: a fresh keypair with no delegation
chain. Fully permitted (arch G4). Ephemeral identities start with zero
evidence, are subject to admission costs where the mesh applies them
(trust §5), and cannot benefit from any prior reputation — which is
precisely the Sybil containment property. Linkability trade-off is the
user's: reuse accrues evidence; rotation buys privacy.

## 7. Rotation and revocation

```cddl
RotateBody = { 1 => AgentID/*old*/, 2 => AgentID/*new*/, 3 => Timestamp,
               ? 4 => uint /*grace-ms, default 7 days*/ }
RevokeBody = { 1 => AgentID/*revoked*/, 2 => Timestamp,
               ? 3 => uint /*reason: 0 unspecified,1 compromise,
                             2 superseded,3 cessation*/ }
```

ROTATE MUST be signed by the *old* key (proving continuity) and SHOULD be
countersigned by the new key (an ext field carries the second signature)
proving possession. REVOKE is signed by the revoked key's parent (for
DK/SK) or by the RIK itself (self-revocation on compromise; reason 1).
Distribution: rotation/revocation records are stored in the DHT under
`H("map:id:" || AgentID)` and gossiped on a low-rate identity topic;
verifiers of high-value operations SHOULD fetch freshness ≤24 h old and
MUST treat known-revoked keys as 2002. There is no global instant
revocation (T8); risk-tier your checks. RIK compromise is unrecoverable by
design — evidence and receipts attach to keys, not names; this is stated
plainly rather than papered over.

## 8. Hardware backing and WebAuthn

RIKs and DKs SHOULD reside in hardware (TPM 2.0, secure enclave, YubiKey/
CTAP2 authenticators). WebAuthn interop: an authenticator-resident Ed25519
credential (COSE alg -8) can serve directly as a RIK; the WebAuthn
assertion format is not carried on the wire — the authenticator simply
performs the RIK's rare signatures (delegations, rotations). Attestation of
hardware residence MAY be advertised in the descriptor SecurityProfile
(discovery §5) and lifts trust priors per local policy only.

## 9. Validation summary (normative checklist)

On any signed object: resolve `kid` → root AgentID; validate delegation
chain per §4 (scope, zone, depth, time); check revocation state per local
freshness policy; verify signature over received bytes (core I1); verify
transport binding B1–B3 for in-session messages. Errors: 2000–2005.

## 10. Security considerations

Key hierarchy limits blast radius but adds chain-validation surface —
implementations MUST cap chain length (default 5) and total cert bytes.
Rotation records signed only by old keys allow an attacker holding a stolen
key to rotate away from the victim; hence the countersignature requirement
and grace period during which *both* keys' signatures are accepted and
either MAY REVOKE (reason 1) — a compromise observed during grace is
recoverable. Timestamp trust follows core §14. Privacy: delegation chains
reveal organizational structure; Zone-2/3 interactions SHOULD present
minimal chains (leaf + one parent).

## 11. IANA-style considerations

Registries established: MAP Key Types (multicodec bindings); MAP Delegation
Caveats; MAP Revocation Reasons.

MAP Delegation Caveats (DelegateBody key 8; §4). Assigned code points
(immutable once merged, ops §3.3):

| Key | Name | Value | Meaning |
|-----|------|-------|---------|
| 1 | task-pinning | 32-byte TaskID | Chain valid only for envelopes referencing that task. |
| 2 | audience | 34-byte AgentID | Chain valid only when validated by that party. |

An unknown caveat key MUST invalidate the chain (§4, fail-closed).

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels.
