# TASK-022 — metered-incremental runtime (terms 3) in map-taskrun

Status: BLOCKED on TASK-021 · Priority: P3 · Target: `map-rs`
(map-taskrun; map-pay/map-tasks consumed as shipped by TASK-021)

Authored by the 2026-07-14 architect session. Normative ground truth:
pay §3 (unit codes, same-exponent), §4/§4.1 (watermark + ordered checks),
tasks §3 (kind-3 = cumulative Usage, CBOR), tasks §8.2 (checkpoints),
core §9 (6001/6005/6007/6009). Go stays vector-parity scope — this
runtime is Rust-only, like the rest of map-taskrun.

---

## 1. Problem Statement

TASK-021 ships the watermark parts; nothing streams money yet. Build the
terms-3 flow on the TASK-007/009 pump architecture: a provider that
meters as it streams and bills at watermarks, and a requester that
verifies every watermark per pay §4.1 (all six checks — including meter
consistency against the kind-3 chunks it actually received) and tops up.
Credit exhaustion pauses with 6001 + final unpaid watermark; CANCEL owes
to the last countersigned checkpoint.

## 2. Functional Requirements

- FR-1 Provider (`MeteredProviderPayment`, payments.rs beside the
  postpaid type): configured with the agreed quote (model 1 in this
  package), a watermark interval (bill every N output tokens; default
  512, matching the checkpoint cadence), and the ledger operator id.
  As the task streams it (a) emits a kind-3 usage-update CHUNK
  (cumulative `Usage`, deterministic CBOR per tasks §3) at or before
  every billing point, then (b) a PAY_INVOICE with a fresh invoice-id,
  the §4.1 delta amount, and the cumulative watermark. Invoices are
  idempotent by id (re-send = identical bytes; reuse the TASK-009
  SettlementLedger discipline).
- FR-2 Requester (`MeteredRequesterPayment`): tracks received kind-3
  chunks (latest cumulative Usage per chunk-seq) and the last ACCEPTED
  watermark; on PAY_INVOICE runs the six §4.1 checks IN ORDER — 1
  presence, 2 task binding (6007), 3 monotonicity, 4 meter consistency
  (its own received-chunk state; this is the check TASK-021 could not
  wire), 5 arithmetic via `map_pay::billed`/`verify_watermark`, 6 cap
  (6005) — replying the specific code on the first failure and paying
  (PAY_PROOF via the LedgerOperator, unchanged from TASK-009) on pass.
  A rejected invoice does not advance the accepted watermark.
- FR-3 Exhaustion: when the requester's ledger balance cannot cover an
  invoice, the provider (on missing/failed proof) pauses the stream and
  sends ERROR 6001 retryable=true with detail `{1 => UsageWatermark}` —
  the final unpaid watermark (core §9 key 5; encode the watermark with
  the TASK-021 codec). On a later PAY_PROOF covering it, the stream
  resumes; on CANCEL, the owed amount recomputes from the last
  countersigned checkpoint's usage (tasks §8.2 key 5) under the same
  billing formula — implement `owed_at_checkpoint()` and test that it is
  ≤ the unpaid watermark's figure.
- FR-4 Wiring: the metered pair composes with the TASK-007 task runtimes
  the same way the postpaid pair does (pump-driven, `Incoming` in,
  `Vec<Outgoing>` out, envelope bytes fed back where I1 digests need
  them per TASK-017). No session/transport changes.
- FR-5 Out of scope: models 2/3 (per-second/per-invocation — the
  arithmetic helper already supports them; the runtime wiring is future),
  prepaid/escrow terms, disputes, Go runtime, live-interop scenarios.

## 3. Non-Functional Requirements

No new deps. Deterministic tests (FixedClock/CountingMsgIds). Zero
warnings.

## 4–9. Architecture / Design / Data / Algorithms / Edge Cases

Mirror the postpaid types rather than generalizing them — two small
explicit state machines beat one parameterized one at this stage
(revisit only when a third terms variant lands). Edge cases the tests
MUST pin: invoice arriving before any kind-3 chunk (fails check 4);
re-sent identical invoice (idempotent: same proof, no double
settlement); watermark equal to the previous (zero-delta invoice —
legal, bills zero, MUST NOT be treated as monotonicity failure);
cap reached exactly (legal) vs exceeded by one mantissa unit (6005);
provider inflating usage beyond its own emitted kind-3 chunk (6009 via
check 4 — the NV-010-style live case).

## 10. Error Handling

Every rejection path emits the specific §4.1 code. 6001 carries the
watermark detail; all other codes carry none.

## 11. Security Considerations

Check 4 is the new trust boundary: the requester only ever pays against
meter readings it has itself received, and the checkpoint anchor bounds
what a dying/malicious stream can claim. The §11-style tests (inflated
bill, replayed invoice, over-cap) are REQUIRED, not optional.

## 12–14. Performance / IO

Per-watermark work is O(1); nothing else matters.

## 15. Acceptance Criteria

- ✓ Workspace green, zero warnings.
- ✓ Happy path: stream → ≥2 incremental invoices → proofs → balances
  move exactly the billed totals → COMPLETE → final invoice settles the
  remainder.
- ✓ All §9 edge cases + §11 adversarial tests present and passing.
- ✓ Exhaustion path: 6001 with decodable watermark detail → top-up →
  resume; and the CANCEL variant owing to the countersigned checkpoint.

## 16. Comprehensive Test Plan

Extend map-taskrun/src/tests.rs in the payments_flow style: a
`metered_leg()` driver mirroring `full_leg()`, plus the named cases.
Cite pay §4.1 check numbers in test comments.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-022 §FR-1..5 in `map-rs/crates/map-taskrun`. Read
> FIRST: pay §4.1 (the six checks are normative and ORDERED), tasks §3
> and §8.2, this package, and the TASK-021 deliverables
> (`map_pay::{UsageWatermark, billed, verify_watermark}`, code 6009) —
> consume them, do not reimplement arithmetic. Model the two new types
> on the existing postpaid pair in payments.rs (same pump conventions,
> same idempotency discipline; envelope bytes via Incoming.raw /
> Session::send returns where a digest needs them). Toolchain:
> PowerShell, `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"`,
> `cargo +stable-x86_64-pc-windows-gnu test --workspace`. Every
> rejection asserts its specific wire code. If the spec under-specifies
> anything you need (e.g. resume signaling after 6001), STOP on that
> item and file a PENDING-ARCHITECT entry in map-spec/ERRATA.md with
> your proposal — do not decide normative behavior yourself. Update
> memory.md §5/§7 + tasks/README.md row 022 on completion; commit +
> push.

## 18. Validation Checklist (reviewer)

```
cargo +stable-x86_64-pc-windows-gnu test --workspace   # (mingw on PATH)
grep -n "6009\|6001\|6005\|6007" map-rs/crates/map-taskrun/src/payments.rs
```

## 19. Known Risks

R1: resume-after-6001 signaling is thin in the spec (the provider
"pauses"; nothing names the resume trigger beyond a covering proof) —
likely PENDING-ARCHITECT material; budget for it. R2: checkpoint state
lives in the task runtime, payment state in the payment types — the
`owed_at_checkpoint` seam must not create a circular dependency; pass
the checkpoint usage IN rather than reaching across. R3: scope creep
into models 2/3 or Go — resist.

## 20. Handoff Notes

After this, the pay family is implemented end-to-end for terms 0 and 3.
Natural next work (no packages yet): prepaid/escrow reordering (§5.2),
disputes (§6), and a live-interop metered scenario once Go grows a task
runtime.
