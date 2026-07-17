# MAP specification changelog

Per draft-map-ops-01 §4, every revision carries a changelog classifying each
change as **editorial** (wording only, no wire/behavior change), **additive**
(new optional field / code point / structure; a conformant -00 peer keeps
interoperating), or **breaking** (would not interoperate; MUST name the major
version that carries it). No revision to date contains a breaking change.

---

## draft-map-\*-01 — 2026-07-13

Folds the 2026-07-12 architect rulings (full rationale in `ERRATA.md`) into
normative text and cuts the `-01` revision of all eight drafts. The frozen
test-vector bytes (TV-001..015, TV-010b, NV-001..008) are **unchanged**;
NV-009 was added additively. Both reference implementations (`map-rs`,
`map-go`) pass the full vector set and the live interop matrix (16/16,
`interop/MATRIX.md`) at this revision.

### Additive

- **core §9** — registered error codes **2006** (countersignature refused),
  **6005** (authorized cap exceeded), **6006** (agreement digest mismatch /
  price switch), **6007** (task-binding mismatch), **6008** (invalid payment
  proof). No existing code changed meaning; `6002` remains the generic
  settlement failure.
- **tasks §2.1** — `InputSpec` now normatively carries keys **2**
  (`[+ Multihash]`, content-addressed) and **3** (`[+ ContextRef]`) alongside
  key 1; at least one key MUST be present. Retrieval semantics remain future
  work; the wire codec is fixed and exercised by TV-007 in both impls.
- **test vectors** — **NV-009** added: a delegation chain whose parent
  declares max-depth 0 while a child delegates further → BAD_DELEGATION
  (2003). Isolates the max-depth bound (identical scopes, no widening).

### Editorial (wording; wire behavior unchanged, frozen by existing vectors)

- **identity §3.2 (B1/B3) & §4** — the delegated-envelope `kid` is the chain
  **leaf's** subject (the leaf key produces the signature); the chain root's
  issuer is the **accountable** identity a verifier resolves and reports. The
  former "chain root MUST equal the envelope `kid`" clause (cryptographically
  impossible for a leaf-signed envelope) is removed. `max-depth` (key 7)
  counting is pinned: a cert with max-depth *d* admits at most *d* certs
  between itself and the leaf. Byte behavior frozen by TV-012 / NV-009.
- **trust §2 / §3** — `RECEIPT_ACK` carries the **complete** dual-signed
  `Receipt`, not the provider signature alone. A countersignature refusal
  (usage/timestamps outside tolerance) is ERROR **2006** (was an unassigned
  interim 5000).
- **discovery §6** — the shard index `s` in the `K'` key preimage is rendered
  as unpadded base-10 ASCII decimal (`"0".."255"`), fixed by the
  derived-constants vector `K'`.
- **core §6.1** — a caution recorded that HELLO fields 1/2 are lists while
  HELLO_ACK's are singletons (guarded by TV-001/TV-003).

### Compatible relaxation

- **tasks §2** — `Quote` key 2 (`PayQuoteRef`) demoted from REQUIRED to
  **OPTIONAL** (payment is optional at the protocol layer). A receiver that
  previously required it now accepts its absence; every `-00` QUOTE remains
  valid. No wire encoding of an existing message changes.

### Reference-implementation note (not a spec change)

- The per-frame SessionID check (core §6.1: a mismatch is a 1006 state
  violation) was already normative in `-00`; the `map-rs` reference had
  omitted it post-handshake and now enforces it (TASK-020 FR-2), matching
  `map-go` and the `wrongsid` interop cells.

### Post-cut addition — 2026-07-14 (additive; folded into the `-01` texts)

The formerly-open usage-watermark item is now defined (design rationale in
`ERRATA.md`, RESOLVED 2026-07-14; implementation packaged as TASK-021/022):

- **pay §4/§4.1** — `PayInvoice` key 4 pinned as `UsageWatermark`
  `{1 TaskID, 2 through-chunk-seq, 3 cumulative Usage,
  4 cumulative-billed}` with normative billing arithmetic (exact
  uint×decimal, no rounding) and six ordered verification checks; the
  ERROR 6001 detail schema `{1 => UsageWatermark}` carries the final
  unpaid watermark.
- **pay §3** — rate-table unit-code registry (0 flat, 1 per-token-in,
  2 per-token-out, 3 per-millisecond, 4 per-invocation) and the
  same-exponent/same-asset rule for a quote's rate table.
- **tasks §3** — kind-3 (usage-update) chunk `content` pinned:
  deterministic-CBOR cumulative `Usage` map.
- **core §9** — code **6009** invoice-watermark violation registered;
  6007's description broadened to cover `UsageWatermark.1` task-binding
  mismatches (same meaning, wider referent).

All additive: postpaid (`terms` ≠ 3) messages are byte-for-byte unaffected,
and key 4 was already reserved-optional. **Vectors TV-016** (a metered
PAY_INVOICE whose model-1 billing 1523×3000 + 812×5000 = 8,629,000 µUSD is
recomputable from the watermark) and **NV-010** (the same invoice with
cumulative-billed off by one → 6009) are frozen (TASK-021); both
implementations reproduce them byte-exact and check the arithmetic
independently.

Also additive (2026-07-14, second pass): **`pay:x402` registered** in the
pay §5.1 settlement-method registry with its defining section — x402
(x402.org) enters as a settlement *method* (signed EIP-3009 authorization
payload + payee-chosen facilitator verify/settle), NOT as a replacement for
the pay negotiation layer (x402's HTTP 402 loop is unused between MAP
peers; it remains appropriate at ops §5 gateways fronting external
x402-priced services). MAP-specific hardening: the authorization nonce MUST
be `SHA-256("MAP-x402-nonce" || invoice-id)` (binds the authorization to
exactly one invoice; the chain's single-use-nonce rule then gives
exactly-once settlement), and `/verify` alone is never settlement. Asset =
the existing `erc20:<chain>:<addr>` form; Amount exponent = token decimals
negated, so mantissas are atomic units (§3 exact arithmetic unchanged).
Implementation packaged as TASK-023 (feature-gated adapter; no vectors —
settlement proofs are chain-dependent and out of deterministic-vector
scope, like the other rail methods).

### Resolved after the cut — 2026-07-14 (additive; folded into `-01`)

- **pay §4.2 metered pause/resume/cancel** — the pause/resume signaling gap
  (PENDING-ARCHITECT surfaced by TASK-022) is closed with a "no new message
  type" ruling: pause is a provider-local decision signalled by the existing
  ERROR 6001 + unpaid-watermark detail (exhaustion = proof absence, as
  postpaid); **resume is the covering PAY_PROOF itself** (no marker); metered
  streams ride the existing FLOW_CREDIT backpressure; CANCEL owes to the last
  countersigned checkpoint. Additive (adds a §4.2 subsection, no wire change)
  and makes the TASK-022 runtime normative verbatim.

### Post-cut addition — 2026-07-16 (additive; folded into the `-01` texts)

**pay §5.2 prepay and escrow defined** (formerly a one-paragraph stub;
implementation packaged as TASK-024). Terms 1 (prepaid) and terms 2
(escrow) get normative flows under the §4.2 "no new message type"
doctrine:

- **pay §5.2.1** — two additive optional fields: `PayInvoice` key **7**
  `phase` (1 hold, 2 release; present **iff** terms 2 — the mirror of the
  §4.1 watermark presence rule) and `PayReceipt` key **5** `kind`
  (0 settled = default/absent, 1 held, 2 released, 3 refunded; unknown
  kinds rejected fail-closed). All pre-existing messages are
  byte-for-byte unaffected.
- **pay §5.2.2 (terms 1 prepaid)** — single pre-delivery invoice that
  MUST equal the quote key 4 total (key 4 now REQUIRED in terms-1/2
  quotes); billable execution MUST NOT start before *settled*.
- **pay §5.2.3 (terms 2 escrow)** — a fourth payment moment **held**;
  escrow = two standard invoice→proof→receipt rounds (hold, then
  release), reusing §4 idempotency per round. Canonical release trigger:
  COMPLETE + verified transcript-root, then a payer-signed release
  authorization (authorization symmetry: payee can never self-release,
  payer can never unwind pre-expiry; expiry auto-refunds). Six ordered
  requester checks; CANCEL anchors the release amount to the last
  countersigned checkpoint via the §4.1 formula.
- **pay §5.2.4** — `pay:credit-ledger` escrow extension: operator-signed
  `MAPLedgerHold` / `MAPLedgerRelease` / `MAPLedgerRefund` entries plus
  payer `MAPLedgerReleaseAuth` / payee `MAPLedgerRefundAuth` inner
  signatures; atomic release-with-remainder-refund; idempotent by
  invoice-id.
- **core §9** — code **6010** escrow violation registered.

Vectors: **TV-017** (terms-2 hold PAY_INVOICE with phase key 7) and
**TV-018** (PAY_RECEIPT kind 1 held) to be added additively; **NV-011**
(PAY_RECEIPT with unrecognized kind → 6010). Frozen bytes untouched.

### Post-cut addition — 2026-07-16 (second pass; additive, no wire change)

**ops §5.1 paid-API gateways (the x402 composition) defined**
(implementation packaged as TASK-025). A gateway fronting an external
x402-priced HTTP service runs **two independent payment legs** —
downstream ordinary MAP payee, upstream x402 HTTP payer in its own name —
under three new gateway rules: **G-5** payment legs never launder (no
cross-leg proof re-presentation; upstream evidence enters only as
gateway-attested transcript content), **G-6** spend order (admit, then
downstream settlement — RECOMMENDED terms 1 prepaid per pay §5.2.2 —
before any upstream spend), **G-7** bounded upstream spend (per-task
ceiling, fail-closed refusal). Deterministic per-(task, attempt)
authorization derivation RECOMMENDED for upstream retry safety. Purely an
ops-layer composition of existing mechanisms; no message, field, or code
point changes.

### Post-cut addition — 2026-07-16 (third pass; additive)

**pay §6 disputes defined** (formerly a sketch naming messages and claim
codes; implementation packaged as TASK-026). The already-assigned code
points 0x0405/0x0406 gain normative CDDL and procedure:

- **§6.1** — `PayDispute` (dispute-id, invoice-id, claim code,
  PayAgreeRef anchor, optional TaskID/free-text) and
  `PayDisputeEvidence` (`[+ EvidenceItem]`, FLOW_CREDIT-chunked).
  `EvidenceItem` = (kind registry 0–8, artifact bytes VERBATIM per I1,
  optional CB). Unknown claim codes / kinds rejected fail-closed;
  dispute-id idempotency; ERROR 6003 signals flow refusal pending
  resolution; trust receipt outcome 4 records the event.
- **§6.2 CB disclosure** — the new architectural piece: in-session
  envelopes sign with `external_aad` = the session channel binding, so
  third parties cannot verify them from bytes alone. An in-session
  evidence item MUST carry its CB (key 3); disclosure is safe (HKDF
  output, not key material) and proves same-session provenance across
  items. CB-less in-session items are unavailable evidence, never
  verified artifacts.
- **§6.3 mechanical verification** — per-claim procedures over
  authenticated evidence: claims 0/2 are rebuttal-shaped (stand
  unrebutted against the party that must hold the artifacts if honest);
  claim 1 recomputes the tasks §8.1 transcript root; claim 3 recomputes
  §4.1 arithmetic against the highest COUNTERSIGNED checkpoint usage;
  claim 4 re-runs the existing 6005/6006/6008/6009/6010 verifiers.
  Three-valued verdict (supported / rebutted / insufficient); MAP never
  emits remedies.
- **§6.4 griefing bounds**; **§8** registers the evidence-kind registry.

No new code points (0x0405/0x0406 were assigned in -00), no changes to
existing message encodings. Vectors: **TV-019** (PAY_DISPUTE),
**TV-020** (PAY_DISPUTE_EVIDENCE), **NV-012** (unknown claim code →
reject) to be added additively; frozen bytes untouched.

### Implementation-session addendum — 2026-07-17 (additive; TASK-026)

Two small §6 gaps closed inline (rulings in `ERRATA.md`, 2026-07-17):
**pay §6.1** evidence-kind registry gains **9 = CHUNK envelope** (the
delivered-transcript evidence claim 1's root recompute requires — the
0–8 registry had no chunk kind); and the tasks §8.2 checkpoint
countersignature bytes are pinned decision-9 style as
`enc(["MAPCheckpointAck", TaskID, chunk-seq, root])` with the kind-5
evidence artifact as CBOR `[checkpoint-content-map, countersig]` (no
byte layout previously existed anywhere). Vectors **TV-019**
(PAY_DISPUTE, claim 3, PayAgreeRef anchored to TV-013 by envelope
digest), **TV-020** (PAY_DISPUTE_EVIDENCE carrying the frozen TV-010b
envelope VERBATIM — the vector itself exercises I1 verbatim-carry), and
**NV-012** (claim 9 → fail-closed reject) frozen additively.

### Still open (tracked in `ERRATA.md`, deferred to a future revision)

- (none)
