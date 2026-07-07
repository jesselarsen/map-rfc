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
