---
title: "MAP Trust: Execution Receipts, Evidence Exchange, and Local Reputation"
docname: draft-map-trust-00
category: std
---

# MAP Trust and Reputation

**Status:** preprint, work in progress. BCP 14 language applies (see
Requirements Language, below).

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## Abstract

Defines the evidence layer of MAP: dual-signed ExecutionReceipts as
immutable facts of past interactions, selective (pull-based) evidence
disclosure, admission-cost mechanisms that make identities non-free where
meshes require it, graduated trust bootstrap for new agents, informative
guidance for computing local trust from evidence, and audit mechanisms
(canary challenges, redundant execution, promise-vs-observation checks).
No global reputation exists in MAP; this document standardizes *evidence*,
not *scores*.

## 1. Model and prohibitions

Trust in MAP is a **local, derived projection over signed facts**:

- P1: Descriptors, status reports, quotes, and any other self-issued
  object MUST NOT carry trust, reputation, hallucination-rate, or
  tasks-completed fields. Self-attestation of trustworthiness is
  meaningless and was the central defect of the source designs
  (critique C1). Public-mesh policy SHOULD treat violations as
  non-conformant descriptors.
- P2: The only protocol-defined trust inputs are: ExecutionReceipts (§3),
  challenge outcomes (§8), promise-vs-observation deltas the evaluator
  measured itself (quoted TTFT vs. receipt TTFT), and vouch records (§5.3).
- P3: How an agent turns evidence into decisions is local policy (§7 is
  informative). Two conformant agents may disagree about a third — by
  design (arch T4).

Data semantics (normative): a receipt is a **domain event** — a past-tense
fact, immutable once dual-signed, identified by its content address.
Receipt stores are append-only; ingestion MUST be idempotent (dedup by
content address) and replay-safe. Trust scores are **derived data** and
MUST be rebuildable from the receipt store alone; implementations MUST NOT
treat a cached score as authoritative over its evidence. Ordering across
receipts is by their signed timestamps within each (peer, capability)
scope; no global order exists or is needed.

## 2. Receipt lifecycle

```
Requester                                   Provider
  | verify COMPLETE transcript-root (tasks §4)  |
  | build ReceiptCore; sign role 1              |
  | -- RECEIPT {core, sig_req} ---------------→ |  verify core against own
  |                                             |  records; sign role 2
  | ←-- RECEIPT_ACK {sig_prov} ---------------- |  store; done
  | store dual-signed receipt                   |
```

Either party MAY hold a receipt; both SHOULD. The provider's incentive to
countersign is direct: dual-signed receipts are its portable, verifiable
track record. A provider MAY refuse to countersign (e.g., it disputes a
rating); the requester then holds a *unilateral* receipt — still valid
evidence, but consumers weight it as an assertion of the issuer alone
(§7). Requesters likewise SHOULD issue receipts for failures and timeouts
(outcomes 1–3): honest negative evidence is what makes the system
informative. A provider that only ever countersigns successes reveals
that pattern in the gap between its receipt density and its quote volume —
itself a signal.

## 3. ExecutionReceipt (wire format)

Matches test vector TV-002 byte-for-byte.

```cddl
Receipt = {
  1 => bstr .cbor ReceiptCore,   ; exact signed bytes — core I1 applies
  ? 2 => bstr .size 64,          ; sig_req  = Sign(enc(["MAPReceipt", 1, core-bytes]))
  ? 3 => bstr .size 64           ; sig_prov = Sign(enc(["MAPReceipt", 2, core-bytes]))
}                                ; at least one of 2,3 present

ReceiptCore = {
  1 => TaskID,                   ; also binds tasks §1 idempotency key
  2 => AgentID,                  ; requester
  3 => AgentID,                  ; provider
  4 => CapURI,
  5 => Timestamp,                ; started
  6 => Timestamp,                ; ended
  7 => uint,                     ; outcome: 0 ok, 1 failed, 2 timeout,
                                 ;          3 cancelled, 4 disputed
  8 => { 1 => uint, 2 => uint,   ; tokens-in, tokens-out,
         3 => uint, 4 => uint }, ; ttft-ms, wall-ms (as observed by signer
                                 ;  of role 1; provider countersignature
                                 ;  attests agreement within tolerance)
  ? 9 => uint .le 100,           ; rating — explicitly a subjective
                                 ;  assertion, never "verified"
  ? 10 => bstr,                  ; payment reference (pay §5)
  ? 11 => Multihash              ; transcript-root (tasks §8.1)
}
```

Validation: both signatures verify over the *received* core bytes with the
role-tagged structure shown (roles prevent signature confusion between
parties); `2`/`3` identities MUST match core fields 2/3 or chain to them
via delegation (identity §4); timestamps sane (`5 ≤ 6`, within object
freshness policy). The transcript-root links the receipt to the concrete
output commitment, so a receipt can anchor a dispute or an audit without
disclosing the output itself.

Countersignature agreement tolerance (assigned 2026-07-06 from TASK-008; the
numeric window trust §3 previously left open). A provider countersigns only if
the requester's claimed core matches its own observation: task-id, capability,
and identities (fields 1–4) MUST be exactly equal, and each usage field (8.1–8.4)
MUST be within ±5% of the provider's OBSERVED value (minimum ±1), with started/
ended timestamps (5,6) within ±2000 ms. The tolerance band is anchored to the
observed (provider, trusted) value, NOT `max(claimed, observed)`, so an inflated
claim cannot widen its own acceptance band — this is the countersignature-farming
defense (§11). A mismatch → the provider replies ERROR and does not countersign;
the requester keeps a unilateral receipt. (The exact wire error code is a
PENDING-ARCHITECT item — no dedicated "receipt claim rejected" code exists in
core §9; implementations use 5000 in the interim. See map-spec/ERRATA.md.)

Outcome mapping (field 7) from the requester's task terminal (tasks §13):
a Verified completion → 0 (ok); a Failed task → 1 (failed); a TTFT-timeout
CANCEL (tasks §2, error 5001) → 3 (cancelled). Codes 2 (timeout) and 4
(disputed) are set by other paths (provider-side deadline miss; the dispute
protocol, pay §6) and are not requester-task-terminal.

## 4. Evidence exchange — selective disclosure

Global receipt gossip is NOT RECOMMENDED (it publishes a who-computed-
what-for-whom social graph; critique). Evidence moves by pull:

```cddl
EvidenceQuery = {                 ; msg-type 0x0302
  1 => AgentID,                   ; subject
  ? 2 => CapURI,                  ; scope
  ? 3 => [Timestamp, Timestamp],  ; window
  ? 4 => uint,                    ; limit (default 64)
  ? 5 => uint                     ; role filter: 1 as-provider, 2 as-requester
}
EvidenceSet = {                   ; msg-type 0x0303; flows under FLOW_CREDIT
  1 => [* Receipt],
  ? 2 => bool                     ; more available
}
```

Any holder MAY answer for any subject — typically you ask a *candidate
provider* for its own receipts, then discount by your trust in each
receipt's requester-signer. Holders choose their audiences (a receipt is
the holder's data; disclosure is voluntary), MAY redact field 9 or supply
rating-blinded variants, and MUST NOT alter signed bytes (redaction =
omission of the optional field only where a re-signed blinded core was
produced at issuance time as a paired variant; otherwise disclose whole or
not at all). Consumers MUST verify every receipt independently — evidence
sets are untrusted transport, like the DHT.

## 5. Admission costs (Sybil economics)

Identity creation is free by default (arch T3); meshes that need scarcity
choose one or more, declared in mesh policy and checked at descriptor
ingestion or first contact:

### 5.1 Proof-of-work join tickets
`JoinTicket = {AgentID, mesh-id, epoch, nonce}` such that
`H(ticket) < target`; epoch-bound (default 30 days) so tickets expire and
grinding cannot be amortized forever. Equalizes nothing between a laptop
and a datacenter — stated plainly — but raises mass-identity cost by
orders of magnitude. A VDF variant substitutes sequential time for
parallel hashing where hardware asymmetry matters.

### 5.2 Stake attestations
An opaque, settlement-method-scoped proof that value is locked and
slashable under that method's rules (e.g., a bonded Lightning channel, an
escrowed deposit). MAP carries the attestation (`ExtMap` of the
descriptor) and defines nothing about custody or slashing — those belong
to the settlement method (pay §5), avoiding the undefined "smart-contract-
like ledger" of the source designs (critique C15/C9).

### 5.3 Social vouching
A vouch is a DelegationCert-shaped record: issuer attests "I vouch for
subject up to scope S until T". Vouches are evidence weighted by the
evaluator's trust in the *voucher*, and transitively discounted like any
opinion (§7.3). A Sybil army vouched only by itself carries zero weight
for anyone outside the ring — the structural containment that makes
per-observer trust Sybil-aware.

### 5.4 Residual risk (honest statement)
No decentralized mechanism eliminates Sybil attacks; MAP's defense is
layered cost plus the fact that fresh identities hold no evidence anyone
weights. What admission costs buy is a *floor price* on attack scale, and
what local trust buys is that reputation cannot be transferred into or out
of a Sybil ring. Claims beyond this are not made (contrast critique C15).

## 6. Graduated bootstrap

New agents (zero evidence) are not excluded; they are *priced in risk*:

- Tier 0 (no evidence): eligible for canary/low-stakes tasks, small
  contexts, escrowed or postpaid terms; requesters SHOULD cap concurrent
  exposure.
- Tier 1 (evidence exists, low confidence): normal tasks with tightened
  TTFT/deadline enforcement.
- Tier 2 (high confidence for a capability): full eligibility per local
  policy.
Tiers are local policy vocabulary, not protocol state; they appear here so
implementations converge on interoperable *behavior* (a mesh where nobody
routes to the unproven never grows — design3's bootstrap observation,
adopted).

## 7. Computing trust locally (informative)

Recommended default — Beta reputation per (subject, capability):

```
For each verified receipt r about subject s, capability c:
  w(r) = trust_in_issuer(r) × decay(now − r.ended)   ; decay λ ≈ 0.98/day
  outcome 0 → α += w(r);  outcomes 1,2 → β += w(r)
  (outcome 3 cancelled: ignore unless pattern; 4: β += 2w pending dispute)
score(s,c)      = α / (α + β)          ; with prior α₀ = β₀ = 1
confidence(s,c) = α + β
```

Gate high-value routing on `confidence ≥ floor`, not score alone.
Transitive opinions (peers' evidence summaries) are discounted by your
trust in the peer and capped at 2 hops; EigenTrust-style global iteration
is compatible but OPTIONAL and never on the routing critical path
(arch §9). Promise-vs-observation deltas (quoted vs. receipt TTFT; claimed
vs. verified transcript root) SHOULD feed the same α/β as strong signals.
Rebuild rule (normative even though the formula is not): whatever function
you choose, recomputing it from the receipt store MUST reproduce your
scores — no write path MAY update a score without its evidence.

## 8. Challenges and audits

- **Canary tasks** (CHALLENGE 0x0304 / CHALLENGE_RESPONSE 0x0305): a
  requester submits a task with a privately known reference answer,
  indistinguishable on the wire from real work; grading is local. Format:
  Challenge = ordinary ASSIGN; the message types exist for *disclosed*
  challenges used in mesh-level audit programs where providers opt in for
  a trust lift.
- **Redundant execution**: dispatch the same task to k providers, compare
  transcript roots or graded outputs; disagreement lowers all, agreement
  raises all — collusion-resistant when the k-set is sampled from
  disjoint trust neighborhoods.
- **Attestation lift**: TEE-attested SecurityProfile raises priors only;
  it never substitutes for evidence (attestation proves the stack, not
  the behavior).

## 9. Gaming analysis

| Attack | Counter |
|---|---|
| Self-praise via Sybil requesters | Issuer-trust weighting: receipts signed by unknowns carry ~0 weight (§7) |
| Collusion ring inflating each other | Weight flows only along the evaluator's own trust edges; rings are internally dense, externally weightless; redundant-execution sampling across neighborhoods detects output fraud |
| Badmouthing (fake negative unilaterals) | Unilateral receipts weighted by trust in issuer; provider's countersigned corpus dominates |
| Whitewashing (reset identity) | Evidence non-transferable; admission cost (§5) re-paid; Tier-0 exposure caps bound damage |
| Receipt spam | Rate limits (core §8.3), storage quotas, payment-linkage preference (receipts referencing settled invoices weight higher) |
| Selective countersigning (hide failures) | Requester-held unilateral negatives still circulate; density-vs-volume gap is itself evidence (§2) |

## 10. Security considerations

Receipts are pseudonymous personal data in some jurisdictions — retention
and disclosure are holder decisions; the protocol never compels
publication (compliance notes: ops §9). Rating (field 9) is subjective by
declaration; UIs MUST NOT present it as verified. Evidence pulls reveal
who is evaluating whom; route via relay where that matters (discovery §9).
The dominant residual risk is correlated model failure masquerading as
collusion (k providers running the same wrong model agree); mitigate by
diversifying `ModelEntry.weights-digest` across the redundant set.

## 11. IANA-style considerations

Registers: receipt outcome codes; evidence query parameters; vouch caveat
keys; challenge disclosure modes.

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels.
