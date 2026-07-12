# TASK-018 — InputSpec keys 2/3 (TV-007 Rust parity) + NV-009 depth vector

Status: READY · Priority: P2 · Targets: `map-rs` (map-tasks, conformance),
`map-spec/test-vectors/` (gen_vectors.py + md, additive only), `map-go`
(conformance list only)
Depends on: TASK-015 (Go conformance exists), TASK-011 (frozen set) ·
Blocks: live interop matrix over ASSIGN

Authored by the 2026-07-12 architect session, implementing two ERRATA
rulings (TV-007 parity; max-depth counting + NV-009).

---

## 1. Problem Statement

Two gaps, one package because both end in conformance-suite updates.

(a) Frozen TV-007 (ASSIGN) carries `InputSpec = {2: [multihash]}`
(content-addressed inputs), but the Rust map-tasks codec models inline
bytes (key 1) only — Rust is consume-blind on TV-007 while Go (TASK-015)
round-trips it. Model keys 2 and 3 in the codec and add the TV-007
produce/consume guard to the Rust conformance crate. Codec + guard ONLY:
actually fetching content-addressed inputs stays future work.

(b) identity §4 (as patched 2026-07-12) pins `max-depth` counting: a cert
with max-depth d admits at most d certificates between itself and the
leaf. TV-012 (depth 1, 1 link) cannot distinguish this from other
readings. Add negative vector **NV-009**: a 2-link chain whose PARENT
cert declares max-depth 0 — child link exceeds it — MUST reject 2003.
Both implementations gain the guard.

## 2. Functional Requirements

- FR-1 map-tasks `InputSpec` models all three keys per tasks §2.1 CDDL:
  `? 1 => bstr` (inline), `? 2 => [+ Multihash]` (content-addressed),
  `? 3 => ContextRef` (model as opaque `Value` if tasks §2.1 leaves it
  open — cite the clause in a comment). At least one key MUST be present;
  encode/decode round-trips each and any combination byte-exactly.
- FR-2 Rust conformance: TV-007 produce (rebuild from seeds byte-exact,
  first-diff-offset reporting) + consume (decode the frozen payload
  through the map-tasks Assign codec, not a raw-CBOR bypass).
- FR-3 Vectors (gen_vectors.py — ADDITIVE ONLY, TV-001..015/NV-001..008
  bytes MUST NOT change; the script's frozen-bytes asserts are the guard):
  add NV-009 = OFFER envelope signed by SK carrying chain [child, parent]
  where parent = RIK-A→DK, scope ["map:cap:inference/text-generation"],
  **max-depth 0**, msg-id `msgid(22)`; child = DK→SK, same scope,
  msg-id `msgid(23)`; OFFER msg-id `msgid(24)`. Same TS/expiry constants
  as NV-007. Required class: BAD_DELEGATION (2003), reason: max-depth
  exceeded. Regenerate the md (UTF-8!), append the NV-009 section in the
  NV-007/008 style, update the ERRATA bookkeeping note (NV counter → 9).
- FR-4 Rust conformance rejects NV-009 with 2003 (via the TASK-010
  `validate_chain`/`open_delegated` path). If the existing Rust
  max-depth logic disagrees with the identity §4 counting rule, FIX THE
  CODE to the spec (spec wins) and say so in the report.
- FR-5 map-go: add "NV-009" to the produce list in
  `conformance/produce.go` (mirroring its NV-007 builder with the depth-0
  parent) and to the parity/reject tests. Go's existing depth check
  (`i <= MaxDepth`) already matches the ruling — expect green without
  logic changes; if not, fix Go to spec and report.
- FR-6 Determinism: no wall-clock, no randomness, anywhere.

## 3. Non-Functional Requirements

Zero new deps in either impl. gen_vectors.py stays self-testing
(in-script signature verify + regression asserts extended to NV-009).

## 4–8. Architecture / Design / Data / Algorithms

InputSpec stays a struct with three Options (Rust idiom per workspace
conventions); validation "at least one present" lives in decode.
NV-009's chain reuses the NV-007 key tiers (RIK-A, DK, SK — seeds
already labelled). Depth rule implementation: for cert at chain index i
(leaf = 0), require `i <= max_depth` when declared.

## 9. Edge Cases

max-depth 0 on the LEAF cert of a 1-link chain is VALID (zero certs
below it) — add a positive unit test in both impls so the boundary is
pinned, not just the violation.

## 10. Error Handling

NV-009 ⇒ 2003 in both impls, asserted by class not string.

## 11. Security Considerations

max-depth is an attenuation bound; an off-by-one in either direction
either breaks legitimate chains or admits unbounded re-delegation. The
positive boundary test (§9) plus NV-009 pin both sides.

## 12–14. Performance / IO

None.

## 15. Acceptance Criteria

- ✓ gen_vectors.py runs clean; TV-001..015 + NV-001..008 bytes unchanged
  (script asserts + git diff shows only the NV-009 addition and header
  counts).
- ✓ Rust: workspace green; conformance count grows by ≥2 (TV-007
  produce+consume, NV-009 reject, depth-boundary positive).
- ✓ Go: `go test ./...` green with NV-009 in produce parity + reject.
- ✓ ERRATA bookkeeping note updated (counter at NV-009).

## 16. Comprehensive Test Plan

Rust: codec unit tests for InputSpec key combinations (cite tasks §2.1);
conformance TV-007/NV-009 cases; depth-boundary positive. Go: NV-009 in
TestProduceParity + TestReject; depth-boundary positive in envelope
package tests.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-018 §FR-1..6. Read FIRST: `map-spec/ERRATA.md` (RESOLVED
> 2026-07-12 entries for TV-007 parity and max-depth), tasks §2.1,
> identity §4 as patched, `map-spec/test-vectors/gen_vectors.py`, and the
> NV-007 sections of the vectors md. Order of work: (1) extend
> gen_vectors.py with NV-009 per §FR-3 — additive, regenerate, verify the
> frozen-bytes asserts still pass and git diff touches only NV-009
> material; (2) extend the Rust map-tasks InputSpec codec and conformance
> per §FR-1/2/4; (3) add NV-009 + the depth-boundary positive to map-go
> per §FR-5 (Go work is conformance-list mechanical — do NOT restructure
> map-go). Python needs pynacl/cbor2/base58; keep the file UTF-8. Rust:
> PowerShell, `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"`,
> `cargo +stable-x86_64-pc-windows-gnu test --workspace`. Go:
> `$env:PATH="C:\Program Files\Go\bin;$env:PATH"; go test ./...` in
> map-go. If existing Rust max-depth logic disagrees with identity §4's
> counting rule, change the CODE, not the spec, and report it. Report
> every ambiguity found; do not touch normative spec text (spec-feedback
> items go to ERRATA as new PENDING-ARCHITECT entries).

## 18. Validation Checklist (reviewer)

```
cd map-spec/test-vectors && python gen_vectors.py      # asserts pass
git diff --stat map-spec/test-vectors/                 # additive only
cargo +stable-x86_64-pc-windows-gnu test --workspace   # (mingw on PATH)
cd map-go && go test ./...
```

## 19. Known Risks

R1: gen_vectors.py accidental reflow changing frozen bytes — the
in-script asserts and git diff review are the guards. R2: Rust TASK-010
depth semantics may differ from the ruling — FR-4 pre-decides: code
follows spec. R3: scope creep into content-addressed FETCH semantics —
resist; codec + guards only.

## 20. Handoff Notes

Update memory.md §6 (vector counter → NV-009; TV-007 now full-parity in
BOTH impls) and tasks/README.md row 018. This closes the last known
byte-level parity asymmetry between impl #1 and impl #2; the next
package after this should be the live interop matrix (Go session layer).
