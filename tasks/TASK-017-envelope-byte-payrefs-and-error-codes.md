# TASK-017 — I1 envelope-byte payment references + ruled error codes

Status: READY · Priority: P1 (interop-breaking bug) · Target: `map-rs`
(crates map-session, map-taskrun; map-pay untouched except doc comments)
Depends on: TASK-009 (payments), TASK-008 (receipts) · Blocks: any live
Rust↔Go interop over the pay flow

Authored by the 2026-07-12 architect session, implementing three ERRATA
rulings (see `map-spec/ERRATA.md`, RESOLVED entries for TASK-009 pay-ref
binding, TASK-008 receipt-refusal code, TASK-009 pay-rejection codes).

---

## 1. Problem Statement

pay §3.1 is normative and STANDS: payment-reference digests are computed
over the **received envelope bytes** of the referenced message (invariant
I1). The Rust runtime instead digests a canonical re-encoding of the
message CONTENT (`payments.rs:96/245`), because `Session::send` returns
`()` and `Session::recv` returns parsed content only — the envelope bytes
never reach the runtime. This is not cosmetic: frozen TV-013 key 2 is the
Multihash of the TV-010b **envelope** bytes, and map-go implements that
form, so today the Rust pay flow computes digests no conformant peer will
ever match. Fix the seam, then the digests.

Secondarily, wire the two error-code rulings: receipt countersignature
refusal is 2006 (not 5000), and the four pay rejections get their own
codes 6005–6008 (not blanket 6002).

## 2. Functional Requirements

- FR-1 map-session seam: `Session::send` returns the SEALED envelope
  bytes it wrote (`Result<Vec<u8>, SessionError>`); `Incoming` gains
  `pub raw: Vec<u8>` — the exact received envelope frame, byte-for-byte,
  before any decoding. No re-serialization anywhere (I1).
- FR-2 Requester pay flow (payments.rs): the stored quote digest becomes
  `pay_ref_digest(&incoming.raw)` of the received PAY_QUOTE envelope;
  the PAY_AGREE it emits carries that digest (pay §3.1).
- FR-3 Provider pay flow: the provider retains the sealed bytes returned
  by `send` for its own PAY_QUOTE and verifies the incoming PAY_AGREE's
  digest against THOSE bytes. Mismatch ⇒ ERROR **6006**, retryable false.
- FR-4 Code rewiring, exactly these mappings (core §9 as patched
  2026-07-12):
  - receipts.rs `RECEIPT_REJECT_CODE`: `codes::TASK_FAILED` → new
    `codes::COUNTERSIGN_REFUSED = 2006` (add the constant to
    map-core `errors.rs`; code points are immutable, add-only).
  - payments.rs `PAY_REJECT` splits: cap exceeded / exponent mismatch ⇒
    **6005**; agreement/quote digest mismatch (price switch) ⇒ **6006**;
    task-binding mismatch or required-binding absence ⇒ **6007**;
    forged/mismatched ledger entry or proof ⇒ **6008**; anything else
    (e.g. unknown invoice id, no active agreement) stays **6002**.
    Add constants `CAP_EXCEEDED=6005`, `AGREEMENT_DIGEST_MISMATCH=6006`,
    `TASK_BINDING_MISMATCH=6007`, `INVALID_PAYMENT_PROOF=6008` to
    map-core errors.rs. Expired quote stays 6000; unoffered method 6004.
- FR-5 `map_pay::codec::pay_ref_digest` keeps its signature (it already
  takes `&[u8]`); update its doc comment to say "received envelope
  bytes" and delete any content-encoding call sites.
- FR-6 Tests: (a) a session-level test that `recv().raw` equals the
  bytes the peer's `send` returned; (b) the pay happy path binds the
  received envelope digest end-to-end (assert the digest in the emitted
  PAY_AGREE equals `pay_ref_digest` of the quote envelope bytes captured
  from the wire, NOT of a re-encoding); (c) the TASK-009 §11 security
  tests updated: price-switch now asserts ERROR 6006, cap-exceeded 6005,
  binding-mismatch 6007, forged-proof 6008, receipt-refusal 2006;
  (d) a regression test that a content-re-encoding digest NO LONGER
  matches (guards against reintroduction).

## 3. Non-Functional Requirements

No new dependencies. `Vec<u8>` clones only where a digest is actually
retained (do not hold every frame). Zero warnings. All existing tests
green after the assertion updates — behavior changes are ONLY the digest
source and the error codes.

## 4. System Architecture

The seam change is deliberately at the session boundary: map-session
already owns the exact frames (it framed/unframed them); exposing them
is strictly additive and keeps map-pay/map-taskrun free of any encoder
coupling. Do NOT thread raw bytes through map-tasks codecs — only the
pay flows and (later) receipt evidence need them.

## 5. Component Design

`Incoming { msg_type, content, msg_id, seq, raw }`. `send` computes the
sealed frame once, writes it, returns it (no double-seal). Provider-side
`PaymentProvider` (or the equivalent struct in payments.rs) stores
`quote_envelope: Vec<u8>` at quote-send time, replacing the stored
re-encoding digest.

## 6. Interface Specifications

Breaking signature change `send(...) -> Result<Vec<u8>, SessionError>`
is accepted (workspace-internal; update the ~handful of call sites, most
ignore the return). Everything else additive.

## 7–9. Data / Algorithms / Edge Cases

Digest = `pay_ref_digest(received_envelope_bytes)` (multihash sha2-256,
unchanged helper). Edge cases: PAY_QUOTE re-received (msg-id replay is
already rejected upstream — no special handling); quote sent before
session established (not possible: pump-driven); zero-length raw is
impossible (framed).

## 10. Error Handling

Every rejection path names one specific code from FR-4; no path may emit
a code outside {6000, 6002, 6004, 6005, 6006, 6007, 6008, 2006} in the
pay/receipt flows. retryable = false for 2006/6005/6006/6007/6008.

## 11. Security Considerations

This closes the actual gap the price-switch defense was designed for:
content-binding let two byte-different envelopes with identical content
(e.g. differing unknown extension bytes preserved under I3) carry the
same digest. Envelope-binding is exact. The regression test in FR-6(d)
is the guard.

## 12–14. Performance / IO

One extra Vec per retained quote; negligible.

## 15. Acceptance Criteria

- ✓ `cargo +stable-x86_64-pc-windows-gnu test --workspace` green, zero
  warnings (mingw on PATH — memory.md §5).
- ✓ FR-6(b): PAY_AGREE digest == digest of wire-captured quote envelope.
- ✓ FR-6(d) regression test present and passing.
- ✓ All five code rewirings observable in tests (2006, 6005–6008).
- ✓ No call site of `pay_ref_digest` passes re-encoded content.

## 16. Comprehensive Test Plan

Extend `map-taskrun/src/tests.rs` (existing §11 security tests) and
map-session's tests. Cite the ruling ("ERRATA 2026-07-12" + pay §3.1 /
core §9) in each new/changed test comment.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs`, implement TASK-017 §FR-1..6 exactly. Read FIRST:
> `map-spec/ERRATA.md` (the three RESOLVED 2026-07-12 entries for pay-ref
> binding and error codes), pay §3.1, core §9 (as patched), and
> `crates/map-taskrun/src/payments.rs` + `receipts.rs` +
> `crates/map-session/src/session.rs`. Change `Session::send` to return
> the sealed envelope bytes and add `Incoming.raw` (exact received frame).
> Re-bind ALL payment-reference digests to envelope bytes (received on
> the digesting side; retained-sent on the verifying side). Add error
> constants 2006/6005/6006/6007/6008 to map-core errors.rs and rewire
> `RECEIPT_REJECT_CODE` and the `PAY_REJECT` sites per the FR-4 mapping —
> each rejection path gets its specific code. Update existing test
> assertions; add the four new tests of FR-6. Do NOT touch vector files
> or map-go. Build/test per §15 (PowerShell, mingw + GNU toolchain on
> PATH). Report any call site where the raw bytes are genuinely
> unavailable rather than working around it with a re-encoding.

## 18. Validation Checklist (reviewer)

```
$env:PATH="C:\msys64\mingw64\bin;$env:PATH"
cargo +stable-x86_64-pc-windows-gnu test --workspace
grep -rn "pay_ref_digest" crates | grep -v envelope   # no content sites
grep -rn "TASK_FAILED" crates/map-taskrun/src/receipts.rs   # none
```

## 19. Known Risks

R1: a hidden call site still digesting content — the FR-6(d) regression
test plus the grep in §18 catch it. R2: `send` signature ripple — small,
workspace-internal. R3: tests that asserted 6002/5000 will fail until
updated — that is the point; update assertions, not codes.

## 20. Handoff Notes

Update memory.md §5 (session seam + digest binding) and tasks/README.md
row 017 on completion; note in the PR that Rust pay-flow digests are now
interop-compatible with map-go and TV-013. Future: PayAgreeRef /
EVIDENCE flows can now also bind envelope bytes — out of scope here.
