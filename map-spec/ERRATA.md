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

RESOLVED 2026-07-17 (inline, TASK-026 implementation session acting with
the architect role) — two small gaps surfaced implementing pay §6:
(a) the §6.1 evidence-kind registry (0–8) had no kind for delivered
transcript CHUNKs, which §6.3 claim 1 (output-mismatch) requires the
payer to present for the root recompute. RULING: kind **9 = CHUNK
envelope (tasks §3)** added to §6.1 (additive registry entry; §8 already
delegates the registry). (b) tasks §8.2 specifies the checkpoint
countersignature as "the requester's own signature over (TaskID,
chunk-seq, root)" but no implementation or byte layout existed. RULING:
signing input pinned decision-9 style as
`enc(["MAPCheckpointAck", TaskID, chunk-seq, root])` (root as the raw
32 bytes), and the §6.1 kind-5 evidence artifact is CBOR
`[checkpoint-content-map, countersig]` with the tasks §8.2 Checkpoint
field numbers. — Implemented in `map-pay::dispute`
(`checkpoint_ack_signing_input`, `CountersignedCheckpoint`); a future
tasks-runtime CHECKPOINT/CHECKPOINT_ACK implementation MUST reuse these
bytes.

---

RESOLVED 2026-07-14 (was PENDING-ARCHITECT 2026-07-14 TASK-022) — metered
(terms 3) stream pause/resume signaling was under-specified: (a) the
trigger by which the provider learns the requester cannot pay, and (b) the
resume envelope after a covering PAY_PROOF. RULING (chose the "no new wire
message" option): pause is a **provider-local decision**, not a wire
handshake — the exhaustion signal is *proof absence*, exactly as postpaid
(the requester withholds its PAY_PROOF; there is no "insufficient"
message). The provider MAY pause whenever it holds an unpaid outstanding
watermark and SIGNALS it with the already-spec'd ERROR 6001 + `{1 =>
UsageWatermark}` detail; RECOMMENDED pause policy is bounded by the
stream's FLOW_CREDIT (core §8.1) grant, so unpaid delivery rides the
backpressure MAP already has. **Resume is the covering PAY_PROOF itself**
— no marker; the provider settles and continues from the exact (cumulative,
monotonic) watermark, and the next CHUNK is the observable resume. On
CANCEL, owed anchors to the last countersigned checkpoint (≤ the unpaid
watermark). Decisive reasons: zero new wire surface (ERROR/PAY_PROOF/
FLOW_CREDIT/CANCEL already exist); symmetric with postpaid; reuses the
existing credit backpressure; and it makes the TASK-022 INTERIM normative
verbatim (provider `pause` → 6001+watermark; requester `retry_pending` →
PAY_PROOF; resume = clearing the paused flag on `on_proof`), so no
implementation rework. Spec patched: new pay §4.2 "Pause, resume, and
cancel" + a §4 intro pointer. → the TASK-022 metered runtime is already
conformant; no follow-up package.

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

---

RESOLVED 2026-07-17 (TASK-020 interop-extension session; ratified in the
2026-07-17 preserve-the-work review) — extending the live interop matrix
(`interop/run-matrix.ps1`) with the task protocol over libp2p/QUIC
(scenarios `task-stream`, `task-multichunk`, `task-transcript`) raised the
question of whether the **Rust ↔ Go task cells** could be wired, or whether
only Go ↔ Go (impl #2 over its own transport) was in reach. RULING (chose
option (a) — wire the Rust ↔ Go cells): `map-cli` gained `map-net` +
`map-taskrun` deps and a `--transport libp2p` task-mode in
`crates/map-cli/src/interop.rs` (a `Libp2pExecutor` mirroring the Go
`scenarioExecutor`, plus pump loops driving `ProviderTask`/`RequesterTask`
over `map_net::MapNode`); `map-net` gained `MapNode::listen_with_peer` to
surface the dialable `/p2p/<peerid>` multiaddr. The matrix demo capability
is the shared `map:cap:demo/excuse` — the interop contract needs only a
shared capability URI + identical transcript codec, both satisfied; the
`rfc-title-o-matic` name in the original plan was a demo-agent detail, and
that binary was ALSO delivered (a `[[bin]]` of `map-cli`,
`src/rfc_title_o_matic.rs`, cap `map:cap:demo/rfc-title`) and live-verified
cross-impl against the capability-agnostic Go `map-requester`
(`OK transcript-root: f1752ebae4594c2f0415694143bf881f53230d7669b4ebbb17545ab58218b7d4`).
The transcript wire format was already cross-impl-verified (TV-008 /
`map-go/tasks/transcript_test.go`), so NO spec change was implied — only
the Rust runtime + interop harness. Result: **16/16 core (TCP, Rust↔Go) +
6/6 task (libp2p, both orientations) PASS**; the core-session 16/16 TCP
matrix is unchanged. Where implemented: `crates/map-cli/src/interop.rs`,
`crates/map-cli/src/rfc_title_o_matic.rs`, `map-go/{tasks,net,dht,cmd}`,
`interop/run-matrix.ps1`.
