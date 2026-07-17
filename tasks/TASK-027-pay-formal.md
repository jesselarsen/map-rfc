# TASK-027 - Formal Analysis: Payment Escrow Authorization Symmetry & Dispute Watermark Binding

Status: READY · Priority: P3 · Target: `formal/` (new models) + `tasks/TASK-027-pay-formal.md`
Depends on: TASK-009 (payments/credit-ledger as implemented), TASK-024 (prepaid-escrow
runtime), TASK-026 (disputes) · Blocks: draft-map-pay leaving draft (ops §6.3 formal-analysis program)

> **Filename caution:** `TASK-018` is already assigned to `TASK-018-inputspec-parity-and-nv009.md`.
> This package uses **TASK-027** to avoid collision.

## 1. Problem Statement
Ops §6.3 commits the family to machine-checked models before beta. The payment plane
currently has **no** formal model — `formal/README.md` lists as open: *"Q6 (escrow)/Q7
(dispute) — pay §5.2/§6 wire; need new `.pv` + TASK-018"*. Two properties carry the
payment security story:
- **(P4) Escrow charge-authorization symmetry** (`pay §5.2` `pay:credit-ledger`): a
  debit/charge is applied **only** when BOTH payer-authorization and payee-authorization
  are present, and never while the flow is paused (fail-closed). No one-sided debit.
- **(P5) Dispute watermark binding** (`pay §6`): an arbiter-authored `dispute_resolved`
  resolution is emitted **only** for an `invoice-id` it actually observed contested under
  the matching `PayAgreeRef` digest — no resolution of an invoice the arbiter never saw
  disputed.

This task builds ProVerif models proving P4 and P5 (or producing counterexample traces —
a counterexample is a SUCCESS OUTCOME that feeds the spec-feedback loop, NOT a failure).

## 2. Functional Requirements
- **FR-1 Tool:** **ProVerif 2.05** (pinned; matches the existing suite). Windows: run under
  **WSL2 Ubuntu** per `formal/README.md` setup (opam install; `eval $(opam env)`). Record
  `proverif --version` in the results header.
- **FR-2 `formal/pay-auth.pv`** (P4, positive): model payer `skP` + payee `skE` as top-level
  **private** names (same "scope to honest principal" convention as `delegation.pv`/
  `hello-cb.pv`). Reuse the `skey/pkey/sign/getmess/checksign/h/kdf` block verbatim from
  `hello-cb.pv`. Domain tags `tagPayAgree`, `tagInvoice`, `tagPause`. Model `pay:credit-ledger`
  escrow as a **2-of-2 authorization gate**: `authorize(payerAuth, payeeAuth, amt, invoiceId)`
  yields a valid `charge` only when BOTH `payerAuth` and `payeeAuth` are present AND the gate
  is not `paused`. A `pause` event disables the gate (fail-closed).
  - **Q1 (P4, positive):** `event(ChargeApplied(payer, payee, inv, amt)) ==>
    event(PayerAuthorized(payer, inv)) && event(PayeeAuthorized(payee, inv)) &&
    not event(GatePaused(inv))`. **Expected: proved TRUE.**
  - **Q2 sanity/reachability:** sessions DO authorize/charge. **Expected: "cannot be proved"
    WITH a trace** (non-vacuity guard; same interpretation as `hello-cb.pv` Q3 /
    `delegation.pv` Q5).
  - In-file abstraction-gaps note: no continuous metering/credit-window arithmetic;
    `expiry`/`invoice-id` are symbolic bitstrings; escrow abstracted as the 2-of-2 gate;
    `pay:credit-ledger` operator trust out of scope.
- **FR-3 `formal/pay-auth-control-onesided.pv`** (P4 negative control, mandatory per
  TASK-014 FR-4): variant where the gate accepts a **payer-only** authorization (simulating a
  missing payee-auth check). **Expected: Q1 becomes NOT provable (a trace exists)** — proving
  the query has teeth. Commit with comments explaining the deliberate weakening.
- **FR-4 `formal/pay-dispute.pv`** (P5, positive): arbiter is an honest top-level private name;
  disputants' keys may be dishonest/insider (leaked). Model `PayDispute` carrying `invoice-id`
  (CDDL key 2, `bstr .size 16`) + `PayAgreeRef` digest (key 4); `PayDisputeEvidence` carrying
  the `invoice-id` it contests.
  - **Q3 (P5, positive):** `event(DisputeResolved(arbiter, invId, agreeRef)) ==>
    event(Contested(invId, agreeRef))`. **Expected: proved TRUE** — arbiter only resolves an
    invoice it actually observed contested under the matching agreement reference.
  - **Q4 sanity/reachability:** disputes DO get resolved. **Expected: "cannot be proved" WITH
    a trace.**
  - In-file abstraction-gaps note: §6.3 claim-code evaluation (rebuttal logic for claims 0–4)
    is out of scope; model binds resolution to the contested watermark only. CB-disclosure
    (§6.2) modeled as an opaque bitstring tag, not re-derived.
- **FR-5 `formal/results/`**: capture `proverif` stdout for all three models
  (`pay-auth.log`, `pay-auth-control-onesided.log`, `pay-dispute.log`). If ProVerif is
  unavailable in the environment, commit the models + expected-verdict statements anyway and
  record "prover not executed — pending" in `formal/README.md`; do NOT block.
- **FR-6 `formal/README.md`** (edit): add `pay-auth.pv` (+ control) and `pay-dispute.pv` to the
  model inventory table; update the open-questions line (*"Q6 (escrow)/Q7 (dispute)…"*) → mark
  **Q6 covered by pay-auth.pv / TASK-027** and **Q7 covered by pay-dispute.pv**; record any
  residual gap (continuous metering). Add the ProVerif invocation command + per-query expected
  verdict (true / not-provable-with-trace). Tool version pin.
- **FR-7 `README.md`** (edit): in the "Status" section, move the formal-analysis escrow/dispute
  items from "open" toward "covered (TASK-027)".
- **FR-8 `map-spec/CHANGELOG.md`** (edit): dated editorial subsection *"Formal analysis:
  payment escrow charge-authorization symmetry + dispute watermark binding (Item 5 / TASK-027)"*;
  reference TASK-027, the `.pv` files, and README updates. No wire/doc-normative change.
- **FR-9** Write this package to `tasks/TASK-027-pay-formal.md`.

## 3. Invariants (must hold)
1. No `ChargeApplied` event without both `PayerAuthorized` and `PayeeAuthorized` for the same
   `invoice-id`, and while not paused.
2. No `DisputeResolved` event for an `invoice-id`/`agreeRef` pair the arbiter never observed as
   `Contested`.

## 4. Acceptance Gate
- All three `.pv` files parse under ProVerif (or are committed with the "pending" note if the
  prover is unavailable).
- Q1 (P4) and Q3 (P5) proved TRUE; Q2/Q4 reachability yield traces; the onesided control's Q1 is
  NOT provable.
- `git status` shows ONLY: `formal/pay-auth.pv`, `formal/pay-auth-control-onesided.pv`,
  `formal/pay-dispute.pv`, `formal/results/*.log`, `formal/README.md`, `README.md`,
  `map-spec/CHANGELOG.md`, `tasks/TASK-027-pay-formal.md`. **No `.rs`, test-vector, or registry
  file changed.**
- README/CHANGELOG/TASK cross-references resolve; no broken `TASK-027` / `.pv` links.

## 5. Mapping to existing models (convention to mirror)
| New file | Mirrors | Notes |
|---|---|---|
| `pay-auth.pv` | `delegation.pv` | 2-of-2 intersection gate ~ tighten-only intersection; honest-principal scoping |
| `pay-auth-control-onesided.pv` | `hello-cb-control-downgrade.pv` | deliberate weakening → query fails |
| `pay-dispute.pv` | `hello-cb.pv` (event style) | watermark-binding ~ CB collision style |
| `README`/CHANGELOG edits | TASK-014 FR-5/FR-6 pattern | |

## 6. Spec references (for the downstream model's citations)
- Escrow / `pay:credit-ledger` / pause: `draft-map-pay-01.md` §5.1 (pause/resume), §5.2 (escrow
  authorization symmetry, "payer and payee both authorize"), §4 (metered credit).
- Disputes / watermark: `draft-map-pay-01.md` §6.1 (PayDispute key 2 `invoice-id`, key 4
  `PayAgreeRef`), §6.2 (CB disclosure), §6.3 (mechanical verification), §6.4 (griefing).
- Impl target for FR citation: `map-rs/crates/map-pay` (confirm exact module via
  `grep -rn "credit-ledger\|pause\|dispute" map-rs/crates/map-pay/src` during implementation).
- Family policy: ops §2 rule 1 (one named source of truth), ops §3.2 (additive cross-cutting
  change → CHANGELOG entry, no PENDING-ARCHITECT routing), ops §6.3 (formal-analysis program).
