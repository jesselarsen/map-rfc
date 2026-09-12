# MAP — external review roles and prompts

A role lineup and ready-to-paste prompts for handing the MAP family to an
independent reviewer (human or model) ahead of Internet-Draft submission.

**Assumptions baked into these prompts:** the reviewer has full repository
access and can run builds, tests, and tooling; the submission target is the
**IETF Independent Submission stream (ISE)** per `map-spec/SUBMISSION.md`.
If either changes, adjust the shared preamble accordingly.

What to do with the findings once they arrive — the triage gate, the fix
order, and the blast-radius checklist — is `map-spec/REVIEW-PROCESS.md`.

The point of splitting the review into roles is that a single "review my RFC"
pass spends its attention on prose. Each role below owns one surface where a
specific, already-visible failure mode lives, and each should be run in its own
fresh session so the reviews stay independent and adversarial.

## Why these roles

The project's real risk surfaces are not evenly distributed, and a generic
"review my RFC" prompt will burn its attention on prose. Each role below owns
one surface where a *specific, already-visible* failure mode lives:

| # | Role | Owns the risk that… |
|---|---|---|
| 1 | ISE Process & Editorial Gatekeeper | the submission is rejected on stream/category/boilerplate before anyone reads the protocol |
| 2 | Security & Cryptography Reviewer (SECDIR-style) | the channel-binding stand-in, replay window, or key hierarchy has a real flaw |
| 3 | Formal-Methods Auditor | the ProVerif "all green" result is vacuous rather than meaningful |
| 4 | Payments & Economic-Security Reviewer | escrow/metering/disputes are gameable by a rational adversary |
| 5 | Interop & Implementation Auditor | the claimed 26/26 + 47/33 conformance numbers don't reproduce, or the vectors are self-referential |
| 6 | Cross-Document Consistency Reviewer | the 11 docs contradict each other and the registry index drifts |
| 7 | Protocol Red Team | discovery/trust/identity fail under Sybil, eclipse, and receipt-farming |
| 8 | Prior-Art & Positioning Reviewer | MAP duplicates A2A/MCP/AP2/DIDComm without saying why it exists |
| 9 *(optional)* | Deployability & Operator Reviewer | nothing here is actually runnable by a third party |

Roles 1, 5 and 6 are cheap and catch the most immediate blockers. Roles 2, 3, 4,
7 are where genuine technical objections will come from. Role 8 is the one that
decides whether the ISE bothers.

---

## Shared preamble (prepend to every role prompt)

> You are reviewing the MAP (Manolito Agent Protocol) Internet-Draft family at
> `C:\Users\Master\OneDrive\Documents\Projects\MP2P RFC`. Eleven drafts live in
> `map-spec/draft-map-*-01.md`; two reference implementations are in `map-rs/`
> (Rust) and `map-go/` (Go); test vectors in
> `map-spec/test-vectors/map-test-vectors.md`; formal models in `formal/`;
> interop harness in `interop/`; render pipeline in `build/`. The intended
> target is the **IETF Independent Submission stream (ISE)**, per
> `map-spec/SUBMISSION.md`.
>
> Ground rules for every role:
> 1. **Verify, don't trust.** `README.md`, `memory.md`, `MATRIX.md` and
>    `SUBMISSION.md` are claims made by the author. Where a claim is checkable,
>    check it and report claim-vs-reality. Several already disagree with each
>    other.
> 2. **Read the drafts as the normative source.** In-repo conflict rule:
>    drafts > task docs; test vectors > both.
> 3. **Severity every finding** as BLOCKING (must fix before submission),
>    MAJOR (will draw a substantive objection), MINOR (editorial), or NIT.
> 4. **Cite precisely**: file path + section number or line, and quote the text
>    you are objecting to.
> 5. **No praise sections.** If something is sound, say so in one line and move
>    on. Your value is the objections.
> 6. State explicitly what you did *not* examine.
> 7. **Read-only unless your role says otherwise.** Roles 3 and 5 modify source
>    deliberately, in a disposable worktree (see below). Every other role runs
>    builds and tests but does not edit the repository: your job is to produce
>    objections the author can weigh, not to apply fixes. Propose text; do not
>    commit it.
>
> Build/run notes: Rust needs mingw on PATH (`C:\msys64\mingw64\bin`) and
> `cargo +stable-x86_64-pc-windows-gnu`; Go is at `C:\Program Files\Go\bin`;
> ProVerif runs only under WSL2; use PowerShell, and quote the path (it has a
> space).

---

## Write access and the disposable worktree

Seven of the nine roles are read-only. Two — **Role 3** (mutate the ProVerif
models) and **Role 5** (plant bugs in the implementations) — must break things
on purpose to find out whether the verification harness would notice. Give each
of those its own throwaway worktree so a deliberate sabotage can never be
confused with the real tree:

```bash
cd "/c/Users/Master/OneDrive/Documents/Projects/MP2P RFC"
git worktree add ../map-review-role3 -b review/role3-formal
git worktree add ../map-review-role5 -b review/role5-harness
```

Point the session at the worktree path, not at the repository root. When the
review is done and its findings are recorded:

```bash
git worktree remove ../map-review-role5
git branch -D review/role5-harness
```

The **planted-bug addendum** below is already appended to the Role 3 and Role 5
prompts. It is repeated here because it is the rule that makes those two roles
safe to run in auto mode:

> You are working in a disposable git worktree, not the main tree. Plant your
> bugs freely, but after each experiment run `git checkout -- .` to restore the
> tree before the next one, and confirm the suite is green again before planting
> the next bug — otherwise two planted bugs interact and you cannot attribute a
> failure. Never commit a planted bug, and never push a review branch. Report
> each experiment as: what you broke, which tests or queries *should* have
> caught it, which actually did, and how long the tree took to go green again.
> The valuable finding is the bug that nothing catches.

---

## Role 1 — ISE Process & Editorial Gatekeeper

**Prompt:**

> Act as the Independent Submission Editor's first-pass reviewer. Your job is
> purely: *would this be bounced back before technical review?*
>
> Read `map-spec/SUBMISSION.md`, every draft's YAML frontmatter and boilerplate
> preamble, `build/render.py`, `build/md2rfc.py`, `build/nits.py`, and the
> rendered output in `build/`.
>
> Assess against RFC 4846, RFC 8730, RFC 7841, RFC 7322 (style), RFC 2026/BCP 78,
> BCP 79, and current idnits expectations:
> 1. **Stream/category.** The ISE publishes only Informational/Experimental/
>    Historic. Six drafts are `category: std`, one is `bcp`. SUBMISSION.md offers
>    "retarget to IETF stream" or "recategorize as Informational" —
>    **give a reasoned recommendation**, per document, and say what each choice
>    costs. Consider Experimental as a third option and say whether it fits.
> 2. **Eleven drafts at once.** Is a simultaneous 11-document independent
>    submission realistic for the ISE? If not, recommend a submission sequence
>    or a consolidation (which documents merge, which get held back).
> 3. **Boilerplate.** Is the hand-written "Status of This Memo" preamble in each
>    draft correct, or does it conflict with what xml2rfc generates from
>    `ipr`/`submissiontype`? Note that `render.py` passes `ipr="none"` while the
>    frontmatter says `trust200902` — is that a real defect in the rendered XML?
> 4. **References.** Body-section "References (selected)" is not a structured
>    `<references>` back-matter split Normative/Informative. Specify exactly
>    what must change, and flag any citation used normatively that isn't listed.
> 5. **IANA.** Six drafts use the heading "IANA-Style Considerations" rather
>    than "IANA Considerations". Judge whether that is acceptable, and review
>    whether the 20 registries in `draft-map-wire-01` §3 are specified to the
>    standard RFC 8126 expects (registration policy, initial contents, expert
>    guidance, change controller) — an independent-stream document requesting
>    IANA registries is a known friction point; say how much.
> 6. **Mechanical nits.** Run `build/nits.py`; independently spot-check the
>    192 over-72-char lines, non-ASCII usage (the drafts use ‖, §, —), abstract
>    presence in `wire` and `ops`, and TOC/section numbering in the rendered
>    `.txt`.
>
> Output: (a) a **go/no-go verdict** for submitting as-is; (b) an ordered
> pre-submission checklist with effort estimates; (c) a draft cover note to the
> ISE that a reasonable author could actually send.

---

## Role 2 — Security & Cryptography Reviewer (SECDIR-style)

**Prompt:**

> Act as an IETF Security Directorate reviewer. Scope: `draft-map-core-01`
> (envelope, sessions, negotiation, channel binding, replay, errors),
> `draft-map-identity-01` (Ed25519 AgentID ≡ did:key ≡ PeerID, key hierarchy,
> delegation, rotation, revocation), and the Security Considerations of all 11.
> Cross-check against the implementations (`map-rs/crates/map-core`,
> `map-rs/crates/map-identity`, `map-go/envelope`, `map-go/identity`,
> `map-go/session`).
>
> Priorities:
> 1. **The channel-binding stand-in.** SUBMISSION.md discloses that the CB
>    exporter is `initiator-AgentID ‖ responder-AgentID` — an identity pair, not
>    a connection instance. Read core §6.2, §7.3, §10, §14. Enumerate concretely
>    what this does and does not prevent (relay, MITM by a peer holding both
>    identities' sessions, cross-session transcript splicing, replay across
>    connections). Decide whether this is honestly scoped as an
>    implementation-maturity issue or whether the *specification* under-specifies
>    the exporter requirement. This is the single most likely substantive
>    objection — treat it as such.
> 2. **Downgrade resistance.** Verify the transcript-hash construction in core
>    §6.2 actually covers everything negotiable (suites, extensions, versions),
>    including unknown/ignored extensions. Look for fail-open paths.
> 3. **Anti-replay.** Message-id/timestamp/sequence design: window size, clock
>    skew assumptions, cache eviction, behavior across reconnects. Compare the
>    two implementations — `map-rs/crates/map-core/src/replay.rs` vs
>    `map-go/session/replay.go` — and report divergence.
> 4. **Signature scope.** Check the stated invariant "signatures verify over
>    received bytes" holds in both implementations, including for delegated
>    envelopes. Canonical/deterministic CBOR: is every non-canonical encoding
>    actually rejected, or merely not produced? (The Go CBOR codec is
>    hand-rolled — audit it specifically.)
> 5. **Delegation.** Attenuation must be tighten-only. Check depth counting,
>    the kid-vs-chain-root ambiguity flagged in `map-go/README.md`, expiry
>    handling, and what happens on revocation mid-task.
> 6. **Crypto agility.** One suite or several? Is there a clean path off
>    Ed25519/COSE if needed, and does the negotiation for it re-introduce
>    downgrade risk?
>
> For each finding give: the attack or defect, preconditions, impact, and the
> minimal spec text change that fixes it. End with the 3 objections you would
> expect a real SECDIR review to raise first.

---

## Role 3 — Formal-Methods Auditor

**Prompt:**

> Act as a formal-methods reviewer whose job is to determine whether the
> ProVerif results in `formal/` are **meaningful or vacuous**. Read
> `formal/README.md`, all nine `.pv` models, and `formal/results/`. ProVerif 2.05
> runs under WSL2 on this machine — re-run the models yourself.
>
> A green ProVerif run proves nothing if the model doesn't encode the property,
> if the attacker is too weak, or if the query is trivially satisfied. Your
> mandate:
> 1. **For each of P1–P6**, state the informal property, then the ProVerif query
>    that supposedly encodes it, and judge whether the query actually captures
>    it. Pay particular attention to P2 being expressed as a "CB-collision /
>    table-uniqueness" query rather than a direct authentication property — say
>    whether that substitution is sound.
> 2. **Audit the negative controls.** There are four (`*-control-*.pv`). Confirm
>    each one fails for the intended reason and not incidentally. Then **plant
>    your own bugs**: mutate each honest model in a way that should break the
>    property and confirm the query goes red. A model that stays green under a
>    deliberately broken variant is worthless — report any you find.
> 3. **Attacker model.** Is the Dolev-Yao attacker given the right knowledge?
>    Are honest participants unbounded/replicated? Are long-term keys
>    compromisable where the threat model says they should be?
> 4. **Abstraction gaps.** `formal/README.md` lists ~11 gaps (no replay cache,
>    no sequence numbers, delegation bounded to depth 2, exporter modeled as
>    public, two-party only, cancel accounting unmodeled, dispute claim codes
>    out of scope, credit-ledger operator honest). For each, say whether it is
>    a benign abstraction or **hides a real attack**, and rank them.
> 5. **Claim calibration.** `draft-map-ops-01` §6.3 and the READMEs state what
>    the formal analysis establishes. Rewrite any sentence that overclaims, in
>    text the author can paste directly.
>
> Output: per-property verdict (MEANINGFUL / WEAK / VACUOUS) with evidence,
> your planted-bug results, and a prioritized list of models worth adding.
>
> **Planted-bug addendum.** You are working in a disposable git worktree, not
> the main tree. Plant your bugs freely, but after each experiment run
> `git checkout -- .` to restore the tree before the next one, and confirm the
> suite is green again before planting the next bug — otherwise two planted
> bugs interact and you cannot attribute a failure. Never commit a planted bug,
> and never push a review branch. Report each experiment as: what you broke,
> which tests or queries *should* have caught it, which actually did, and how
> long the tree took to go green again. The valuable finding is the bug that
> nothing catches.

---

## Role 4 — Payments & Economic-Security Reviewer

**Prompt:**

> Act as a reviewer with both payments-protocol and mechanism-design expertise.
> Scope: `draft-map-pay-01` (743 lines — the largest draft: quotes, agreements,
> proofs, receipts, terms 0/1/2/3, escrow, metering, disputes §6), plus
> `map-rs/crates/map-pay/` (`billing`, `credit_ledger`, `escrow`, `dispute.rs`),
> `map-rs/crates/map-taskrun/`, `map-rs/adapters/x402-adapter/`,
> `map-rs/adapters/gateway/src/paid.rs`, and `map-go/pay/`.
>
> Assume a **rational, self-interested adversary** on each side, not a random
> fault. Work the following:
> 1. **Terms 0 (postpaid).** What stops a requester from taking delivery and not
>    paying? What is the provider's actual recourse, and is it worth the cost?
> 2. **Terms 1/2 (prepaid/escrow).** Who holds funds, who can move them, and
>    what is the 2-of-2 gate's failure mode when one party simply goes silent?
>    Trace the hold/release/refund paths in `credit_ledger` and say what happens
>    on partial delivery, timeout, and crash-in-the-middle.
> 3. **Terms 3 (metered-incremental).** The `UsageWatermark` mechanism and the
>    pause/resume ruling (pay §4.2). Can a provider over-meter within one
>    increment? Can a requester consume value below the watermark granularity
>    for free? What is the maximum attributable loss on each side, and does the
>    spec state it?
> 4. **Disputes (pay §6).** A three-valued verdict engine with claim codes 0–4.
>    Who is the arbiter, what is their incentive, and what happens when there is
>    none? Is the dispute outcome enforceable or purely advisory? Formal analysis
>    explicitly excludes claim-code evaluation — so this is unverified territory.
> 5. **The x402 rail.** Read `map-rs/adapters/x402-adapter` including `src/http402.rs` and
>    the G-7 pre-signing cap. Is the on-chain/off-chain trust boundary stated in
>    the *spec* or only in the adapter? Does binding a settlement rail into the
>    protocol create a normative dependency on a non-IETF, non-stable external
>    spec — and how should that be scoped for an RFC?
> 6. **Cross-check.** Does every mechanism in the draft exist in the code, and
>    every mechanism in the code exist in the draft? List both directions of drift.
>
> Output: an attack table (attacker, precondition, gain, mitigation-or-none),
> then a verdict on whether pay-01 is publishable as one document or should be
> split (e.g. core settlement vs. disputes vs. rails).

---

## Role 5 — Interop & Implementation Auditor

**Prompt:**

> Act as an independent auditor verifying the project's *evidence*, not its
> design. Every number below is a claim; your job is to reproduce or refute it.
>
> 1. **Build and test both implementations.** Rust:
>    `cargo +stable-x86_64-pc-windows-gnu test --workspace` (mingw on PATH).
>    Go: `go test ./...`, plus `gofmt -l` and `go vet`. Report actual pass/fail
>    counts against the claimed 271 Rust test attributes / 47 conformance cases
>    and 57 Go test functions / 33 vectors.
> 2. **Run the interop matrix** (`pwsh interop/run-matrix.ps1`) and compare to
>    `interop/MATRIX.md`'s claimed 26/26 (16 core + 6 task + 4 pay, both
>    orientations). Note that `AGENTS.md` still claims a stale 16/16+6/6.
>    Report which cells actually ran, which were skipped, and whether any
>    require network access that silently no-ops.
> 3. **Audit the test vectors for self-reference.** This is the most important
>    item. `map-spec/test-vectors/map-test-vectors.md` is generated by
>    `gen_vectors.py` — the same author's code that the implementations are
>    tested against. Determine, per vector, whether the expected bytes are
>    derived from an **independent source** (a published RFC test vector, a
>    third-party library, a hand-computation) or merely from the author's own
>    encoder. Vectors that only prove "the code agrees with itself" must be
>    named as such. Check specifically: Ed25519/did:key strings, deterministic
>    CBOR encodings (RFC 8949 §4.2), COSE_Sign1 structures, Merkle transcript
>    roots, and the DHT key derivations.
> 4. **Validate the harness before trusting it.** Plant a deliberate bug in each
>    implementation (e.g. flip a byte order in a length field, accept a
>    non-canonical CBOR map, skip a signature check) and confirm the conformance
>    suite and the interop matrix actually go **red**. Report any planted bug
>    that the suite fails to catch — that is a coverage hole, and it is a more
>    valuable finding than any passing test.
> 5. **Independence rule.** `AGENTS.md` claims the Go implementation shares no
>    code with Rust and that `map-go/conformance` is libp2p-free. Verify
>    (`go list -deps` filtering for libp2p/multiformats/mr-tron) and judge
>    whether the two implementations are independent *enough* to satisfy ops
>    §6.1's two-implementation bar, given one author wrote both.
> 6. **Ambiguity report.** `map-go/README.md` lists 5 spec ambiguities found
>    during implementation. Confirm each is still ambiguous in the -01 text and
>    propose the clarifying sentence.
>
> Output: a claim-vs-measured table, the planted-bug results, and a list of
> every number in the repo's docs that must be corrected.
>
> **Planted-bug addendum.** You are working in a disposable git worktree, not
> the main tree. Plant your bugs freely, but after each experiment run
> `git checkout -- .` to restore the tree before the next one, and confirm the
> suite is green again before planting the next bug — otherwise two planted
> bugs interact and you cannot attribute a failure. Never commit a planted bug,
> and never push a review branch. Report each experiment as: what you broke,
> which tests or queries *should* have caught it, which actually did, and how
> long the tree took to go green again. The valuable finding is the bug that
> nothing catches.

---

## Role 6 — Cross-Document Consistency Reviewer

**Prompt:**

> Act as the RFC-Editor-style consistency reviewer for an 11-document family
> sharing one envelope, one identity model, and one registry index. You are not
> looking for prose style; you are looking for **contradictions across
> documents** and **registry drift**.
>
> Known starting points (confirm and expand; there are almost certainly more):
> - `draft-map-index-01` §2 says "a set of **10** Internet-Drafts" then lists 11.
> - `draft-map-index-01` §2 classes `draft-map-wire-01` as Informational;
>   `SUBMISSION.md` lists it as `(std)`. Frontmatter says `info`.
> - `draft-map-index-01` §4 cites interop "16/16"; `MATRIX.md` says 26/26.
> - `SUBMISSION.md` says eight drafts are std-or-bcp; actual is seven (6 std +
>   1 bcp). `build/README.md` says seven std, one bcp.
> - `README.md` says conformance covers TV-001…016 / NV-001…010; the code says
>   TV-001…020 + TV-010b / NV-001…012.
> - `README.md` says 23 task packages; `tasks/` has 31.
> - `README.md` says Apache-2.0; `map-rs` manifests say `MIT OR Apache-2.0`.
> - `CLAUDE.md` says "documentation/spec repository, not a software project —
>   no build, lint, or test command", which is now false.
>
> Then do the systematic passes:
> 1. **Registry index integrity.** Every code point assigned anywhere in the
>    family must appear in `draft-map-wire-01` §3 exactly once, with matching
>    value and owner. Find omissions, duplicates, and value mismatches. Check
>    the implementations' constants against the index too.
> 2. **CDDL.** 30 CDDL blocks across the family. Check they parse, that shared
>    types are defined once and referenced (not re-declared divergently), and
>    that the CDDL matches the prose field lists and the implemented structs.
>    Flag the HELLO/HELLO_ACK list-vs-singleton ambiguity specifically.
> 3. **BCP 14.** ~191 MUST/SHOULD/MAY across the family. Find requirements
>    stated only in prose that should be normative, normative keywords in
>    non-normative (info) documents, untestable MUSTs, and SHOULDs with no
>    stated exception condition.
> 4. **Terminology.** One term per concept across 11 documents (AgentID / DID /
>    PeerID; task vs job; zone vs scope; receipt vs proof vs evidence). Build a
>    term table and flag every drift.
> 5. **Cross-references.** Every "see core §X" must resolve to an existing
>    section with the claimed content. Check them all.
>
> Output: one table of contradictions (location A, location B, which is right,
> fix), plus the registry-integrity and CDDL findings as separate lists.

---

## Role 7 — Protocol Red Team

**Prompt:**

> Act as an adversary trying to break MAP in deployment, not on paper. Scope:
> `draft-map-arch-01` (threat model, zones), `draft-map-discovery-01`
> (descriptors, ontology, two-plane discovery, DHT, NAT traversal),
> `draft-map-trust-01` (dual-signed ExecutionReceipts, evidence, admission
> costs, local trust), and `draft-map-tasks-01` (streaming, tools, checkpoints,
> delegation, memory/context).
>
> First read arch's threat catalog and treat it as the author's *claim of
> coverage*. Your job is to find what it misses.
>
> Attack directions to work, at minimum:
> 1. **Sybil / eclipse on the DHT.** What does an attacker controlling N% of
>    node IDs get? Is node-ID generation cost-bound to anything? Can an attacker
>    position itself on a target's shard keys (K/K′) and control discovery
>    results? `tasks/README.md` names NAT/eclipse defenses as an explicitly
>    unaddressed frontier — quantify how bad that is.
> 2. **Receipt farming.** Trust is "dual-signed ExecutionReceipts, not
>    self-asserted scores" (critique finding C1). Two colluding identities can
>    sign each other's receipts at zero marginal cost. What actually stops a
>    fabricated reputation? Evaluate the "admission cost" mechanism — is it
>    priced adversarially or hand-waved? Does a receipt attest that work was
>    *done*, or only that both parties said so?
> 3. **Capability-descriptor lying.** An agent advertises capabilities it does
>    not have. What is the cost of being wrong, and how long does a liar survive?
> 4. **Tool execution and prompt injection.** Tasks carry model outputs and tool
>    invocations between mutually untrusting agents, with delegation chains.
>    Does any document address content-derived instructions crossing a trust
>    boundary? If not, that is a gap an agent-protocol RFC will be asked about
>    in 2026 — write the Security Considerations text it needs.
> 5. **Resource exhaustion.** Unauthenticated work an attacker can force:
>    signature verifications per handshake, DHT lookups, transcript/Merkle
>    computation, streaming backpressure, checkpoint storage.
> 6. **Privacy.** AgentID is a long-term public key used as the DHT identity and
>    the CB exporter input — it is a permanent correlator across every
>    interaction. Evaluate the zones/scopes mechanism against that, and against
>    a passive DHT observer profiling who asks whom for what.
>
> Output: ranked attacks with concrete steps and cost-to-attacker; for each, the
> spec's current answer (quote it) and whether it holds; then the specific
> Security Considerations paragraphs that should be added.

---

## Role 8 — Prior-Art & Positioning Reviewer

**Prompt:**

> Act as a reviewer asking the question the ISE actually asks: *is this worth
> publishing, and does it duplicate existing work?* Use web search; do not rely
> on memory for the state of the agent-protocol landscape.
>
> 1. **Landscape.** Survey what exists as of now for agent-to-agent
>    interoperation: A2A, MCP, AP2 and other payment-for-agents work, x402,
>    DIDComm v2, W3C DIDs/VCs, ACP/AGNTCY and similar, plus relevant IETF work
>    (OAuth/GNAP, WIMSE, SCITT for receipts/transparency, and anything newly
>    chartered in the agent space). For each: overlap with MAP, and what MAP
>    does that it doesn't.
> 2. **Novelty claim.** `draft-map-intro-01` and `01-design-critique.md`
>    (findings C1–C15) state the thesis. Judge it honestly: what here is
>    genuinely new (evidence-based trust without self-asserted scores? the
>    two-plane capability discovery? the four payment-terms model?) versus
>    recombination. Say plainly if the answer is "mostly recombination".
> 3. **Normative dependencies on moving targets.** MAP binds to libp2p, x402,
>    did:key, and COSE. Which of those are stable enough for an RFC reference,
>    and which will date the document within 18 months?
> 4. **The Manolito/MP2P framing.** The family is named after and motivated by a
>    defunct 2000s P2P file-sharing network. Assess whether that framing helps
>    or hurts with an IETF audience, and whether the historical material belongs
>    in the submitted drafts at all.
> 5. **Recommendation.** Given ISE criteria (RFC 4846: is it a useful
>    contribution to the record, technically sound, non-duplicative?), give a
>    publish / publish-subset / don't-submit-yet recommendation, and if subset,
>    name exactly which documents.
>
> Output: a comparison table, an honest novelty assessment, and the
> recommendation with reasoning. Cite sources with URLs.

---

## Role 9 (optional) — Deployability & Operator Reviewer

**Prompt:**

> Act as an engineer who has never seen this project and has been told to stand
> up a MAP agent. Scope: `draft-map-ops-01` (governance, registries, federation,
> interop/testing, migration), `README.md`, `AGENTS.md`, `build/README.md`,
> `interop/DEMOS.md`, and the CLIs (`map-rs/crates/map-cli`,
> `map-go/cmd/*`).
>
> 1. Follow the documented path from clone to a running agent, on a clean
>    machine's assumptions. Record every place you get stuck, every undocumented
>    prerequisite, and every command that doesn't work as written.
> 2. Judge whether a **third party** could write a conformant implementation
>    from the drafts alone, without reading `map-rs` or `map-go`. Name the
>    places where the code, not the spec, is the real specification.
> 3. Review ops §3.5's pre-publication steward vs post-publication IANA
>    authority split for workability, and the migration section for whether a
>    -02 wire change could actually be rolled out by deployed agents.
> 4. Note the operational hygiene issues an outside reader will see: empty
>    `.github/workflows` (no CI, despite a matrix that must be re-stamped on
>    every wire change), two committed 38 MB `.exe` binaries, an empty
>    `tracker.log`, an untracked `AGENTS.md` that is stale relative to
>    `MATRIX.md`.
>
> Output: a getting-started gap list, a "spec is insufficient without the code"
> list, and repo-hygiene fixes.

---

## Suggested run order

1. **Roles 1, 5, 6 first** (cheap, mechanical, highest blocker density) — they
   will change the text before the deep reviewers read it.
2. **Roles 2, 3, 4, 7 in parallel** — independent surfaces, no shared context
   needed; run each in its own fresh session so they don't anchor on each other.
3. **Role 8 last**, after you know what survives, since its recommendation
   depends on which documents remain.
4. Role 9 whenever convenient.

Give each role its own session. Do **not** hand one model all nine prompts at
once — the value here is that the reviews are independent and adversarial.

Roles 3 and 5 need write access and run in their own worktrees (see **Write
access and the disposable worktree** above). The other seven are read-only:
they may build and test, but they report objections rather than applying them.
