# MAP Spec Errata — PENDING-ARCHITECT queue

The architect role is unstaffed after 2026-07-06 (tasks/README.md "Operating
without the architect"). Items here are wire-affecting, security-relevant, or
judgment-shaped questions a coding package did NOT pre-decide, so they are NOT
patched into normative text unilaterally. A future architect session resolves
this queue (CLAUDE.md directs that session to read this file first).

Format: `PENDING-ARCHITECT: <date> <package> — <quote/issue> — <proposal>`.

---

PENDING-ARCHITECT: 2026-07-05 TASK-007 — tasks §2 `Quote = {1 TaskID, 2 =>
PayQuoteRef, 3 ttft, 4 expiry, ?5 ext}` makes field 2 (PayQuoteRef) REQUIRED
(no `?`), but TASK-007 FR-1 defers all pay refs ("Pay refs are OPTIONAL and
omitted in this package"). Every QUOTE the echo runtime emits therefore omits
field 2 and is draft-non-conformant. The conflict rule says drafts win over
task-doc text, and demoting a required field is wire-affecting — hence this
entry rather than a silent absorption. Proposal: change tasks §2 to
`? 2 => PayQuoteRef` (payment is optional at the protocol layer; TASK-009
wires it). The map-tasks codec already models it as an omittable
`Option<Value>`, so no code change is needed once ruled.

PENDING-ARCHITECT: 2026-07-05 TASK-007 — the frozen vector TV-007 (ASSIGN)
carries `InputSpec = {2: [multihash]}` (content-addressed), but TASK-007 §7
scopes the map-tasks `InputSpec` codec to inline bytes (key 1) only; keys 2/3
(content-addressed fetch / ContextRef) are out of scope. Consequence: no Rust
codec round-trips the frozen TV-007 ASSIGN, which is a parity hole for
TASK-015 (Go impl "at frozen-vector parity" — ops §6.1 two-implementation
gate). Not fixed here (echo mints its own inline ASSIGNs). Proposal: a later
package models InputSpec keys 2/3 and adds a TV-007 produce/consume guard to
the conformance crate; until then, TV-007 is "consume-only" for Rust.

PENDING-ARCHITECT: 2026-07-06 TASK-008 — no core §9 error code exists for a
provider REJECTING a RECEIPT because the requester's claimed usage/timestamps
fall outside the countersignature tolerance (trust §3). The 5xxx range is
execution (5000 task-failed, 5001 deadline, 5002 cancelled, …); none means
"receipt claim rejected / observation mismatch". TASK-008 assigns no new code
point (code points are immutable, assign-only-where-a-package-declares-FINAL).
Interim: map-taskrun `provider_countersign` replies ERROR 5000 (TASK_FAILED).
Proposal: register a dedicated code — candidate 2006 (auth/authz: "receipt
countersignature refused") or a trust-family 7xxx range — and update
map-taskrun's `RECEIPT_REJECT_CODE`.

PENDING-ARCHITECT: 2026-07-06 TASK-008 — trust §2's lifecycle diagram shows the
countersignature returned as `RECEIPT_ACK {sig_prov}` (just the provider
signature), but TASK-008 FR-2 (and this implementation) return the FULL
dual-signed `Receipt {1: core, 2: sig_req, 3: sig_prov}` as the RECEIPT_ACK
content — simpler for the requester (no reassembly) and there is no frozen
vector for the ACK. Draft-vs-task-doc divergence on a wire-message shape.
Proposal: update the §2 diagram to `RECEIPT_ACK {full dual Receipt}`; no code
change. (Recorded rather than silently absorbed, per the Quote.2 precedent.)
