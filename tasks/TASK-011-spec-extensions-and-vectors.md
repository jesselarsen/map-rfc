# TASK-011 — Spec Completion: Extension Code Points, Reservations, Vector Round 3

Status: READY · Priority: P2 · Target: `map-spec/` (spec-authoring task) + conformance sync
Depends on: TASK-004 · Blocks: draft advancement; TASK-014 (formal) benefits

---

## 1. Problem Statement

Rev-00's acknowledged spec gaps (memory.md §4): TBD-BID / TBD-KVX /
TBD-GIT extensions have no code points and only sketches; reservations
(tasks §6) lack CDDL and an FSM; and the vector suite lacks a bound
PAY_QUOTE (pay §3 key 9 now normative) and payment-flow coverage. This
is a SPEC-AUTHORING task executed by the coding agent under tight
guardrails: the code points below are the architect's final assignments
(immutable once merged, ops §3.3); the agent's job is to write the
normative text, CDDL, and vectors around them — NOT to choose numbers.

## 2. Functional Requirements

- FR-1 Registry assignments (FINAL — record in core §5/§15 message-type
  and extension-key registries and ops §3):
  - Extension keys: `1 = ext-bid` (tasks §2.2), `2 = ext-kvx`
    (tasks §8.4), `3 = ext-git` (tasks §9 mention).
  - Message types: `0x020F = RESERVE`, `0x0210 = LEASE`,
    `0x0211 = LEASE_RELEASE` (0x0206 REMAINS unassigned, decision 11).
- FR-2 Reservations (tasks §6, sketch → normative): CDDL + FSM:
  `Reserve = {1 => bstr .size 16 rsv-id, 2 => CapURI, 3 => Timestamp
  from, 4 => Timestamp until, ?5 => TaskConstraints, ?6 => PayRef,
  ?7 => ExtMap}`; `Lease = {1 => rsv-id, 2 => uint status: 0 granted /
  1 rejected / 2 revoked, ?3 => [Timestamp, Timestamp] granted window,
  ?4 => PayQuoteRef, ?5 => ExtMap}`; `LeaseRelease = {1 => rsv-id,
  ?2 => uint reason}`. FSM: Requested → Granted → (Active during window)
  → Released/Expired/Revoked; a granted lease obliges the provider to
  ACCEPT a conforming ASSIGN inside the window or the miss is
  receipt-recordable (outcome 1) — write the normative MUSTs, the
  idempotency rule (rsv-id), and Security Considerations (reservation
  squatting → PayRef-carried holding cost is the mitigation; write it).
- FR-3 ext-bid (tasks §2.2, sketch → complete extension spec): fields it
  adds to Offer/Quote ExtMap under ext key 1: `{1: bid-deadline
  Timestamp, 2: queue-wait-ms uint}`; negotiation via HELLO extensions
  (core §6.1 key 3); explicitly NO clearing rules (C5) — selection stays
  local policy.
- FR-4 ext-kvx and ext-git: SHORT normative stubs only (one section
  each): negotiation key, zone restriction (KVX: Zone ≤1 or attested,
  per tasks §8.4; GIT: Zone ≤1, shallow/path-scoped/token-gated per
  tasks §9), payload container `{1: version, 2: bstr opaque}`, and an
  explicit "full semantics deferred; code point reserved" marker. Do NOT
  invent full designs.
- FR-5 Vector round 3 (extend gen_vectors.py; TV-001..012 frozen):
  - TV-013 bound PAY_QUOTE (key 9 = TV-002 task-id) — the vector my
    2026-07-04 pay §3 patch anticipated;
  - TV-014 PAY_AGREE binding TV-013 (digest computed per pay §3.1);
  - TV-015 RESERVE + TV-016 LEASE(granted) pair;
  - NV-007 PAY_QUOTE answering an OFFER but key 9 mismatching (MUST
    reject per pay §3);
  - NV-008 LEASE granted outside the requested window (MUST reject).
  Skip any vector that already landed from the architect's 2026-07-04
  notes — check the vectors file first and report what existed.
- FR-6 Conformance sync: map-conformance consumes every new TV/NV.
- FR-7 Registry hygiene: every new code point/extension gets a row in
  the owning document's IANA-style section AND ops §3 if a new registry
  column is needed; assignments marked immutable.

## 3. Non-Functional Requirements

BCP 14 language, unweakened; RFC section conventions matching the
existing drafts; one-source-of-truth (no duplicated normative text —
cross-reference); Python vector rules per TASK-004 §3 (deterministic,
self-testing, idempotent, regression asserts).

## 4–8. Architecture / Design / Interfaces / Data / Algorithms

Spec-side: edits land in draft-map-tasks-01.md (§2.2, §6, §8.4, §9),
draft-map-core-01.md (§5 ranges note, §15 registries), draft-map-ops-01
(§3 registry rows), draft-map-pay-01.md (cross-refs only). Rust-side:
msg_type.rs gains RESERVE/LEASE/LEASE_RELEASE consts + names (mirror
rule, decision 11); conformance vectors2/extended pattern continues. No
runtime reservation implementation in this task (codecs + vectors only;
the runtime is future work).

## 9. Edge Cases

Reservation window in the past → Lease rejected(1). Overlapping
reservations → provider policy (write as such, not protocol). rsv-id
reuse with different window → idempotency violation, reject.

## 10. Error Handling

Assign reservation-range behavior to existing 3xxx/5xxx codes where
they fit; propose new codes only if none fit (report first).

## 11. Security Considerations

Each new spec section MUST have its own Security Considerations
paragraph (squatting, lease-DoS, bid-sniping via queue-wait-ms lies —
tie the last to trust §8 checkability).

## 12–14. Performance / Input / Output

N/A beyond vector determinism.

## 15. Acceptance Criteria

- ✓ All FR-1 assignments recorded in ALL owning registries; 0x0206
  untouched.
- ✓ tasks §6 has complete CDDL + FSM + MUSTs + Security Considerations;
  §2.2/§8.4/§9 have their extension sections (full for bid, stubs for
  kvx/git).
- ✓ gen_vectors.py green, idempotent, TV-001..012 bytes unchanged;
  new TVs/NVs published with full hex + lengths.
- ✓ map-conformance green including all new cases.
- ✓ msg_type.rs mirror updated; workspace green, zero warnings.

## 16. Comprehensive Test Plan

In-script vector asserts (signatures, regressions, NV properties);
conformance produce+consume per TV, reject-with-class per NV; msg_type
name() covers new consts.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> This is a SPEC-AUTHORING task in `map-spec/` plus vector/conformance
> sync in `map-rs/`. The code-point assignments in §FR-1 are FINAL — use
> them verbatim, never renumber. Write the normative text per §FR-2..4
> matching the style of the existing drafts (BCP 14, CDDL blocks,
> Security Considerations per section, registry rows). Where §FR-4 says
> stub, write a stub — do not design full KVX/GIT semantics. Then extend
> `gen_vectors.py` per §FR-5 (check which vectors already exist first;
> TV-001..012 frozen with regression asserts) and sync map-conformance
> and msg_type.rs. `cargo +stable-x86_64-pc-windows-gnu` from
> PowerShell. Deliver: spec diffs, vectors, code, report listing every
> registry row added and anything that felt under-determined.

## 18. Validation Checklist (reviewer)

```
grep -n "0x020F\|0x0210\|0x0211" map-spec/draft-map-core-01.md map-rs/crates/map-core/src/msg_type.rs
grep -n "ext-bid\|ext-kvx\|ext-git" map-spec/draft-map-*.md
cd map-spec/test-vectors && python3 gen_vectors.py && python3 gen_vectors.py && git diff --quiet map-test-vectors.md && echo idempotent
cd ../../map-rs && cargo +stable-x86_64-pc-windows-gnu test --workspace
```

## 19. Known Risks

R1: spec-authoring by a coding agent risks style drift — mitigated by
the explicit style anchor (match existing drafts) and reviewer diff
reading. R2: FR-5 may partially duplicate vectors landed from the
architect's earlier notes — the check-first instruction handles it.

## 20. Handoff Notes

This closes the last acknowledged REV-00 spec gaps except formal
analysis (TASK-014) and delegation depth already handled (TASK-010).
After this + TASK-010, consider cutting rev-01 of tasks/identity/core
per ops §4 (changelog: additive).
