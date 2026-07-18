# D-Milestones Plan: Pay-Family Go Interop, Paid Demo, I-D Submission Readiness

## Context

The MAP project's goals are (1) an RFC family presentable to the IETF and (2) working
demos. The audit of 2026-07-17 closed all hygiene items and left three forward
milestones. Current state that shapes them:

- The **pay family** (quote→agree→invoice→proof→receipt, metering, prepay/escrow,
  disputes) is implemented end-to-end **in Rust only**. map-go has **zero pay code**
  (only opaque `cbor.Value` pay-ref fields in task messages and hand-built conformance
  vectors). This is the largest remaining gap between "protocol family" and "two
  interoperable implementations" (the ops §6.1 bar the drafts cite).
- The **demos** (map-excuse, rfc-title-o-matic) exercise only the free task path.
  Nothing on either side has ever pumped a task FSM and a payment FSM in ONE live
  session loop — the paid demo is the first such integration.
- **I-D rendering already exists** (`build/render.py` + `build/md2rfc.py` → xml2rfc
  `.txt`/`.html` for all 11 drafts), but the drafts lack author/ipr metadata a
  datatracker submission requires, deps are unpinned, and no submission checklist
  exists.

User decisions: the paid demo showcases the **prepaid gate** ("no pay, no work",
terms 1); draft author metadata = **Jesse Larsen <jesselarsen@gmail.com>**.

Work is packaged as three TASK specs in the repo's split-role tradition
(architect authors the package; implementer executes its §17-style prompt).
**Order: TASK-029 → TASK-030 (needs 029's Go requester-side pay); TASK-031 is
independent and can run in parallel with either.** Each lands as its own commit
with a matrix re-verify + MATRIX.md re-stamp where applicable.

House rules that bind all three: BCP-14 discipline; frozen vector bytes untouched;
no new wire types/code points (everything needed already exists); map-go protocol
core stays stdlib-only (pay is protocol core ⇒ **`map-go/pay` must be pure stdlib**,
enforced by the existing `go list -deps` boundary check); gofmt/vet clean; zero
Rust warnings; commit+push with matrix re-stamp precedent (`aa973aa`/`9eea9aa`).

Toolchain: PowerShell; Rust `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"`,
`cargo +stable-x86_64-pc-windows-gnu`; Go via `C:\Program Files\Go\bin`;
Python via `py -3` (pynacl/cbor2/base58 installed; xml2rfc on PATH).

---

## TASK-029 — Go pay runtime (postpaid + metered + prepaid) + live pay interop cells

**Goal:** port the pay wire codecs, credit-ledger method, and the terms-0/1/3
runtimes to map-go, byte-compatible with Rust, and prove it live: new
`pay-postpaid` and `pay-metered` cells in the interop matrix, both orientations.

### New package `map-go/pay` (pure stdlib — this is protocol core)

Port from `map-rs/crates/map-pay/src/{codec,billing,credit_ledger}.rs` and mirror
the existing Go codec idiom in `map-go/tasks/msg_types.go` (`ToValue`/`FromValue`
pairs, `reqUint`/`optUint` helpers, integer map keys, only-present-keys emitted):

1. `codec.go` — `Amount{Exponent,Mantissa}` (CBOR tag 4 `[e,m]`), `AssetAmount`
   (`[asset_text, amount]`), `MethodOffer`, `PayQuote` (keys 1–9 incl. terms key 7,
   task-binding key 9), `PayAgree`, `PayQuoteRef`/`PayAgreeRef` (typed versions of
   the opaque fields in `tasks/msg_msgs.go`), `PayInvoice` (keys 1–7 incl. optional
   watermark key 4 and phase key 7, fail-closed phase ∈{1,2}), `PayProof`,
   `PayReceipt` (optional proof-digest key 4, kind key 5 fail-closed ≤3),
   `UsageWatermark` (all four keys required). Reuse `tasks.Usage` (identical keys)
   and the `{0x12,0x20}` multihash prefix already in `tasks/msg.go` for
   `PayRefDigest(bytes) = 0x12 0x20 ‖ SHA-256(bytes)`.
   Constants: msg types 0x0400–0x0404, terms 0–3, phases, kinds, unit codes,
   `CreditLedgerMethod = "pay:credit-ledger"`, error codes 6000–6010.
2. `billing.go` — `Billed(model, rateTable, usage)` exact checked arithmetic
   (models 1/2/3; int64 mantissa with big-int-free overflow checks via int64/
   math bounds — mirror Rust's i128-intermediate checks with int64+overflow
   detection or `math/big` is NOT allowed? stdlib `math/big` IS stdlib — use it
   for intermediates if simpler) and `VerifyWatermark(agreed, lastAccepted, inv)`
   returning the specific wire code (6009/6007/6005), same six-check order as
   Rust `billing::verify_watermark`.
3. `credit_ledger.go` — `EntryProof` (`CBOR [entry_bytes, sig]`),
   `entry_bytes = enc(["MAPLedgerEntry", invoice-id, payer, payee, [asset,amount], ts])`,
   in-process `LedgerOperator` (ed25519 sign via stdlib; balances, `Pay` idempotent
   by invoice-id) and `VerifyEntry`. (Escrow hold/release entries are OUT — terms 2
   Go port is future work; prepaid needs only `Pay`.)
4. `runtime.go` (or split files) — pump-style FSMs mirroring
   `map-rs/crates/map-taskrun/src/{payments,metered,escrow}.rs` with the Go pump
   contract, but **raw-aware**: `OnIncoming(msgType uint64, content cbor.Value,
   raw []byte, nowMs uint64) []tasks.Outgoing`:
   - `ProviderPayment` / `RequesterPayment` (terms 0 postpaid): quote (bind sealed
     bytes from `session.Send`'s return — the Go `bind_quote_envelope`) → agree
     (digest `Incoming.Raw`) → invoice → proof (operator) → receipt. Error codes
     6004/6005/6006/6008 exactly as Rust.
   - `PrepaidProviderPayment` / `PrepaidRequesterPayment` (terms 1): single
     pre-delivery invoice == quote total; provider `ExecutionPermitted()` gate
     (needed by TASK-030); requester rejects amount ≠ total with 6010.
   - `MeteredProviderPayment` / `MeteredRequesterPayment` (terms 3): provider
     `Bill(usage, throughSeq, invoiceID, now)` emits kind-3 CHUNK + watermark
     PAY_INVOICE (delta amount); requester records meters from kind-3 chunks and
     runs the six §4.1 checks in order before paying; pause = ERROR 6001 with
     `{1: UsageWatermark}` detail; resume = covering PAY_PROOF.
5. Tests: codec round-trips against the **conformance oracles** (decode the frozen
   TV-010/010b/013/016/017/018 payloads from `map-go/conformance/produce.go`
   content builders — or directly consume the vector file like conformance does),
   billing arithmetic vs the frozen constants (1523×3000+812×5000=8_629_000; NV-010
   off-by-one rejects 6009), ledger idempotency/balance conservation, and full
   in-process postpaid/prepaid/metered legs mirroring the Rust test matrices
   (`payments_flow`, `metered_flow`, `escrow_flow` prepaid cases in
   `map-rs/crates/map-taskrun/src/tests.rs`).

### Pay-aware pump + interop scenarios

6. `map-go/tasks/runner.go` (or a new `pay`-side runner): the existing
   `RunProvider`/`RunRequester` drop `Incoming.Raw`. Add raw-threading variants
   (e.g. `RunProviderPaid(sess, task *ProviderTask, pay PayPump)` — or extend the
   scenario code in `cmd/map-interop` to hand-pump `sess.Recv()` routing
   `0x04xx`+raw to the pay FSM and task types to the task FSM, which is how the
   Rust side will do it in TASK-030). Keep the existing runner signatures
   unchanged (other callers exist).
7. `map-go/cmd/map-interop/main.go`: new libp2p scenarios `pay-postpaid` and
   `pay-metered` — listener = provider (task `scenarioExecutor` + pay provider FSM,
   quote issued alongside the task QUOTE, sealed bytes bound), dialer = requester
   (task + pay requester FSM + in-process `LedgerOperator` seeded from a new
   `--operator-seed` flag, requester pre-credited). Success criteria printed as
   the final `OK <scenario>` only when: task transcript-root verified AND
   PAY_RECEIPT received AND (postpaid: exactly one settlement; metered: ≥2
   incremental invoices settled with monotonic watermarks). Extend the
   `runLibp2pDial`/`Listen` scenario guard.
8. `map-rs/crates/map-cli/src/interop.rs` (inside the gated `libp2p_task` module):
   the same two scenarios for the Rust peer, composing the existing
   `ProviderTask`/`RequesterTask` with `payments::{Provider,Requester}Payment` and
   `metered::*` — the first live task+pay single-session loop on the Rust side
   (route by msg_type: 0x04xx → pay pump with `inc.raw`, else task pump; feed
   `bind_quote_envelope` from `s.send(PAY_QUOTE,…)`'s sealed return). Reuse
   `LedgerOperator` from map-pay (map-cli already links map-taskrun→map-pay under
   `libp2p-interop`; add a direct `map-pay` non-optional-under-feature dep if
   import needs it).
9. `interop/run-matrix.ps1`: `$SEED_OP = "04" * 32`; `$payScenarios =
   @("pay-postpaid","pay-metered")` driven by a `RunPayCell` modeled on
   `RunTaskCell` (libp2p, LISTENING-multiaddr handshake, `--operator-seed`
   appended for the dialer), both orientations → **+4 cells (total 26)**. Update
   the RESULT line and MATRIX.md (new "Pay matrix" section + Latest-run stamp).

### Out of scope (record in the package)
Terms-2 escrow + disputes Go port; x402 in Go; DHT-published pay descriptors.

### Acceptance
- `map-go/pay` pure stdlib (extend the README boundary check command with `./pay`).
- `go test ./...` green incl. new pay tests; gofmt/vet clean.
- Rust workspace green, zero warnings (default + `--no-default-features` map-cli).
- Matrix **26/26 PASS** both orientations; MATRIX.md re-stamped at the commit.
- memory.md §"entry N" area + CHANGELOG entry (additive, impl/harness only);
  tasks/TASK-029 file added with DONE row in tasks/README.md.

---

## TASK-030 — Paid demo: prepaid-gated rfc-title-o-matic + paying Go requester

**Goal:** the demo that shows the protocol's point — an agent that will not work
until it is paid, cross-implementation. Rust provider charges via terms-1 prepaid
over the credit ledger; the Go requester pays and gets the task output.
Depends on TASK-029 (Go `pay` package, prepaid requester FSM, `--operator-seed`).

1. `map-rs/crates/map-cli/src/rfc_title_o_matic.rs`: add `--price <cents>`
   (0 or absent = today's free behavior, unchanged). When priced, `run_one`
   becomes the first combined pump on the Rust demo path:
   - Compose `ProviderTask::new(TitleExecutor)` + `PrepaidProviderPayment`
     (quote terms 1, total = price, task_binding = the session's TaskID — issue
     PAY_QUOTE immediately after the task QUOTE goes out; bind sealed bytes).
   - **The gate:** hold the ASSIGN→execution path until
     `pay.execution_permitted()`; on ASSIGN while unsettled, defer (buffer the
     ASSIGN and pump pay first) or reply the task-level retryable ERROR 6001 —
     match the Rust prepaid pair's existing semantics (see
     `map-rs/crates/map-taskrun/src/escrow.rs` PrepaidProviderPayment and its
     tests) rather than inventing new ones. Print `SETTLED <amount>` +
     `RECEIPT <invoice-id-hex>` lines when payment lands (demo-visible proof).
   - Operator identity via `--operator-seed` (demo runs an in-process
     requester-side operator exactly like the interop cells; the provider only
     needs the operator AgentID — derive from the seed's pubkey, or accept
     `--operator <agent-id>`; prefer seed for symmetry with the matrix).
2. `map-go/cmd/map-requester/main.go`: add `--pay`, `--operator-seed`,
   `--budget <cents>` flags. When `--pay`: instantiate `pay.LedgerOperator`
   (credit self with budget), `pay.PrepaidRequesterPayment`, pump task+pay over
   the dialed session (route 0x04xx+Raw to pay). Print `OK paid: <amount>` +
   `OK receipt: <hex>` alongside the existing `OK result` / `OK transcript-root`
   lines. Refuse quotes over budget with the existing 6005 path (demo-able
   "too expensive" run: `--budget` below price → clean FAIL line, no work done,
   no money moved).
3. Negative demo path is part of the deliverable: a run WITHOUT `--pay` against a
   priced provider must show the gate holding (no output chunks, provider prints
   nothing settled) — document the exact expected output.
4. `interop/DEMOS.md`: new "## Run the paid demo (cross-impl)" section in the
   established style (PowerShell, Start-Process + Select-String LISTENING,
   deterministic seeds, exact expected stdout for paid, unpaid, and over-budget
   runs). Update the intro table (rfc-title-o-matic gains "prepaid, credit-ledger"
   column note).
5. Optional stretch (include only if trivial after 1–2): a `pay-prepaid` matrix
   cell reusing the demo wiring — if added, matrix becomes 28/28.

### Acceptance
- Live cross-impl run verified: Go `map-requester --pay` × Rust
  `rfc-title-o-matic --price` → title delivered, receipt held, balances moved
  exactly once (paste the transcript-root + receipt lines into DEMOS.md).
- Unpaid and over-budget runs behave and print as documented.
- Free path byte-identical to today (no flags = no pay messages at all).
- Workspace + map-go green; matrix unchanged (or 28/28 if the stretch cell landed);
  DEMOS.md + memory.md + CHANGELOG + tasks/README row updated; commit+push.

---

## TASK-031 — I-D submission readiness (hardening the existing pipeline)

**Goal:** take the existing `build/render.py`+`md2rfc.py` pipeline from "renders"
to "submittable": required metadata, pinned reproducible tooling, and a
verified pre-submission checklist. Independent of 029/030.

1. **Author/ipr metadata** (drafts + renderer): extend the 4-key YAML frontmatter
   of all 11 `map-spec/draft-map-*-01.md` with
   `author: {name: Jesse Larsen, email: jesselarsen@gmail.com}`, `ipr: none`
   (independent-stream; matches render.py's current hardcoded ipr="none" — verify
   `submissiontype: independent` + `ipr: none` is the combination xml2rfc accepts,
   else `trust200902`), and `date: {auto}` handling. Teach `build/md2rfc.py` to
   read these instead of the hardcoded `DRAFTS` tuples in `render.py` (keep the
   tuples as fallback). Classify as editorial in CHANGELOG (no wire change).
2. **Reproducibility:** add `build/requirements.txt` (xml2rfc pinned + any md2rfc
   deps) and a `build/README.md` documenting the exact render command
   (`py -3 build/render.py`), expected outputs, and the check that a re-render is
   idempotent (byte-stable .txt output modulo the date line). Commit the
   regenerated `.xml/.txt/.html` for all 11 drafts with the new metadata.
3. **Validation pass:** run `xml2rfc --v3` strict mode and an `idnits`-equivalent
   review per draft (idnits may not run on Windows easily — at minimum script the
   mechanical checks: required sections present [Abstract, Security
   Considerations, IANA Considerations, References split normative/informative],
   line lengths, non-ASCII handling already covered by esc(), boilerplate
   presence). Fix what it finds in the drafts (expect: references sections need
   normative/informative split formalized; some drafts may be missing an explicit
   Abstract heading — md2rfc.py's handling to be confirmed). Every draft fix is
   editorial-only; frozen vectors and normative CDDL untouched.
4. **Submission checklist:** `map-spec/SUBMISSION.md` — the datatracker
   independent-stream submission steps (account, draft upload of the rendered
   `.xml`, the 10 protocol drafts + index ordering, ISE process pointer per
   RFC 8730, and the two known pre-submission caveats to disclose in the cover
   note: the CB-exporter stand-in [core §14] and single-author status). Include
   the exact rendered filenames.
5. Explicit non-goals: actual datatracker upload (user action), draft -02 content
   changes, kramdown-rfc migration (md2rfc.py is the pipeline of record — its
   error logs show kramdown/mmark were tried and rejected).

### Acceptance
- `py -3 build/render.py` regenerates all 11 drafts cleanly from the new
  frontmatter; second run idempotent; outputs committed.
- Strict xml2rfc pass, zero errors; mechanical-nits script output clean or each
  remaining nit listed + justified in SUBMISSION.md.
- SUBMISSION.md complete; CHANGELOG editorial entry; tasks/README row; commit+push.

---

## Verification (end-to-end, all three)

1. `cd map-rs; cargo +stable-x86_64-pc-windows-gnu test --workspace` — green, 0 warnings
   (also `-p map-cli --no-default-features`).
2. `cd map-go; gofmt -l . ; go vet ./... ; go test ./...` — clean/green; boundary check
   `go list -deps ./cbor ./envelope ./identity ./session ./hello ./receipt ./discovery ./conformance ./pay | grep -E 'libp2p|multiformats|mr-tron'` → empty.
3. `pwsh interop/run-matrix.ps1` → **26/26** (28/28 with the stretch cell); MATRIX.md
   re-stamped at each landing commit (the `9eea9aa` precedent).
4. Paid demo manual run per DEMOS.md: paid / unpaid / over-budget transcripts match
   the documented output; balances move exactly once.
5. `py -3 build/render.py` twice — idempotent, xml2rfc strict clean.
6. Frozen-vector guard: `py -3 map-spec/test-vectors/gen_vectors.py` → byte-stable;
   Rust conformance 47 / map-go 33 counts unchanged (no new vectors in these tasks).
7. Each task: memory.md + CHANGELOG + tasks/TASK-0NN file + tasks/README.md row;
   commit + push per landing.
