# MAP Spec Errata — PENDING-ARCHITECT queue

The architect role was unstaffed 2026-07-06..2026-07-11. On 2026-07-12 an
architect session ruled on every queued item (rulings below, spec patched
in the same change). Implementation work arising from the rulings is
packaged as TASK-017 and TASK-018 (see `tasks/README.md`).

**As of the rev-01 cut (2026-07-13) every RESOLVED item below is folded into
the normative text of the `-01` drafts; the errata annotations were removed
from the specs and the classified delta recorded in `CHANGELOG.md`. This file
is retained as the historical rationale for each ruling.**

Format: `RESOLVED <ruling-date> (was PENDING-ARCHITECT <date> <package>) —
<issue> — <ruling> — <where implemented>`.

---

PENDING-ARCHITECT: 2026-07-14 TASK-022 — metered (terms 3) stream
pause/resume signaling is under-specified. pay §4/§4.1 specify that the
provider pauses an exhausted stream with ERROR 6001 (retryable) carrying the
final unpaid watermark in the detail map, and that on CANCEL the owed amount
anchors to the last countersigned checkpoint. Two mechanics are NOT pinned:
(a) the TRIGGER by which the provider learns the requester cannot pay an
outstanding invoice — postpaid has no "I can't pay" message; the requester
simply withholds the PAY_PROOF, so the provider only infers exhaustion from
absence, which a synchronous protocol cannot express without a timeout or an
explicit signal; and (b) the RESUME envelope after a covering PAY_PROOF —
pay §4 says "the stream resumes" but names no resume message, leaving open
whether the provider re-issues the paused invoice, emits a fresh CHUNK, or
continues silently. TASK-022 implements an INTERIM that only sequences the
already-specified messages (a provider-internal `pause` that emits the
spec'd 6001+watermark; a requester `retry_pending` that re-attempts the
withheld proof after top-up; resume = the provider clearing its paused flag
on `on_proof`) and adds NO new wire message — so nothing normative is
invented. Proposal for a future ruling: either (1) add an explicit
requester→provider "insufficient, need credit" signal (candidate: reuse
ERROR 6001 in the requester→provider direction, or a new FLOW-style control),
and a provider→requester RESUME marker (candidate: a kind-2 log CHUNK or a
re-sent invoice with the same id under the idempotency rule); or (2)
formally define the pause as an out-of-band/timeout condition and keep the
wire silent, documenting that resume is "provider continues emitting chunks
once a covering proof is observed". Not decided here (wire-affecting).

---

RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-05 TASK-007) — tasks §2
made Quote field 2 (PayQuoteRef) REQUIRED while every echo-runtime QUOTE
omits it. RULING: field 2 is OPTIONAL (`? 2 =>`); payment is optional at
the protocol layer, and a required pay ref would force a pay stack on
gratis meshes. tasks §2 patched. No code change (the map-tasks codec
already models `Option<Value>`). Demoting required→optional is the
compatible direction for existing receivers.

RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-05 TASK-007) — the
frozen TV-007 ASSIGN carries content-addressed `InputSpec = {2:
[multihash]}` but the Rust map-tasks codec models inline key 1 only, so
Rust cannot round-trip TV-007 (Go can, since TASK-015). RULING: model
InputSpec keys 2 (content-addressed) and 3 (ContextRef) in map-tasks and
add a TV-007 produce/consume guard to the Rust conformance crate; fetch
SEMANTICS (actually retrieving content-addressed inputs) remain future —
the codec and the vector guard are the deliverable. → TASK-018.
[IMPLEMENTED 2026-07-12: map-tasks InputSpec models keys 1/2/3 (≥1 required);
Rust conformance TV-007 is now produce+consume. Both impls at full TV-007
parity.]

RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-06 TASK-008) — no error
code existed for a provider refusing to countersign a receipt whose
claimed usage/timestamps exceed the countersignature tolerance (trust §3);
interim was 5000 TASK_FAILED (misleading: the task did not fail). RULING:
register **2006 countersignature refused** in the 2xxx auth/authz family
(a refusal to attest is signature-adjacent; a new 7xxx family for one code
is not justified). core §9 + trust §2 patched; `RECEIPT_REJECT_CODE`
rewiring → TASK-017. retryable = false (the requester must change its
claim, not retry it). [IMPLEMENTED 2026-07-12.]

RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-06 TASK-008) — trust §2
diagram showed `RECEIPT_ACK {sig_prov}` while the implementation returns
the full dual-signed Receipt. RULING: the implementation shape is
normative — RECEIPT_ACK carries the COMPLETE dual-signed Receipt (no
reassembly at the requester; no frozen vector existed for the thin form).
trust §2 patched. No code change.

RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-06 TASK-009) — pay §3.1
requires payment-reference digests over received ENVELOPE bytes (I1), but
map-session surfaces only parsed content, so TASK-009 bound digests to a
canonical re-encoding of content. RULING: **pay §3.1 stands as written —
envelope-byte binding.** Decisive: frozen TV-013 key 2 is the Multihash of
the TV-010b ENVELOPE bytes, so a content-binding implementation computes a
DIFFERENT digest and fails interop against any conformant peer (map-go
already implements the envelope-byte form). map-session must expose
sealed-on-send / raw-on-recv envelope bytes and map-taskrun payments must
bind them. → TASK-017 (the substantive package of this queue).
[IMPLEMENTED 2026-07-12: `Session::send` returns sealed bytes, `Incoming.raw`
added; requester digests `inc.raw`, provider binds via `bind_quote_envelope`.]

RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-06 TASK-009) — four
distinct pay rejections (cap exceeded, quote-digest mismatch, task-binding
mismatch, forged/mismatched proof) all collapsed to 6002. RULING: register
**6005 authorized-cap-exceeded, 6006 agreement-digest-mismatch, 6007
task-binding-mismatch, 6008 invalid-payment-proof**; 6002 remains the
generic settlement failure. core §9 patched; `PAY_REJECT` split →
TASK-017. [IMPLEMENTED 2026-07-12.]

RESOLVED 2026-07-12 (was PENDING-ARCHITECT 2026-07-08 TASK-010) — identity
§4 "the chain root MUST equal the envelope `kid`" was cryptographically
impossible for leaf-signed data envelopes. RULING: `kid` MUST equal the
chain LEAF's subject; the chain root's issuer is the accountable identity
verifiers resolve and report. identity §4 AND §3.2 B1/B3 patched (B1/B3
carried the same contradiction); the old editorial note replaced with a
resolution pointer. Wire behavior unchanged (frozen by TV-012). No code
change (both impls already do this).

RESOLVED 2026-07-12 (map-go ambiguity report items, TASK-015) — (a) shard
textual form in K′ preimages pinned to unpadded base-10 ASCII decimal
(discovery §6 patched); (b) `max-depth` counting pinned: a cert with
max-depth d admits at most d certificates between itself and the leaf
(identity §4 patched); a depth-exceeded negative vector NV-009 exercises
it → TASK-018 [IMPLEMENTED 2026-07-12: NV-009 added to gen_vectors.py
(additive; frozen bytes untouched) + reject guards and depth-boundary
positive tests in both impls]; (c) Hello-vs-HelloAck field-shape asymmetry: caution note
added to core §6.1; (d) policy-reject NVs (NV-004/006/008) intentionally
carry no core §9 code — no change. NOTE: with 6007 now registered, an
in-session rejection of a mismatched task binding SHOULD use ERROR 6007
(the NV-006 conformance expectation stays a policy check; the code is for
the wire reply).

---

## Still open (future packages; not blocking)

RESOLVED 2026-07-14 (was OPEN: pay §4 `PayInvoice` key 4 had no concrete
shape) — ARCHITECT DESIGN: `UsageWatermark = {1 TaskID, 2
through-chunk-seq, 3 cumulative Usage, 4 cumulative-billed}` with the
§4.1 ordered verification checks. Key decisions and why: CUMULATIVE
semantics, never delta (byte-identical retransmission ⇒ idempotency for
free; self-contained verification; monotonicity is a component-wise >=);
reuse the existing Usage map shape (one usage vocabulary across
Complete.4, ReceiptCore.8, checkpoints, kind-3 chunks, watermarks);
rate-table unit codes pinned to per-SINGLE-unit prices (per token, per
millisecond) so every bill is an exact uint×decimal product — NO rounding
rule exists anywhere in pay, by construction; §3 same-exponent/same-asset
rule makes comparisons mantissa-only (codifies the reference impl's
existing `amount_within` behavior); tasks §3 kind-3 chunk content pinned
to deterministic-CBOR cumulative Usage (the meter readings check 4
verifies against — a provider MUST meter before it bills); new code 6009
invoice-watermark violation (6007 wording broadened to cover
UsageWatermark.1; cap breach stays 6005); the 6001 "final unpaid
watermark" travels in the ERROR detail map as {1 => UsageWatermark}; on
CANCEL the owed amount anchors to the last countersigned checkpoint.
Spec patched (pay §3/§4/§4.1, tasks §3, core §9) in the same change.
→ TASK-021 (codec + TV-016/NV-010 vectors, both impls) then TASK-022
(terms-3 runtime, Rust).

NOTE (bookkeeping, TASK-010): the widened-scope negative vector is NV-007
(TASK-010's planned "NV-006" was already taken by the bound-PAY_QUOTE
pass). The NV counter now stands at **NV-009** (TASK-018's max-depth
vector, added 2026-07-12); a future negative vector takes NV-010.
