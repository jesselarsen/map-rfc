# TASK-009 — Payments MVP: `pay:credit-ledger`

Status: READY (after TASK-007; parallel with TASK-008) · Priority: P2 ·
Target: extend `map-pay` + `map-taskrun`
Depends on: TASK-007 · Blocks: nothing

---

## 1. Problem Statement

Payment negotiation (pay §2–§4) has wire definitions and a settle-once
ledger but no message flow. This task implements the postpaid happy path
with the simplest settlement method, `pay:credit-ledger` (pay §5.1: a
ledger-operator-signed entry is the proof), threaded through TASK-007's
task flow: PAY_QUOTE (task-bound, key 9) → PAY_AGREE → task runs →
PAY_INVOICE → PAY_PROOF → PAY_RECEIPT. Metered-incremental (terms 3),
escrow, and disputes stay future.

## 2. Functional Requirements

- FR-1 Codecs in `map-pay` (pure, per pay §3/§3.1/§4 CDDL exactly):
  PayQuote (incl. ?9 TaskID binding), PayAgree, PayQuoteRef, PayAgreeRef,
  PayRef, PayInvoice, PayProof, PayReceipt, MethodOffer.
- FR-2 Task binding rule (pay §3): quoting against an OFFER with TaskID
  → key 9 MUST be present and equal; requester rejects mismatch/absence
  in that case. Standing quotes (no key 9) remain agreeable.
- FR-3 Refs are (id, digest-of-received-envelope-bytes) (pay §3.1):
  compute with `map_discovery::multihash_sha256` over the FULL received
  envelope bytes; `Quote.2 = PayQuoteRef`, `Assign.6 = PayAgreeRef`
  (TASK-007 codecs already carry the options — wire them now).
- FR-4 CreditLedger method: `method URI "pay:credit-ledger"`; an
  in-process `LedgerOperator { kp }` maintains balances and, on
  `pay(invoice-id, payer, payee, amount)`, emits a signed entry
  `enc(["MAPLedgerEntry", invoice-id, payer, payee, [asset, amount],
  ts])` + Ed25519 signature — that signature is the PAY_PROOF payload
  (field 3); the payee verifies it against the operator's known AgentID.
- FR-5 Flow runtimes (pump-driven, in `map-taskrun::payments`):
  provider issues PAY_QUOTE (flat model 0, one rate entry) alongside
  QUOTE; requester PAY_AGREEs (choosing credit-ledger, authorized cap =
  quote total); after Verified COMPLETE, provider sends PAY_INVOICE
  (amount = quoted flat total; idempotent by invoice-id); requester pays
  via LedgerOperator, sends PAY_PROOF; provider verifies entry sig +
  amount + invoice binding, `SettlementLedger::settle_once`, replies
  PAY_RECEIPT {1 invoice-id, 2 amount, 3 ts, ?4 proof digest}.
- FR-6 Authorized-cap enforcement: invoice amount > agreed cap →
  requester refuses to pay, sends ERROR 6xxx (check errors.rs range) —
  the runaway-cost defense (pay §3).
- FR-7 Idempotency: re-sent PAY_INVOICE → same PAY_PROOF re-sent;
  re-sent PAY_PROOF → settle_once returns AlreadySettled, same
  PAY_RECEIPT re-sent; no double balance movement (test on the
  operator's balances).

## 3. Non-Functional Requirements

Pinned deps only; `Amount` arithmetic: same-exponent addition only
(reject mixed exponents rather than normalizing — scaffold rule,
document it); zero warnings; pump-driven, injected clocks.

## 4. System Architecture

Codecs + LedgerOperator in `map-pay` (the pay bounded context owns
settlement-method semantics); flows in `map-taskrun::payments`
(add map-pay to its deps — direction legal).

## 5. Component Design

```
crates/map-pay/src/        (extend)
  codec.rs       FR-1 to_value/from_value
  credit_ledger.rs  LedgerOperator (balances: HashMap<AgentId, i128>,
                 entry signing/verification), EntryProof type
crates/map-taskrun/src/
  payments.rs    provider_pay_flow / requester_pay_flow state machines,
                 composing with the TASK-007 runtimes via events
```

## 6. Interface Specifications

`LedgerOperator::pay` preconditions: payer balance ≥ amount (else
`Insufficient`), invoice-id unseen (else returns the ORIGINAL entry —
idempotent). Entry verification: `verify_strict` against operator
AgentID; amount/parties/invoice must match the invoice being settled.
All flows return Outgoing lists; terminal payment states sticky.

## 7. Data Structures

Balances in minor units as i128 mantissa at a fixed per-asset exponent
(operator config); `EntryProof = { entry_bytes, sig }` serialized as
CBOR array for the PAY_PROOF field-3 payload.

## 8. Algorithms

Digest-binding chain: quote-envelope digest → PayQuoteRef → PayAgree.2
must equal it (verify on the provider before honoring the agreement);
invoice references quote-id (pay §4 PayInvoice.2); proof references
invoice-id. Each link checked with received bytes.

## 9. Edge Cases

PAY_AGREE for an expired quote → reject (clock past PayQuote.6).
PAY_AGREE digest mismatch (tampered/different quote) → ERROR, no
agreement. Invoice before COMPLETE → provider bug; requester rejects
(state). Insufficient balance → requester surfaces Insufficient, no
PAY_PROOF, provider may re-invoice later (idempotent). Amount exponent
mismatch between quote and invoice → reject.

## 10. Error Handling

Every rejection carries the pay-range wire code (6xxx per errors.rs);
missing code points for specific pay failures → report + propose
(candidate: distinct codes for cap-exceeded vs digest-mismatch).

## 11. Security Considerations

Tests MUST cover: forged ledger entry (wrong operator key) rejected;
proof replayed against a different invoice rejected (invoice binding);
task-binding mismatch (FR-2) rejected; cap-exceeded refusal (FR-6);
quote-digest switch (§8) rejected — this is pay §7's price-switch
defense made executable.

## 12. Performance Considerations

None. Note: LedgerOperator is trusted third-party BY DESIGN for this
method (pay §5.1 "private mesh ledger") — the trust tradeoff is the
method's property, priced by the payee; say so in doc comments.

## 13. Input Specifications

Pump-driven Incoming + operator handle + injected clock.

## 14. Output Specifications

Outgoing lists; settled state observable via SettlementLedger +
operator balances; PAY_RECEIPT bytes retained (feeds
ExecutionReceipt.payment-ref in a future package — expose them).

## 15. Acceptance Criteria

- ✓ Workspace green, zero warnings, no new deps.
- ✓ End-to-end test: TASK-007 echo + full FR-5 payment leg; balances
  moved exactly once; PAY_RECEIPT verifies.
- ✓ All §11 security tests pass.
- ✓ FR-7 idempotency tests pass (both retry directions).
- ✓ pay §3 task-binding MUST enforced (mismatch test).
- ✓ Spec patches recorded for any missing 6xxx codes used.

## 16. Comprehensive Test Plan

Unit: all codecs roundtrip (optional fields present/absent); Amount
same-exponent guard; operator pay/insufficient/idempotent; entry
verify (good/forged/mismatched-amount).
Integration: end-to-end; retries both directions; expired quote; digest
switch; cap exceeded; task-binding mismatch; standing quote (no key 9)
agreeable path.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, extend `crates/map-pay` (codec.rs, credit_ledger.rs) and
> `crates/map-taskrun` (payments.rs; add map-pay dep) per §4–§14. CDDL
> field numbers from `map-spec/draft-map-pay-00.md` §3/§3.1/§4 — verify
> before coding; STOP and report gaps (candidates: PayInvoice.4 usage-
> watermark exact shape — omit if unclear and report; distinct 6xxx
> codes). Digests over received envelope bytes only (I1). Idempotency is
> normative (ops §2 rule 3): retried invoices/proofs move money at most
> once — prove it on operator balances. `cargo
> +stable-x86_64-pc-windows-gnu` from PowerShell; full suite per fix.
> Deliver: code, tests per §16, spec-patch notes, report.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo +stable-x86_64-pc-windows-gnu test --workspace
cargo test -p map-pay -p map-taskrun -- --list | wc -l   # >= 25 combined
```

## 19. Known Risks

R1: PayInvoice.4 (usage watermark) is loosely specified — omitted here
(postpaid flat); metered terms 3 future. R2: Amount arithmetic
simplification (same-exponent) may need revisiting for real methods —
documented constraint, not silent.

## 20. Handoff Notes

Update memory.md §5/§8 (step 5 done). Future pay packages: metered
terms 3 + FLOW/watermark interplay, escrow (5.2), disputes (§6), and a
bound-PAY_QUOTE + payment-flow test vector set (extend TASK-011's vector
work if not already landed).
