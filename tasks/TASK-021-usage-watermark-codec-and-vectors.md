# TASK-021 — UsageWatermark codec + TV-016/NV-010 (both impls at parity)

Status: READY · Priority: P2 · Targets: `map-rs` (map-pay, map-core errors,
conformance), `map-spec/test-vectors/` (additive), `map-go` (conformance)
Depends on: TASK-018 (vector process), TASK-015 (Go conformance) ·
Blocks: TASK-022 (metered runtime)

Authored by the 2026-07-14 architect session. The design is NORMATIVE as of
this session's spec patch — read pay §3 (unit codes + same-exponent rule),
pay §4/§4.1 (UsageWatermark CDDL + verification), tasks §3 (kind-3 chunk
content), core §9 (6009) BEFORE this package; they are the ground truth
this package implements. Design rationale: ERRATA.md (RESOLVED 2026-07-14).

---

## 1. Problem Statement

pay §4.1 now defines `UsageWatermark` (PayInvoice key 4) and its billing
arithmetic, closing the last undefined wire surface in the family. This
package makes the definition EXECUTABLE: codecs, the exact-arithmetic
billing helper, two new frozen vectors, and conformance guards in BOTH
implementations — so TASK-022's metered runtime builds on tested parts and
a Go/Rust disagreement about the shape is impossible later. Runtime flows
(incremental invoicing, 6001 pause) are OUT of scope here (TASK-022).

## 2. Functional Requirements

### FR-1 map-pay codec (Rust)

- `UsageWatermark` struct per pay §4: `task: [u8;32]` (1),
  `through_seq: u64` (2), `usage: Usage` (3 — reuse
  `map_tasks::msg::Usage`; if the dep direction forbids map-pay →
  map-tasks, mirror the 4-field struct locally and say so),
  `cumulative_billed: AssetAmount` (4). `to_value`/`from_value`, all four
  keys REQUIRED, round-trip byte-exact through the deterministic encoder.
- `PayInvoice` gains `watermark: Option<UsageWatermark>` (key 4), encoded
  between keys 3 and 5; existing postpaid-flat call sites pass `None` and
  their bytes MUST NOT change (regression-guard with an existing test).
- Unit-code constants per pay §3: `UNIT_FLAT=0, UNIT_TOKEN_IN=1,
  UNIT_TOKEN_OUT=2, UNIT_MS=3, UNIT_INVOCATION=4`.

### FR-2 billing arithmetic (Rust, map-pay)

`billed(model, rate_table, usage) -> Result<AssetAmount, BillingError>`
implementing pay §4.1 exactly:

- model 1: `usage.tokens_in × rate[1] + usage.tokens_out × rate[2]`;
  model 2: `usage.wall_ms × rate[3]`; model 3: `1 × rate[4]`.
- Exact integer arithmetic on decimal mantissas (`u64 counter ×
  i64 mantissa`, checked, into i128 then validated back to the Amount
  mantissa type) — overflow is an error, never a wrap or a rounding.
- Enforce the §3 same-exponent/same-asset rule across every rate the
  model consumes; violation is a `BillingError` (callers map it to 6009
  on the verify side).
- `verify_watermark(agreed, last_accepted, invoice) -> Result<(), u16>`
  returning the specific core §9 code per the pay §4.1 ordered checks
  1/2/3/5/6 (check 4, meter consistency, needs chunk state the runtime
  owns — TASK-022 wires it; document the split).

### FR-3 error code (Rust, map-core)

`codes::INVOICE_WATERMARK_VIOLATION: u16 = 6009` in errors.rs (add-only).

### FR-4 vectors (gen_vectors.py — ADDITIVE ONLY; frozen bytes untouched)

- **TV-016** — PAY_INVOICE (0x0402), signed by Key B, msg-id `msgid(25)`,
  timestamp TS. Content: invoice-id `bytes([0x30])*16`, quote-id
  `bytes([0x10])*16` (the TV-010 quote id), amount = cumulative-billed
  (first invoice), method payload `h''`, timestamp TS, and watermark
  `{1: task_id (the TV-006 TaskID), 2: 8, 3: {1:1523, 2:812, 3:431,
  4:42000}, 4: ["iso4217:USD", 4([-6, 8629000])]}` — the billing model-1
  arithmetic made visible: rates `rate[1]=4([-6,3000])` per token-in,
  `rate[2]=4([-6,5000])` per token-out ⇒ `1523×3000 + 812×5000 =
  8_629_000` micro-USD. State those rates in the vector's prose so a
  verifier can recompute.
- **NV-010** — the same invoice re-sealed with msg-id `msgid(26)` whose
  watermark `cumulative-billed` mantissa is `8629001` (off by one): opens
  fine (well-formed, correctly signed) but MUST be rejected by the pay
  §4.1 arithmetic check — wire class 6009 (a policy reject with a code,
  unlike NV-004/006/008; the conformance assertion is on the arithmetic
  verdict). In-script assert: the good watermark's arithmetic verifies
  and the bad one fails, mirroring NV-001's in-script signature check.
- Update the trailing print summary; regenerate the md (UTF-8); frozen
  TV-001..015/NV-001..009 byte-identical (script asserts + additive-only
  git diff are the guard).

### FR-5 Rust conformance

TV-016: open + `PayInvoice::from_value` round-trips byte-exact (produce
guard, the TV-007 pattern) + `billed(...)` reproduces cumulative-billed +
`verify_watermark` accepts. NV-010: opens, then `verify_watermark`
rejects with 6009. Case-count assertion goes 39 → 41.

### FR-6 map-go conformance

Produce TV-016/NV-010 from seeds in `conformance/produce.go` (raw
`cbor.Map` construction like the other pay vectors — no new Go package),
add both to the parity list (25 → 27 vectors), and add a small
`billedModel1` helper + test in the conformance package asserting the
TV-016 arithmetic and the NV-010 off-by-one rejection — INDEPENDENTLY
written from pay §4.1, not ported from the Rust helper.

### FR-7 determinism

No wall-clock, no randomness anywhere; all constants above are fixed.

## 3. Non-Functional Requirements

Zero new deps in either impl. gen_vectors.py stays self-testing.

## 4–9. Architecture / Design / Data / Algorithms / Edge Cases

The codec lives in map-pay (the pay context owns the shape); arithmetic
lives beside it because verification is arithmetic. Edge cases the tests
MUST pin: mantissa overflow on `u64 × i64` (checked error, not panic);
mixed exponents across rate[1]/rate[2] rejected; watermark with a missing
key rejected at decode; `PayInvoice` without watermark still byte-stable
(postpaid regression); zero usage bills zero (first watermark before any
tokens is legal).

## 10. Error Handling

`verify_watermark` returns wire codes (6009/6007/6005) — never strings —
so TASK-022 and the conformance suite assert identically.

## 11. Security Considerations

The arithmetic IS the security boundary: a provider inflating
cumulative-billed past rate×usage is caught by check 5 (NV-010 is the
executable proof); runaway totals are caught by the cap (6005). Checked
arithmetic prevents an overflow from wrapping into a small "valid" bill.

## 12–14. Performance / IO

Trivial.

## 15. Acceptance Criteria

- ✓ gen_vectors.py runs clean; git diff of the vectors md is additive
  only; TV-016 = the stated bytes-from-seeds; NV-010 differs only in
  msg-id and the one mantissa digit (plus signature).
- ✓ Rust: workspace green, zero warnings; conformance 41 cases;
  postpaid PAY_INVOICE bytes unchanged (regression test).
- ✓ Go: `go test ./...` green; 27 vectors byte-exact; independent
  arithmetic check passes/rejects TV-016/NV-010.
- ✓ `codes::INVOICE_WATERMARK_VIOLATION == 6009` exists and is what the
  Rust verifier returns.

## 16. Comprehensive Test Plan

Codec unit tests (cite pay §4/§4.1 clauses per test), arithmetic edge
cases (§9 list), the two conformance cases per impl, and the postpaid
byte-stability regression.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-021 §FR-1..7, in order. Read FIRST: pay §3 (unit codes,
> same-exponent rule), pay §4/§4.1, tasks §3 (kind-3 content), core §9
> (6009), `map-spec/test-vectors/gen_vectors.py`, and this package.
> Work order: (1) extend gen_vectors.py per FR-4 — additive; verify the
> in-script frozen-byte asserts still pass and `git diff` shows only
> additions; (2) Rust FR-1/2/3/5 (`cargo +stable-x86_64-pc-windows-gnu
> test --workspace` from PowerShell with `C:\msys64\mingw64\bin`
> prepended to PATH); (3) Go FR-6 (`C:\Program Files\Go\bin` on PATH;
> `go vet ./... && gofmt -l . && go test ./...`), writing the Go
> arithmetic check from the SPEC, not from the Rust code (independence
> rule). The watermark's billing constants are pinned in FR-4 — if your
> computed product disagrees with 8_629_000, YOUR arithmetic is wrong;
> re-read pay §4.1. Do not touch runtime flows (map-taskrun) — that is
> TASK-022. Report any spec ambiguity you hit as a PENDING-ARCHITECT
> entry in map-spec/ERRATA.md rather than deciding it yourself. Update
> memory.md §6 (vector counter → TV-016/NV-010) + tasks/README.md row
> 021 on completion; commit + push.

## 18. Validation Checklist (reviewer)

```
cd map-spec/test-vectors && python gen_vectors.py   # asserts pass
git diff --stat map-spec/test-vectors/              # additive only
cargo +stable-x86_64-pc-windows-gnu test --workspace   # (mingw on PATH)
cd map-go && go test ./...                          # 27 vectors
grep -n "6009" map-rs/crates/map-core/src/errors.rs
```

## 19. Known Risks

R1: arithmetic disagreement between the two impls — the shared frozen
TV-016 constant is the arbiter; both must reproduce 8_629_000 from the
same inputs. R2: accidental reflow of frozen vector bytes — script
asserts + diff review. R3: dep-direction snag on `Usage` reuse (map-pay
must not depend on map-tasks if it doesn't already) — FR-1 pre-authorizes
the local mirror, document it.

## 20. Handoff Notes

On green: TASK-022 (metered runtime) is unblocked — it consumes
`UsageWatermark`, `billed`, `verify_watermark`, and code 6009 exactly as
shipped here. Update the CHANGELOG "still open" note (the watermark CDDL
is no longer open).
