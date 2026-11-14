# Issue: bind settlement to signed usage and to the rail voucher

**Labels to suggest:** spec, payments


---

## Problem

A receipt's `settlement` (`paymentRef`, `amount`, `currency`) is a pointer
the parties report, which INAM doesn't verify (§4.1, §10). That's the
right boundary for a registry, but it leaves two gaps for the parties
themselves:

1. Nothing in the signed content records **what was used**: tokens,
   calls, milliseconds. A requester countersigning `amount: "12.50"` has
   no signed basis for checking it, and a later reader can't tell which
   work the amount paid for.
2. Nothing links the receipt to the **specific rail authorization** that
   paid for it. `paymentRef` is free-form (`"x402:tx_88a1"`), so a reader
   holding both the receipt and an x402 or MPP record can't check
   mechanically that they refer to the same delivery. The rail record
   proves money moved, and the receipt proves work was agreed; neither
   proves the other.

## Proposed change

Two optional fields inside `settlement`, which is already in the canonical
content hashed into `receiptId` (§4.2), so both signatures cover them:

```json
"settlement": {
  "paymentRef": "x402:...",
  "amount": "12.50",
  "currency": "USDC",
  "usage": { "tokensIn": 1523, "tokensOut": 812, "calls": 1, "wallMs": 42000 },
  "voucherDigest": "sha256:<hex>"
}
```

- `usage`: cumulative counters, non-negative integers within I-JSON
  range. Rule: a requester SHOULD NOT countersign if `usage` exceeds what
  it observed itself, or if `amount` exceeds the agreed price of `usage`.
- `voucherDigest`: SHA-256 of the rail's own canonical bytes for the
  authorization that paid. For x402 `exact`/`upto` that is the signed
  authorization; for x402 `batch-settlement` and MPP sessions it is the
  cumulative voucher covering `usage`. Never a re-encoding by INAM, so
  anyone can re-derive it from the rail's record. Per-rail byte rules are
  worked out in MAP's [`spec/mapping-payment-rails.md`](https://github.com/jesselarsen/map-rfc/blob/6cc324e/spec/mapping-payment-rails.md),
  including where a rail names fields but defines no canonical byte form.
  For MPP that form is unverified.

The registry's role doesn't change: it validates the shape, never checks
the rail.

## What it breaks

These are new optional fields inside signed content, so receipts that
carry them get a different `receiptId`. Receipts without them are
unchanged. Under GOVERNANCE.md this is a wire-format change: 7-day
comment, SPEC bump, both runtimes and SDKs, and cross-language vectors.

Open question for the maintainer: does `receiptContent.ts` copy
`settlement` through verbatim, or pick known fields? If it picks, older
SDKs would compute a different `receiptId` for a receipt carrying the
new fields. That would decide whether this needs `receiptVersion: "1.1"`.

---

*Context: this comes from MAP, the Mutual Attestation Protocol ([spec](https://github.com/jesselarsen/map-rfc/blob/6cc324e/spec/layer.md), [pre-registered evaluation](https://github.com/jesselarsen/map-rfc/blob/6cc324e/eval/HYPOTHESES.md), [paper draft](https://github.com/jesselarsen/map-rfc/tree/6cc324e/paper)), an Apache-2.0 accountability layer over A2A. I've suspended its development and won't be implementing this. I'm filing it so the design and the evidence are on record here for whoever wants them.*
