---
title: "MAP Payments: Negotiation, Settlement-Method Abstraction, Metering, and Disputes"
docname: draft-map-pay-01
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

Rate-table **unit codes** (registry; Specification Required, ops §3):

| unit | meaning | used by model |
|---|---|---|
| 0 | flat total (the whole task) | 0 |
| 1 | per token in | 1 |
| 2 | per token out | 1 |
| 3 | per millisecond of wall time | 2 |
| 4 | per invocation | 3 |

Units price a SINGLE counted unit (per token, per millisecond — not
per-1k), so every billed amount is an exact `uint counter × Amount`
product: no rounding rule exists anywhere in this document, by
construction. **Same-exponent rule**: all Amounts in one quote's rate
table MUST share one decimal exponent and MUST be in one Asset;
`cumulative-billed` and invoice amounts derived from them carry that same
exponent, so verifiers compare mantissas directly (an exponent mismatch
is a rejection, never a conversion).

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
"pay for what was delivered" well-defined for both sides. The exact
pause/resume/cancel mechanics (no new message type; resume is the covering
PAY_PROOF) are §4.2. Invoices are
idempotent by invoice-id: re-sending an invoice or a proof MUST NOT
double-bill or double-settle (receivers dedup; rails are consulted by
proof, not by count).

```cddl
PayInvoice = { 1 => bstr .size 16 /*invoice-id*/, 2 => bstr /*quote-id*/,
  3 => [Asset, Amount], ? 4 => UsageWatermark, 5 => bstr /*method
  payload: e.g. BOLT12 offer, processor intent id*/, 6 => Timestamp,
  ? 7 => uint /*escrow phase, terms 2 only: 1 hold, 2 release — §5.2.1*/ }
PayProof   = { 1 => bstr /*invoice-id*/, 2 => tstr /*method*/,
  3 => bstr /*proof payload: preimage, intent confirmation, tx ref*/ }
PayReceipt = { 1 => bstr /*invoice-id*/, 2 => [Asset, Amount],
  3 => Timestamp, ? 4 => Multihash /*proof digest*/,
  ? 5 => uint /*kind: 0 settled (default), 1 held, 2 released,
               3 refunded — §5.2.1; unknown values MUST be rejected*/ }

UsageWatermark = {                    ; PayInvoice key 4 (terms 3 only)
  1 => TaskID,                        ; the metered task this invoice bills
  2 => uint,                          ; through-chunk-seq: highest chunk-seq
                                      ;  covered by this invoice (inclusive)
  3 => Usage,                         ; CUMULATIVE usage since task start at
                                      ;  that point (tasks §4 Complete key 4
                                      ;  shape: {1 tokens-in, 2 tokens-out,
                                      ;  3 ttft-ms, 4 wall-ms})
  4 => [Asset, Amount]                ; cumulative-billed: total billed
}                                     ;  through and including THIS invoice
```

### 4.1 Watermark semantics and verification

A watermark is **cumulative since task start**, never a delta: a re-sent
invoice is byte-identical (idempotency falls out), each invoice is
independently verifiable, and monotonicity is a component-wise `>=`.
All four keys are REQUIRED when the watermark is present. Under terms 3
(metered-incremental) every PAY_INVOICE MUST carry key 4; under any other
terms key 4 MUST be absent.

`cumulative-billed` (key 4) MUST equal the agreed rate table applied to
the cumulative usage (key 3) under the agreed pricing model (§3):

- model 1 (per-token):  `tokens-in × rate[1] + tokens-out × rate[2]`
- model 2 (per-second): `wall-ms × rate[3]` (rate unit 3 is per
  MILLISECOND — §3 unit codes — so the product is exact)
- model 3 (per-invocation): `invocations × rate[4]` (one invocation per
  task in this revision)

All products are exact (uint counter × decimal-fraction rate under the §3
same-exponent rule); no rounding is defined or permitted. The invoice
`amount` (key 3) MUST equal this invoice's `cumulative-billed` minus the
`cumulative-billed` of the last invoice the requester settled for this
task (zero for the first).

The requester MUST verify, in order, before paying:

1. presence: key 4 present iff terms 3 — else ERROR **6009** (core §9,
   invoice-watermark violation);
2. task binding: `TaskID` equals the task bound by the agreement — else
   ERROR 6007;
3. monotonicity: `through-chunk-seq`, every `Usage` component, and
   `cumulative-billed` are `>=` the last ACCEPTED watermark's — else 6009;
4. meter consistency: the watermark's `Usage` equals the usage carried by
   the latest received usage-update chunk (tasks §3, kind 3) with
   `chunk-seq <= through-chunk-seq` — a provider MUST meter (emit the
   kind-3 chunk) before it bills — else 6009;
5. arithmetic: the model formula above and the amount-delta rule — else
   6009;
6. cap: `cumulative-billed` does not exceed the PAY_AGREE authorized cap
   (§3) — else ERROR 6005.

A rejected invoice is not settled and does not advance the accepted
watermark.

### 4.2 Pause, resume, and cancel

Metered settlement introduces **no new message type**: pause, resume, and
cancel are expressed entirely with messages already defined (ERROR,
PAY_PROOF, FLOW_CREDIT, CANCEL). The exhaustion signal is *proof absence*,
exactly as in the postpaid flow — a requester that cannot or will not pay
an invoice simply withholds its PAY_PROOF; there is no "insufficient"
message.

**Pause** is a provider-local decision, not a wire handshake. A provider
MAY pause the stream whenever it holds an unpaid outstanding watermark
(billed minus settled), and it SIGNALS the pause with ERROR **6001**
(retryable) whose detail map (core §9 key 5) carries the final unpaid
watermark as `{1 => UsageWatermark}`. RECOMMENDED policy: pause when the
unpaid amount would exceed the stream's granted flow credit (core §8.1) or
the provider's own per-stream exposure limit — so that unpaid delivery is
bounded by exactly the backpressure MAP already provides. While paused, the
provider MUST NOT emit further billable CHUNKs, MUST NOT re-issue a new
invoice-id for already-billed usage (the paused invoice stands, idempotent
by id — the 6001 watermark merely restates the outstanding position so a
requester that missed the invoice can reconstruct it), and MUST NOT advance
the watermark.

**Resume** requires no marker: the covering **PAY_PROOF** IS the resume
trigger. When the provider observes a PAY_PROOF that settles the
outstanding invoice(s) which caused the pause, it settles (PAY_RECEIPT) and
continues streaming from the exact watermark — no gap and no double-bill,
since the watermark is cumulative and monotonic. The next CHUNK the
requester receives is itself the observable resume. A metered stream also
remains governed by FLOW_CREDIT throughout: a requester that wishes to stop
receiving (and thus being billed) MAY simply stop granting credit, starving
the stream through the existing core §8.1 mechanism.

**Cancel**: on CANCEL (tasks §2, msg-type 0x020C) instead of top-up, the
amount owed anchors to the last countersigned checkpoint (tasks §8.2 key 5
usage, billed under the same §4.1 formula), which both sides hold signed —
never to an unpaid watermark the requester never countersigned. The owed
amount is therefore always ≤ the final unpaid watermark: a dying or
malicious stream can claim payment only for delivery the requester has
already acknowledged at a checkpoint.

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
| `pay:x402` | x402 (HTTP-native stablecoin; EIP-3009 authorizations, facilitator settle) | signed transfer authorization; settled = facilitator/chain settlement |

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

Method definition — `pay:x402` (added 2026-07-14, ARCHITECT DESIGN; see
CHANGELOG). Settles via the x402 protocol (x402.org, Apache-2.0): the payer
signs an exact-amount, time-boxed, single-use on-chain transfer authorization
(EIP-3009 `transferWithAuthorization` for EVM stablecoins such as USDC) and
the payee — directly or through a payee-chosen **facilitator** service's
`/verify` and `/settle` endpoints — executes it on chain. MAP adopts x402's
*settlement payload and facilitator model only*: x402's own HTTP
402/`X-PAYMENT` request loop is NOT used between MAP peers (PAY_QUOTE /
PAY_AGREE / PAY_INVOICE already carry the negotiation with stronger binding);
it remains appropriate at a gateway (ops §5) that fronts external
x402-priced HTTP services as MAP capabilities.

Asset/amount mapping: the quote's `Asset` MUST be the `erc20:<chain>:<addr>`
form (§1) naming the token the scheme settles, and the Amount exponent MUST
be the token's decimals negated (USDC ⇒ −6), so the mantissa IS the atomic
unit count — the §3 exact-arithmetic rules apply unchanged.

Params (MethodOffer.2, deterministic-CBOR map):

```cddl
X402Params = {
  1 => uint,            ; x402Version (1 in this revision)
  2 => tstr,            ; scheme ("exact" in this revision)
  3 => tstr,            ; network id (e.g. "base", "base-sepolia")
  4 => tstr,            ; payTo: payee on-chain address
  ? 5 => tstr,          ; facilitator base URL (informative — payee's choice)
  ? 6 => uint           ; max authorization validity the payee accepts (s)
}
```

Proof payload (PayProof.3, deterministic-CBOR map):

```cddl
X402Proof = {
  1 => bstr             ; the x402 PaymentPayload, verbatim (the JSON bytes
                        ;   an X-PAYMENT header would carry, base64-decoded):
                        ;   scheme/network + the signed EIP-3009 authorization
}
```

**Invoice binding (MAP-specific MUST):** the authorization's 32-byte nonce
MUST be `SHA-256("MAP-x402-nonce" || invoice-id)`; the payee MUST verify it.
This binds the authorization to exactly one MAP invoice, and the chain's
single-use-nonce rule then enforces exactly-once settlement across retries
(layered under MAP's invoice-id idempotency, not replacing it).

Verification (payee): decode the payload; check x402Version/scheme/network/
`payTo` against the offered params, the token contract against the quote's
Asset, `value` == the invoice Amount mantissa (atomic units), the nonce rule
above, and a validity window covering now + expected settle latency; then
settle (facilitator `/settle` or direct submission). **A successful `/verify`
alone is NOT settlement** — the payee MUST NOT reply PAY_RECEIPT (or release
anything priced) before the settlement succeeds/confirms per its params.
Finality: reversal-resistant once the transfer confirms; the facilitator is
a payee-chosen, payee-trusted intermediary (the payer's exposure is bounded
by the exact-amount, time-boxed, single-use authorization) — reversibility
and facilitator trust are method properties the payee prices in, per this
section's doctrine. Refund reference: the tx hash of a reverse transfer
(out of MVP scope).

### 5.2 Prepay and escrow (terms 1 and 2)

Both variants move settlement BEFORE delivery. Neither adds a message
type (the same doctrine as §4.2): they are reorderings of the §2 flow
plus two additive optional fields — the invoice `phase` and the receipt
`kind` (§5.2.1). What they demand of the rail differs: prepay needs
nothing beyond §5.1; escrow is a **method capability** — only methods
that support conditional holds (hold-invoice Lightning, processor
auth-then-capture, contract escrow, the §5.2.4 credit-ledger extension)
may be offered in a terms-2 quote, and including a MethodOffer in a
terms-2 quote asserts that the offered instance supports it. MAP's role
is unchanged in kind: the rail holds and moves the money; MAP defines the
signed statements, their ordering, and the canonical release trigger
both parties can evaluate — **COMPLETE + verified transcript-root**
(tasks §4).

A quote under terms 1 or 2 MUST carry key 4 (estimated/maximum total):
it is the amount settled or held up front, under the §3 same-exponent
rule. Terms are exclusive; combined metered-escrow (a held ceiling drawn
down by watermarks) is not defined in this revision.

#### 5.2.1 Receipt kinds and invoice phases (additive fields)

`PayReceipt` key 5 `kind`: 0 settled (the default when absent — all
pre-existing receipts are unchanged), 1 held, 2 released, 3 refunded.
Receivers MUST reject a receipt with an unrecognized kind (fail-closed;
ERROR 6010). Only kinds 0 and 2 are settlement statements and feed
`ExecutionReceipt.payment-ref`; kind 1 is the payee's signed statement
that a conforming hold exists on the rail; kind 3 records a return of
held funds to the payer.

`PayInvoice` key 7 `phase`: 1 hold, 2 release. Key 7 MUST be present iff
the governing agreement's terms are 2 (the mirror of the §4.1 watermark
presence rule); a violation is ERROR 6010 (escrow violation, core §9).

#### 5.2.2 Terms 1 — prepaid

```
Requester(payer)                          Provider(payee)
   | ←---- PAY_QUOTE {terms 1} ---------- |
   | -- PAY_AGREE ----------------------→ |
   | ←---- PAY_INVOICE ------------------ |   amount = quote key 4 total
   |   pay on the rail (out of band)      |
   | -- PAY_PROOF ----------------------→ |   verify under method rules
   | ←---- PAY_RECEIPT {settled} -------- |
   |            ... task executes ...     |
   | ←------------- COMPLETE ------------ |
```

Rules: the single PAY_INVOICE MUST equal the quote's key 4 total and
carries no watermark (§4.1) and no phase key (§5.2.1). The provider MUST
NOT begin billable execution before the invoice is *settled* — delivery
keys off settled (§1), which is precisely what the payer buys with a
prepay. An amount other than the quoted total is ERROR 6010. Excess of
prepay over eventual usage is not overbilling (the payer agreed the
total; dispute claim 3 does not apply); non-delivery after settlement is
dispute claim 0, with the receipt and the absent COMPLETE as evidence,
remedied by the method's refund reference or the mesh's §6 process.

#### 5.2.3 Terms 2 — escrow

Escrow adds a fourth moment to §1's three: **held** — value locked on
the rail such that the payer cannot unilaterally reclaim it, the payee
cannot unilaterally claim it, and a defined expiry returns it to the
payer. Under terms 2, delivery obligations key off *held*; final
settlement keys off *released*.

```
Requester(payer)                          Provider(payee)
   | ←---- PAY_QUOTE {terms 2} ---------- |
   | -- PAY_AGREE ----------------------→ |
   | ←---- PAY_INVOICE {phase 1 hold} --- |   amount = quote key 4 total
   |   place hold on the rail (oob)       |
   | -- PAY_PROOF {hold proof} ---------→ |   verify hold under method
   | ←---- PAY_RECEIPT {kind 1 held} ---- |
   |            ... task executes ...     |
   | ←------------- COMPLETE ------------ |   transcript-root (tasks §4)
   |   verify COMPLETE + transcript-root  |
   | ←---- PAY_INVOICE {phase 2 release}- |   amount = final owed ≤ held
   |   authorize release on the rail (oob)|
   | -- PAY_PROOF {release proof} ------→ |
   | ←---- PAY_RECEIPT {kind 2 released}- |   remainder refunds to payer
```

Escrow is **two invoice rounds**, each the standard
invoice→proof→receipt unit: hold and release carry distinct invoice-ids,
both bound to the same quote-id, and §4's idempotency rules apply per
round unchanged (a re-sent hold or release invoice is byte-identical; a
re-delivered proof settles nothing twice).

The requester MUST verify, in order, before acting on a terms-2 invoice:

1. phase: key 7 present and ∈ {1, 2}, per §5.2.1 — else ERROR 6010;
2. task binding: as §4.1 check 2 — else 6007;
3. hold invoices: amount equals the quote key 4 total, and no prior hold
   exists for this agreement (one hold per agreement) — else 6010;
4. release invoices: a COMPLETE for the bound task has been received AND
   its transcript-root verified (tasks §4) — the canonical release
   trigger. A release invoice before that MUST NOT be paid: 6010;
5. release amount: ≤ the held amount; and if the task ended by CANCEL,
   exactly the §4.1 formula applied to the last countersigned
   checkpoint's usage (the §4.2 cancel anchor) — else 6010;
6. cap: as §4.1 check 6 — else 6005.

**Authorization symmetry** (the property that makes a hold an escrow and
not a transfer): the rail/operator MUST NOT move held funds to the payee
without a payer-signed release authorization bound to (hold-invoice-id,
release-invoice-id, amount) — the payee cannot self-release; and held
funds MUST NOT return to the payer before expiry without a payee-signed
refund authorization — the payer cannot unwind a hold it granted. At
release, the remainder (held minus released) returns to the payer in the
same rail operation. A payer that verifies COMPLETE and then withholds
its release signature is dispute claim 2 (non-payment), with the kind-1
receipt, the signed COMPLETE, and the transcript as evidence — MAP makes
the lie expensive to sustain (§6); the method and mesh make it
remediable.

**Expiry**: every hold MUST carry an expiry (method params); at expiry,
unreleased funds return to the payer in full, and a release fails on the
rail thereafter (ERROR 6002 at the MAP layer). Payees SHOULD price the
expiry at deadline + expected release latency with margin — an expiry
shorter than the task deadline is self-harm, not a protocol violation.

Failure matrix (honest-party exposure is bounded in every row): COMPLETE
never arrives → expiry refunds the payer (dispute claim 0 if a record
matters); CANCEL → release invoice for the checkpoint-anchored amount,
remainder refunded (a kind-3 receipt for the remainder is OPTIONAL);
provider vanishes → expiry refund; payer vanishes after COMPLETE →
dispute claim 2, and the payee's loss is bounded by work it can prove it
delivered. The payee's exposure is bounded by the hold (it never works
beyond held value); the payer's by the expiry (money returns absent its
own signature).

#### 5.2.4 `pay:credit-ledger` escrow extension

The §5.1 credit-ledger method supports terms 2 with three additional
operator-signed entry forms (same `[entry-bytes, sig]` proof envelope
and verification rules as §5.1). Params (MethodOffer.2) under terms 2:
`{? 1 => uint}` — maximum hold validity the operator accepts, seconds.

```
hold-entry    = enc(["MAPLedgerHold", hold-invoice-id, payer, payee,
                     [Asset, Amount], expiry-ts, ts])
release-auth  = enc(["MAPLedgerReleaseAuth", hold-invoice-id,
                     release-invoice-id, [Asset, Amount]])
                ; signed by the PAYER, presented to the operator
refund-auth   = enc(["MAPLedgerRefundAuth", hold-invoice-id])
                ; signed by the PAYEE (pre-expiry refund)
release-entry = enc(["MAPLedgerRelease", hold-invoice-id,
                     release-invoice-id, payer, payee,
                     [Asset, Amount], ts])
refund-entry  = enc(["MAPLedgerRefund", hold-invoice-id, payer,
                     [Asset, Amount], ts])
```

Operator rules: a hold moves Amount from the payer's balance into an
escrow bucket keyed by hold-invoice-id (idempotent by that id;
insufficient balance → no entry). A release requires a valid payer
`release-auth` over exactly this (hold, release, amount) triple, amount
≤ held, and now < expiry; it atomically credits the amount to the payee,
returns the remainder to the payer, and closes the hold (idempotent by
release-invoice-id). A refund requires expiry passed OR a valid payee
`refund-auth`, returns the held amount to the payer, and closes the
hold. The hold entry is the PAY_PROOF payload for the phase-1 invoice;
the release entry for the phase-2 invoice.

`pay:credit-ledger` deliberately accommodates community meshes running
reputation-priced barter or operator-run tabs — with the ledger operator
named and chosen, not smuggled in as protocol (the fix for C9's
accidental central bank).

## 6. Disputes

Scope: MAP adjudicates *evidence availability*, not outcomes. The
protocol's whole job here is that each party can (a) state a precise
claim bound to a precise agreement, (b) hand over the signed artifacts
that bear on it, and (c) let ANY third party the disputants choose (mesh
arbiter, method-level chargeback, court) check the claim mechanically
without trusting either disputant. Timeboxes, arbiter selection, and
remedies are mesh/method policy — deliberately outside this document.
Design goal: make lying *expensive to sustain* — every step of a MAP
payment leaves a signed artifact the liar must contradict.

### 6.1 Messages

```cddl
PayDispute = {                        ; msg-type 0x0405; either party
  1 => bstr .size 16,                 ; dispute-id (fresh)
  2 => bstr .size 16,                 ; invoice-id under dispute
  3 => uint,                          ; claim code (registry, §8):
                                      ;  0 non-delivery, 1 output-mismatch,
                                      ;  2 non-payment, 3 overbilling,
                                      ;  4 terms breach
  4 => PayAgreeRef,                   ; the governing agreement (§3.1) —
                                      ;  anchors quote, terms, cap, task
  ? 5 => bstr .size 32,               ; TaskID, if the agreement is bound
  ? 6 => tstr .size (0..1024),        ; free-text statement (never evidence)
  7 => Timestamp
}

PayDisputeEvidence = {                ; msg-type 0x0406; both parties
  1 => bstr .size 16,                 ; dispute-id
  2 => [+ EvidenceItem],
  ? 3 => bool                         ; more available (chunk under
}                                     ;  FLOW_CREDIT, core §8.1)

EvidenceItem = {
  1 => uint,                          ; kind (registry, §8):
                                      ;  0 PAY_QUOTE envelope
                                      ;  1 PAY_AGREE envelope
                                      ;  2 PAY_INVOICE envelope
                                      ;  3 PAY_PROOF envelope
                                      ;  4 PAY_RECEIPT envelope
                                      ;  5 countersigned checkpoint
                                      ;     (Checkpoint + the requester
                                      ;      countersignature, tasks §8.2)
                                      ;  6 COMPLETE envelope
                                      ;  7 ExecutionReceipt (dual-signed,
                                      ;     trust §3)
                                      ;  8 method artifact (ledger entry,
                                      ;     release-auth, x402 payload,
                                      ;     gateway upstream evidence …)
  2 => bstr,                          ; the artifact VERBATIM: for envelope
                                      ;  kinds, the received envelope bytes
                                      ;  (core I1 — never re-serialized)
  ? 3 => bstr .size 32                ; the session channel binding (CB)
}                                     ;  under which an in-session envelope
                                      ;  was signed — see §6.2
```

Unrecognized claim codes and evidence kinds MUST be rejected
(fail-closed). Disputes are idempotent by dispute-id; re-sent evidence
messages MUST NOT duplicate items in the verifier's view (dedup by item
content address). Opening a dispute does not suspend other machinery by
itself; a party refusing to continue a flow pending resolution signals
ERROR 6003 (dispute opened). A receipt with outcome 4 (disputed, trust
§3) records the event on the trust plane either way, whoever proves
right.

### 6.2 Third-party verifiability and CB disclosure

Inner-signed artifacts (ExecutionReceipts, checkpoint
countersignatures — decision-9-style signatures over stated bytes) are
verifiable by anyone from the bytes alone. Envelope artifacts are
COSE_Sign1 over the received bytes with `external_aad` = the session
channel binding (CB) when sent in-session (core §7.3) — so a third
party CANNOT verify them without the CB. Therefore: an EvidenceItem
carrying an in-session envelope MUST include key 3 (the CB value);
out-of-session envelopes (empty external_aad) omit it. Disclosing a CB
post-hoc is safe — it is an HKDF output, not key material, and reveals
nothing about session traffic keys — and it is *useful*: all in-session
items verifying under the SAME disclosed CB are thereby proven to come
from one session between the two disputants. A verifier MUST treat an
in-session item whose CB is absent or fails verification as unavailable
evidence, not as a verified artifact.

### 6.3 Mechanical verification procedure

Given a PayDispute and both parties' evidence sets, a verifier (either
disputant, or any third party) proceeds: authenticate every item per
§6.2 against the AgentIDs bound by the agreement (item signers MUST be
the disputants or the method's named operator; anything else is
discarded); resolve the agreement (kind-1 item matching PayAgreeRef's
digest, and its kind-0 quote matching the agreement's quote digest —
the 6006 machinery); then evaluate the claim:

- **0 non-delivery** (payer claims): supported iff the payer's evidence
  shows value moved — a settled PAY_RECEIPT (kind 0/2) or verified
  proof — AND the payee's evidence contains NO countersigned checkpoint
  and no COMPLETE for the bound task. This is rebuttal-shaped: absence
  is never proven, so the claim *stands unrebutted* if the payee, who
  by construction holds the countersigned artifacts if delivery
  happened, produces none.
- **1 output-mismatch** (payer claims): supported iff the COMPLETE's
  transcript-root does not equal the root recomputed (tasks §8.1) over
  the delivered transcript the payer presents, at or before the
  COMPLETE's chunk range. Fully mechanical.
- **2 non-payment** (payee claims): rebuttal-shaped dual of claim 0:
  stands unless the payer produces a verifying PAY_PROOF / receipt /
  release-auth covering the invoice (escrow: the release round).
- **3 overbilling** (payer claims): supported iff an invoice's
  cumulative-billed (or a terms-1/2 amount) exceeds the §4.1 formula
  applied to the highest COUNTERSIGNED usage (checkpoint key 5) in
  evidence, under the agreement's rate table — exact arithmetic, no
  tolerance. (Watermarks the payer accepted but did not countersign
  bill validly under §4.1; overbilling is measured against what the
  payer ACKNOWLEDGED, which is why checkpoints exist.)
- **4 terms breach** (either): supported iff the evidence itself
  exhibits a violation of §3–§5.2 rules that existing verifiers detect
  mechanically: quote-digest mismatch (6006), cap breach (6005),
  watermark violation (6009), escrow-phase/prepaid violation (6010),
  invalid proof (6008). The claim names the rule; the verifier re-runs
  the corresponding check over the presented artifacts.

The verdict vocabulary is three-valued: **supported**, **rebutted**, or
**insufficient evidence** — MAP never emits remedies. Both
rebuttal-shaped claims (0, 2) resolve against the party that withholds
artifacts it must hold if honest; that asymmetry is the availability
doctrine doing its work.

### 6.4 Griefing bounds

A dispute is itself a signed artifact: frivolous disputants accumulate
their own evidence trail (§7), and outcome-4 receipts weigh against
both parties' trust projections pending resolution (trust §5). Evidence
volume rides FLOW_CREDIT (§6.1 `more` chunking) — a party cannot flood;
a verifier MAY cap total evidence size as local policy and rule
"insufficient" beyond it.

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
codes; terms codes; dispute claim codes; dispute evidence-item kinds
(§6.1); unit codes for rate tables.

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels.
