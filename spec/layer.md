# MAP (Mutual Attestation Protocol) — Accountability Layer, version 2.0-draft

This document specifies MAP as a thin accountability layer for delegated
agent work. It replaces the 11-document v1 family (tag `map-v1-ietf`) per
`docs/ADR-0001-thin-accountability-layer.md`. MAP is no longer a full
protocol stack; it defines evidence, local trust, checkpointed resumption,
metering-to-evidence binding, data-sensitivity labeling, and discovery
records, and it runs *over* three existing ecosystems rather than
reimplementing them: [A2A](https://a2a-protocol.org/latest/) carries task
exchange, an [ADS](https://datatracker.ietf.org/doc/draft-mp-agntcy-ads/)-compatible
Kademlia DHT carries discovery records, and MPP sessions / x402 V2 carry
payment. This is a versioned, self-hosted specification, not an IETF
submission; it carries no "Status of This Memo," no IANA section, and no
BCP 78/79 boilerplate.

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in BCP 14
[RFC2119] [RFC8174] when, and only when, they appear in all capitals, as
shown here.

## 1. Scope and non-goals

<!-- scope drawn from ADR-0001 keep/replace/drop table and v1 arch-01 §4.1 non-goals -->

### 1.1 What this layer does

MAP defines five things, all provider- and transport-agnostic:

1. **Evidence.** Dual-signed `ExecutionReceipt`s (§6) as the immutable,
   append-only record of what happened between a requester and a
   provider on one task.
2. **Local trust.** A per-observer projection computed only from evidence
   the observer itself holds and can verify, with a normative rebuild
   rule (§8). No global reputation exists.
3. **Countersigned checkpoints and cross-provider resumption.** A
   requester-countersigned commitment to transcript progress and
   cumulative usage that lets a task resume on a *different* provider
   after the original one fails, with billing bounded to what was
   actually acknowledged (§7).
4. **Metering bound to evidence.** A binding between checkpoint/receipt
   usage fields and the payer-signed cumulative vouchers that actually
   move money on an external rail (§9).
5. **Data-sensitivity zones.** A dual-signed, attributable label on how
   sensitive a task's data was and what a provider claims to handle,
   enforced by refusal rules rather than cryptographic confidentiality
   (§10).
6. **Two-plane discovery records.** Stable, signed capability
   advertisements and on-demand volatile status, published into a
   Kademlia DHT compatible with ADS (§11).

### 1.2 What this layer delegates

MAP does not carry task payloads, streaming, tool calls, or negotiation
messages; that is A2A's job (`spec/binding-a2a.md` defines how A2A Task
and Message objects map onto the structures below — transcript leaves,
zone propagation, checkpoint triggers). MAP does not define a DHT wire
protocol, routing table, or transport; that is the libp2p Kademlia
binding's job (`spec/binding-libp2p-discovery.md`). MAP does not move
money; invoicing, batch settlement, and payment channels belong to the
rail (`spec/mapping-payment-rails.md` maps MPP sessions and x402 V2 onto
the `voucher-digest` fields defined here).

```
+-------------------------------------------------------------------+
| MAP Accountability Layer (this document)                          |
|   Evidence (§6)  Trust (§8)  Checkpoints/Resumption (§7)           |
|   Metering binding (§9)  Zones (§10)  Discovery records (§11)      |
+-------------------------------------------------------------------+
        |                    |                       |
        v                    v                       v
+---------------+   +-------------------+   +----------------------+
| A2A            |   | Kademlia DHT      |   | MPP session / x402 V2|
| (task exchange,|   | (ADS-compatible;  |   | (payer-signed         |
|  streaming,    |   |  stable+volatile  |   |  cumulative vouchers, |
|  tool calls)   |   |  discovery plane) |   |  batch settlement)    |
+---------------+   +-------------------+   +----------------------+
binding-a2a.md        binding-libp2p-        mapping-payment-rails.md
                       discovery.md
```

This layer contributes no transport, no routing, and no money movement
of its own; it contributes the signed structures — receipts,
checkpoints, descriptors, and the fields that bind usage to a voucher —
that make delegated work on top of these three ecosystems attributable.

### 1.3 Non-goals

- **No global reputation.** Trust is a local, per-observer projection
  (§8); two honest observers MAY disagree about a third party. Nothing
  in this layer computes or publishes a single score for an agent.
- **No settlement, escrow, or dispute resolution.** MAP references a
  `voucher-digest` (§9) as an accountability anchor; it does not define
  invoices, holds, releases, refunds, or an adjudication procedure. The
  v1 settlement registry, credit ledger, and dispute protocol are
  retired (§13).
- **No confidentiality guarantee from zones.** A zone label (§10) is an
  attributable, dual-signed claim about data sensitivity, not an
  encryption boundary or a TEE attestation. A provider that mishandles
  Zone-1 data after correctly claiming Zone-1 support is *accountable*
  (the signed label makes the violation provable), not *prevented*.
- **No semantic-quality guarantee.** MAP makes execution attributable
  and evidence-bearing. It says nothing about whether an output was
  correct, safe, or good.

## 2. Terminology

<!-- adapted from v1 arch-01 §3 -->

- **Agent**: a protocol participant identified by an AgentID (§3). Acts
  as Requester, Provider, or both, per task.
- **AgentID**: a self-certifying identifier derived from an Ed25519
  public key (§3).
- **Requester / Provider**: the party commissioning a task / the party
  executing it, per A2A task. Roles are per-task, not per-agent.
- **Task**: a unit of delegated work exchanged over A2A and identified
  by a `TaskID` (`spec/binding-a2a.md` §1 defines the binding to A2A's
  own task identifier).
- **Descriptor**: a signed, content-addressed stable-plane advertisement
  of an agent's capabilities and accepted zone labels (§11).
- **Capability**: a registered hierarchical URI naming a service class
  under `map:cap:` (§11.3).
- **Evidence**: a signed fact admissible to the local trust projection —
  in this version, exclusively dual-signed `ExecutionReceipt`s (§6) and
  unilateral negative receipts (§6.4, rule R-4).
- **Receipt**: an `ExecutionReceipt` (§6), the dual-signed record of one
  completed (or failed, timed out, or disputed) task.
- **Transcript**: the ordered sequence of output/tool content produced
  during a task, committed to by a Merkle root (§5).
- **Checkpoint**: a `CheckpointArtifact` (§7), a dual-signed snapshot of
  transcript progress and cumulative usage that is a valid resumption
  point.
- **Projection**: the locally computed trust score and confidence for a
  (subject, capability) pair (§8). Derived data, never authoritative
  over the receipt store it is built from.
- **Zone**: a data-sensitivity class from 0 (Local) to 3 (Open) labeling
  a task and the peers eligible to run it (§10).
- **Voucher**: a payer-signed, cumulative payment artifact on an
  external rail (MPP session or x402 V2 batch settlement) referenced by
  its digest from a checkpoint or receipt (§9). MAP does not define the
  voucher's own wire format.
- **Rail**: an external settlement mechanism (an MPP session, an x402 V2
  facilitator) that is the sole source of truth for whether value moved.
- **Binding**: a companion document (`spec/binding-a2a.md`,
  `spec/binding-libp2p-discovery.md`, `spec/mapping-payment-rails.md`)
  that carries this layer's structures over a specific external
  protocol.

## 3. Identity

<!-- adapted from v1 identity-01 §3 -->

### 3.1 AgentID

Identity in MAP is a keypair, not a name. There is no PKI and no
certificate authority.

```cddl
AgentID = bstr .size 34            ; 0xED 0x01 || 32-byte Ed25519 public key
```

Normatively, `AgentID(bytes) = 0xED 0x01 || raw-32-byte-Ed25519-public-key`.
The text form is `multibase(base58btc, AgentID(bytes))`, which always
begins with the multibase prefix character `z` (e.g. `z6Mk...`).

Only one key type is defined in this version (Ed25519, RFC 8032); the
`0xED 0x01` multicodec prefix is carried for exactly one reason — so the
text form is byte-identical to a `did:key` identifier (§3.2) — not to
support algorithm negotiation. A future revision that adds a second key
type will widen the `AgentID` length; this version fixes it at 34 bytes.

### 3.2 Informative equivalences

These notes are non-normative; nothing in this layer requires a did:key
resolver or a libp2p stack.

- **did:key.** `"did:key:" || AgentID(text)` is exactly the did:key
  identifier for the same Ed25519 key (W3C did:key v0.9). An AgentID
  needs no separate resolution step to be read as one.
- **libp2p PeerID.** When an agent is reached over the libp2p discovery
  binding (`spec/binding-libp2p-discovery.md`), an Ed25519 libp2p PeerID
  is `multihash(identity, protobuf-pubkey)` over the same raw public
  key; implementations MAY derive the PeerID from the AgentID's key
  material, but the two identifiers are not required to be the same
  bytes on the wire — the binding document states the exact rule.

### 3.3 Signers

This version does not carry delegation certificates, key hierarchies, or
rotation/revocation records. Every signature this layer requires (on a
receipt, half of a receipt; on a checkpoint, half of a checkpoint; on a
descriptor) MUST be produced by the private key of the AgentID that the
structure names in that role. A receipt or checkpoint whose purported
requester or provider signature does not verify against the AgentID
named in the corresponding core field is not evidence (§6.4 rule R-1).

This is a deliberate simplification. v1 identity-01 §4 defined
attenuating `DelegationCert` chains (root/device/session key hierarchy,
capability-scoped and zone-scoped caveats, fail-closed on an unknown
caveat) so that, among other things, a receipt signer could "chain to"
the named party via delegation. That machinery is not adopted here: it
added chain-validation surface for a case (organizational identities
acting through short-lived session keys) that is now better handled by
whatever key-management the A2A or wallet layer underneath an agent
already provides. If a future revision needs delegated signing for
receipts or checkpoints, v1 identity-01 §4's semantics — scope/zone/depth
only ever attenuate, an unknown caveat invalidates the whole chain, the
envelope signer MUST be the chain leaf — are the model to reuse rather
than re-derive.

**Deferred:** key rotation and revocation records are out of scope for
this version; an AgentID compromise is unrecoverable within the layer
itself (an agent simply mints a new AgentID and starts over with no
carried-over evidence, which is also the correct Sybil-containment
behavior per §8).

### 3.4 Key custody (informative)

This document does not mandate where an AgentID's private key lives.
v1 identity-01 §8 recommended hardware backing (TPM, secure enclave,
FIDO2/CTAP2 authenticator) for long-lived root keys under its three-tier
hierarchy; that recommendation transfers loosely even though the
hierarchy itself is dropped (§3.3) — an AgentID used across many tasks
accumulates evidence (§6, §8) that has real value to protect, exactly as
a v1 root key did. Whether a given deployment holds its AgentID key in
hardware, in a wallet the surrounding A2A agent already manages, or in
plain software is outside this document's scope; nothing here depends
on the answer, because every verification rule (§4.3, §6.4, §7.2) is
stated over signatures and bytes, never over key-custody assumptions.

## 4. Encoding and signatures

<!-- adapted from v1 core-01 §4 -->

### 4.1 Deterministic CBOR

Every structure defined in this document that is signed, hashed, or
content-addressed MUST use the RFC 8949 §4.2.1 core deterministic
encoding profile: shortest-form integers; definite-length strings,
arrays, and maps only; map keys sorted bytewise on their encoded form;
no duplicate keys. Floating-point values MUST NOT appear in signed
structures; fractional quantities use a CBOR decimal fraction (tag 4,
`[exponent, mantissa]`) with integer components, never a float.

### 4.2 Two signing patterns

This layer uses two distinct signing shapes, matching how each artifact
is consumed:

**Inner-signed artifacts** — `ExecutionReceipt` (§6) and
`CheckpointArtifact` (§7). Each is a CBOR map carrying its core as a
`bstr .cbor Core` byte string plus zero, one, or two 64-byte Ed25519
signatures alongside it. The signing input is domain-separated and
role-tagged:

```
signing-input(tag, role, core-bytes) = enc([tag, role, core-bytes])
```

where `tag` is a fixed `tstr` naming the artifact class (`"MAPReceipt"`,
`"MAPCheckpoint"`), `role` is a `uint` (1 = requester, 2 = provider), and
`core-bytes` is the *exact received* `bstr` contents of the core field —
never a re-serialization. `enc(...)` is §4.1's deterministic CBOR
encoding of the three-element array. Because the signing input is a
closed-form function of bytes anyone can see, these artifacts are
verifiable by any third party without session context — this is what
lets a checkpoint be handed to a successor provider or a receipt be
disclosed to an evaluator with nothing else attached.

**Enveloped artifacts** — the `AgentDescriptor` (§11.2). Signed as a
COSE_Sign1 [RFC9052] object, CBOR tag 18, over the standard COSE
`Sig_structure`. Used only where an existing ecosystem convention
(A2A Agent Cards use JWS; this layer's own descriptor wrapper uses
COSE_Sign1 for consistency with the rest of MAP) expects a self-describing
signed envelope rather than a bare core-plus-signature pair.

### 4.3 Invariant I1

**Verifiers MUST verify over the received bytes and MUST NOT
re-serialize before verifying.** This applies to both signing patterns
above: a verifier decodes only as far as it must to extract the `bstr`
core (or, for COSE_Sign1, the `protected` and `payload` byte strings),
builds the signing input from those exact bytes, and checks the
signature. Re-encoding a decoded structure and signing or verifying the
re-encoding is non-conformant — it silently tolerates any bytes the
deterministic-encoding profile (§4.1) would have rejected, defeating the
malleability defense that profile exists to provide.

### 4.4 Validation summary (normative checklist)

On any inner-signed artifact this layer defines (`Receipt`, §6;
`CheckpointArtifact`, §7), a verifier MUST, in order:

1. Decode only as far as the outer map and extract the `bstr` core
   (key 1) without decoding *into* it — the core's own bytes are what
   gets hashed into the signing input, not a re-decoded structural
   equivalent.
2. For each signature present (key 2 and/or key 3), build
   `signing-input(tag, role, core-bytes)` per §4.2 and verify the
   64-byte Ed25519 signature against the AgentID the artifact's own
   *content* (not the signature's mere presence) says holds that role —
   §6.4 rule R-1 and §7.2 for the two artifact types respectively.
3. Reject the artifact outright — not "downgrade to unilateral," not
   "accept with a flag" — if a present signature fails to verify. A
   present-but-invalid signature is not equivalent to an absent one:
   an absent signature is silence; an invalid one is evidence of
   tampering or forgery and MUST NOT be treated as if the signer simply
   declined to sign.
4. Only after signatures verify, apply the artifact-specific rules
   (§6.4 for receipts; §7.3 for checkpoint monotonicity).

On an `AgentDescriptor` (§11.2), a verifier MUST additionally check that
`protected.kid` (the COSE key id) equals `DescriptorCore` key 1 (the
subject) — a descriptor signed by a key other than the one it claims to
describe is not self-certifying and MUST be discarded, never cached.

## 5. Transcript commitment

<!-- adapted from v1 tasks-01 §8.1 -->

The `transcript-root` is the Merkle root, under SHA-256, over a task's
output content in delivery order:

```
leaf(c)        = H(0x00 || enc([c.seq, c.kind, c.content]))
                 ; enc = deterministic CBOR (§4.1) of the triple
parent(l, r)   = H(0x01 || l || r)        ; domain-separated from leaves
odd node       = promoted unchanged to the next level
empty root     = H(0x00 || enc([]))       ; zero content items
H              = SHA-256                  ; fixed in this version (§4)
```

The `0x00`/`0x01` domain separation prevents leaf/interior confusion
attacks; the deterministic-CBOR leaf encoding makes leaves injective in
`(seq, kind, content)`. The root is carried, everywhere this document
references it, as a `Multihash`:

```cddl
Multihash = bstr .size 34          ; 0x12 0x20 || 32-byte SHA-256 digest
                                    ; (multicodec sha2-256, length 32)
```

This is exactly the v1 construction (tasks-01 §8.1), unchanged, because
it is shared by checkpoints (§7), receipts (§6), and — informatively —
whatever dispute or audit process a deployment layers on top; changing
it would break all three at once for no benefit.

**What this section does not define:** how A2A `Part`s (text, file,
data, or tool-call/tool-result parts on an A2A `Message` or `Artifact`)
map onto the `(seq, kind, content)` triple above. That mapping —
including which A2A content counts as a transcript leaf at all — is
defined by `spec/binding-a2a.md`, not here. This document only fixes the
commitment function once the triples exist.

## 6. ExecutionReceipt

<!-- adapted from v1 trust-01 §3 -->

An `ExecutionReceipt` is the dual-signed, immutable, past-tense fact of
one task's outcome. It MUST keep v1's wire bytes compatible: an
unmodified v1 `ReceiptCore` decodes and re-encodes identically here,
with two new optional keys added.

### 6.1 Wire format

```cddl
Receipt = {
  1 => bstr .cbor ReceiptCore,   ; exact signed bytes — invariant I1 applies
  ? 2 => bstr .size 64,          ; sig_req  = Ed25519(signing-input("MAPReceipt", 1, core-bytes))
  ? 3 => bstr .size 64           ; sig_prov = Ed25519(signing-input("MAPReceipt", 2, core-bytes))
}                                 ; at least one of 2, 3 MUST be present

ReceiptCore = {
  1 => TaskID,                   ; bstr .size 32, derived per binding (binding-a2a.md §4)
  2 => AgentID,                  ; requester
  3 => AgentID,                  ; provider
  4 => CapURI,                   ; tstr .regexp "map:cap:[a-z0-9][a-z0-9/._-]{0,126}"
  5 => Timestamp,                ; started; uint, ms since Unix epoch UTC
  6 => Timestamp,                ; ended
  7 => uint,                     ; outcome: 0 ok, 1 failed, 2 timeout,
                                  ;          3 cancelled, 4 disputed
  8 => Usage,                    ; as proposed by the first signer; the
                                  ;  second signature attests agreement
                                  ;  within the countersigner's tolerance
                                  ;  of its own observation (§6.5)
  ? 9  => uint .le 100,          ; rating — a subjective assertion, never
                                  ;  presented as verified
  ? 10 => bstr,                  ; payment reference; carries a Multihash
                                  ;  voucher-digest when metering is used (§9)
  ? 11 => Multihash,             ; transcript-root (§5)
  ? 12 => uint .le 3,            ; zone the task ran under (§10)
  ? 13 => Multihash              ; resumed-from: digest of the countersigned
                                  ;  CheckpointArtifact (§7) this task
                                  ;  resumed from, if any
}

Usage = { 1 => uint,             ; tokens-in
          2 => uint,             ; tokens-out
          3 => uint,             ; ttft-ms
          4 => uint }            ; wall-ms
                                  ; cumulative since task start (§9)
```

Keys 1–11 are unchanged from v1 trust-01 §3 (confirmed against
`map-rs/crates/map-trust/src/lib.rs::ReceiptCore` and test vector
TV-002, which this document does not renumber). Keys 12 and 13 are new
in this version.

### 6.2 Outcome codes

Unchanged from v1: 0 ok, 1 failed, 2 timeout, 3 cancelled, 4 disputed.
How a binding maps its own task-terminal states onto these five codes is
that binding's concern (`spec/binding-a2a.md` §6 defines the mapping
from A2A task states); this document only fixes the vocabulary and its
meaning to the trust projection (§8).

Two of the five codes are worth flagging because they are not produced
by the requester's own terminal state, and conflating them with the
codes that are is a real interoperability hazard:

- **Code 2 (timeout)** is produced by a *provider-side* deadline miss —
  the provider fails to complete before the task's agreed deadline
  elapses. It is reported to the requester as a binding-specific
  failure signal and recorded as outcome 2, distinct from a
  requester-initiated cancellation (outcome 3): the same wall-clock
  outcome ("nothing was delivered in time") gets a different code
  depending on which party's action ended the task, because trust
  weighting (§8.2) treats a provider's missed deadline as negative
  provider evidence and a requester's own cancellation as, by default,
  no evidence at all.
- **Code 4 (disputed)** is produced by whatever dispute or arbitration
  process a deployment layers on top of MAP — this layer defines no
  dispute protocol of its own (§1.3) — and simply records that the
  event is contested on the trust plane. A receipt with outcome 4
  weighs against both parties' projections pending resolution (§8.2),
  which is deliberate: disputing is not free for either side to invoke
  carelessly.

This five-code vocabulary is separate from whatever result/status
vocabulary the underlying A2A task used (A2A task states, or any
`Complete`-shaped result a future binding defines) — the two describe
different things (the *receipt's* verdict about the interaction vs. the
*task's* own terminal state) and MUST NOT be conflated even where their
numeric values happen to coincide.

### 6.3 Lifecycle

Either party MAY propose the receipt, and the binding fixes the order.
The A2A binding has the provider propose it at the terminal task state,
because that is where A2A's event stream ends (`spec/binding-a2a.md` §3,
A2A-7). The rules are symmetric:

```
First signer (proposer)                  Second signer (countersigner)
  | build ReceiptCore from own           |
  |   observation; sign own role         |
  | -- receipt {core, sig_first} ------→ |  verify transcript-root (§5) against
  |                                      |   what it delivered/received; check
  |                                      |   every field against its OWN
  |                                      |   observation (§6.5); sign own role
  | ←-- receipt {core, sig_req, sig_prov}|  store
  | store dual-signed receipt            |
```

Either party MAY hold a receipt; both SHOULD.

A countersigner can decline because the proposed usage or timestamps fall
outside its tolerance (§6.5). It then returns "countersignature refused",
and each side keeps only what it signed itself:
- A requester-only receipt is admitted by R-4 as negative evidence for
  outcomes 1, 2 and 4.
- A provider-only receipt is excluded by R-4 from every projection.

Requesters SHOULD issue receipts for failures and
timeouts too (outcomes 1–3): a provider that only ever countersigns
successes reveals that pattern in the gap between its dual-signed
receipt density and its work volume — a signal a local trust policy MAY
use, though this document does not standardize how.

### 6.4 Validation rules

A receipt is admissible evidence only if it passes all of the following.
These rules MUST be applied identically whether a receipt is being
ingested incrementally or the receipt store is being rebuilt from
scratch (§8.1) — they are part of the projection's *apply* step, not a
check a caller may perform inconsistently.

- **R-1.** Both present signatures verify over the *received* core bytes
  with the role-tagged signing input (§4.2); the signer of `sig_req`
  MUST equal core field 2 (requester) and the signer of `sig_prov` MUST
  equal core field 3 (provider) — exactly, no delegation chain (§3.3).
  A receipt failing this check is not evidence: it is discarded, not
  downgraded.
- **R-2.** A receipt with `requester == provider` (core fields 2 and 3
  identical) MUST be rejected. Such a receipt cannot be evidence of an
  interaction between two parties; admitting it lets a single identity
  manufacture arbitrarily many "successes" against itself.
- **R-3.** A receipt whose `ended` (core field 6) is later than the
  evaluator's clock plus a skew bound (default 120 s) MUST NOT
  contribute to any projection computed at that evaluation time; and
  `started <= ended` (core fields 5, 6) is required, or the receipt is
  rejected. This prevents a party from back-dating or future-dating a
  receipt into or out of a decay window (§8.2).
- **R-4.** Only **dual-signed** receipts (both `sig_req` and `sig_prov`
  present and verifying) contribute *positive* evidence (outcome 0) to
  any projection. A **requester-only** (unilateral) receipt — `sig_req`
  present, `sig_prov` absent — MAY contribute *negative* evidence only,
  and only for outcomes 1 (failed), 2 (timeout), or 4 (disputed),
  weighted by the evaluator's trust in the issuer (§8.3). A
  **provider-only** receipt (`sig_prov` present, `sig_req` absent) MUST
  NOT contribute to any projection at all — a provider cannot vouch for
  its own performance unilaterally.

R-2 through R-4 close part of reviewer finding A1 (receipt farming).
R-2 excludes a self-dealing receipt (`requester == provider`) regardless
of signature validity. R-3 stops future-dated receipts escaping decay.
R-4 stops a provider from minting positive evidence alone.

R-2 through R-4 do **not** stop a Sybil ring. There, the attacker holds
the provider key *and* many throwaway requester keys, so every receipt is
genuinely dual-signed by two distinct identities. That attack is closed by
issuer weighting (§8.3, T-3): an issuer with no evidence relationship to
the observer weighs 0.

### 6.5 Countersignature tolerance

The second signer countersigns a receipt (or checkpoint) only if the
proposed core matches its own observation:
- task-id, capability and identities (core fields 1–4) MUST be exactly
  equal;
- each `Usage` component (core field 8, sub-keys 1–4) MUST fall within a
  tolerance window of the **countersigner's own observed** value.

Whichever party signs first, the anti-farming property is the same: the
second signature anchors to independent observation.

Unlike v1, which fixed this window normatively at ±5% (minimum ±1) for
usage counters and ±2000 ms for timestamps, this version treats the
window as a **local policy parameter**:

- `tolerance.usage_pct` — default 5
- `tolerance.time_ms` — default 2000

The anti-farming rationale is unchanged and remains normative: the
window MUST be anchored to the countersigner's own observed value,
**never** to `max(claimed, observed)`. Anchoring to the max lets an
inflated claim widen its own acceptance band; anchoring to the
countersigner's observation does not. A countersigner that declines because a claim falls
outside its configured window returns "countersignature refused"; the
exact wire carriage of that refusal (an error code, an A2A task-state
transition, or otherwise) is binding-specific and is not defined in this
document.

### 6.6 Gaming analysis

<!-- adapted from v1 trust-01 §9, filtered to structures this layer still defines -->

| Attack | Counter |
|---|---|
| Self-praise via Sybil requesters (the attacker controls the provider key and many requester keys, so receipts are genuinely dual-signed) | T-3: issuers with no evidence relationship to the observer weigh 0, so the ring contributes nothing. R-4 alone does not help here: the provider is the attacker. |
| Self-praise via a lone provider | R-4: only dual-signed receipts carry positive evidence |
| Self-dealing (one identity as both parties) | R-2: `requester == provider` is rejected outright, independent of signature validity |
| Collusion ring inflating each other | Weight flows only along the evaluator's own trust edges (T-3); a ring is internally dense and externally weightless to anyone outside it |
| Badmouthing (fake negative unilaterals) | Unilateral receipts are weighted by the evaluator's trust in the issuer (§8.3); an issuer with no evidence relationship to the evaluator contributes nothing |
| Whitewashing (reset identity after bad receipts) | Evidence is non-transferable between AgentIDs (§3.3); a fresh identity starts at zero evidence and zero weight under any other observer's projection |
| Selective countersigning (hide failures) | A provider that only countersigns successes cannot suppress the requester's own unilateral negative receipts (R-4 still admits them as negative evidence); the gap between a provider's dual-signed density and its total task volume is itself a signal a local policy MAY use |
| Countersignature-band inflation | §6.5: the tolerance window anchors to the provider's *observed* value, never `max(claimed, observed)`, so an inflated claim cannot widen its own acceptance band |
| Free-riding requester (withholds countersignatures or vouchers) | The provider stops at the checkpoint it cannot get acknowledged (§7, §9.4). It loses at most min(L, c) leaves per task, plus any leaves generated while an asynchronous countersignature is in flight. This is **not** bounded across fresh requester identities: this layer defines no trust in requesters. Local policy can limit it with a short first interval, a rail that takes payment up front, or the provider's own record of requesters |
| Receipt refusal and vindictive customers | R-4 lets a requester withhold positive evidence by refusing to countersign. It can also file unilateral negatives that the provider cannot disprove, and that are not bound to a real task (the provider chose the A2A task identifier and never signed). T-3 weights them by the evaluator's trust in the issuer. That bounds a stranger's effect, not the effect of an issuer the evaluator already agrees with |

Two v1 trust-01 §9 entries are dropped as no longer applicable: "receipt
spam" (rate limiting and storage quotas are now transport-binding
concerns, §12) and "payment-linkage preference" (there is no MAP-native
invoice to link a receipt to; §9's `voucher-digest` is a pointer, not a
preference signal this layer scores).

## 7. Checkpoints and cross-provider resumption

<!-- restructures v1 tasks-01 §8.2-8.3; retires CHECKPOINT/CHECKPOINT_ACK -->

v1's `CHECKPOINT`/`CHECKPOINT_ACK` messages and the `"MAPCheckpointAck"`
signing input are **retired** in this version, replaced entirely by the
structure below. The reason is a defect the 2026-09-18 review
demonstrated concretely (finding R4-02): v1's countersignature covered
only `(TaskID, chunk-seq, root)` — the requester's acknowledgment never
signed cumulative *usage*, so a reviewer proof-of-concept edited the
usage figure carried alongside a checkpoint without invalidating its
signature. The structure here mirrors the shape of an `ExecutionReceipt`
(§6) specifically so both parties sign the *exact same bytes*, usage
included.

### 7.1 Wire format

```cddl
CheckpointArtifact = {
  1 => bstr .cbor CheckpointCore,  ; exact signed bytes — invariant I1 applies
  ? 2 => bstr .size 64,            ; sig_req  = Ed25519(signing-input("MAPCheckpoint", 1, core-bytes))
  ? 3 => bstr .size 64             ; sig_prov = Ed25519(signing-input("MAPCheckpoint", 2, core-bytes))
}

CheckpointCore = {
  1  => TaskID,
  2  => AgentID,               ; requester
  3  => AgentID,                ; provider
  4  => uint,                   ; through-seq (inclusive) — the highest
                                 ;  transcript seq (§5) this checkpoint covers
  5  => Multihash,              ; transcript-root computed over seq 0..=through-seq
  6  => Usage,                  ; cumulative since task start, through this
                                 ;  checkpoint (§6.1 Usage shape)
  7  => uint .le 3,             ; zone the task is running under (§10)
  8  => Timestamp,
  ? 9  => Multihash,            ; voucher-digest: the cumulative rail voucher
                                 ;  covering usage through this checkpoint (§9)
  ? 10 => Multihash             ; resumed-from: digest of the countersigned
                                 ;  CheckpointArtifact (from the previous
                                 ;  provider) this task was resumed from;
                                 ;  same value as ReceiptCore key 13
}
```

### 7.2 What makes a checkpoint a valid resumption point

A `CheckpointArtifact` is a **valid resumption point** if and only if it
is dual-signed (both `sig_req` and `sig_prov` present and verifying per
§4.2/§4.3, signer identities matching core fields 2/3 exactly, as in
receipt rule R-1). The signing order is fixed and asymmetric:

- **C-1.** The **provider signs first**, over the core bytes it
  constructs from its own execution state.
- **C-2.** The **requester countersigns only after verifying the
  transcript root** (core field 5) against the transcript content it
  actually received through `through-seq` (core field 4) — recomputing
  the root per §5 and rejecting on mismatch. A requester MUST NOT
  countersign a checkpoint whose root it has not independently
  recomputed.

This ordering is the direct fix for v1 critique C11 ("never resume from
state attested only by the party that just failed or may be
malicious"): resumption is always keyed to a checkpoint the requester
itself validated, never to a provider's bare claim.

### 7.3 Monotonicity

Within one task, across the sequence of checkpoints a provider produces:

- **C-3.** `through-seq` (core field 4) MUST be strictly increasing
  checkpoint-to-checkpoint. A checkpoint that does not advance
  `through-seq` is not a new resumption point.
- **C-4.** Each `Usage` component (core field 6, sub-keys 1–4) MUST be
  non-decreasing checkpoint-to-checkpoint (usage is cumulative since
  task start, exactly as in §9). A checkpoint whose usage regresses
  relative to its immediate predecessor is invalid and MUST be
  rejected by the requester before countersigning.

### 7.4 Resumption procedure

On provider loss (or any condition that ends the current provider's
participation), the requester selects a successor provider B and
initiates resumption, provider-agnostically:

1. The requester sends B the last **valid resumption point** (§7.2) —
   the countersigned `CheckpointArtifact` — together with the transcript
   prefix (seq 0..=through-seq) it covers, inline or content-addressed.
2. B MUST independently recompute the transcript root (§5) over the
   received prefix and reject the resumption if it does not match core
   field 5 of the checkpoint. B MUST NOT begin executing on an
   unverified prefix.
3. B's first delivered content item carries `seq = through-seq + 1`.
   Nothing in this layer requires deterministic continuation across
   heterogeneous providers; the seam at the resumption point is
   permitted and is a binding concern (`spec/binding-a2a.md`) to make
   visible in the delivered content.
4. The successor's own `ExecutionReceipt` for the resumed portion of the
   task MUST carry core key 13 (`resumed-from`) equal to the Multihash
   digest of the `CheckpointArtifact` bytes it resumed from.

*Scope (informative).* The transcript holds only the content a provider
delivered. Resuming from it is complete only for tasks whose state is
their delivered output, such as streamed generation. It does not carry a
provider's hidden state (tool results, scratchpads, working files). A
successor cannot observe external side effects its predecessor caused
(payments, writes, messages), so it may repeat them. This layer does not
make such effects idempotent. Tasks that have them need application-level
checkpoints or idempotency keys, outside this layer.

```
Provider A                    Requester                    Provider B
    |  ... executing, checkpointing (C-1/C-2) ...  |             |
    |  ← countersigned checkpoint k -------------→ |             |
    |         (A's last valid resumption point)    |             |
    X  (A lost)                                    |             |
                                                     |             |
                       requester selects B, sends checkpoint k    |
                       + transcript prefix [0..=through-seq] ---→ |
                                                     |   recompute root (§5),
                                                     |   reject on mismatch
                                                     | ←-- resumed execution
                                                     |     from through-seq+1
                                                     |
                       B's receipt: resumed-from = digest(checkpoint k)
```

A resumed task therefore accrues **two** `ExecutionReceipt`s (one per
provider that did work on it, each dual-signed independently under §6),
chained only by the `resumed-from` field — there is no merged, single
receipt for the whole task, and no requirement that A and B ever
interact directly.

### 7.5 Billing bound on a failed provider

**C-5.** The amount owed to a provider that fails mid-task is computed
**only** from the `Usage` in that provider's **last dual-signed
checkpoint** — never from a higher figure the provider merely claimed
after the fact and never from a later, un-countersigned observation.
The requester therefore never pays for usage it did not acknowledge.
Overbilling is zero under this rule, not merely bounded. What the
checkpoint interval bounds is the provider's risk. The usage it
delivered after its last dual-signed checkpoint goes unpaid: the gap
between the last point the requester acknowledged and the point of
failure. With a checkpoint every c transcript leaves (C-6), that is
fewer than c leaves per failure.

**C-6 (default interval).** Absent a negotiated value, a provider SHOULD
propose a checkpoint every c transcript leaves, and at the terminal
state. The default depends on how the countersignature travels:
- **c = 8** when it returns asynchronously while the task keeps running
  (binding-a2a A2A-20);
- **c = 32** when the provider pauses for each countersignature, a
  synchronous round trip (binding-a2a A2A-21).

The interval is local policy. The defaults come from evaluation E2/E4
(`eval/HYPOTHESES.md`):

- *Waste.* At λL = 1 expected failure per task and L = 10³ Parts,
  countersigned resume at c = 8 wastes 3.2 Parts per task. That is
  0.42% of the waste of restarting from scratch (ratio 0.0042 [0.0036,
  0.0050], hypothesis H2). With no intermediate checkpoints (c = ∞) it
  wastes 591.6. Waste grows roughly linearly in c:
  15.7 Parts at c = 32, 64.8 at c = 128.
- *CPU cost.* A checkpoint costs two signatures and about 310 bytes
  (TV-021 is 312). At c = 8, MAP signing and verification add 0.079% to
  a task that generates one token per 20 ms (criterion benchmark of the
  reference implementation). At c = 1 they add 0.63%. These figures
  exclude the network.
- *Round-trip cost (estimated, not measured).* Assume a 50–100 ms round
  trip.
  - A synchronous pause adds one round trip per checkpoint: 31–63% of
    task time at c = 8, 7.8–16% at c = 32 and 2.0–3.9% at c = 128. Priced
    as work, that is 2.5–5 Parts per checkpoint.
  - An asynchronous countersignature adds no pause. Instead the
    provider's unacknowledged exposure grows by the Parts generated
    while it is in flight: 2.5–5 beyond c.
- *Exposure.* Under voucher-per-checkpoint metering, a provider's
  unpaid exposure is at most price(c Parts). This is H5, asserted in
  every run. So c also sets the credit a provider extends.

Price a signature at κ ∈ {0.01, 0.1} Parts of work, which is signing
only. The c that minimizes waste + κ·signatures then lies between 8 and
32 in most E2 settings. The exceptions are c = 128 when failures are
rare on long tasks, and c = 1 when κ = 0.01 and failures are frequent on
short tasks. c = 8 is the conservative end of that range, and it is the
asynchronous default.

A synchronous pause raises κ to 1.25–2.5 per signature (2.5–5 Parts per checkpoint, two signatures each). At κ = 1 the
minimizing c is 32 at the H2 operating point, and 32–128 in 8 of 9 E2
settings. That gives the synchronous default. Deployments with expensive
signatures or round trips, or with long and reliable tasks, MAY raise
either default.

### 7.6 Engine-state / KV-cache shortcut

**Deferred:** v1 tasks-01 §8.4 defined an opaque engine-state blob
(`ext-kvx`) that could shortcut re-warm between identical
engine+model+quantization peers within Zone ≤1. That shortcut is out of
scope for this version; resumption here is always by transcript replay
(§7.4). A future revision MAY reintroduce an opaque-state fast path, but
it is not part of the accountability layer's contribution and adds
verification surface this version does not need.

## 8. Local trust projection

<!-- adapted from v1 trust-01 §1, §7 -->

Trust in MAP is a **local, derived projection over signed facts** — not
a protocol-level score, not global, and not authoritative over the
evidence it is built from.

### 8.1 Rebuild rule (normative)

**T-1.** Recomputing a projection from the receipt store MUST reproduce
any incrementally maintained projection. No write path may update a
score without its evidence: a cached projection is never authoritative
over the receipt store, and an implementation MUST be able to discard
every cached projection and rebuild it, byte-for-byte behavior
identical, from the stored receipts alone. This is what makes the
projection *derived data* rather than protocol state: receipt ingestion
is append-only and idempotent (dedup by content address), and the
projection is a pure function of the store's contents at evaluation
time.

**T-2.** The receipt validation rules of §6.4 (R-1 through R-4) are part
of the projection's *apply* step. They MUST be applied identically by
incremental ingestion and by full rebuild — not layered on separately by
a caller, and not skippable by either path. A rebuild that omits R-4's
dual-signature gate, for instance, is non-conformant even if incremental
ingestion enforces it correctly.

**T-1 and T-2 in practice:** a receipt store is an append-only log keyed
by content address (§4.3's `Multihash` over the encoded `Receipt`, §6.1);
"rebuild" means replaying that log from empty state through the same
apply function an incremental ingester uses. Decay MUST be computed from
the receipt's `ended` field (core field 6) and the evaluation time, never
from ingestion time. That makes the projection a pure function of (store
contents, evaluation time, policy), independent of arrival order. Two
implementations fed the same receipt store MUST agree on which receipts
are admissible (R-1 through R-4), even if they implement §8.2's formula
differently or not at all.

### 8.2 Recommended computation (Beta reputation)

The following is the RECOMMENDED default, not itself normative in its
exact form — implementations MAY use another scoring function, subject
to T-1 and T-2:

```
For each admissible receipt r about subject s, capability c:
  w(r) = issuer_weight(r) × decay(now − r.ended)   ; decay λ ≈ 0.98/day
  outcome 0 (dual-signed only, R-4)   → α += w(r)
  outcomes 1, 2 (dual- or unilateral) → β += w(r)
  outcome 3 (cancelled)               → ignore unless a local pattern policy says otherwise
  outcome 4 (disputed)                → β += 2·w(r), pending resolution

score(s, c)      = α / (α + β)          ; prior α₀ = β₀ = 1
confidence(s, c) = α + β
```

High-value routing decisions SHOULD gate on `confidence(s, c) ≥ floor`,
never on `score` alone — a score of 1.0 from a single receipt and a
score of 0.97 from five hundred are not the same claim.

**Parameter defaults.** Evaluation E1 used the values below, and they are
the RECOMMENDED starting point. They were fixed before the runs and never
tuned, so the evaluation shows they are sufficient, not that they are
optimal.

| Parameter | Default | Meaning |
|---|---|---|
| decay λ | 0.98 per day | Applied to `ended` (T-1), never to ingestion time |
| prior α₀, β₀ | 1, 1 | |
| future-skew tolerance | 120 s | R-3: a receipt whose `ended` is more than this beyond the evaluation time is inadmissible |
| per-issuer cap | 50 | T-3: the most weighted, decayed evidence (α + β) that one issuer contributes to one (subject, capability) projection. Any excess is scaled down proportionally, after issuer weighting |
| issuer weighting | W1 | §8.4 |

Under 0.98/day decay, an issuer's steady-state mass in a cell is its
receipt rate / (1 − 0.98). The cap of 50 therefore binds only when one
issuer sustains more than about one receipt per day against one subject.
The cap works as a rolling window, and that is how the reference
implementation realizes T-3's rolling-window SHOULD. In `map-trust`,
`ProjParams::recommended()` returns this table.

### 8.3 Issuer weighting

**T-3.** `issuer_weight(r)` in §8.2 (or any equivalent term in an
alternative scoring function) is local policy, but it MUST have these
properties:

- The observer's own receipts — receipts where the observer itself is
  the requester and therefore the issuer of `sig_req` — weigh **1**.
- An issuer with **no evidence relationship to the observer** weighs
  **0 by default** — not the Beta prior's neutral 0.5, and not any
  positive default.
- Per-issuer contribution to a single (provider, capability) projection
  SHOULD be capped within a rolling time window, so that one prolific
  issuer (even a legitimately trusted one) cannot dominate a projection
  by sheer volume of small-value interactions.

The zero-default for unknown issuers is the direct fix for reviewer
finding A1: under v1's Beta-prior default weighting, a thousand freshly
created identities that only ever dual-sign each other's receipts (a
Sybil ring with no external evidence relationship to the evaluator)
drove a projection to score 0.998 at confidence 501 in about 150 ms of
wall-clock receipt generation. Weighting an unknown issuer at 0 means
that ring contributes exactly nothing to any observer outside it — the
ring is internally dense and externally weightless, by construction of
T-3 rather than by hoping a decay curve or a rate limit catches it in
time.

### 8.4 Recommended default weighting function (W1)

T-3 fixes the two endpoints: self = 1, unrelated = 0. The RECOMMENDED
function for every other issuer is **W1, direct agreement**. It is local
policy, like the rest of §8.2, and any function that satisfies T-3
conforms. Observer `o` computes it from the same admissible receipts
that feed its projection:

```
rate_i(p) = α_ip / (α_ip + β_ip) over issuer i's receipts about subject p,
            defined only where i holds ≥ 3 such receipts
a(o, i)   = max(0, 1 − 2·MAD(o, i))    ; agreement
            MAD = mean |rate_o(p) − rate_i(p)| over the subjects p that
            o and i both have rates for; a(o, i) = 0 if there are fewer
            than 3 such subjects
w_o(i)    = a(o, i)
```

In words, an issuer earns weight only by agreeing with the observer's
own first-hand experience of the same subjects. Under T-3 this counts
as an evidence relationship, because every term is computed from
receipts the observer itself issued or verified.
- A Sybil ring that only agrees with itself shares no rated subjects
  with the observer, so it gets weight 0.
- An observer with fewer than 3 rated subjects gives every other issuer
  weight 0, so W1 reduces to W0.
- Honest agreement can be bought: Sybils can run truthful tasks to earn
  weight (the camouflage attack below). W1 bounds what that buys to
  direct agreement with the observer. No weight is passed on to others.
- *Caveat (informative, evaluation A10).* That bound applies per issuer.
  It does not limit how many issuers there are. At α = 0.2 under
  camouflage, W1 held at 1.6 camouflaging Sybils per honest requester.
  At 6.4 it fell behind W0 (attacker share 0.077 against 0.072), and at
  25.6 it reached 0.165. It never became worse than random. Each such
  Sybil buys real tasks, 6.4 times the honest workload at the 6.4 ratio.
  A deployment that expects camouflaging Sybils to outnumber its honest
  requesters several-fold should consider W0, or a cap on the total
  weight of second-hand evidence.

Recompute the weights off the routing critical path, at most once per
decay period (the evaluation used once per simulated day).
`map-rs/sim/src/scenarios/weights.rs` is the reference implementation.

**Alternatives.**
- **W0 (self only)** is the minimal conformant choice. It has no
  agreement computation and admits no second-hand evidence at all.
- **W2 (personalized agreement propagation)** runs 10 power-iteration
  steps, restart 0.15, over the row-normalized agreement graph among
  all issuers, with `w_o(i) = min(1, t[i] / t[o])`. It is slightly
  better than W1 in most settings, but it is NOT RECOMMENDED where
  Sybil identities can outnumber honest requesters (see below).

**Evidence.** All figures are steady-state attacker task share, over 20
seeds with 95% CIs, across the seven E1 attacks (farming, farming with
camouflage, collusion ring, whitewashing, on-off, slander, combined).

1. *Pre-registered decision D5, α = 0.2 only* (Amendments A4/A6 of
   `eval/HYPOTHESES.md`). Mean over attacks: W0 0.0796, W1 0.0312,
   W2 0.0290. W2 won D5 by 0.2 points.
2. *Pre-registered minimax re-test, α ∈ {0.05, 0.1, 0.2, 0.3, 0.5} ×
   seven attacks* (Amendment A9, `eval/results/5bc7702/MINIMAX.md`).
   Each arm's worst cell:

   | Weighting | Worst cell | Attacker share [95% CI] | Honest success |
   |---|---|---|---|
   | W0 | α = 0.5, on-off | 0.348 [0.346, 0.350] | 0.960 |
   | **W1** | **α = 0.5, camouflage** | **0.200 [0.196, 0.204]** | **0.978** |
   | W2 | α = 0.5, camouflage | 0.550 [0.543, 0.557] | 0.867 |

   The minimax rule selects W1. Elsewhere W2 does slightly better. Under
   on-off, where pooled evidence catches the off phase sooner, it leads
   by 0.003 at α = 0.05 up to 0.040 at α = 0.5. In the other attacks at
   α ≤ 0.3 it leads by at most 0.003. Its failure is the camouflage case:
   - At α = 0.5 each adversarial provider runs 20 Sybil requesters, so
     2,000 Sybils face 500 honest requesters.
   - Their truthful reports about honest providers earn them agreement
     edges, and propagation concentrates mass in the Sybil cluster.
   - W2 then does worse than random selection (0.500).

   Under W1 the same Sybils gain weight only through direct agreement
   with each observer, so the collapse does not happen.

Two regimes limit every weighting:
- *Sparse evidence.* With 0.1 tasks per requester per round, W0 beat
  random selection by only 0.9–2.3 points (L2).
- *Cold start.* Local trust needs history. In rounds 1–5, Ours-L without
  a canary had a higher attacker share than the canary baseline, at
  every α and attack (L1).

A per-task canary in front of the projection, as in the evaluation's
Ours-L+C arm, kept the attacker share at or below 0.15 in every cell.
The two are complementary.

### 8.5 Graduated bootstrap (informative)

<!-- adapted from v1 trust-01 §6 -->

A new AgentID starts at `confidence(s, c) = α₀ + β₀ = 2` for every
capability — effectively zero. This is not a defect to route around; it
is the correct state for an identity with no verifiable history (§3.3).
A local policy consuming this projection is RECOMMENDED to price that
state as risk rather than exclusion:

- **Zero evidence** (near-prior confidence): eligible for low-stakes or
  canary-shaped tasks, small contexts, with concurrent exposure to any
  one such provider capped locally.
- **Evidence exists, confidence below the routing floor**: eligible for
  normal tasks with tightened deadline enforcement.
- **Confidence at or above the routing floor**: full eligibility per
  local policy (§8.2).

These tiers are local-policy vocabulary, not protocol state — nothing in
this document defines a wire representation for "tier." They are stated
here only so independent implementations converge on interoperable
*behavior*: a mesh where no local policy ever routes to an unproven
identity never lets any identity accumulate the evidence needed to
leave that state, which defeats the entire local-trust design of §8.

## 9. Metering bound to evidence

<!-- re-expresses v1 pay-01 §4 UsageWatermark semantics -->

Money moves on an external rail: an MPP session or an x402 V2
batch-settlement scheme, both of which already implement cumulative,
payer-signed, pause-until-paid metering. MAP does not define invoices,
payment proofs, or settlement receipts. What MAP defines is the binding
between the `Usage` this layer already tracks (checkpoints, §7; receipts,
§6) and the voucher that actually moved money — so that a party
reconstructing what was paid for can point at exactly which
checkpoint/receipt usage a given voucher covers, and vice versa.

### 9.1 Usage semantics

**M-1.** `Usage` (§6.1) is **cumulative since task start**, never a
delta. A re-issued figure for the same point in the task is therefore
byte-identical to any earlier figure for that same point, which is what
makes voucher and checkpoint idempotency fall out for free rather than
needing a separate mechanism.

**M-2.** `Usage` components MUST be monotone non-decreasing across
successive checkpoints or receipts for the same task (§7.3, rule C-4).

**M-3.** A billed amount MUST be an exact product of an integer counter
(a `Usage` component) and a decimal-fraction rate, under the
**same-exponent rule**: within one pricing agreement, every rate and
every derived amount MUST share one decimal exponent and one asset, so
verifiers compare mantissas directly. No rounding rule exists anywhere
in this document, by construction — there is nothing to round.

**M-4.** Amounts MUST be non-negative:

```cddl
Amount = #6.4([exponent: int, mantissa: uint])
                                    ; CBOR decimal fraction (RFC 8949 §3.4.4);
                                    ; mantissa is a uint, never a signed int
Asset  = tstr                      ; "iso4217:USD", "erc20:base:0x...",
                                    ; "credit:<mesh>", per the rail
```

M-4 is the direct fix for reviewer finding R4-01: v1's `Amount` carried
a signed mantissa, and a negative amount at any point in an invoice
chain moved funds backwards relative to what was owed. Fixing the CDDL
type — `mantissa: uint` rather than `int` — makes the defect
unrepresentable rather than merely disallowed by prose.

### 9.2 Pricing model

Unit codes for a rate table, unchanged from v1 pay-01 §3:

| unit | meaning |
|---|---|
| 0 | flat total (the whole task) |
| 1 | per token in |
| 2 | per token out |
| 3 | per millisecond of wall time |
| 4 | per invocation |

### 9.3 Binding to the voucher

**M-5.** The link between evidence and money is the `voucher-digest`
field: `CheckpointCore` key 9 and `ReceiptCore` key 10 (as a `Multihash`
carried inside the general-purpose `bstr` payment-reference field) each
MAY carry the digest of the rail voucher whose cumulative amount covers
the usage recorded at that point. This is a **reverse binding** — MAP
does not mint the voucher or dictate its format; it only lets a party
holding both the checkpoint/receipt and the voucher prove they refer to
the same delivery. `spec/mapping-payment-rails.md` defines exactly how
an MPP session receipt or an x402 V2 settlement artifact is digested for
this purpose.

**M-6.** A requester SHOULD NOT sign a voucher whose cumulative amount
exceeds:

```
price(usage in the latest dual-signed CheckpointArtifact)
  + price(one checkpoint interval of credit the requester chooses to extend)
```

This bounds a requester's payment exposure to what it has actually
acknowledged plus a deliberate, bounded extension of credit — never to
an unbounded, unacknowledged figure a provider merely claims.

### 9.4 Pause and resume

MAP defines no pause/resume message of its own. Pause and resume are
whatever mechanism the rail already provides. For example, an MPP session
server emits `payment-need-voucher` and waits for a new voucher. Per-rail
behaviour is in `spec/mapping-payment-rails.md`. A provider that
is owed more than it has been paid simply withholds further delivery
until the rail signals a covering voucher; this layer's only
contribution is that the checkpoint the provider stops at (§7) is itself
the dual-signed record of what it is owed for, per the billing bound of
§7.5.

### 9.5 Worked example (informative)

A provider agrees a per-token rate table: tokens-in at
`Amount = [exponent -6, mantissa 2]` (i.e. 0.000002 units/token) and
tokens-out at `[-6, 8]` (0.000008 units/token), both in `Asset =
"erc20:base:0x...usdc"`, satisfying M-3's same-exponent rule (both
carry exponent -6). At checkpoint 1, cumulative usage is 1,000
tokens-in / 400 tokens-out:

```
price(usage@ckpt1) = 1000 × 2 + 400 × 8 = 2000 + 3200 = 5200   ; at exponent -6
                    = [-6, 5200]                                ; 0.0052 asset units
```

Under M-6, the requester SHOULD NOT sign a voucher whose cumulative
amount exceeds `price(usage@ckpt1) + price(one checkpoint interval of
credit)` — e.g., if the requester is willing to extend one further
interval's worth of usage on trust (say, up to 1,000 more tokens-out at
the same rate, `[-6, 8000]`), the ceiling for the next voucher is
`[-6, 5200 + 8000] = [-6, 13200]`. If the provider then fails before a
second checkpoint, C-5 anchors the amount owed to checkpoint 1's
`[-6, 5200]` regardless of how much of that extended credit the
requester was prepared to cover — the requester never owes for usage it
did not itself countersign.

## 10. Data-sensitivity zones

<!-- adapted from v1 arch-01 §8.1 -->

### 10.1 Zone numbering

Unchanged from v1 arch-01 §8.1:

| Zone | Name | Meaning |
|---|---|---|
| 0 | Local | This device only. Never leaves the host. |
| 1 | Trusted | Explicitly allowlisted peers (own fleet, partners). Raw proprietary data MAY flow here. |
| 2 | Community | Authenticated but untrusted mesh peers. Only abstracted or non-sensitive payloads. |
| 3 | Open | Anonymous/ephemeral counterparties and public gateways. |

### 10.2 Minimal enforceable semantics

Lower numbers are more sensitive. A task's zone label `z` is the
*loosest* class of counterparty its payload may reach: a zone-1 task
may go only to counterparties the requester classifies as zone 0 or 1.

A counterparty's zone is **the requester's own classification**, never
the provider's claim:
- 1: on the requester's allowlist
- 2: an authenticated mesh peer
- 3: an ephemeral identity or public gateway

A provider cannot talk its way into a more trusted class.

Zone labels appear in these places:

- A descriptor's **accepted zones** (§11.2, `DescriptorCore` key 5): the
  zone labels the provider is willing to accept tasks under. A provider
  MAY refuse tasks labeled with other zones, for example because it will
  not take on zone-1 handling obligations.
- A checkpoint's **zone** (§7.1, `CheckpointCore` key 7): the label the
  task is running under.
- A receipt's **zone** (§6.1, `ReceiptCore` key 12): the label the task
  ran under.

**Z-1.** A requester MUST NOT delegate or resume a task labeled zone `z`
to a provider whose zone, as the requester classifies it, is numerically
greater than `z`.

**Z-2.** Because the zone label travels inside the dual-signed
checkpoint and receipt, a provider that accepted a task at a claimed
zone and then mishandled it cannot deny having made that claim — the
dual signature is the accountability mechanism. This is the entirety of
what zone enforcement means in this version: it makes a violation
**attributable**, not impossible.

**Z-3 (no laundering).** A provider that receives content under zone
label `z` and subcontracts or resumes any part of the task onward MUST
NOT present that content to a successor at a numerically higher zone
than `z`. Zone tightens or stays constant along a resumption chain; it
never widens. Concretely: if a task's checkpoint (§7) carries zone 1 and
the task is resumed onto a successor, the resumption request MUST carry
zone 1 or lower. Z-1 already excludes any successor that the resuming
requester classifies at a looser (numerically greater) zone than that
label.

**Z-4.** A zone violation (Z-1 or Z-3) is not a message this layer
defines a wire error for — enforcement is behavioral (a requester simply
does not delegate/resume to an ineligible provider) and, when it fails,
**receiptable**: a provider that dual-signed a checkpoint or receipt
carrying zone label `z` has acknowledged receiving the payload under
that label. If the payload later surfaces outside zone `z`, that
signature is exactly the kind of attributable fact §10.2 exists to
create. Detecting
and acting on such an inconsistency is local policy; this document only
guarantees the evidence exists to detect it.

### 10.3 What zones are not

Zones provide accountability, not confidentiality. A zone label is a
claim, dual-signed after the fact by both the party that made it and
the party that relied on it — it is not encryption, not a TEE boundary,
and not a technical guarantee that Zone-1 data cannot leave a Zone-1
provider's process. Remote attestation (TEE-backed execution) is
explicitly out of scope for this version; a deployment that needs an
actual confidentiality boundary layers attestation underneath this
layer's labeling, and this document says nothing about how.

## 11. Discovery records (two-plane)

<!-- adapted from v1 discovery-01 §1-§3, §5-§6, §8 -->

### 11.1 Two-plane rationale

Unchanged from v1: stable facts (identity, capabilities, accepted zones,
pricing policy) change on the order of days; volatile facts (queue
depth, current latency) change on the order of seconds. Systems that
push volatile facts through global storage or gossip route on stale
data. Therefore:

- **D-1.** Volatile fields MUST NOT appear in descriptors or DHT
  records.
- **D-2.** Routing decisions requiring volatile facts obtain them by
  direct on-demand query (§11.4) to a shortlist derived from the stable
  plane.
- **D-3.** Latency is measured by the querier, never advertised.

### 11.2 Stable plane: AgentDescriptor

An out-of-session, COSE_Sign1-signed structure (§4.2), content-addressed
by `descriptor-cid = Multihash(envelope-bytes)`:

```cddl
AgentDescriptor = #6.18([                ; COSE_Sign1, CBOR tag 18
  protected:   bstr .cbor { 1 => -8, 4 => AgentID },  ; alg EdDSA, kid = subject
  unprotected: {},
  payload:     bstr .cbor DescriptorCore,
  signature:   bstr .size 64
])

DescriptorCore = {
  1  => AgentID,                    ; subject (accountable identity)
  2  => uint,                       ; seq -- strictly increasing per subject
  3  => [+ tstr],                   ; service endpoints (A2A card URL(s) and/or
                                     ;  multiaddrs; binding-a2a.md,
                                     ;  binding-libp2p-discovery.md)
  4  => [+ CapEntry],                ; capabilities offered
  5  => [+ uint .le 3],              ; zone labels this provider accepts tasks
                                     ;  under (§10.2); NOT a trust claim
  6  => bstr,                       ; A2A Agent Card, RFC 8785 JCS canonical
                                     ;  JSON bytes
  7  => bstr,                       ; the Agent Card's JWS signature
  ? 8  => [+ ModelEntry],
  ? 9  => HardwareClass,
  ? 10 => PricingHint,
  ? 11 => SecurityProfile,
  ? 12 => uint,                     ; descriptor validity, ms (<= 24h, D-5)
  ? 13 => ExtMap                    ; { * int => any }
}

CapEntry = {
  1 => CapURI,                      ; tstr .regexp "map:cap:[a-z0-9][a-z0-9/._-]{0,126}"
  ? 2 => { * int => any }           ; capability-specific parameters
}

ModelEntry = {
  1 => tstr,                        ; model name (informative)
  ? 2 => Multihash,                 ; weights digest -- the verifiable id
  ? 3 => uint,                      ; max context tokens
  ? 4 => tstr,                      ; quantization label
  ? 5 => tstr                       ; runtime ("ollama","vllm","gateway")
}

HardwareClass = {                   ; COARSE BUCKETS ONLY
  ? 1 => uint,                      ; vram-class: 0=n/a,1<=8,2=16,3=24,
                                     ;  4=32,5=48,6=64,7=96,8=128,9>128 GB
  ? 2 => uint,                      ; ram-class (same scale)
  ? 3 => uint,                      ; accel: 0 none,1 consumer-gpu,
                                     ;  2 datacenter-gpu,3 npu,4 cpu-only
  ? 4 => uint                       ; bandwidth-class: 1<=100Mb,2<=1Gb,3>1Gb
}

PricingHint = {                     ; nonbinding advertisement only
  ? 1 => tstr,
  ? 2 => Amount,                    ; per-1M-tokens (§9.1)
  ? 3 => Amount,                    ; per-minute
  ? 4 => bool                       ; barter accepted
}

SecurityProfile = {
  ? 1 => uint,                      ; sandbox: 0 none,1 container,2 microVM,
                                     ;  3 wasm,4 tee
  ? 2 => bool,                      ; remote attestation available
  ? 3 => [* tstr]                   ; tool allowlist
}
```

**Fields kept from v1 discovery-01 §2** (unchanged in shape): subject,
seq, capabilities, model/hardware/pricing/security hints, descriptor
validity, extension map.

**Fields added**:
- Key 5: accepted zone labels (v1 carried a `[+ ZoneLabel]` list). This
  states what the provider is *willing* to take. The requester's own
  classification governs Z-1.
- Keys 6/7 (the A2A Agent Card and
its JWS, new — this is the mechanism by which a MAP descriptor and an
A2A Agent Card are the same claim, signed twice, once per ecosystem's
native format). **Fields dropped, and why**: v1's `[+ Version]` /
`[+ uint]` MAP-version and cryptographic-suite lists (v1 keys 3, 4) are
gone because this layer no longer negotiates a suite (§4) — Ed25519 and
SHA-256 are fixed; `[+ MemoryTopic]` (v1 key 11) is gone because
distributed context retrieval was part of the retired task protocol and
is now, if an agent offers it at all, an A2A skill advertised on the
Agent Card itself, not a MAP-native structure.

**D-4 (normative on the JWS key).** The Agent Card's JWS signature
(`DescriptorCore` key 7) MUST use the EdDSA algorithm with the subject
AgentID's Ed25519 key. This is what lets a verifier check the AgentID
signature (COSE, over the whole descriptor) and the Agent Card signature
(JWS, over the card bytes alone) with the same key material and treat
disagreement between them as a discardable, non-conformant descriptor.

Validation: the COSE_Sign1 signature verifies against `kid` (§4.3);
`seq` MUST exceed any cached `seq` for the subject or the descriptor is
stale and discarded; **D-5**, validity (key 12, ≤ 24 h) MUST be checked
on ingest, not deferred to use time — this is the direct fix for
reviewer finding A8 (an expired descriptor accepted at ingest and never
re-checked stays "valid" in a cache indefinitely).

### 11.3 Capability ontology and sharded keys

Capabilities are hierarchical URIs under `map:cap:` (v1 discovery-01
§4, unchanged): matching is by exact segment prefix. A provider record
is published under a sharded key so that a popular capability does not
concentrate every provider's record in one DHT neighborhood:

```
shard(agent) = first byte of SHA-256(AgentID bytes)
K(cap, s)    = Multihash("map:idx:1:" || cap || ":" || s)
               ; s rendered as unpadded base-10 ASCII decimal ("0".."255"),
               ; e.g. shard 141 contributes the three bytes 0x31 0x34 0x31
```

Providers publish only to their own shard `K(cap, shard(agent))`.
Queriers sample `r` uniformly random shards (RECOMMENDED r = 3, doubling
until enough candidates or r = 16) and merge results.

**D-6 (normative, querier side — fix for reviewer finding A3, shard
flooding).** A querier MUST discard any provider record found under
shard `s` whose `shard(agent) != s`. A record that does not belong in
the shard it was found under is either a bug or an attempt to
concentrate influence in one shard by mis-publishing; discarding it
costs the querier one hash computation and closes the attack regardless
of which. A querier MUST also verify the descriptor signature (§4.3)
before shortlisting a candidate from any record — an unverified record
is untrusted transport, exactly like the DHT that carried it.

### 11.4 Volatile plane

Pulled on demand from a shortlist already derived from the stable plane
(v1 discovery-01 §3, unchanged):

```cddl
StatusQuery  = { ? 1 => [+ CapURI], ? 2 => bstr .size 8 /* echo */ }
StatusReport = {
  1 => Timestamp,
  ? 2 => uint,        ; queue-length
  ? 3 => uint,        ; est-ttft-ms for the queried capability, self-estimated
  ? 4 => uint,        ; accepting: 0 no, 1 yes, 2 yes-if-reserved
  ? 5 => uint,        ; free vram-class right now
  ? 6 => bstr .size 8 ; echo (lets the querier compute its own RTT — D-3)
}
```

Reports are self-estimated hints, not commitments. **D-7.** Ranking
candidates by a `StatusReport` (or any other querier-side utility) is
querier policy; this document defines the fields, never a scoring
function.

### 11.5 Node-ID grinding (evaluated; not mitigated at this layer)

Reviewer finding A4 is node-ID grinding: an attacker cheaply generates
about 60,000 candidate AgentIDs per second until enough of them land in a
chosen shard. An earlier draft proposed an epoch-salted shard as a hook,
pending evaluation:

**D-8 (NOT RECOMMENDED as a mitigation).**

```
shard_e(agent) = first byte of SHA-256(epoch_salt || AgentID bytes)
```

Evaluation E3b measured it and found no benefit. The attacker's grind
rate was this machine's measured keygen-plus-shard rate, 60,901 attempts
per second on one core. With it, the attacker held a share of the
target shard of 1.000 [1.000, 1.000] in every configuration:

- n ∈ {10³, 10⁴, 10⁵} providers
- attack windows of 1 hour, 1 day and 1 week
- a fixed `shard()`, or salts rotated every hour or every day

The cause is structural, not a matter of tuning. One core adds about
60,901 / 256 ≈ 238 identities per second to any chosen shard, while a
shard of a 10⁵-provider mesh holds about 390 honest providers. The
attacker therefore holds a majority of a fresh shard within about
2 seconds of each rotation. No practical epoch is that short.

Grinding is only a targeted form of the underlying problem: an
AgentID costs nothing. An attacker who wants every shard can simply mint
AgentIDs uniformly, and no shard function changes that. This layer
therefore makes no discovery-level Sybil-resistance claim.

- A querier MUST NOT treat D-8 as a defence against A4.
- An implementation MAY still support `shard_e` for other purposes, such
  as periodically rebalancing shard load. A querier MUST NOT assume a
  mesh uses it unless the mesh says so.

Every captured-shard record is a verified descriptor (D-6, §4.3) of an
identity with no evidence history. Capture cannot forge another agent's
descriptor or its evidence. But the trust layer can rank only the
candidates discovery returns. When a flooded discovery layer returns
almost no honest providers, T-3 and §8.5 have nothing better to route to.

*Measured (informative, evaluation A11).* The setting is α = 0.2 under
farming with W1:
- Flood identities at 10 per honest provider make discovery return an
  adversary 91% of the time, and adversaries then get 0.888 of tasks.
- A requester that keeps its best-scored known-good providers on its
  shortlist (k = 3 slots), with a per-task canary in front, holds the
  attacker to 0.036 at that flood level and 0.085 at 100 per honest
  provider.
- A new requester has no such providers, and gets 0.182 and 0.660.

Requesters are therefore advised to keep known-good providers on their
shortlists alongside discovery results, and to run a canary. Discovery
capture remains an open problem for new requesters. Admission cost for
identities (stake, proof of work, attested hardware) is out of scope.

### 11.6 Anti-eclipse and record integrity

<!-- adapted from v1 discovery-01 §8 -->

**D-9.** Queriers SHOULD use disjoint-path Kademlia lookups (d = 3
disjoint paths, S/Kademlia-style) when fetching descriptors or
capability records. The libp2p discovery binding
(`spec/binding-libp2p-discovery.md`) states the lookup procedure.
Evaluation E3c simulated Kademlia with k = 20, α = 3, and a fraction f
of malicious routers that answer only with each other. The results
replicate the known S/Kademlia result:

- *Benefit.* Three disjoint paths matter at scale. At N = 10⁵ with no
  churn, lookup success rose from 0.893 to 0.985 at f = 0.2, and from
  0.755 to 0.930 at f = 0.3.
- *Small meshes.* At N ≤ 10⁴ a single path already succeeds ≥ 0.96 of
  the time at f ≤ 0.3, and d = 3 raises that to ≥ 0.997.
- *Cost.* d = 3 costs 1.3–1.7× the messages of one path, and fewer
  sequential hops, because the paths run in parallel.

The reference `map-net` does not implement D-9 yet
(`binding-libp2p-discovery.md` DHT-12). DHT routing tables SHOULD
enforce IP-prefix diversity per bucket. This is not evaluated.

Because descriptor integrity is end-to-end — the
`descriptor-cid` is a content address, and every record carries a
signature verified per §4.3 — the DHT itself is untrusted storage: an
eclipse attack against a victim's routing table can *hide* providers
(an availability degradation) but cannot *forge* one, since a forged
descriptor would need the subject's private key to pass §4.3
verification. Bootstrap lists MUST draw on multiple independent sources
(static configuration, operator-controlled DNS, cached peers from prior
sessions) so that no single compromised or eclipsed source can steer an
agent's entire view of the mesh.

### 11.7 Discovery privacy

<!-- adapted from v1 discovery-01 §9 -->

A DHT lookup for a capability reveals interest in that capability to
whichever nodes answer it; a `StatusQuery` (§11.4) inherently reveals a
requester-candidate pair to the candidate. This layer offers the same
options v1 did, in increasing order of cost, and does not mandate any of
them:

- Shard sampling (§11.3) already hides *which* specific providers a
  querier ultimately saw, since only a random subset of shards is
  queried.
- Routing a query through a relay peer strips the querier's own address
  from the candidate's view (a relay's own definition is a binding
  concern, not fixed here).
- Decoy queries against unrelated capabilities dilute a passive
  observer's confidence about genuine intent.
- A Zone-3 ephemeral identity (§10.1) for the discovery session avoids
  linking the query to any AgentID with accumulated evidence at all.

These reduce, but do not eliminate, discovery-plane metadata leakage. A
deployment with strong anonymity requirements SHOULD run its A2A and
discovery traffic over an anonymity network in addition to the options
above; this document does not specify one.

## 12. Threat model

<!-- adapted from v1 arch-01 §7 TH1-TH15 -->

### 12.1 Adversary model

<!-- adapted from v1 arch-01 §6-§7 -->

This layer assumes an adversary that can create unlimited AgentIDs
(§3.3 imposes no admission cost of its own), observe and inject traffic
on any public path the underlying bindings use, participate
simultaneously as requester, provider, and DHT node, collude with other
adversarial participants, and replay recorded traffic where the binding
does not itself prevent it. No participant is trusted by default (v1
arch-01 T1); every claim this layer makes evidentiary MUST be
independently verifiable (T2) or is explicitly marked as an unverifiable
assertion (the receipt `rating` field, §6.1 key 9, is the only such
field remaining). Trust decisions are local (T4): this document does
not assume, and its rules do not require, that two honest observers
converge on the same view of a third party. Clocks are loosely
synchronized (±120 s, §6.4 rule R-3); no global time authority exists.

Out of scope for this layer specifically (though not necessarily for
the bindings underneath it): a global passive adversary deanonymizing
participants via cross-network traffic-timing correlation, and
compromise of a participant's own endpoint (a stolen private key signs
exactly as validly as its rightful holder's — §3.3's deferral of
rotation/revocation means such a compromise is unrecoverable within
this layer, by design).

### 12.2 Threat catalog

The threats below are the subset of v1 arch-01's TH1–TH15 that remain
relevant to what this layer still defines. Threats tied to the retired
task protocol, tool-call gating, or the retired payment/escrow/dispute
machinery are noted as removed, not carried forward with a stale
mitigation.

| # | Threat | Disposition in this layer |
|---|---|---|
| TH1 | Sybil flooding of discovery/trust | Per-observer evidence-based trust with zero-default issuer weighting (§8.3, T-3) is the primary defense; admission-cost mechanisms are out of scope for this version (not reintroduced) |
| TH2 | Eclipse of a victim's routing view | Delegated to the libp2p Kademlia discovery binding (`spec/binding-libp2p-discovery.md`); descriptor integrity end-to-end via signature (§4.3) bounds an eclipse to degradation, never forgery |
| TH3 | Replay of signed messages | Delegated to the discovery/A2A bindings; the inner-signed artifacts of this layer (§4.2) are verifiable independent of any session and carry no replay window of their own — a stale receipt or checkpoint is still a valid *fact*, just about an old point in time (R-3, §6.4) |
| TH4 | MITM / identity misbinding | Delegated to the A2A/libp2p bindings' own transport authentication; this layer's signatures (§4) bind artifacts to AgentIDs regardless of transport |
| TH6 | Poisoned models / model substitution | `ModelEntry.weights-digest` (§11.2) remains the verifiable model identity; redundant-execution auditing is a deployment-level practice this document does not standardize |
| TH8 | Malicious checkpoint injection | Requester-countersigned checkpoints (§7.2, C-1/C-2) — strengthened relative to v1 by also signing usage (the R4-02 fix, §7 preamble) |
| TH10 | Reputation manipulation (self-praise, ballot stuffing, badmouthing) | No self-reported scores; dual-signature requirement (R-4); issuer-trust weighting (T-3) |
| TH11 | Routing-layer attacks (query bias, record suppression) | Multi-shard sampling + signed, seq-numbered records (§11.3); delegated in detail to the discovery binding |
| TH12 | Metadata leakage (fingerprints, intent, social graph) | Bucketed hardware fields (§11.2 `HardwareClass`); selective receipt disclosure remains a holder decision (§14.2) |
| TH13 | Identity theft / key compromise | Out of scope for this version (§3.3 defers rotation/revocation); an AgentID compromise is unrecoverable within the layer |
| TH14 | Downgrade of version/suite | Not applicable: this version negotiates no suite (§4) |
| ~~TH5~~ | Spam / resource exhaustion via task traffic | Removed with the task protocol; flow control is now A2A's concern |
| ~~TH7~~ | Prompt injection via delegated content | Removed with the task protocol's tool-call gating; zone labeling (§10) is this layer's only remaining lever, and it is attribution, not prevention |
| ~~TH9~~ | Payment fraud | Removed with settlement/escrow (§1.3); the rail is the fraud boundary, and MAP's only contribution is the `voucher-digest` binding (§9.3) |
| ~~TH15~~ | Malicious tool execution on providers | Removed with the task protocol's tool-grant mechanism |

### 12.3 Reviewer findings — disposition

| Finding | Description | Disposition |
|---|---|---|
| A1 | Receipt farming: a Sybil ring of dual-signers drove a projection to score 0.998 at confidence 501 in ~150 ms; self-dealt and future-dated receipts outranked honest ones; rebuild ignored the dual-signature gate | Addressed in spec (implementation in P3): T-3 zero-default issuer weighting (Sybil ring); R-2 (self-dealing); R-3 (future dating); R-4 + T-2 (positive evidence needs dual signatures, applied identically by rebuild) |
| A3 | Shard flooding — records published or accepted outside their correct shard | Addressed: D-6, normative on the querier side (discard wrong-shard records). Evaluated (E3a, H3 holds): with the filter the attacker's candidate share equals its fair share, against 0.46–0.91 without it |
| A4 | Node-ID grinding — ~60k candidate AgentIDs/second to land a favorable shard | **Not mitigated at this layer**, stated as a limitation (§11.5). E3b showed that the D-8 epoch salt gives no protection, because identities cost nothing. Captured shards carry zero-evidence identities, which the trust layer (T-3, §8.5) prices as untrusted |
| A7 | Zones unenforceable — v1 defined the label but no enforcement messages | Re-scoped: zones are now attributable, dual-signed labels with a refusal rule (Z-1); this document is explicit that they are accountability, not confidentiality (§10.3) |
| A8 | Rate limits / replay-cache cost / TTL not checked | Rate limiting and replay caching are delegated to the transport bindings; descriptor TTL is normative and checked on ingest in this layer (D-5) |
| R4-01 | Negative payment amounts moved funds backwards | Addressed in spec: M-4, `Amount` mantissa is `uint`, unrepresentable as negative |
| R4-02 | Checkpoint had no signed usage; the codec existed only inside the retired dispute machinery | Addressed in spec (codec in P3): the new `CheckpointArtifact` (§7) signs usage as part of the same bytes as everything else |

## 13. Changes from v1 (informative)

<!-- points at the map-v1-ietf tag for retired text -->

| Area | v1 | This version |
|---|---|---|
| Task protocol | `OFFER`/`QUOTE`/`ASSIGN`/`CHUNK`/`COMPLETE` messages, own state machine | Retired; replaced by an A2A extension binding (`spec/binding-a2a.md`) |
| Settlement | `PAY_QUOTE`/`PAY_INVOICE`/`PAY_PROOF`/`PAY_RECEIPT`, escrow (terms 1/2), credit-ledger method, dispute protocol (§6 claims 0–4) | Retired; money moves on MPP/x402 rails, referenced only via `voucher-digest` (§9.3) |
| Checkpoints | `CHECKPOINT`/`CHECKPOINT_ACK`, `"MAPCheckpointAck"` signing input over `(TaskID, chunk-seq, root)` only | Retired; `CheckpointArtifact` (§7) signs usage too, fixing R4-02 |
| Registries / IANA | Per-document "IANA-Style Considerations," a family-wide wire registry (`draft-map-wire-01`) | Retired; this is not an IETF submission and defines no registries |
| Receipt bytes | `ReceiptCore` keys 1–11 | Unchanged; keys 12 (zone) and 13 (resumed-from) added, both optional |
| Countersignature tolerance | Normative ±5% / ±2000 ms | Demoted to a local policy parameter (§6.5), still anchored to the observed value |
| Identity | Root/device/session key hierarchy; `DelegationCert` chains; rotation/revocation records | Dropped for simplicity (§3.3); signers must be the named AgentIDs directly |
| Cryptographic suites | Negotiated (suite 1 MTI, suite 2 recommended, suite 3 reserved for PQ) | Fixed: Ed25519 + SHA-256 only (§4) |
| Discovery | `[+ Version]`/`[+ uint]` suite lists in descriptors; `MemoryTopic` | Dropped (no suite negotiation; memory retrieval is an A2A skill now) |
| Discovery | No A2A Agent Card carriage | Added: descriptor carries the A2A Agent Card's JCS bytes + JWS (§11.2) |
| Zones | Label defined, no enforcement messages (finding A7) | Re-scoped as attributable dual-signed labels with a refusal rule (§10) |
| Trust | Beta-prior default weight (0.5) for unknown issuers | Zero-default for unknown issuers (T-3), closing the A1 Sybil-ring exploit |
| Trust | No weighting function for known issuers; no per-issuer cap | W1 direct agreement is the recommended default (§8.4, chosen by a pre-registered minimax re-test); per-issuer cap 50 (§8.2), both from evaluation E1 |
| Checkpoint cadence | Unspecified | RECOMMENDED c = 8 transcript leaves (C-6), from evaluation E2/E4 |
| Amounts | Signed `int` mantissa (finding R4-01: negative payments) | `uint` mantissa (M-4) |
| Anti-grinding | Epoch-salted shard proposed as a mitigation | Evaluated (E3b) and found ineffective; D-8 is NOT RECOMMENDED as a mitigation, and A4 is a stated limitation (§11.5) |
| Anti-eclipse | Disjoint-path lookups RECOMMENDED, unevaluated | D-9 SHOULD, supported by evaluation E3c; not yet in `map-net` |
| Name | "Manolito Agent Protocol," MP2P/file-sharing heritage | MAP retained as the acronym, re-expanded to **Mutual Attestation Protocol**; all MP2P framing dropped |
| Publication track | IETF Independent Submission (`map-v1-ietf:map-spec/SUBMISSION.md`) | Dropped; published as a versioned, self-hosted specification plus an arXiv systems paper |

Retired text remains available in full at git tag `map-v1-ietf`
(commit `4107aaa`); it is not deleted, only superseded.

## 14. Security and privacy considerations

### 14.1 Security considerations

See §12 for the threat catalog and finding dispositions. Two points
apply across every section above and are not repeated elsewhere:
invariant I1 (§4.3) — verify received bytes, never a re-serialization —
is the malleability defense underlying every signature check in this
document, and its violation would silently reopen whichever
deterministic-encoding guarantee (§4.1) a producer relied on. Second,
this layer's own security properties are only as strong as the binding
carrying it: session establishment, transport authentication, and
replay protection are the A2A and libp2p discovery bindings'
responsibility, not redefined here (§12, TH2–TH4).

### 14.2 Privacy considerations

<!-- brief, per v1 trust-01 §4 -->

Receipts are pseudonymous personal data in some jurisdictions; this
document never compels their publication. A receipt holder chooses its
own disclosure audience. Redaction of an optional field (the rating, §6.1
key 9) is only valid as a paired, re-signed blinded variant produced at
issuance time — a holder MUST NOT alter signed bytes post hoc and call
the result a redaction; short of a paired variant, a receipt is
disclosed whole or not at all. Consumers of a disclosed receipt MUST
verify it independently (§4.3) — a disclosed evidence set is untrusted
transport, exactly like a DHT record.

Zones (§10) bound what data a task's payload MAY be exposed to, but the
dual-signed zone label is itself metadata: it reveals that a
(requester, provider) pair transacted at a given sensitivity level. A
deployment with strong anonymity requirements needs mitigations this
document does not define (routing the task exchange over an anonymity
network, Zone-3 ephemeral identities per capability) at the binding
layer, not here.

## 15. References

### 15.1 Normative

- [RFC2119] Bradner, S., "Key words for use in RFCs to Indicate
  Requirement Levels," BCP 14, RFC 2119.
- [RFC8174] Leiba, B., "Ambiguity of Uppercase vs Lowercase in RFC 2119
  Key Words," BCP 14, RFC 8174.
- [RFC8949] Bormann, C. and P. Hoffman, "Concise Binary Object
  Representation (CBOR)," RFC 8949 (§4.2.1 core deterministic encoding).
- [RFC8610] Birkholz, H., Vigano, C., and C. Bormann, "Concise Data
  Definition Language (CDDL)," RFC 8610.
- [RFC9052] Schaad, J., "CBOR Object Signing and Encryption (COSE):
  Structures and Process," RFC 9052.
- [RFC9053] Schaad, J., "CBOR Object Signing and Encryption (COSE):
  Initial Algorithms," RFC 9053.
- [RFC8032] Josefsson, S. and I. Liusvaara, "Edwards-Curve Digital
  Signature Algorithm (EdDSA)," RFC 8032.
- [RFC6234] Eastlake 3rd, D. and T. Hansen, "US Secure Hash Algorithms
  (SHA and SHA-based HMAC and HKDF)," RFC 6234; or FIPS 180-4, "Secure
  Hash Standard (SHS)."
- [RFC8785] Rundgren, A., Jordan, B., and S. Erdtman, "JSON Canonicalization
  Scheme (JCS)," RFC 8785.

### 15.2 Informative

- Agent2Agent Protocol (A2A) v1.0. <https://a2a-protocol.org/latest/>
- W3C CCG, "did:key Method v0.9." Community Group Report.
- libp2p Kademlia DHT specification. <https://github.com/libp2p/specs/tree/master/kad-dht>
- AGNTCY, "Agent Discovery Service (ADS)." draft-mp-agntcy-ads-02,
  IETF Internet-Draft (informative track).
- MPP (Machine Payment Protocol) sessions. <https://mpp.dev>
- x402 V2. <https://x402.org>
- Jøsang, A. and R. Ismail, "The Beta Reputation System," Proceedings of
  the 15th Bled Electronic Commerce Conference, 2002.
- Kamvar, S. D., Schlosser, M. T., and H. Garcia-Molina, "The EigenTrust
  Algorithm for Reputation Management in P2P Networks," WWW 2003.
- "Agentic Peer-to-Peer Networks: From Content Distribution to
  Capability and Action Sharing." arXiv:2603.03753, March 2026.
