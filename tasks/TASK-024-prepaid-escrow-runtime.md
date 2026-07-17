# TASK-024 — Prepaid (terms 1) + escrow (terms 2) settlement: codecs, credit-ledger escrow, runtime pairs, vectors

Status: READY · Priority: P3 · Target: `map-rs` (map-pay, map-taskrun,
conformance, gen_vectors.py) + `map-go` (codec parity only)
Depends on: TASK-021 (pay codecs + billing), TASK-022 (runtime pump shape),
TASK-009 (credit-ledger operator) · Blocks: disputes (pay §6) design —
escrow release/refund artifacts are dispute evidence.

Authored by the 2026-07-16 architect session. Normative ground truth:
**pay §5.2 (rewritten in the same session — READ IT FIRST; §5.2.1–§5.2.4
define every MUST below)**, pay §3 (quote key 4 now REQUIRED under terms
1/2), pay §4.1 (watermark presence — key 4 absent under terms 1/2),
core §9 (new code 6010), tasks §4/§8 (COMPLETE + transcript-root, the
release trigger), tasks §8.2 (countersigned checkpoint, the CANCEL
anchor). The CHANGELOG entry dated 2026-07-16 is the delta summary.

---

## 1. Problem Statement

PayQuote terms codes 1 (prepaid) and 2 (escrow) have existed since -00
but had no defined flow; §5.2 was a stub. The spec now defines both. Build
them end to end in the reference stack: the two additive wire fields, the
6010 error code, the credit-ledger escrow extension (the reference
escrow-capable rail), requester/provider runtime pairs mirroring the
TASK-022 metered architecture, and the three additive vectors — WITHOUT
touching frozen bytes, adding dependencies, or introducing message types.

## 2. Functional Requirements

### FR-1 codec: PayInvoice.7 phase, PayReceipt.5 kind (map-pay::codec)

- `PayInvoice` gains `phase: Option<u64>` (key 7). Encode omits when
  `None` (existing invoices byte-identical — regression-test exactly as
  TASK-021 did for key 4). Decode accepts absent or `1 | 2`; any other
  value is a decode error (fail-closed).
- `PayReceipt` gains `kind: Option<u64>` (key 5). Encode omits when
  `None`. Decode accepts absent or `0..=3`; anything else is a decode
  error (this is what NV-011 pins).
- Constants: `PHASE_HOLD: u64 = 1`, `PHASE_RELEASE: u64 = 2`,
  `KIND_SETTLED: u64 = 0`, `KIND_HELD: u64 = 1`, `KIND_RELEASED: u64 = 2`,
  `KIND_REFUNDED: u64 = 3`, and `TERMS_PREPAID: u64 = 1`,
  `TERMS_ESCROW: u64 = 2` (alongside the existing terms-3 usage).

### FR-2 error code (map-core::errors)

`pub const ESCROW_VIOLATION: u16 = 6010;` with a doc comment quoting the
core §9 referent list (phase presence, prepaid/hold amount ≠ quoted
total, premature/oversized release, duplicate hold, unrecognized receipt
kind).

### FR-3 verification helper (map-pay, new module `escrow.rs`)

The §5.2.3 six-check requester procedure as a pure function, mirroring
`billing::verify_watermark`'s shape (specific wire code out, first
failure wins, IN ORDER):

```rust
pub struct EscrowContext {
    /// The accepted hold (None until a hold invoice was accepted+settled).
    pub held: Option<AssetAmount>,
    /// COMPLETE received AND transcript-root verified (tasks §4) — the
    /// §5.2.3 canonical release trigger. Caller-supplied: runtime state.
    pub complete_verified: bool,
    /// If the task ended by CANCEL: the last countersigned checkpoint's
    /// usage (tasks §8.2 key 5) — the §4.2 cancel anchor.
    pub cancel_checkpoint_usage: Option<Usage>,
}
pub fn verify_escrow_invoice(agreed: &AgreedTerms, ctx: &EscrowContext,
    invoice: &PayInvoice) -> Result<(), u16>
```

Checks (each returning the exact code): (1) phase presence iff
`agreed.terms == 2`, value ∈ {1,2} → 6010; (2) task binding via
`agreed.task` when the invoice carries a watermark — under terms 2 it
never does, so bind via the agreement context as TASK-022's requester
does → 6007; (3) hold: amount == the quote key 4 total (add
`quoted_total: Option<AssetAmount>` to `AgreedTerms` — additive field,
existing constructors updated) and `ctx.held.is_none()` → 6010;
(4) release: `ctx.complete_verified` (or a cancel checkpoint exists) →
6010; (5) release amount: same-asset/exponent `<=` vs `ctx.held`, and if
`cancel_checkpoint_usage` is set, amount == `billed(model, rates, usage)`
exactly → 6010; (6) cap: cumulative outflow ≤ `authorized_cap` → 6005.
Terms-1 invoices (no phase key) verify amount == quoted total → 6010.
Reuse `billing::le` semantics (asset+exponent mismatch = violation, never
conversion).

### FR-4 credit-ledger escrow (map-pay::credit_ledger)

Extend `LedgerOperator` per §5.2.4 (all idempotent by the governing
invoice-id, mirroring `pay()`):

- `hold(hold_invoice_id, payer, payee, aa, expiry_ts, now_ts) ->
  Result<EntryProof, LedgerError>` — moves balance into an escrow bucket
  keyed by hold-invoice-id; emits the `MAPLedgerHold` entry. New
  `LedgerError::HoldExpiredOrMissing` and `LedgerError::AuthInvalid`
  variants as needed.
- `release_auth_bytes(hold_id, release_id, aa) -> Vec<u8>` (free fn) —
  `enc(["MAPLedgerReleaseAuth", hold-id, release-id, [Asset,Amount]])`;
  the PAYER signs these bytes with its own Keypair (inner signature,
  decision-9 style — not an envelope).
- `release(release_invoice_id, hold_invoice_id, payer_auth_sig, aa,
  now_ts) -> Result<EntryProof, LedgerError>` — verifies the payer sig
  over exactly this triple against the payer AgentID recorded in the
  hold; requires `aa <= held` and `now_ts < expiry`; atomically credits
  payee, refunds remainder to payer, closes the hold; emits
  `MAPLedgerRelease`.
- `refund(hold_invoice_id, payee_refund_auth: Option<&[u8; 64]>, now_ts)
  -> Result<EntryProof, LedgerError>` — allowed iff expiry passed OR a
  valid payee sig over `enc(["MAPLedgerRefundAuth", hold-id])`; returns
  held funds; emits `MAPLedgerRefund`.
- Payee-side verifiers mirroring `verify_entry`: `verify_hold_entry` /
  `verify_release_entry` (operator sig + field equality against the
  respective invoice and, for release, the hold).

### FR-5 runtime pairs (map-taskrun, new module `escrow.rs`)

On the TASK-022 pump architecture (Incoming→Vec<Outgoing>, state enums,
`bind_quote_envelope`, envelope-byte digests):

- `PrepaidProviderPayment` / `PrepaidRequesterPayment` (terms 1): quote
  (terms 1, key 4 REQUIRED) → agree → single invoice == quoted total →
  proof (existing `pay()` rail) → receipt (kind absent = settled).
  Provider exposes `execution_permitted() -> bool`, false until settled —
  the §5.2.2 gate the task pump consults before starting billable work.
- `EscrowProviderPayment` / `EscrowRequesterPayment` (terms 2): the full
  §5.2.3 two-round flow. Provider: `invoice_hold()` after agree;
  `on_hold_proof` verifies via FR-4 → emits kind-1 receipt; exposes
  `execution_permitted()` (true once held); after the pump reports
  COMPLETE sent, `invoice_release(final_owed)`; `on_release_proof` →
  kind-2 receipt. Requester: runs FR-3 on every invoice; on a hold
  invoice places the hold (FR-4) and sends PAY_PROOF; on a valid release
  invoice signs `release_auth`, obtains the release entry from the
  operator, sends PAY_PROOF; `set_complete_verified()` /
  `set_cancel_checkpoint(usage)` feed the FR-3 context. Requester `new()`
  takes the requester `Keypair` (it must sign release-auths).
- CANCEL path: `owed_at_checkpoint`-style helper computing the release
  amount from checkpoint usage via `map_pay::billed` (mirror
  `MeteredProviderPayment::owed_at_checkpoint`).

### FR-6 vectors (additive; frozen bytes untouched)

In `gen_vectors.py`, Rust conformance, and map-go:

- **TV-017** — a terms-2 hold PAY_INVOICE (key 7 = 1, no key 4). Both
  impls produce byte-exact + consume.
- **TV-018** — a PAY_RECEIPT with key 5 = 1 (held). Ditto.
- **NV-011** — TV-018's bytes with key 5 = 9 (unrecognized kind), fresh
  msg-id, re-signed → both impls REJECT with the 6010 class (mirror the
  NV-010 construction recipe exactly).

Rust conformance 41 → 44 cases; map-go 27 → 30 vectors. map-go work is
CODEC ONLY (the two optional fields + fail-closed ranges + vectors); no
Go runtime (that is the separate "Go task runtime" frontier item).

### FR-7 end-to-end tests (map-taskrun)

1. Prepaid happy path: quote→agree→invoice→proof→receipt→
   `execution_permitted()` flips true; invoice ≠ quoted total → 6010 and
   no payment.
2. Escrow happy path: hold round → kind-1 receipt → execution → complete
   → release round (release < held) → kind-2 receipt AND the operator
   refunded the remainder to the payer in the same operation (balance
   assertions).
3. Premature release: release invoice before `set_complete_verified()` →
   6010, no auth signed.
4. Oversized release: amount > held → 6010.
5. Duplicate hold invoice (fresh id, same agreement) → 6010.
6. CANCEL: `set_cancel_checkpoint(usage)` → only the exact
   checkpoint-billed amount releases; wrong amount → 6010.
7. Expiry: `refund` after expiry succeeds without payee auth; `release`
   after expiry fails at the operator; pre-expiry refund without payee
   auth fails; with payee auth succeeds.
8. Authorization symmetry negatives: forged/mismatched payer release-auth
   (wrong amount, wrong release-id, wrong signer) → operator
   `AuthInvalid`, payee `verify_release_entry` false → 6008.
9. Idempotency: re-sent hold proof / release proof settles nothing twice
   (receipts and balances unchanged) — reuse the TASK-022 resent-proof
   test shape.

## 3. Non-Functional Requirements

Zero new dependencies (default and only build). Zero warnings. No panics
on wire input; checked arithmetic only (i128 intermediates as
billing.rs). Frozen vectors byte-identical (regression: encode an
existing postpaid PayInvoice/PayReceipt and compare against a pinned
constant, as TASK-021's key-4 regression did).

## 4. System Architecture

Everything lands in existing crates: map-pay owns wire truth + ledger
rail; map-taskrun owns flow sequencing; conformance owns vectors. No new
message types (§5.2 doctrine), no new crates, no session/net changes.
Escrow is two standard invoice rounds — the runtime composes existing
machinery rather than forking it. Do NOT generalize the payment runtimes
over settlement methods (the third-method altitude rule still holds;
credit-ledger is hardwired here exactly as in TASK-022).

## 5–9. Design / Data / Algorithms / Edge Cases

Edge cases the tests MUST pin (beyond FR-7): terms-1/2 quote missing
key 4 → requester rejects at quote time (6010 class, before agree);
phase key present under terms 0/1/3 → 6010; release with
`cancel_checkpoint_usage` set AND `complete_verified` true → COMPLETE
wins (full-owed release; cancel anchor applies only when the task ended
by CANCEL); hold amount with mismatched exponent/asset vs quoted total →
6010 (never convert); operator `release` where `aa == held` exactly →
remainder 0, no refund entry needed, balances exact.

## 10. Error Handling

Ledger-internal errors stay typed (`LedgerError`); requester/provider
surfaces emit wire codes exactly as §5.2 assigns: 6010 for §5.2 check
failures, 6007 task binding, 6005 cap, 6008 forged/mismatched entries,
6002 rail-failure passthrough (e.g. post-expiry release attempt).

## 11. Security Considerations

The two §5.2.3 authorization-symmetry MUSTs are the substantive surface —
test both directions (FR-7 #7, #8). The payer's release-auth is an inner
signature over exact bytes (decision-9 style): pin the byte layout with a
fixed-output test so Go can twin it later. The release trigger
(`complete_verified`) is requester-LOCAL state — the provider never
learns why a release wasn't authorized except via dispute; do not leak a
new signal.

## 12–14. Performance / IO

Irrelevant; all in-process.

## 15. Acceptance Criteria

- ✓ Workspace green, zero warnings; map-go `go test ./...` + gofmt/vet
  green.
- ✓ Frozen vectors byte-identical; Rust conformance 44; map-go 30.
- ✓ FR-7 tests 1–9 + §5–9 edge list green; 6010 exercised for every
  referent in the core §9 description.
- ✓ No new deps (`cargo tree` unchanged); no new message types; map-pay
  postpaid/metered tests untouched and green.
- ✓ tasks/README.md row 024 + memory.md §5/§8 updated on completion.

## 16. Comprehensive Test Plan

Codec round-trips + absent-key byte-regression (FR-1); NV-011 reject in
both impls; ledger unit tests per FR-4 op (incl. idempotency + balance
conservation: sum of all balances + escrow buckets is invariant);
release-auth fixed-bytes test; FR-7 integration suite; TV-017/018
produce+consume both impls.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-024 §FR-1..7 in `map-rs` (map-pay, map-taskrun,
> conformance, map-spec/test-vectors/gen_vectors.py) and `map-go` (codec
> parity + vectors only). Read FIRST: map-spec/draft-map-pay-01.md §5.2
> (all four subsections — every MUST there is an acceptance test), the
> 2026-07-16 CHANGELOG entry, core §9 (code 6010), and this package.
> Mirror TASK-021's additive-vector recipe (gen_vectors.py appends; frozen
> bytes untouched — verify by re-running the full vector suite in BOTH
> impls) and TASK-022's runtime pump shape for the new pairs. Do NOT add
> dependencies, message types, or method-generic runtime abstractions.
> Toolchain: PowerShell, `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"`,
> `cargo +stable-x86_64-pc-windows-gnu test --workspace`; Go on PATH via
> `C:\Program Files\Go\bin`, `go test ./...` in map-go. If any §5.2 text
> is ambiguous or contradicts an existing test, STOP and record a
> PENDING-ARCHITECT entry in map-spec/ERRATA.md rather than guessing.
> Update memory.md §5/§8 + tasks/README.md row 024 on completion; commit +
> push.

## 18. Validation Checklist (reviewer)

```
cargo +stable-x86_64-pc-windows-gnu test --workspace       # green, 0 warn
cd map-go && go test ./...                                  # green
grep -n "6010\|ESCROW_VIOLATION" map-rs/crates/map-core/src/errors.rs
grep -n "MAPLedgerHold\|MAPLedgerReleaseAuth" map-rs/crates/map-pay/src/credit_ledger.rs
python map-spec/test-vectors/gen_vectors.py                 # frozen bytes stable
```

## 19. Known Risks

R1: the release trigger is requester-local (`complete_verified`) — a
tempting shortcut is auto-trusting COMPLETE without transcript-root
verification; the FR-7 #3 test must construct a received-but-unverified
COMPLETE state to pin the distinction. R2: balance-conservation bugs in
release-with-remainder (an atomic three-way move) — the invariant test in
§16 is mandatory, not optional. R3: `AgreedTerms` grows a field
(`quoted_total`) used by three runtimes — update all constructors in one
change and let the compiler find call sites; do not default it. R4: scope
creep toward disputes (pay §6) — the kind-3 refund receipt and dispute
wiring are OUT; only emit what §5.2 defines.

## 20. Handoff Notes

After this, all four terms codes (0/1/2/3) are implemented and the pay
family's §5.2 stub debt is retired. The escrow artifacts (kind-1/2
receipts, hold/release entries, release-auths) are precisely the evidence
set pay §6 disputes consume — disputes (the natural next package) can now
be designed against concrete types. Remaining unpackaged frontier:
disputes (pay §6), ops §5 x402-gateway composition, method-generic
runtimes (the third-method trigger is now closer: credit-ledger escrow is
still the same method — a structurally different rail is what triggers
it), Go task runtime + metered/escrow interop.
