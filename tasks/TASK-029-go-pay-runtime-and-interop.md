# TASK-029 — Go pay runtime (terms 0/1/3) + live pay interop cells

Status: READY · Priority: P2 · Target: `map-go` (new package `pay`;
`cmd/map-interop`), `map-rs/crates/map-cli` (interop pay scenarios),
`interop/run-matrix.ps1` + `MATRIX.md`
Depends on: TASK-009/021/022/024 (the Rust pay reference being ported),
TASK-020 extension (libp2p task cells — the harness pattern) ·
Blocks: TASK-030 (paid demo needs the Go prepaid requester).

Authored by the 2026-07-18 architect session, from the approved D-milestones
plan (`tasks/PLAN-D-milestones.md` §TASK-029
— the plan text is normative for scope; this file adds the execution contract).
Normative wire ground truth: pay §1–§5.2 and the Rust reference
`map-rs/crates/map-pay/src/{codec,billing,credit_ledger}.rs` +
`map-rs/crates/map-taskrun/src/{payments,metered,escrow}.rs` (byte behavior;
the INDEPENDENCE rule is relaxed here exactly as it was for map-go's session
layer: the spec governs, the Rust source may be consulted for sequencing, but
the frozen conformance vectors are the arbiter of bytes).

---

## 1. Problem Statement

The pay family exists only in Rust; map-go has zero pay code. Ops §6.1's
two-implementation bar is therefore unmet for the family the drafts feature
most. Port the codecs, the credit-ledger method, and the terms-0/1/3 runtimes
to Go — byte-compatible — and prove it LIVE: `pay-postpaid` and `pay-metered`
cells in the interop matrix, both orientations (matrix 22/22 → 26/26).

## 2. Functional Requirements

FR-1..FR-9 are the nine numbered items of the approved plan §TASK-029,
verbatim — implement them as written there. Binding clarifications:

- **FR-1 (codec) key layouts** are frozen by the Rust CDDL comments in
  `map-pay/src/codec.rs` and pinned by the conformance vectors: PayQuote keys
  1–9 (terms=7, task-binding=9), PayAgree 1–4, PayInvoice 1–7 (watermark=4
  optional, phase=7 fail-closed ∈{1,2}), PayProof 1–3, PayReceipt 1–5
  (proof-digest=4 optional, kind=5 fail-closed ≤3), UsageWatermark 1–4 all
  required, MethodOffer 1–2, Amount = CBOR tag 4 [exp, mantissa],
  AssetAmount = [tstr, Amount]. `PayRefDigest = 0x12 0x20 ‖ SHA-256(bytes)`
  over RECEIVED ENVELOPE BYTES (`Incoming.Raw` inbound; `session.Send`'s
  returned sealed frame outbound) — never a content re-encoding (ERRATA
  2026-07-12 / TASK-017).
- **FR-2 (billing)**: same six-check order and wire codes as
  `billing::verify_watermark` (presence→6009, task→6007, monotonicity→6009,
  meter-consistency is the CALLER's check as in Rust, arithmetic/delta→6009,
  cap→6005). Arithmetic exact; overflow is an error, never a wrap
  (`math/big` intermediates are acceptable — stdlib).
- **FR-3 (ledger)**: `entry_bytes = enc(["MAPLedgerEntry", invoice-id, payer,
  payee, [asset,amount], ts])`, operator ed25519 over those bytes; `Pay`
  idempotent by invoice-id returns the ORIGINAL entry with no balance
  movement. Escrow hold/release entries are OUT of scope.
- **FR-4 (runtimes)**: pump contract `OnIncoming(msgType uint64, content
  cbor.Value, raw []byte, nowMs uint64) []tasks.Outgoing` — raw-aware because
  the pay-ref digest needs it. Mirror the Rust state machines and error codes
  exactly (postpaid: 6000/6004/6005/6006/6008; prepaid adds the
  `ExecutionPermitted()` gate + 6010 amount check; metered: 6001 pause with
  `{1: UsageWatermark}` ERROR detail, resume = covering PAY_PROOF, no new
  message type).
- **FR-6 (pump)**: do NOT change `RunProvider`/`RunRequester` signatures;
  hand-pump in the scenario code (route 0x0400–0x0404 + raw to the pay FSM,
  task types to the task FSM, in one `sess.Recv()` loop).
- **FR-7/8 (scenarios)**: `pay-postpaid` and `pay-metered` over libp2p in both
  peers. OK criteria: transcript-root verified AND receipt(s) held AND
  (postpaid: exactly one settlement; metered: ≥2 incremental invoices with
  monotonic watermarks). New `--operator-seed` flag both peers; requester
  hosts the in-process `LedgerOperator`, pre-credited. Rust side composes the
  EXISTING `payments::*` / `metered::*` pairs inside the gated `libp2p_task`
  module (add an optional-under-`libp2p-interop` `map-pay` dep to map-cli if
  the import requires it).
- **FR-9 (driver)**: `$SEED_OP = "04" * 32`; `RunPayCell` modeled on
  `RunTaskCell`; RESULT line gains the pay tally; MATRIX.md gains a "Pay
  matrix" section + Latest-run re-stamp at the landing commit.

## 3. Non-Functional / House Rules

`map-go/pay` PURE STDLIB (extend the README boundary-check command with
`./pay`; it MUST print nothing). Frozen vectors untouched (conformance counts
stay 47/33 — no new vectors in this task; the existing pay TVs are the codec
oracles). gofmt/vet clean; Rust workspace zero warnings (default AND
`-p map-cli --no-default-features`). No new wire types or code points.

## 15. Acceptance Criteria

Exactly the plan's §TASK-029 Acceptance block, plus: the FR-5 test list
implemented (codec-vs-oracle round-trips incl. TV-016/017/018 payload decode,
frozen billing constants + NV-010 shape, ledger idempotency/conservation, and
in-process postpaid/prepaid/metered legs mirroring the Rust `payments_flow` /
`metered_flow` / prepaid `escrow_flow` test matrices).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-029 per the approved plan section (read
> `tasks/PLAN-D-milestones.md` §TASK-029
> AND this file's FR clarifications first — they are the contract). Port
> byte-behavior from the Rust reference (`map-rs/crates/map-pay`,
> `map-rs/crates/map-taskrun/src/{payments,metered,escrow}.rs`) but let the
> frozen conformance vectors arbitrate bytes; mirror the Go codec idiom of
> `map-go/tasks/msg_types.go`. Toolchain: PowerShell; Go via
> `C:\Program Files\Go\bin` (`go test ./...`, gofmt, vet); Rust via
> `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"` and
> `cargo +stable-x86_64-pc-windows-gnu test --workspace` (also
> `-p map-cli --no-default-features`). Then `pwsh interop/run-matrix.ps1`
> MUST print 26/26 (16 core + 6 task + 4 pay). Re-stamp MATRIX.md at the
> landing commit (the `9eea9aa` precedent). If the spec and the Rust source
> disagree, or a seam is missing beyond a trivial accessor, STOP and file
> PENDING-ARCHITECT in map-spec/ERRATA.md. Update memory.md + CHANGELOG
> (additive, impl/harness only) + tasks/README.md row 029; commit + push.

## 18. Validation Checklist (reviewer)

```
cd map-go; gofmt -l .; go vet ./...; go test ./...
go list -deps ./cbor ./envelope ./identity ./session ./hello ./receipt \
  ./discovery ./conformance ./pay | grep -E 'libp2p|multiformats|mr-tron'   # empty
cd ../map-rs; cargo +stable-x86_64-pc-windows-gnu test --workspace          # green, 0 warn
cargo +stable-x86_64-pc-windows-gnu build -p map-cli --no-default-features  # 0 warn
pwsh ../interop/run-matrix.ps1                                              # 26/26
py -3 ../map-spec/test-vectors/gen_vectors.py                               # byte-stable
```

## 19. Known Risks

R1: amount/exponent edge parity (negative mantissa, exponent mismatch) — the
frozen constants + NV-010 pin the arithmetic, but port the SAME-EXPONENT
comparison rule (mantissa-only, mismatch = violation, never convert) or the
metered cells will pass while diverging semantically. R2: the metered cell
needs the provider to bill from REAL streamed usage — reuse the task
executor's chunk counts, don't invent a parallel meter. R3: scope creep into
escrow/disputes Go — OUT; record as future work. R4: `Incoming.Raw` lifetime —
copy it if the session reuses buffers.

## 20. Handoff Notes

After this, both implementations run the negotiation+settlement core of the
pay family live, and TASK-030 (paid demo) is unblocked. Remaining Go pay gap:
terms-2 escrow + disputes (future package). MATRIX.md becomes the citable
ops §6.1 evidence for the pay drafts.
