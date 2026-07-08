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

PENDING-ARCHITECT: 2026-07-06 TASK-009 — pay §3.1 requires payment-reference
digests over "the received ENVELOPE bytes (I1 — never over a re-serialization)",
but the pump/session architecture surfaces parsed message CONTENT to the
runtimes: neither `Session::send` (returns `Result<()>`) nor `recv` (returns
`Incoming` = parsed content) exposes the sealed/received envelope frame. TASK-009
therefore computes `pay_ref_digest` over the canonical re-encoding of the
referenced message content (deterministic → both peers agree; a switched quote
→ different content → different digest, so the price-switch defense holds).
This is a re-serialization, contrary to §3.1's parenthetical. Proposal: either
(a) accept content-binding for payment refs (amend §3.1), or (b) thread the
sealed-on-send / raw-on-recv envelope bytes through map-session so refs can bind
the exact envelope. Not resolved unilaterally (wire-adjacent, I1-adjacent).

PENDING-ARCHITECT: 2026-07-06 TASK-009 — the pay 6xxx range (core §9: 6000
quote-expired, 6001 payment-required, 6002 settlement-failed, 6003 dispute,
6004 method-unsupported) has no distinct codes for the several TASK-009 pay
rejections: cap-exceeded (FR-6), quote-digest mismatch / price-switch (§7),
task-binding mismatch (FR-2), and forged/mismatched ledger proof. All four use
6002 (SETTLEMENT_FAILED) in the interim (`PAY_REJECT` in map-taskrun/payments.rs);
6000 is used for expired quotes and 6004 for an unoffered method. Proposal:
register distinct codes (candidates: 6005 authorized-cap-exceeded, 6006
agreement-digest-mismatch, 6007 task-binding-mismatch, 6008 invalid-payment-
proof) and update `PAY_REJECT`. No new code points assigned here (assign-only-
where-a-package-declares-FINAL).

PENDING-ARCHITECT: 2026-07-08 TASK-010 — identity §4 chain validation states
"the chain root MUST equal the envelope `kid`", but TASK-010 FR-3 requires the
data envelope be signed by the delegated LEAF key ("verify the envelope
signature against the LEAF key; the kid MUST be the leaf subject"). These
contradict: the `kid` (COSE key id) names the key that produced the envelope
signature, and the actual signer of a delegated data envelope is the leaf (SK),
not the root (RIK) — so `kid` == root is cryptographically impossible unless the
root signs every message (defeating the purpose of delegation). The reference
implementation follows FR-3 (kid == chain leaf subject; the root is returned as
the accountable identity), and TV-012 freezes this at the byte level. An
editorial note was added to identity §4 pointing here; the normative sentence
was left unchanged pending an architect ruling. Proposal: replace "the chain
root MUST equal the envelope `kid`" with "the envelope `kid` MUST equal the
chain LEAF's subject; the chain root is the accountable identity." (Wire-frozen
already, so this is a wording fix, but security-relevant — hence queued.)

Also noted (report, not blocking): pay §4 `PayInvoice` key 4 (usage watermark)
is `{ usage watermark }` with no concrete field shape; TASK-009 (postpaid flat)
omits it. A metered-terms (terms 3) package must define its CDDL.

Also noted (TASK-010, not blocking): the negative vector for widened delegation
scope is **NV-007**, not the "NV-006" named in TASK-010 FR-6 — NV-006 was already
taken by the 2026-07-08 bound-PAY_QUOTE pass. The NV counter now stands at 007;
TASK-011's planned NV-007/008 should continue from NV-008.
