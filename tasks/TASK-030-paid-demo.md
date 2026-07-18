# TASK-030 — Paid demo: prepaid-gated rfc-title-o-matic + paying Go requester

Status: READY (blocked by TASK-029) · Priority: P2 · Target:
`map-rs/crates/map-cli/src/rfc_title_o_matic.rs`, `map-go/cmd/map-requester`,
`interop/DEMOS.md` (+ optional matrix cell)
Depends on: **TASK-029** (Go `pay` package, `PrepaidRequesterPayment`,
`--operator-seed` convention), TASK-024 (Rust prepaid pair — REUSE, don't
reinvent) · Blocks: nothing.

Authored by the 2026-07-18 architect session, from the approved D-milestones
plan (`tasks/PLAN-D-milestones.md` §TASK-030 — normative for scope; this file
adds the execution contract).

---

## 1. Problem Statement

The demos exercise only the free task path — nothing shows the protocol's
point. Build the cross-implementation demo of an agent that will not work
until paid: the Rust `rfc-title-o-matic` charges terms-1 prepaid over the
credit ledger; the Go `map-requester` pays and receives the title. This is
also the FIRST integration of a task FSM and a payment FSM in one live
session loop on either side's demo path — the pattern TASK-029's interop
scenarios establish is reused here, not re-derived.

## 2. Functional Requirements

FR-1..FR-5 are the five numbered items of the plan §TASK-030, verbatim.
Binding clarifications:

- **FR-1 (provider)**: the gate is TASK-024's `PrepaidProviderPayment` —
  compose it, do not fork its semantics. Sequencing: task QUOTE and PAY_QUOTE
  (terms 1, total = `--price`, task_binding = the session's TaskID) go out
  together; `bind_quote_envelope` is fed the sealed bytes `s.send(PAY_QUOTE,…)`
  returns; ASSIGN received while `!execution_permitted()` is DEFERRED
  (buffered, pay pumped first) — pick deferral over ERROR 6001 unless the
  TASK-024 tests demand otherwise, and say which in the report. The single
  prepaid PAY_INVOICE is issued on PAY_AGREE. Free path (`--price` absent or
  0): behavior byte-identical to today — no pay object constructed, no pay
  messages ever sent.
- **FR-1 operator identity**: `--operator-seed <hex32>` (derive the AgentID
  provider-side; the requester runs the in-process operator, as in the
  TASK-029 cells and `$SEED_OP = "04"*32`).
- **FR-2 (requester)**: `--pay --operator-seed <hex> --budget <cents>`.
  Budget enforcement is the EXISTING cap path (quote total > budget → the
  6005-class refusal before any agreement; print a clean FAIL line, zero
  messages of money moved). Output contract adds `OK paid: <amount>` and
  `OK receipt: <invoice-id-hex>` lines before the existing `OK result` /
  `OK transcript-root` lines.
- **FR-3 (negative paths — deliverables, not afterthoughts)**:
  (a) unpaid run (no `--pay` vs priced provider): the gate holds — no output
  chunks, provider prints no SETTLED line; document the exact stdout both
  sides. (b) over-budget run: clean refusal, no work, no movement.
- **FR-4 (DEMOS.md)**: "## Run the paid demo (cross-impl)" in the established
  style (PowerShell, Start-Process + Select-String LISTENING, deterministic
  seeds, EXACT expected stdout for paid/unpaid/over-budget). Paste the real
  transcript-root + receipt lines from a live verified run.
- **FR-5 (stretch)**: `pay-prepaid` matrix cell ONLY if it falls out of the
  demo wiring trivially (matrix → 28/28); otherwise skip and note it.

## 3. Non-Functional / House Rules

Zero new deps either impl. Free path regression-guarded (a test or a recorded
manual check that a no-flag run emits no 0x04xx frames). gofmt/vet clean;
Rust zero warnings (default + `--no-default-features`, where the demo bin is
already `required-features`-gated). Frozen vectors untouched.

## 15. Acceptance Criteria

The plan §TASK-030 Acceptance block, verbatim.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-030 per `tasks/PLAN-D-milestones.md` §TASK-030 plus this
> file's clarifications. PREREQUISITE: TASK-029 DONE (uses its Go
> `pay.PrepaidRequesterPayment` and pump pattern) — if not, STOP. Read FIRST:
> pay §5.2.2, `map-rs/crates/map-taskrun/src/escrow.rs` (PrepaidProvider/
> Requester + tests — reuse, don't reinvent the gate), the TASK-029 interop
> scenario code (the combined-pump pattern), and `interop/DEMOS.md` (style).
> Verify LIVE cross-impl: Go `map-requester --pay` against Rust
> `rfc-title-o-matic --price` — paid, unpaid, and over-budget runs — and
> paste the actual output into DEMOS.md. Toolchain as in TASK-029 §17. If
> prepaid gate semantics in escrow.rs are ambiguous for the deferred-ASSIGN
> case, STOP and file PENDING-ARCHITECT in map-spec/ERRATA.md. Update
> memory.md + CHANGELOG (impl/demo only) + tasks/README.md row 030; run
> `pwsh interop/run-matrix.ps1` (unchanged 26/26, or 28/28 with the stretch
> cell — re-stamp MATRIX.md if so); commit + push.

## 18. Validation Checklist (reviewer)

```
# free path unchanged:
target/debug/rfc-title-o-matic --seed <hex>            # behaves exactly as before
# paid path (three documented runs): follow DEMOS.md verbatim; outputs match
cd map-go; go test ./...; gofmt -l .; go vet ./...
cd ../map-rs; cargo +stable-x86_64-pc-windows-gnu test --workspace   # green, 0 warn
grep -n "OK paid\|SETTLED\|RECEIPT" interop/DEMOS.md                 # real run pasted
```

## 19. Known Risks

R1: ordering deadlock in the combined pump (task waits on pay, pay waits on a
frame behind a task frame) — the TASK-029 scenario loop is the template;
single recv-loop routing prevents it. R2: the deferred-ASSIGN choice must
match TASK-024 semantics — STOP rule covers ambiguity. R3: demo output drift —
DEMOS.md must carry ACTUAL run output, not intended output.

## 20. Handoff Notes

After this the project has its headline demo: "no pay, no work,"
cross-implementation, with receipts. Natural sequels: a metered demo
(streaming top-ups), an escrowed demo, and wiring the demo agents to publish
priced descriptors via the DHT.
