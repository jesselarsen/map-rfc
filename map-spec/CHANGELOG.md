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

### Still open (tracked in `ERRATA.md`, deferred to a future revision)

- **pay §4/§4.1 metered pause/resume signaling** (PENDING-ARCHITECT
  2026-07-14, surfaced by TASK-022): the provider pauses an exhausted stream
  with ERROR 6001 + the unpaid watermark, but the *trigger* by which it
  learns the requester cannot pay, and the *resume* envelope after a covering
  proof, are not pinned. TASK-022 ships an interim that only sequences the
  already-specified messages (no new wire message). A future ruling either
  adds an explicit requester→provider "insufficient" signal + a
  provider→requester resume marker, or formally defines the pause as a
  timeout/out-of-band condition.
