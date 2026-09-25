# Binding metering evidence to payment rails (informative), version 2.0-draft

**Status: INFORMATIVE.** This document is not normative for MAP conformance.
It maps the layer's metering evidence (`UsageWatermark`-equivalent usage
carried in `CheckpointCore`/`ReceiptCore`, layer spec, *Metering and voucher
binding*) onto two external rails — MPP sessions and x402 V2 — plus a
generic fallback. Where a normative claim about a rail's own wire format is
made, it is cited to that rail's spec; MAP does not modify either rail.

Rule IDs (RAIL-*) are provided so implementation notes and tests can cite
this document precisely, but nothing here is a MAP conformance requirement
by itself — a MAP implementation MAY support zero, one, or several rails.

## 0. Sources and dates

- MPP: https://mpp.dev/intents/session (general session flow — voucher
  field names are **NOT specified at this generic layer**; see RAIL-1) and
  https://mpp.dev/payment-methods/tempo/session (Tempo method binding).
  Tempo's `SessionReceipt` TypeScript type was read directly off that page:
  `channelId`, `acceptedCumulative`, `spent`, `reference`, `challengeId`,
  `method`, `intent`, `status`, `timestamp`, `txHash?`, `units?`. Tempo
  network: chain id 4217 (mainnet) / 42431 (testnet "Moderato"). No
  document date was present on either page; treat both as current as of
  2026-09-24 (today). **UNVERIFIED**: the exact byte-level canonical
  encoding of a Tempo voucher prior to signing (the page describes the
  cumulative-authorization semantics but does not give a wire/ABI
  specification for the signed bytes) — RAIL-2 defines a MAP-side digest
  input that does not depend on that detail.
- x402 V2: `coinbase/x402` GitHub repository, `specs/x402-specification-v2.md`
  (Version History: v2.0, 2025-12-9), `specs/schemes/upto/scheme_upto.md`,
  `specs/schemes/batch-settlement/batch_settlement.md`,
  `specs/transports-v2/http.md` (headers `PAYMENT-REQUIRED`,
  `PAYMENT-SIGNATURE`, `PAYMENT-RESPONSE`, confirmed verbatim from the
  transport doc's own "Header Summary" table). CAIP-2 network identifiers
  (`eip155:8453`, `solana:...`, etc.) confirmed in §11.1 of the core spec.

## 1. Common conventions

RAIL-1 (voucher-digest, generic rule): where a rail defines its own
canonical byte form for the signed authorization (a serialized transaction,
an EIP-712 typed-data hash, a rail-native wire encoding), `voucher-digest`
is the Multihash SHA-256 (`0x12 0x20 || SHA-256(bytes)`) of **exactly those
bytes** — never a MAP re-encoding of the same fields, so a dispute can be
settled by re-deriving the digest from the rail's own record. Where a rail
gives field *names* but not a canonical byte form (MPP's generic session
layer, and Tempo specifically — see §0), `voucher-digest` is instead the
Multihash SHA-256 of `enc()` (deterministic CBOR, RFC 8949 §4.2.1) of a
fixed-order CBOR array built from the fields that rail's method binding
*does* name. Per-rail field orders are given in §2/§3.

RAIL-2 (Amount → base units, exact-only): the layer's usage-derived amount
is a `[uint counter, decimal-fraction rate]` product under one shared
decimal exponent (the same discipline v1 `draft-map-pay-01.md` §3 used:
"Same-exponent rule... `cumulative-billed`... carr[ies] that same exponent,
so verifiers compare mantissas directly"). Mapping to a rail's integer
atomic units is exact **by construction, never by rounding**: the rail's
token decimals fix the required exponent (for example USDC ⇒ exponent −6,
so the mantissa already **is** the atomic-unit integer once the rate table
is expressed at that exponent). RAIL-3: if the layer's computed
cumulative amount's exponent does not exactly equal `−decimals` for the
rail's asset, the conversion MUST be rejected, never rounded or
truncated — this is the same "no rounding rule exists anywhere... by
construction" discipline v1 pay §3 established, carried over unchanged
because it is a sound rule independent of which rail consumes the number.

## 2. MPP sessions

RAIL-4 (canonical voucher bytes): MPP's own `intents/session` page defines
only the high-level session request shape (`amount`, `currency`,
`description`, `expires`, `methodDetails`, `recipient`) and explicitly
delegates voucher/authorization byte formats to "method-specific session
implementations" (§0). For the Tempo method, the only field set actually
named is the `SessionReceipt` shape read off the page — which is a
*receipt* (post-hoc summary: `acceptedCumulative`, `spent`, `channelId`,
`reference`, `txHash?`) rather than the signed voucher a client sends
mid-session. Because Tempo's pre-signature wire form is UNVERIFIED (§0),
this binding does not attempt to reconstruct it; RAIL-5 instead defines
`voucher-digest` over the **named, stable fields** available from the
public method binding, in this fixed CBOR array order:

```cddl
MppTempoVoucherFields = [
  channelId:  bstr,     ; from SessionReceipt.channelId, 0x-prefixed hex decoded to bytes
  cumulative: [int, int], ; Amount (this doc's RAIL-2 convention), from acceptedCumulative
  reference:  tstr,     ; SessionReceipt.reference
  method:     tstr       ; SessionReceipt.method, e.g. "tempo"
]
```

`voucher-digest = Multihash(enc(MppTempoVoucherFields))`. RAIL-6: if a
future revision of the Tempo (or another MPP method) page publishes a
canonical pre-signature byte form, that form supersedes RAIL-5 for that
method per RAIL-1's general rule — RAIL-5 is a documented fallback, not a
claim that MPP lacks its own canonical form generally.

RAIL-7 (Usage → cumulative amount): the layer's `CheckpointCore`/
`ReceiptCore` `Usage` (`tokens-in`, `tokens-out`, `ttft-ms`, `wall-ms`) maps
to MPP's `acceptedCumulative` via the same rate-table × counter product
described in RAIL-2, evaluated once per checkpoint (§4) rather than per
request — MPP's own description already frames this as "the client signs a
voucher with increasing cumulative amounts as service is consumed... each
voucher authorizes 'I have now consumed up to X total,'" which is the same
cumulative-since-start semantics the layer's own `Usage` field already has
(no delta arithmetic needed at the mapping boundary; both sides are
cumulative-from-zero).

RAIL-8 (pause/resume): pause is MPP's own `payment-need-voucher` mechanism —
this binding does not define a MAP-native pause message; a checkpoint that
cannot be funded by an outstanding voucher simply is not proposed further
until MPP signals resumption on its own session, exactly as v1 `draft-map-
pay-01.md` §4.2 already established for the generic case ("no new message
type... the exhaustion signal is proof absence"). RAIL-9: because MPP's
session layer is stateful (a single `channelId` spans many
checkpoints/vouchers), a resumption onto a different provider (layer spec,
*Checkpoints and cross-provider resumption*) requires a **new** MPP session
(new `channelId`) at the successor — the cumulative counter does not carry
over at the rail level even though the layer's own transcript/usage
counters do (§4's exposure bound accounts for this).

## 3. x402 V2

RAIL-10 (two schemes, two different fits — a distinction the source
material makes and this binding preserves rather than picking one):

- **`upto`** (`scheme_upto.md`) authorizes a maximum and settles a single
  actual amount ≤ that maximum, **once** — its own spec is explicit that
  "Multi-settlement / streaming: Settling the same authorization multiple
  times... [is] NOT supported by `upto`." It fits a **single checkpoint
  interval's worth of authorization**, or a whole short task's authorized
  cap, not ongoing cumulative metering across many checkpoints.
- **`batch-settlement`** (`batch_settlement.md`) is the scheme whose shape
  actually matches MAP's cumulative-checkpoint model: a "capital-backed"
  network binding under this scheme is explicitly illustrated by the
  spec's own example — "Payment channel streaming. A client and provider
  open a payment channel once. Each request increments a signed running
  total (a receipt)." — which is the same repeated-cumulative-voucher
  pattern as MPP §2, just under x402's envelope instead.

RAIL-11 (canonical voucher bytes, `upto`): the EIP-3009-style
`authorization` object x402 V2 defines (`x402-specification-v2.md` §5.2.2:
`from`, `to`, `value`, `validAfter`, `validBefore`, `nonce`) together with
its EIP-712 `signature` **is** the rail's own canonical byte form — the
EIP-712 signing hash over the `TransferWithAuthorization` typed data (§6.1.1
of the same document). `voucher-digest = Multihash(signature-bytes)` per
RAIL-1's first case (a rail-native canonical form exists; use it directly,
specifically the signature bytes, since they are the unique, non-reusable
artifact per authorization — `upto`'s own "Single-Use Authorization"
property, `scheme_upto.md` §1, guarantees this is collision-free per
authorization).

RAIL-12 (canonical voucher bytes, `batch-settlement`): this scheme's own
spec states the commitment format "is defined entirely by the network
binding" (no protocol-wide byte form). Following RAIL-1's second case,
`voucher-digest = Multihash(enc(X402BatchVoucherFields))` with:

```cddl
X402BatchVoucherFields = [
  network:     tstr,        ; CAIP-2, e.g. "eip155:8453"
  asset:       tstr,        ; token contract address or currency code
  payTo:       tstr,
  cumulative:  [int, int],  ; Amount, this doc's RAIL-2 convention
  commitmentId: tstr        ; the settlement result's commitment identifier
                            ;  once known; empty tstr before first settle
]
```

RAIL-13 (Usage → amount): under `upto`, the `amount` field of
`PaymentRequirements` is phase-dependent per the rail's own spec — "maximum"
at verify time, "actual amount to settle" at settle time
(`scheme_upto.md` §5). This binding sets the verify-time maximum from the
layer's rate table applied to the *checkpoint interval's* usage bound (not
the whole task), and the settle-time actual amount from the usage actually
countersigned at that checkpoint (RAIL-2's exact-conversion rule applies to
both). Under `batch-settlement`, the mapping is the same cumulative-product
rule as MPP (RAIL-7), evaluated per checkpoint, carried in
`X402BatchVoucherFields.cumulative`.

RAIL-14 (pause/resume): x402's own mechanism is per-request authorization —
no resource is served without a valid `PAYMENT-SIGNATURE`. Under `upto`
per checkpoint interval, a checkpoint simply is not proposed (equivalently,
the provider pauses delivery) until a fresh `PAYMENT-SIGNATURE` covering
the next interval's authorization arrives; under `batch-settlement`, pause
follows the same "no new message type, proof absence is exhaustion" rule
as RAIL-8.

RAIL-15 (headers, informative — carriage, not this binding's own wire): a
rail-native x402 exchange over HTTP uses `PAYMENT-REQUIRED` (server→client,
base64 `PaymentRequired`), `PAYMENT-SIGNATURE` (client→server, base64
`PaymentPayload`), and `PAYMENT-RESPONSE` (server→client, base64
`SettlementResponse`) — confirmed verbatim from `transports-v2/http.md`'s
own header-summary table. Where the task exchange itself runs over
`binding-a2a.md` rather than raw HTTP, these headers are not present; the
`voucherDigest` sub-key defined there (`binding-a2a.md` §3) is what carries
the rail linkage into the layer's own evidence instead, exactly the point
of `voucher-digest` as a rail-agnostic reverse binding (ADR-0001,
"Voucher fields in MPP/x402 may not carry a free-form reference").
Observation, not a finding this document relies on: an existing third-party
x402-over-A2A transport (`transports-v2/a2a.md`) already threads x402
payloads through A2A `Message.metadata` under `x402.payment.*` keys — a
structurally similar move to this document's own approach, done
independently for a different purpose (settlement itself, not evidence
binding).

## 4. Checkpoint cadence, voucher cadence, and provider exposure

RAIL-16: this binding does not require voucher cadence to equal checkpoint
cadence, but the useful case is voucher-per-checkpoint (one authorization
proposal-and-settle cycle per checkpoint interval), because the checkpoint
is what makes "pay for what was delivered" well-defined for both sides
(the same point v1 pay §4 made about the old native metering; it is
rail-independent). Under that alignment, maximum provider exposure at any
moment is:

```
exposure ≤ price(one checkpoint interval) + credit_extended
```

where `price(one checkpoint interval)` is the rate table applied to
`checkpointIntervalParts` (or whatever local cadence parameter is in
force, `binding-a2a.md` §1) worth of usage, and `credit_extended` is
whatever additional unbilled allowance the requester's local policy grants
beyond the last settled voucher (zero, by default, under either rail's
"pause on unpaid" behavior, RAIL-8/RAIL-14). This is a direct
generalization of v1 pay §4.2's exposure argument ("a dying or malicious
stream can claim payment only for delivery the requester has already
acknowledged at a checkpoint") restated rail-agnostically.

## 5. What the rail proves vs. what MAP evidence proves

RAIL-17: the rail's voucher/settlement proof — an MPP `SessionReceipt`, an
x402 `SettlementResponse` — proves **money moved** (or was validly
authorized to move) between two addresses/accounts for a claimed amount.
It says nothing about *what work* that amount paid for, nothing about
*who* (which AgentID) actually performed it, and nothing about the
transcript of what was delivered. MAP's `Receipt`/`CheckpointArtifact`
prove the reverse: dual-signed agreement between the two AgentIDs about
usage, outcome, and a transcript root — but on their own carry no proof
that a rail payment for that usage actually settled. `voucher-digest`
(RAIL-1) is the only link between the two; verifying it requires
independently checking the rail's own settlement record. Neither party
should treat one kind of evidence as substituting for the other — a
provider holding a dual-signed Receipt with a `voucher-digest` still has
to check the rail to know it was actually paid, and a payer holding a
settled voucher still has to check the Receipt/CheckpointArtifact to know
what it paid for.

## 6. Other rails (note)

RAIL-18: **Lightning via MPP** — MPP's method-integration list (§0) also
covers Lightning; this document does not separately spec it because its
integration point is identical in shape to Tempo's (§2): a method-specific
session binding this document has not independently read field-by-field.
The RAIL-5 fallback (digest over whatever fields that method binding does
publicly name) applies the same way. **Card rails** (e.g. a
`pay:stripe-intent`-style method, v1 pay §5.1's registry entry, informative
prior art) have no native cumulative-voucher concept at all; if used
under this layer, cadence is necessarily one settlement per checkpoint
(no `upto`/`batch-settlement`-style running total), and `voucher-digest` is
the Multihash of the processor's own intent-confirmation payload.

## 7. Fallback: rails without a voucher

RAIL-19: for a rail with no voucher concept the layer's evidence can still
reference — cash, an out-of-band wire transfer, a manual invoice — the
requester places the Multihash digest of whatever payment proof it does
hold (a bank confirmation PDF's hash, an invoice ID's hash, anything
content-addressable) into `CheckpointCore` key 9 / `ReceiptCore` key 10
(both already typed as an OPTIONAL Multihash "payment reference" per the
interface contract). This carries no rail-specific semantics and this
document defines none for it beyond "it is *a* digest of *something* the
requester considers its payment evidence" — verifying what that evidence
actually proves is entirely out of band and out of MAP's scope, which is
the honest floor this fallback is meant to represent.

## References

MPP, https://mpp.dev/intents/session and
https://mpp.dev/payment-methods/tempo/session (read 2026-09-24; no
in-page revision date). x402 Foundation / Coinbase, `x402-specification-v2.md`
(v2.0, 2025-12-9), `schemes/upto/scheme_upto.md`,
`schemes/batch-settlement/batch_settlement.md`, `transports-v2/http.md`,
`transports-v2/a2a.md` (informative) — all from `coinbase/x402`, read
2026-09-24. `map-v1-ietf:map-spec/draft-map-pay-01.md` §3-§4 (v1, informative — origin
of the Amount/same-exponent/no-rounding conventions this document reuses).
ADR-0001-thin-accountability-layer.md (rationale for treating rails as
external, §"Risks"). [RFC8949] CBOR.
