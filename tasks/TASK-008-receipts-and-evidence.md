# TASK-008 — Receipts and Evidence Exchange

Status: READY (after TASK-007) · Priority: P1 · Target: extend `map-trust` + `map-taskrun`
Depends on: TASK-007 · Blocks: nothing hard (TASK-009 parallel-safe)

---

## 1. Problem Statement

Receipts exist as wire structures (TV-002-exact) with a store and a
projection, but nothing creates them from a real task or moves them
between peers. This task wires the receipt lifecycle (trust §3: requester
signs → RECEIPT → provider countersigns → RECEIPT_ACK) onto TASK-007's
task completion, and implements pull-based evidence exchange
(EVIDENCE_QUERY/EVIDENCE_SET, trust §4) over sessions.

## 2. Functional Requirements

- FR-1 Receipt creation: `make_receipt_core(inputs: &ReceiptInputs,
  requester: &AgentId, provider: &AgentId, rating: Option<u8>) ->
  ReceiptCore` (outcome mapping: task Verified→0, Failed→1, ttft-cancel→
  3-cancelled; per trust §3 outcome enum 0 ok / 1 failed / 2 timeout /
  3 cancelled / 4 disputed).
- FR-2 RECEIPT flow (0x0300/0x0301) over a session: requester seals
  `Receipt {1: core-bytes, 2: sig_req}` as RECEIPT content; provider
  verifies core matches ITS OWN observation of the task (task-id, cap,
  usage within tolerance ±5% per-field, timestamps within ±2 s), then
  replies RECEIPT_ACK carrying the dual-signed
  `Receipt {1: core, 2: sig_req, 3: sig_prov}`; both sides
  `ReceiptStore::ingest` (idempotent). Tolerance violation → provider
  replies ERROR 5002-family (use trust-range code; check errors.rs) and
  does NOT countersign.
- FR-3 EVIDENCE_QUERY/EVIDENCE_SET (0x0302/0x0303) per trust §4 CDDL:
  query {1 subject, ?2 cap, ?3 [from,to], ?4 limit (default 64),
  ?5 role-filter}; response {1: [* Receipt], ?2: more}. Holder filters
  its store; consumer verifies EVERY receipt independently
  (`Receipt::verify()`) before `TrustProjection::apply` — evidence sets
  are untrusted transport (trust §4).
- FR-4 End-to-end: extend the TASK-007 integration test — after echo
  completes Verified, run FR-2, then a third agent C sessions to B,
  EVIDENCE_QUERYs about B, receives the dual-signed receipt, verifies,
  applies to its projection, and `score(B, cap)` is Some.
- FR-5 No self-reported trust anywhere (decision 2): the API surface
  must make it impossible to apply an unverified receipt to a projection
  (typestate: `apply` takes `Verified`, which only `verify()` mints —
  this already holds in map-trust; preserve it through the new plumbing).

## 3. Non-Functional Requirements

Pinned deps; no threads inside flows (pump-driven like TASK-007);
zero warnings. Evidence batches respect max_message_size: limit
batch to 8 receipts per EVIDENCE_SET, `more` for the rest (receipts are
~330 B; 8 is conservative and simple).

## 4. System Architecture

Receipt/evidence FLOWS live in `map-taskrun` (it already owns the
session-facing runtimes and may depend on trust) — add `map-trust` to
its deps. Pure helpers (FR-1, tolerance check) live in `map-trust`.
Dependency direction stays legal (taskrun → {tasks, trust, session,
core, identity}).

## 5. Component Design

```
crates/map-trust/src/     (extend)
  lifecycle.rs   make_receipt_core, within_tolerance(a,b) -> bool
crates/map-taskrun/src/   (extend)
  receipts.rs    requester_issue(session, store, inputs...) /
                 provider_countersign(incoming, own_observation, kp,
                 store) -> Vec<Outgoing>
  evidence.rs    serve_evidence(incoming, store) -> Vec<Outgoing>;
                 query_evidence(session, subject, ...) -> Vec<Receipt>
```

## 6. Interface Specifications

`provider_countersign` precondition: provider has a recorded
`ReceiptInputs` for the task (its own observation from TASK-007's
ProviderTask — expose it there if not already). Postcondition: on
success both signatures verify and the ACK carries the FULL dual-signed
receipt bytes; store ingest happened exactly once (duplicate ACK →
Ingest::Duplicate, no error). All verification over received bytes (I1):
the countersigner signs the REQUESTER'S core bytes, never a re-encoding.

## 7. Data Structures

Reuse `Receipt`/`ReceiptCore`/`ReceiptStore`/`TrustProjection`
unchanged. `Observation` = ReceiptInputs + AgentIds (type alias or small
struct in map-taskrun).

## 8. Algorithms

Tolerance (FR-2): |a−b| ≤ max(1, 0.05·max(a,b)) per usage field;
timestamps |Δ| ≤ 2000 ms. Evidence filter: linear scan of store (fine at
scaffold scale), window/cap/role filters ANDed; role filter checks which
signature slots are present + subject position.

## 9. Edge Cases

Receipt for unknown task → provider ERROR, no countersign. Rating absent
→ core omits key 9 (already optional). Query matching nothing → empty
set, `more` absent. Duplicate RECEIPT (retry) → same ACK re-sent
(idempotent, ops §2 rule 3). Receipt with only sig_prov ingested via
evidence → verify() must still pass structurally (trust §3: at least one
sig) but projection apply requires BOTH (dual-signed only — decision 2);
enforce and test.

## 10. Error Handling

Tolerance failure names the offending field in the report/log payload
(but ERROR detail MUST NOT echo usage values to unauthenticated peers —
in-session peers are authenticated, so detail is allowed here).

## 11. Security Considerations

The dual-sign-before-trust rule is the core defense (C1): tests MUST
show (a) single-signed receipts never reach a projection, (b) a receipt
with a tampered core byte fails verify, (c) a receipt whose task-id
doesn't match any local observation is never countersigned (prevents a
requester farming countersignatures for fictional work).

## 12. Performance Considerations

Linear store scans acceptable; note projection apply is O(1) per receipt.

## 13. Input Specifications

Pump-driven Incoming + injected clock for tolerance windows.

## 14. Output Specifications

Outgoing lists; updated stores/projections; counters (receipts issued /
countersigned / rejected).

## 15. Acceptance Criteria

- ✓ Workspace green, zero warnings, no new deps.
- ✓ FR-4 three-agent end-to-end test passes.
- ✓ §11 security tests (a)(b)(c) pass.
- ✓ Duplicate-RECEIPT idempotency test passes.
- ✓ Tolerance boundary tests (exactly at, just past) pass.
- ✓ TV-002 conformance still green (wire format untouched).

## 16. Comprehensive Test Plan

Unit: make_receipt_core outcome mapping (all 4 task terminals);
within_tolerance boundaries; evidence filters (each, combined); batch
split at 8 + `more`.
Integration: FR-4 end-to-end; duplicate RECEIPT; tolerance-violation
rejection; single-signed-via-evidence blocked from projection.
Security: §11 (a)(b)(c).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, extend `crates/map-trust` (lifecycle.rs: FR-1 helpers +
> tolerance) and `crates/map-taskrun` (receipts.rs, evidence.rs; add
> map-trust to its deps) per §4–§14. Field numbers and flows per trust
> §3–§4 (`map-spec/draft-map-trust-00.md`) — verify before coding; STOP
> and report any under-specification (candidate: the exact ERROR code
> for tolerance rejection — check `errors.rs` 5xxx range and propose if
> absent). NEVER let an unverified or single-signed receipt reach
> TrustProjection::apply. Countersign over the requester's received core
> bytes (I1). Wire format is frozen by TV-002 — conformance guards it.
> `cargo +stable-x86_64-pc-windows-gnu` from PowerShell; full suite per
> fix. Deliver: code, tests per §16, report.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo +stable-x86_64-pc-windows-gnu test --workspace
cargo test -p map-conformance   # TV-002 unchanged
grep -rn "apply" map-rs/crates/map-taskrun/src | grep -v Verified  # inspect
```

## 19. Known Risks

R1: trust §3 doesn't pin the tolerance numerically — the ±5%/±2 s here
is the architect's assignment; the implementer records it in trust §3 as
a spec patch (same-change rule). R2: outcome mapping for ttft-cancel
(3-cancelled) vs timeout (2) is judgment — documented in FR-1, patch
trust §3 with the mapping table.

## 20. Handoff Notes

Update memory.md §5/§8 (step 4 done). After this, the trust loop is
closed: work → dual-signed evidence → local score. TASK-009 adds the
payment leg on the same completion event.
