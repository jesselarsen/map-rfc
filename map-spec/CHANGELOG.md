# MAP specification changelog

Per draft-map-ops-01 §4, every revision carries a changelog classifying each
change as **editorial** (wording only, no wire/behavior change), **additive**
(new optional field / code point / structure; a conformant -00 peer keeps
interoperating), or **breaking** (would not interoperate; MUST name the major
version that carries it). No revision to date contains a breaking change.

---

## External review Track 0: claim corrections — 2026-09-18

**Editorial only.** No wire, registry, code-point, or normative-mechanism
change; frozen test vectors byte-stable; no CDDL block altered. Every change
corrects a statement the repository made about itself that an external review
found to be false. Rulings and reasoning are in `ERRATA.md`.

### Editorial (drafts)

- **All 11 drafts** — the Status-of-This-Memo preamble claimed "two independent
  interoperating implementations"; corrected to "two implementations by one
  author, under a source-isolation rule". In IETF usage "independent" means
  different parties.
- **`draft-map-ops-01` §6.1** — interop figure 16/16 → 26/26 (16 core + 6 task
  + 4 pay); deleted the false claim that cross-language task and payment flows
  "remain future work"; added the two qualifications on the advancement rule
  (independence is mechanical, and the matrix covers only the mechanisms it
  exercises).
- **`draft-map-ops-01` §6.3** — the formal-analysis program is restated to
  match what the models establish. The payment-layer models (`pay-auth.pv`,
  `pay-dispute.pv`, `pay-meter.pv`) have failing reachability guards and prove
  their properties over an empty trace set; they are no longer relied on for
  any claim. P2 is no longer described as anti-relay.
- **`draft-map-ops-01` §7.2** — removed "(the attached preprint)" and added an
  editorial note that §7.1–§7.3 are ruled for removal before publication.
- **`draft-map-arch-01` §1** — removed "taken from the attached design work".
  A standalone document cannot cite material its readers cannot obtain.
- **`draft-map-index-01` §2/§4** — "10 Internet-Drafts" above a table of 11;
  interop figure; and the submission-status paragraph now records that the
  Independent Stream cannot host the family's registries.
- **`draft-map-intro-01` §6** — implementation independence and the formal
  claim, as above.
- **`draft-map-wire-01`, `draft-map-ops-01`** — document-specific Abstracts
  added. Both previously rendered `md2rfc.py`'s generic family placeholder.

### Editorial (supporting documents)

- `SUBMISSION.md` — the Stream section is rewritten: RFC 8726 §2/§4/§5 bar the
  Independent Stream from creating the ~20 registries this family defines or
  granting the policies they use, which relabeling does not fix. Records the
  current path (self-hosted registries; two narrow registry-free extracts) and
  a MUST-resolve list that now leads with the demonstrated defects.
- `formal/README.md` — results table corrected against the repository's own
  stored logs: P5 and P6 reachability queries are unreachable, and the P4
  control does not discriminate.
- `README.md` — interop figure; TV/NV counts; task-package count; and the
  Status section now states implementation coverage in three explicit tiers
  instead of claiming the protocol is "implemented twice".
- `CLAUDE.md` — no longer claims this is a spec-only repository with no build
  or test command.
- `ERRATA.md` — first pass of `REVIEW-PROCESS.md`'s intake process; ten RULED
  entries.

### Tooling

- `build/nits.py` — detected a placeholder Abstract as present because it
  grepped for the word "Abstract"; now matches the placeholder sentence against
  whitespace-collapsed text and hard-fails. Verified to go red for exactly
  `wire` and `ops` before their abstracts were written.
- `build/render.py` — xml2rfc's stderr was discarded on zero-exit runs, which
  is how a silently dropped `submissionType` attribute stayed invisible;
  warnings are now printed.

---

## I-D submission readiness: author metadata, render hardening, nits, SUBMISSION.md (TASK-031) — 2026-07-18

Editorial only. No wire, registry, code-point, or normative-mechanism change;
frozen test vectors byte-stable; no CDDL block altered. Takes the existing
`build/` render pipeline from "renders" to "submittable".

### Editorial (drafts)

- **All 11 drafts** — front-matter gains `author: Jesse Larsen` /
  `email: jesselarsen@gmail.com` / `ipr: trust200902` (renders into the front
  page + Authors' Addresses).
- **Section-heading casing normalized** to Title Case across the family
  ("Security Considerations", "IANA-Style Considerations") — several drafts had
  lowercase "considerations".
- **Missing required sections added** (concise, correct pointer sections):
  Security Considerations + IANA Considerations to `draft-map-index-01`,
  `draft-map-intro-01`, `draft-map-wire-01`; IANA Considerations to
  `draft-map-arch-01` and `draft-map-ops-01`. Every draft now has Abstract,
  Security Considerations, IANA Considerations, References, and Authors'
  Addresses (verified by `build/nits.py`).

### Tooling (render-only; no draft mutation)

- **`build/md2rfc.py`** hardened: reads author/email/ipr from front-matter
  (was a hardcoded placeholder); reads sources as UTF-8 (was cp1252 —
  mojibaked every §/—); drops the document H1 past leading blanks and the
  hand-written "Status of This Memo" preamble (xml2rfc regenerates the correct
  boilerplate); routes `## Abstract` into RFC front matter; strips
  author-written `N.` section numbers so xml2rfc's numbering is authoritative
  (was "1.16. 14. Security Considerations"); emits `version="3"` (fixes the
  empty Authors' Address block); declares the independent stream only for
  categories the ISE accepts (info/exp/historic).
- **`build/nits.py`** (new) — stdlib mechanical pre-submission check (required
  sections; >72-char and non-ASCII line counts).
- **`build/requirements.txt`** (new, `xml2rfc==3.34.0`), **`build/README.md`**
  (new). The whole `build/` pipeline + rendered `.xml/.txt/.html` for all 11
  drafts are now tracked (were inadvertently under a blanket `build/`
  gitignore; narrowed to ignore only transient artifacts).
- **`map-spec/SUBMISSION.md`** (new) — the ISE/Datatracker submission
  checklist, plus the cover-note disclosures (CB-exporter stand-in;
  single-author preprint status) and the three tracked pre-submission items:
  the std/bcp-category vs independent-stream conflict (an author decision),
  the 192 >72-char lines (diagram/CDDL/table reformatting), and formalizing
  References into a normative/informative `<back>` split.

The render is idempotent (fixed date; a second `render.py` run is
byte-identical).

---

## Paid demo: prepaid-gated rfc-title-o-matic × paying map-requester (TASK-030) — 2026-07-18

Additive change (implementation/demo only; no wire, registry, or normative
change). The headline demo — an agent that will not work until it is paid —
cross-implementation: Rust payee, Go payer, over `pay:credit-ledger`.

- **`map-rs` `rfc-title-o-matic`** — new `--price <cents>` / `--operator-seed`
  flags. Priced mode runs the terms-1 prepaid gate (pay §5.2.2, reusing
  TASK-024's `PrepaidProviderPayment`) BEFORE any task work: no task message
  is exchanged until the `PAY_RECEIPT` settles. Prints `SETTLED`/`RECEIPT`.
  Sequencing is sequential pay-then-task (TASK-030 §FR-1 allowed the
  deferred-ASSIGN interleave as an alternative; sequential is strictly
  stronger for "no work before pay" and composes with the Go requester's
  task-only `RunRequester`). Free path (no `--price`) is byte-identical to
  before — no payment object, no `0x04xx` frame.
- **`map-go/cmd/map-requester`** — new `--pay` / `--operator-seed` /
  `--budget` flags. When paying, hosts the in-process credit-ledger operator
  (pre-credited to the budget), pays the prepaid invoice, then runs the task;
  refuses over-budget quotes cleanly before agreeing (no money moved).
- **`interop/DEMOS.md`** — "Run the paid demo" section with the live paid /
  over-budget / unpaid transcripts.

Live-verified cross-impl: paid run settles 500¢ and delivers the title
(transcript root `7bd6…5137`); over-budget and unpaid runs do no work and
move no money.

---

## Live interop matrix extended: pay family over libp2p/QUIC, both orientations (TASK-029) — 2026-07-18

Additive change (ops §6.1): the live interop matrix now exercises the **pay
family (negotiation + `pay:credit-ledger` settlement, pay §2–§5) over a real
libp2p/QUIC session** in both orientations, closing the two-implementation gap
for the family the drafts feature most. Implementation + harness only; no wire
format, registry, or normative-text change (the pay codec is byte-verified
against the existing frozen vectors).

### Additive (implementation / harness; no spec change)

- **`map-go/pay`** — new pure-stdlib package: the pay-family wire codecs
  (Amount/AssetAmount/PayQuote/Agree/Invoice/Proof/Receipt/UsageWatermark),
  exact metered billing (`Billed`/`VerifyWatermark`), the `pay:credit-ledger`
  method (`LedgerOperator`/`EntryProof`/`VerifyEntry`), and the terms-0/1/3
  pump runtimes (postpaid / prepaid / metered). Byte-arbitrated by the frozen
  conformance vectors (TV-010b/013/016/017/018). Terms-2 escrow + pay §6
  disputes are NOT ported (future work). Kept libp2p-free — the conformance
  surface stays dependency-clean (independence marker; `go list -deps ./pay`
  links no libp2p).
- **`map-go/cmd/map-interop`** — `pay-postpaid` / `pay-metered` libp2p
  scenarios (payee = listener, payer = dialer hosting the ledger operator via
  `--operator-seed`), run after the task transcript verifies on the same
  session.
- **`map-rs` `map-cli`** — the same two scenarios in the gated `libp2p_task`
  interop module, composing the existing `map-taskrun::{payments,metered}`
  pairs (the first live task+pay single-session loop on the Rust side);
  `map-pay` added as an optional dep under `libp2p-interop`.
- **`interop/run-matrix.ps1`** — `$SEED_OP`; a pay matrix (2 scenarios × 2
  orientations) appended. Result: **16/16 core + 6/6 task + 4/4 pay PASS**.
- **`interop/MATRIX.md`** — Pay matrix section + Latest-run re-stamp.

---

## Post-review hardening: CB-exporter security note + map-cli feature gate — 2026-07-17

Editorial + implementation follow-ups from the preserve-the-work review; no
wire-format, registry, or behavior change.

### Editorial (wording; no wire/behavior change)

- **core §14 (Security Considerations)** — added an explicit
  channel-binding-strength paragraph: the §7.3 anti-relay/endpoint-binding
  guarantees are only as strong as the transport exporter, and the
  `/map/1.0.0` libp2p binding currently uses the identity-pair stand-in
  (`initiator-AgentID ‖ responder-AgentID`, core §10) which is weaker than a
  true key-schedule exporter (binds the identity pair, not the connection
  instance). Surfaces in the Security Considerations section the limitation
  that §10 already documented, so an IETF reviewer finds it where expected.
  The CB derivation is transport-agnostic — a stronger exporter drops in with
  no wire change.
- **`interop/MATRIX.md`, `interop/DEMOS.md`** — the same CB-exporter caveat
  added next to the 6/6 task-matrix / demo results (previously stated the
  exporter neutrally).

### Implementation / harness (no spec change)

- **`map-rs` `map-cli`** — the TASK-020 libp2p deps (`map-net`, `map-taskrun`,
  `map-discovery`, `map-mesh`, `libp2p`) moved behind a **`libp2p-interop`**
  feature (default-ON, so the interop matrix and `cargo build --workspace`
  are unchanged); `--no-default-features` now yields a minimal, libp2p-free
  `map-cli` (keygen / projection / TCP interop). The `rfc-title-o-matic` demo
  bin carries `required-features = ["libp2p-interop"]`. Restores the crate's
  `real-backend` / `real-x402` convention of gating heavy/network deps. Zero
  warnings in both feature configurations.

---

## Live interop matrix extended: task protocol over libp2p/QUIC, both orientations (TASK-020 extension) — 2026-07-17

Additive change (ops §6.1): the live interop matrix (`interop/run-matrix.ps1`)
now exercises the **task protocol (tasks §1–§13) over a real libp2p/QUIC
session** in both orientations, proving the full task runtime interoperates
across both implementations. The transcript codec is cross-impl-verified
(TV-008), so the roots printed by each side match byte-for-byte. No wire
format, registry, or normative-text change — this is implementation + harness
only.

### Additive (implementation / harness; no spec change)

- **`map-go/tasks`** — complete task runtime: `TranscriptBuilder` (Merkle root),
  `ProviderTask`/`RequesterTask` FSMs, flow-credit streaming, `RunProvider`/
  `RunRequester` session pumps. (Impl #2 task runtime, end-to-end.)
- **`map-go/net`** — real libp2p/QUIC transport: `MapNode` (`/map/1.0.0`
  substream, 4-byte-BE framing, channel-binding exporter = initiator‖responder
  AgentID, Kademlia DHT for descriptor publish/lookup).
- **`map-go/discovery`** — stdlib-only descriptor codec + sharded
  capability-key derivation (discovery §5/§6). Kept libp2p-free so the
  conformance surface stays dependency-free (independence marker).
- **`map-go/dht`** — the libp2p/Kademlia `Router` over the sharded
  capability key, in its OWN package (imports `net` + `discovery`) so the
  codec above never links libp2p.
- **`map-go/cmd/map-excuse`** — Go demo agent "MAP-Excuse-As-A-Service"
  (`map:cap:demo/excuse`), a real task-answerer over libp2p.
- **`map-go/cmd/map-interop`** — `--transport libp2p` + task scenarios
  (`task-stream`, `task-multichunk`, `task-transcript`).
- **`map-rs` `map-cli`** — gained `map-net` + `map-taskrun` deps; `interop.rs`
  `--transport libp2p` branch with a `Libp2pExecutor` (mirrors the Go
  `scenarioExecutor`) driving `ProviderTask`/`RequesterTask` over
  `map_net::MapNode`. `map-net` gained `MapNode::listen_with_peer`.
- **`interop/run-matrix.ps1`** — runs the 3 task scenarios × 2 orientations over
  libp2p/QUIC (6 cells) in addition to the 16 core-session TCP cells. Result:
  **16/16 core + 6/6 task PASS**.
- **`interop/MATRIX.md`**, **`interop/DEMOS.md`** — updated to the 6/6 task
  result (both orientations); `map-spec/ERRATA.md` PENDING-ARCHITECT item
  resolved.

### Editorial (wording; no wire/behavior change)

- **`memory.md`** — entry N. records the extension; the §5 "live matrix met"
  note now covers the task cells too.

---

## Formal analysis: continuous metering arithmetic & quote/agree/invoice/proof binding (Item 9 / TASK-028) — 2026-07-17

Additive cross-cutting change (ops §3.2): extends the TASK-027 payment
formal-analysis program (ops §6.3) to cover the metered-billing path. No
wire format, registry, test-vector, or implementation (Rust/Go) change. Both
reference implementations remain at 16/16 live interop.

### Additive

- **`formal/pay-meter.pv`** (new) — ProVerif 2.05 model of P6: under terms 3
  (metered-incremental), a `PAY_INVOICE` watermark's `cumulative-billed`
  equals `usage × rate` (pay §3 same-exponent rule) and stays within the
  `PAY_AGREE` authorized cap (pay §4.1 steps 5–6); the requester settles only
  well-formed invoices (no settled over-bill / over-cap — Q-overbill); a billed
  CHUNK is emitted only after a verified `PAY_PROOF` settles the covered usage
  (settled-not-submitted — Q-underpay); and an invoice bills only a quote the
  requester agreed to (PAY_AGREE digest binding, pay §3 — Q-bind). Rate and cap
  are modeled as private constants; the honest billing+cap derivation
  `billed_capped(u)` is the only value the requester will settle.
- **`formal/results/pay-meter.out`** (new) — ProVerif 2.05 output: Q-overbill,
  Q-underpay, Q-bind all **proved TRUE**; Q-reach **reachable** (non-vacuity).
- **`tasks/TASK-028-pay-meter.md`** (new) — implementation-specification
  package mirroring TASK-027.

### Editorial (wording; no wire/behavior change)

- **`formal/README.md`** — `pay-meter.pv` added to the model inventory and
  query table (P6, Q-overbill/Q-underpay/Q-bind); reproduce loop includes it;
  the "continuous metering" open-question note updated to mark P6 covered and
  name the single remaining corner (checkpoint-anchored cancel accounting).
- **`README.md`** (Status → genuinely-open-work) — the payment/escrow formal
  line now states the metering arithmetic + quote/agree/invoice/proof binding
  are machine-checked (TASK-028); only the checkpoint-anchored cancel corner
  remains as remaining formal work.

---

## Publishability: index bundle + IPR/license headers (Item 8) — 2026-07-17

Editorial/additive cross-cutting change (ops §3.2): makes the family
submittable as an IETF Independent Stream set. No wire format, registry,
test-vector, or implementation (Rust/Go) change. Both reference
implementations remain at 16/16 live interop.

### Additive

- **`map-spec/draft-map-index-01.md`** (new) — family cover/index: abstract,
  one-paragraph overview, the authoritative 10-document index table (mirrors
  arch §11), reading order, and submission-status section. Defines no wire
  format, code point, or protocol behavior (no second source of truth).
- **`LICENSE`** (new, repo root) — full Apache License 2.0 text, matching the
  README's "Apache-2.0" claim. Closes the missing-license-file gap.

### Editorial (wording; no wire/behavior change)

- **Front-matter consistency** — `submissiontype: independent` added to the
  seven drafts that lacked it (core/identity/discovery/tasks/trust/pay/ops),
  so all ten carry the Independent Stream submission type (arch/intro/wire
  already had it).
- **Per-draft IPR note** — a BCP 78/BCP 79 copyright paragraph added to all
  ten drafts (Apache-2.0 working tree; contributors disclose IPR per BCP 79;
  published RFC under IETF Trust terms, pointer to ops §3.5). Consistent with
  the ops §3.5 IPR statement.
- **Cross-links** — `draft-map-index-01` added to the README contents table
  (row "I") and to `draft-map-arch-01` §11 document map; the index points to
  intro/arch as entry points.

## Documentation hygiene pass (Item 7) — 2026-07-17

Editorial-only changes (ops §3.2): three targeted clarity fixes. No wire
format, registry, test-vector, or implementation (Rust/Go) change. Both
reference implementations remain at 16/16 live interop.

### Editorial (wording; no wire/behavior change)

- **discovery §11** — the fingerprinting MUST NOT (no high-entropy optional
  fields in public descriptors) now has a concrete safe/leaky descriptor
  example: a safe descriptor carries only coarse registered fields (two
  providers in a bucket stay indistinguishable); a leaky one adds a
  per-instance `device-uuid`/`nonce` and thereby violates the rule. Such
  fields MUST instead use a Zone-3 ephemeral identity or a direct channel.
- **trust §3** — receipt outcome code **2 (timeout)** is now explicitly wired:
  a provider-side `deadline` miss (tasks §82) reported as error 5001 (tasks
  §105/§454) records a receipt with outcome 2, distinct from the
  requester-terminal CANCEL (outcome 3). Code 4 (disputed) already pointed at
  pay §6; both are now stated with their cross-references. Added a note that
  the ExecutionReceipt field-7 outcome vocabulary is **separate** from the
  `Complete` message's own outcome field (tasks §181: 0 ok, 1 partial,
  2 failed) — the overlapping numbers are different structures and MUST NOT
  be conflated.
- **ops §3.3** — reaffirms message type `0x0206` as a permanent,
  deliberately-unassigned reservation (wire §3.20): never reused; any claim
  on `0x0206` is non-conformant.

## Governance & registry-ownership clarification for external submission (Item 6) — 2026-07-17

Editorial / additive cross-cutting change (ops §3.2): clarifies registry
ownership and the external-submission path so the family is submittable as an
IETF Independent Stream document without an undefined "steering process". No
wire format, registry, test-vector, or implementation (Rust/Go) change. Both
reference implementations remain at 16/16 live interop.

### Additive

- **ops §3.5 (new) — Registry stewardship and IETF submission**: defines the
  protocol **steward** (document editor(s) named on the ops front matter) as
  the pre-publication registry authority who appoints designated experts
  (resolving the previously-undefined "steering process" in §3.1); states that
  on IETF publication **IANA** operates every registry under the policies in
  `draft-map-wire-01.md` §4; fixes the pre/post-publication authority split;
  records the IPR position (source tree Apache-2.0; contributors disclose per
  BCP 79; published RFC under IETF Trust terms); and gates submission on the
  §6.1 two-implementation criterion.

### Editorial (wording; no wire/behavior change)

- **Status-of-This-Memo blocks** — all ten `draft-map-*-01.md` documents now
  carry one consistent block: Internet-Draft-style preprint, two
  implementations pass the live interop matrix (ops §6.1), *not yet submitted
  to the IETF (intended independent stream; see ops §3.5)*, distribution
  unlimited, expires six months after publication, cite as work in progress.
  The six documents that previously used a shorter `Status:` line
  (core/identity/discovery/tasks/trust/pay/ops) were brought up to the full
  boilerplate already used by arch/intro/wire.
- **`README.md`** — status note adds the intended IETF Independent Stream
  publication path (pointer to ops §3.5).

## Formal analysis: payment escrow symmetry + dispute watermark (Item 5 / TASK-027) — 2026-07-17

Documentation-only / additive cross-cutting change (ops §3.2): adds machine-checked
ProVerif models for two payment-plane security properties and wires them into the
formal-analysis program. No wire format, registry, test-vector, or implementation
(Rust/Go) change. Both reference implementations remain at 16/16 live interop.

### Additive

- **`formal/`** — new ProVerif 2.05 models:
  - `pay-auth.pv` — proves **P4** escrow charge-authorization symmetry (pay §5.2
    `pay:credit-ledger`): a `PHASE_RELEASE` debit requires BOTH a payer HOLD and a
    verified payee release token; pause is fail-closed. Q1 proved `true`.
  - `pay-auth-control-onesided.pv` — mandatory P4 negative control (drops the
    expected-payee binding): the weakened gate is reachable, confirming Q1 has teeth.
  - `pay-dispute.pv` — proves **P5** dispute watermark binding (pay §6.1/§6.3): an
    arbiter `DisputeResolved` is emitted only for an `invoice-id`/`PayAgreeRef` pair
    it observed contested. Q3 proved `true`.
  - `formal/results/{pay-auth,pay-auth-control-onesided,pay-dispute}.log` — captured
    prover output.
- **`tasks/TASK-027-pay-formal.md`** — implementation-specification package for the
  above (mirrors TASK-014 structure).

### Editorial (wording; no wire/behavior change)

- **`formal/README.md`** — inventory table, property-to-spec mapping, and run
  command extended with P4/P5; new modeling-notes subsections (Q1 P4, Q3 P5);
  payment abstraction gaps recorded (continuous metering, claim-code evaluation,
  ledger-operator trust).
- **`README.md`** — "Status" open-work item for the payment/escrow formal model
  updated to "covered (TASK-027)"; `formal/` contents-table description extended.

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

---

### Documentation refresh — 2026-07-17 (editorial; no wire/registry/vector change)

Aligns the family's public-facing status with the implemented scope
(prepaid/escrow §5.2, disputes §6, and the x402 paid-API gateway composition
ops §5.1 are all specified and implemented; the live 16/16 interop matrix
meets the ops §6.1 advancement gate).

- **README.md** — "Status" section rewritten: removed the stale
  "deliberately out of scope" claim for prepaid/escrow (pay §5.2) and the
  dispute protocol (pay §6), and the "one open package is TASK-023-x402"
  line (TASK-023/024/025/026 are DONE); now lists the genuinely-open items
  (ext-kvx/ext-git stub semantics, relay/NAT interop scenario, formal model
  of the pay/escrow path). Header status line reconciled with the met gate.
- **draft-map-tasks-01 §16** and **draft-map-ops-01 §7.2** — stale
  "TBD-BID / TBD-KVX / TBD-GIT" labels replaced with the assigned extension
  keys 1/2/3 (`ext-bid`, `ext-kvx`, `ext-git`, core §5.1).
- **draft-map-discovery-01 §4** — the gateway-provenance codec note reworded:
  the `gateway-of` params key (FINAL) is the authoritative wire carrier; the
  scaffold `Descriptor` codec gap is noted as a separate additive task, not an
  interim carrier.
- **All eight `draft-map-*-01` documents** — status lines reconciled with the
  ops §6.1 advancement gate (two implementations + live matrix met; documents
  remain in draft pending human/expert review).
- **New `draft-map-intro-01.md`** (Introduction & Motivation): problem
  statement, high-level summary of findings C1–C15, a problem→mechanism→spec
  table, explicit non-goals, reading guide, and maturity statement. Slotted as
  reading-order item 0; cross-linked from the README contents table and
  arch §11 document map, and the README reading-order sentence updated.

No breaking, additive-field, or code-point changes; all conformance classes,
test vectors, and wire encodings are byte-for-byte unaffected.

---

### Wire reference & master registry index — 2026-07-17 (editorial; no wire/registry/vector change)

- **New `draft-map-wire-01.md`** (Wire Format & Registry Index): a single
  consolidated, non-normative reference. §2 gives the envelope → Body →
  content → transcript-commitment PDU layering diagram (in-session vs
  out-of-session contexts, invariants I1–I4). §3 is the master registry
  index: every assigned code point (cryptographic suites, message types,
  body keys, extension keys, error codes, transport bindings, AgentID key
  types, delegation caveats, revocation reasons, capability ontology,
  CapEntry params, HW/sandbox classes, chunk kinds, reservation/lease fields,
  task caveat keys, receipt outcomes, settlement methods, pricing models /
  terms / unit codes, dispute claims / evidence kinds) copied with a
  "Defined in §X" pointer to its owning draft. §4 summarizes IANA policies
  and the immutability/deprecation rules (ops §3).
- Each owning draft's IANA-style section (core §15, identity §11, discovery
  §12, tasks §16, trust §11, pay §8) is trimmed to a pointer to the master
  index, keeping its normative CDDL/definitions in place — enforcing the
  "one named source of truth per fact" policy (ops §2 rule 1) and preventing
  registry-table drift across documents.
- README contents table: added the wire reference as row "R" (and the
  pre-existing duplicate "#1" on the critique/arch rows corrected to 1/2,
  remaining rows renumbered 3–9). arch §11 document map and the intro
  reading guide now reference `draft-map-wire-01.md`.

No wire/registry/vector changes; all encodings byte-for-byte unaffected.
