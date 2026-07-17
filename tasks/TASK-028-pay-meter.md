# TASK-028 - Formal Analysis: Continuous Metering Arithmetic & Quote/Agree/Invoice/Proof Binding

Status: READY · Priority: P3 · Target: `formal/` (new model) + `tasks/TASK-028-pay-meter.md`
Depends on: TASK-009 (payments as implemented), TASK-027 (escrow/dispute models),
`draft-map-pay-01.md` §3/§4 · Blocks: draft-map-pay leaving draft (ops §6.3 formal-analysis program)

## 1. Problem Statement
Ops §6.3 commits the family to machine-checked models before beta. TASK-027 covered
the escrow charge-authorization symmetry (P4) and the dispute watermark binding (P5),
but `formal/README.md` lists as still open: *"the full quote→agree→invoice→proof binding
and continuous-metering arithmetic remain out of scope (ops §6.3 lists these as remaining
formal work)."* Two properties carry the metered-billing security story (`pay §3`/`§4`):

- **(P6) Metering arithmetic & settlement binding:** under terms 3 (metered-incremental),
  a `PAY_INVOICE` watermark's `cumulative-billed` MUST equal `usage × rate` under the
  agreed rate table (same-exponent rule, §3) and MUST NOT exceed the `PAY_AGREE` authorized
  cap (§4.1 steps 5–6). The requester settles **only** well-formed invoices; a settled
  over-bill or over-cap is unreachable (Q-overbill). Delivery (billed CHUNKs) advances
  **only** after a verified `PAY_PROOF` settles the covered usage — "settled not submitted"
  (Q-underpay). An invoice bills **only** a quote the requester agreed to (PAY_AGREE digest
  binding, §3; Q-bind).
- Continuous metering does not affect the P4/P5 symmetry properties proved in TASK-027;
  this task adds the metering layer on top of "agreed".

This task builds a ProVerif model proving P6's three sub-properties (or producing
counterexample traces — a counterexample is a SUCCESS OUTCOME that feeds the spec-feedback
loop, NOT a failure).

## 2. Functional Requirements
- **FR-1 Tool:** **ProVerif 2.05** (pinned; matches the existing suite). Windows: run under
  **WSL2 Ubuntu** per `formal/README.md` setup (opam install; `eval $(opam env)`). Record
  `proverif --version` in the results header.
- **FR-2 `formal/pay-meter.pv`** (P6, positive): model requester `skR` + provider `skPv` as
  top-level **private** names (same "scope to honest principal" convention as `pay-auth.pv`/
  `pay-dispute.pv`). Reuse the `skey/pkey/sign/getmess/checksign/h` block verbatim from
  `pay-auth.pv`. Abstract the agreed **rate** and **cap** as private `free` constants; model
  the honest billing+cap derivation as `fun billed_capped(bitstring): bitstring [data]` (u →
  u × rate, defined only for usage within cap — folds the §4.1 step-5 arithmetic AND step-6
  cap into one private derivation the requester recomputes and the attacker cannot satisfy).
  Domain tags `tagInvoice`, `tagProof`. The provider emits invoice claims freely (billing is
  a claim, not itself enforced) and emits a billed CHUNK at usage u **only after** observing a
  verified requester `PAY_PROOF` for usage u (settled-not-submitted). The requester agrees to
  the quote digest, then settles an observed invoice **only** when its `cumulative-billed`
  equals `billed_capped(u)` (arithmetic + cap) — so a settled over-bill/over-cap is unreachable.
  - **Q-overbill (P6, §4.1):** `event(Settled(qd,u,cb)) ==> event(Agreed(qd)) && cb = billed_capped(u)`.
    **Expected: proved TRUE.**
  - **Q-underpay (P6, §4.2):** `event(ChunkEmitted(qd,u)) ==> event(Settled(qd,u,amt_dummy(u)))`.
    **Expected: proved TRUE** (delivery gated on settlement of the same usage).
  - **Q-bind (P6, §3):** `event(Settled(qd,u,cb)) ==> event(Agreed(qd))`. **Expected: proved TRUE**
    (PAY_AGREE digest binding — the requester only settles an agreed quote).
  - **Q-reach sanity/reachability:** `not event(Settled(...))` expected **"cannot be proved" WITH a
    trace** (non-vacuity guard; same interpretation as `pay-auth.pv` Q2 / `pay-dispute.pv` Q4).
  - In-file abstraction-gaps note: single metered unit (tokens-out, model 1) stands in for the
    multi-unit rate table under the same-exponent rule; `usage`/`invoice-id` are symbolic
    bitstrings; the escrow 2-of-2 gate (P4) and dispute watermark (P5) are out of scope here;
    the §4.2 pause/cancel mechanics and checkpoint-anchored cancel (tasks §8.2/§11) are abstracted
    to the settled-not-submitted gate.
- **FR-3 `formal/results/pay-meter.out`:** capture `proverif` stdout. If ProVerif is unavailable,
  commit the model + expected-verdict statements and record "prover not executed — pending" in
  `formal/README.md`; do NOT block.
- **FR-4 `formal/README.md`** (edit): add `pay-meter.pv` to the model inventory table; update the
  open-questions line to mark the **quote→agree→invoice→proof binding and continuous-metering
  arithmetic as covered by pay-meter.pv / TASK-028**; record residual gap (checkpoint-anchored
  cancel accounting, if any). Add the ProVerif invocation command + per-query expected verdict.
  Tool version pin.
- **FR-5 `README.md`** (edit): in the "genuinely open work" list, update the payment/escrow formal
  line to state the metering arithmetic + quote/agree/invoice/proof binding are now machine-checked
  (TASK-028), leaving only the checkpoint-anchored cancel corner as remaining formal work.
- **FR-6 `map-spec/CHANGELOG.md`** (edit): dated editorial subsection *"Formal analysis: continuous
  metering arithmetic & quote/agree/invoice/proof binding (Item 9 / TASK-028)"*; reference TASK-028,
  the `.pv` file, and README updates. No wire/doc-normative change.
- **FR-7** Write this package to `tasks/TASK-028-pay-meter.md`.

## 3. Invariants (must hold)
1. No `Settled` event for `(qd,u,cb)` unless `Agreed(qd)` and `cb = billed_capped(u)` (rate-correct,
   within cap) — no settled over-bill / over-cap.
2. No `ChunkEmitted(qd,u)` without a preceding `Settled(qd,u,_)` — delivery is gated on settlement of
   the same usage (settled-not-submitted).
3. No `Settled(qd,_,_)` for a quote digest the requester never agreed to (PAY_AGREE digest binding).

## 4. Acceptance Gate
- `pay-meter.pv` parses under ProVerif (or is committed with the "pending" note if the prover is
  unavailable).
- Q-overbill, Q-underpay, Q-bind proved TRUE; Q-reach yields a trace.
- `git status` shows ONLY: `formal/pay-meter.pv`, `formal/results/pay-meter.out`, `formal/README.md`,
  `README.md`, `map-spec/CHANGELOG.md`, `tasks/TASK-028-pay-meter.md`. **No `.rs`, test-vector, or
  registry file changed.**
- README/CHANGELOG/TASK cross-references resolve; no broken `TASK-028` / `.pv` links.

## 5. Mapping to existing models (convention to mirror)
| New file | Mirrors | Notes |
|---|---|---|
| `pay-meter.pv` | `pay-auth.pv` / `pay-dispute.pv` | honest-principal scoping; private rate/cap derivation ~ private key scoping |
| README/CHANGELOG edits | TASK-027 FR-4/FR-6 pattern | |

## 6. Spec references (for the downstream model's citations)
- Quotes/agreements, same-exponent rule, authorized cap, PAY_AGREE digest binding:
  `draft-map-pay-01.md` §3 (§3 step 5–6 enforcement in §4.1).
- Metering, incremental PAY_INVOICE at watermarks, PAY_PROOF top-up, settled-not-submitted,
  idempotent invoices, pause/cancel: `draft-map-pay-01.md` §4 (§4.1 watermark semantics & verification
  steps 1–6; §4.2 pause/resume/cancel).
- Family policy: ops §2 rule 1 (one named source of truth), ops §3.2 (additive cross-cutting change →
  CHANGELOG entry, no PENDING-ARCHITECT routing), ops §6.3 (formal-analysis program).
