---
title: "MAP Payments: Negotiation, Settlement-Method Abstraction, Metering, and Disputes"
docname: draft-map-pay-00
category: std
---

# MAP Payments and Settlement

**Status:** preprint, work in progress. BCP 14 language applies (see
Requirements Language, below).

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## Abstract

Defines payment *negotiation and evidence* for MAP: quotes, agreements,
invoices, settlement proofs, payment receipts, and a dispute-evidence
protocol — over a registry of pluggable settlement methods (Lightning,
card-processor intents, stablecoins, ACH, Interledger, private credit
ledgers). MAP mandates no currency, token, chain, or processor; it
standardizes how parties agree on price, prove settlement occurred on the
chosen rail, and exchange evidence when they disagree. This replaces the
orchestrator-minted-credit designs rejected in the critique (C9).

## 1. Model and data semantics

Money moves on an external **settlement rail**; MAP moves **signed
statements about** that movement. Ownership is explicit:

- The **rail** is the sole source of truth for whether value moved. MAP
  proofs are *pointers into* the rail's truth, verifiable under the
  method's own rules.
- Each party owns its local payment-state view; there is no shared
  payment state machine, no distributed transaction spanning rail and
  task, and no exactly-once assumption about rail operations — rails
  duplicate, delay, and fail-after-success, and every flow below is
  retry-safe under those conditions.
- The dual-referenced pair (ExecutionReceipt.payment-ref ↔ PayReceipt)
  is the durable cross-context fact linking work to money; both are
  append-only events, deduplicated by content address.

Distinguish three moments explicitly (never conflate them): **agreed**
(PAY_AGREE exchanged), **submitted** (payer initiated on the rail),
**settled** (proof verifies under the method). Delivery obligations key
off *settled* or off negotiated credit terms — never off *submitted*.

Amounts: CBOR decimal fractions (tag 4, integer mantissa/exponent) plus an
asset identifier — never floats (core §4.2).

```cddl
Amount = #6.4([int, int])            ; [exponent, mantissa]
Asset  = tstr                        ; "iso4217:USD", "asset:sat",
                                     ; "erc20:<chain>:<addr>", "credit:<mesh>"
```

## 2. Message flow

```
Requester(payer)                          Provider(payee)
   | ←------------- PAY_QUOTE ----------- |   (solicited by OFFER, tasks §2)
   | -- PAY_AGREE {quote-id, method} ---→ |
   |            ... task executes; usage-update chunks ...
   | ←------------- PAY_INVOICE --------- |   (final or incremental)
   |   pay on the rail (out of band)      |
   | -- PAY_PROOF {invoice-id, proof} --→ |   verify under method rules
   | ←------------- PAY_RECEIPT --------- |   signed "settled" statement
```

Prepaid and escrowed variants reorder steps (§5.2); credit terms
(pay-after, net-N) collapse INVOICE/PROOF to post-delivery.

## 3. Quotes and agreements

```cddl
PayQuote = {                          ; msg-type 0x0400
  1 => bstr .size 16,                 ; quote-id
  2 => uint,                          ; pricing model: 0 flat, 1 per-token
                                      ;  (in/out rates), 2 per-second,
                                      ;  3 per-invocation
  3 => { * uint => [Asset, Amount] }, ; rate table keyed by unit code
  ? 4 => [Asset, Amount],             ; estimated / maximum total
  5 => [+ MethodOffer],
  6 => Timestamp,                     ; expiry
  ? 7 => uint,                        ; terms: 0 postpaid, 1 prepaid,
                                      ;  2 escrow, 3 metered-incremental
  ? 8 => ExtMap,
  ? 9 => bstr .size 32                ; task binding: TaskID (tasks §1)
}
MethodOffer = { 1 => tstr /*method URI, §5*/, 2 => bstr /*opaque params*/ }

PayAgree = {                          ; msg-type 0x0401
  1 => bstr .size 16,                 ; quote-id (binds exact quote bytes
  2 => Multihash,                     ;  via quote-envelope digest)
  3 => tstr,                          ; chosen method URI
  ? 4 => [Asset, Amount]              ; authorized cap (hard ceiling)
}
```

A signed PAY_AGREE referencing the quote digest is the enforceable
agreement both sides hold; ASSIGN carries its reference (tasks §2.1).
Quotes are binding until expiry (unlike descriptor PricingHints, which are
advertisements only). The authorized cap is a hard constraint: usage
beyond it without a fresh agreement is unbillable — the requester-side
runaway-cost defense.

Task binding (key 9): when a PAY_QUOTE answers an OFFER that carries a
TaskID (tasks §2), key 9 MUST be present and equal that TaskID, and the
requester MUST treat a mismatching or absent binding as a quote for a
different task (reject or re-quote). Quotes without key 9 are standing
quotes: they MAY be agreed to and then bound to a task only via the
ASSIGN's PayAgreeRef. This closes quote replay across tasks; replay of
the quote bytes themselves is already excluded by the envelope msg-id
and the PAY_AGREE digest binding.

### 3.1 Payment references (exported reference types)

Other documents refer to payment state by identity only (ops §2). The
following types are the canonical cross-context references; digests are
computed over the **received envelope bytes** of the referenced message
(core invariant I1 — never over a re-serialization):

```cddl
PayQuoteRef = {                       ; reference to a PAY_QUOTE
  1 => bstr .size 16,                 ; quote-id
  2 => Multihash                      ; digest of received PAY_QUOTE envelope
}
PayAgreeRef = {                       ; reference to a PAY_AGREE
  1 => bstr .size 16,                 ; quote-id
  2 => Multihash                      ; digest of received PAY_AGREE envelope
}
PayRef = {                            ; pre-quote pricing expectation (OFFER)
  ? 1 => uint,                        ; acceptable pricing model (as PayQuote.2)
  ? 2 => [Asset, Amount],             ; maximum acceptable total
  ? 3 => [+ tstr],                    ; acceptable settlement method URIs (§5)
  ? 4 => PayAgreeRef                  ; standing agreement to price under
}                                     ; at least one member MUST be present
```

`Quote.2` (tasks §2) is a `PayQuoteRef`; `Assign.6` (tasks §2.1) is a
`PayAgreeRef`; `Offer.4` (tasks §2) is a `PayRef`. A `PayRef` is an
expectation, not a commitment: a provider MAY quote outside it, and the
requester's remedy is simply not to agree.

## 4. Metering and incremental settlement

For streaming work, usage-update chunks (tasks §3, kind 3) carry running
`{tokens-in, tokens-out, ms}`. Under terms 3 (metered-incremental) the
provider MAY issue incremental PAY_INVOICEs at usage watermarks; the
requester tops up with PAY_PROOFs. If credit is exhausted, the provider
pauses the stream with ERROR 6001 (retryable) and a final unpaid
watermark; the requester either pays or CANCELs, owing usage to the last
**countersigned checkpoint** (tasks §11) — the checkpoint is what makes
"pay for what was delivered" well-defined for both sides. Invoices are
idempotent by invoice-id: re-sending an invoice or a proof MUST NOT
double-bill or double-settle (receivers dedup; rails are consulted by
proof, not by count).

```cddl
PayInvoice = { 1 => bstr .size 16 /*invoice-id*/, 2 => bstr /*quote-id*/,
  3 => [Asset, Amount], ? 4 => { usage watermark }, 5 => bstr /*method
  payload: e.g. BOLT12 offer, processor intent id*/, 6 => Timestamp }
PayProof   = { 1 => bstr /*invoice-id*/, 2 => tstr /*method*/,
  3 => bstr /*proof payload: preimage, intent confirmation, tx ref*/ }
PayReceipt = { 1 => bstr /*invoice-id*/, 2 => [Asset, Amount],
  3 => Timestamp, ? 4 => Multihash /*proof digest*/ }
```

PAY_RECEIPT is the payee's signed past-tense fact "this invoice settled";
it feeds ExecutionReceipt.payment-ref and is the payer's permanent proof
of payment independent of rail-side record retention.

## 5. Settlement methods

### 5.1 Registry (Specification Required; initial entries)

| URI | Rail | Proof semantics |
|---|---|---|
| `pay:lightning-bolt12` | Lightning | invoice payment preimage |
| `pay:stripe-intent` | card processors | intent id + processor confirmation fetchable by payee |
| `pay:erc20-stablecoin` | EVM stablecoins | tx hash + chain/confirmations params |
| `pay:ach` | ACH | trace number; settled = rail-final (days) |
| `pay:interledger` | Interledger/Web Monetization | STREAM receipt |
| `pay:credit-ledger` | private mesh ledger | ledger-operator-signed entry |

Each entry's defining document specifies: params schema, proof schema,
verification procedure, finality/reversal model, and refund reference
format. Reversibility is a *method property* the payee prices in (an ACH
"settled" can still reverse; a preimage cannot) — MAP surfaces it, never
hides it.

Method definition — `pay:credit-ledger` (added 2026-07-06 from TASK-009). A
named ledger operator (a mesh-configured AgentID, trusted BY DESIGN for this
method — the payee prices that trust in). Params (MethodOffer.2): none required
for the MVP. Proof payload (PayProof.3): CBOR `[entry-bytes, sig]` where

```
entry-bytes = enc(["MAPLedgerEntry", invoice-id, payer-AgentID,
                   payee-AgentID, [Asset, Amount], ts])
sig         = Ed25519 by the operator over entry-bytes
```

Verification (payee): `verify_strict` `sig` against the operator AgentID, then
the entry's invoice-id / payer / payee / [Asset, Amount] MUST equal the invoice
being settled. Finality: instant and irreversible within the ledger (no
chargeback path); balances move exactly once per invoice-id (operator dedups).
Refund reference: a fresh entry in the reverse direction (out of MVP scope).

### 5.2 Escrow and prepay

Escrow is a method capability, not a MAP mechanism: methods that support
conditional release (hold-invoice Lightning, processor auth-then-capture,
contract escrow) advertise it in MethodOffer params; MAP's role is only
that COMPLETE + verified transcript-root is the canonical release
trigger both parties can evaluate. `pay:credit-ledger` deliberately
accommodates community meshes running reputation-priced barter or
operator-run tabs — with the ledger operator named and chosen, not
smuggled in as protocol (the fix for C9's accidental central bank).

## 6. Disputes

Scope: MAP adjudicates *evidence availability*, not outcomes.

```
PAY_DISPUTE {invoice-id, claim code}        (either party; 0x0405)
PAY_DISPUTE_EVIDENCE {invoice-id, items}    (both; 0x0406)
```

Claim codes: 0 non-delivery, 1 output-mismatch (transcript-root ≠
delivered), 2 non-payment, 3 overbilling (usage > countersigned
watermarks), 4 terms breach. Evidence items are exactly the signed
artifacts already defined — quote, agreement, invoices, proofs,
countersigned checkpoints, COMPLETE, receipts — so every claim above is
mechanically checkable by any third party the disputants choose (mesh
arbiter, method-level chargeback, court). Timeboxes, arbiter selection,
and remedies are mesh/method policy; a receipt with outcome 4 (disputed)
records the event either way. Design goal: make lying *expensive to
sustain* — every step of a MAP payment leaves a signed artifact the liar
must contradict.

## 7. Security considerations

Proof replay: proofs bind to invoice-ids; invoice-ids bind to quote
digests; core replay rules (§7) cover the envelopes — a captured proof
re-settles nothing. Price-switch: quotes are bound by digest in PAY_AGREE,
so a payee cannot substitute terms. Griefing via disputes: disputes are
signed and receiptable; frivolous disputants accumulate their own
evidence trail. Sanctions/AML/KYC obligations attach to settlement
methods and their operators, not to the negotiation layer; gateways and
method implementations MUST NOT be assumed compliance-free (ops §9).
Metadata: invoices reveal counterparties and amounts to rail operators
per the chosen method — surface this in method definitions so zone policy
can exclude rails inconsistent with the task's sensitivity.

## 8. IANA-style considerations

Registers: settlement method URIs (Specification Required); pricing model
codes; terms codes; dispute claim codes; unit codes for rate tables.

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels.
