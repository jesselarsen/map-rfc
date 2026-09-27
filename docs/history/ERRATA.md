# MAP Spec Errata — rulings record

Two kinds of entry live here. **RULED** entries dispose of findings from the
external review of 2026-09-18 (process: `REVIEW-PROCESS.md`; roles:
`REVIEW-PROMPTS.md`), newest first. **RESOLVED** entries below them are the
original PENDING-ARCHITECT queue.

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

## Paper red-team review, 2026-09-27

Findings are registered in `paper/REVIEW-redteam-1.md` (RT-xx) and
`paper/REVIEW-redteam-2.md` (R2-xx), against the paper draft at `c67ad76`.
Rulings below follow `REVIEW-PROCESS.md` Stage 2.

RULED 2026-09-27 (review finding R2-08, roles 1/2/3 — PC, Protocol red team,
Methodology) — resuming from the output prefix assumes that all task state is
in the delivered Parts. It does not carry hidden provider state (tool results,
scratchpads, working files), and a successor may repeat external side effects
it cannot see. E2's "0.42%" counts only regenerated Parts, not the
successor's cost to ingest the prefix. — RULING: **ACCEPT-EDITORIAL** +
**DOCUMENT**. Changes:
- design: a "Scope of resumption" paragraph (output-state tasks only; no
  hidden state; side effects may repeat; MAP does not provide idempotency);
- E2: "What the waste metric counts". The expected prefix replayed is λL²/2,
  about 500 Parts at the H2 point against 3.2 regenerated. H2 holds while
  the ingest-to-generate cost ratio ρ < 1.5. The engine-state fast path is
  deferred (layer §7.6);
- the intro, abstract ("regenerates", "a streamed task") and conclusion are
  scoped the same way; the abstract stays at 1,901 characters;
- Limitations gains a "Resumption scope" bullet;
- spec layer §7.4 gains an informative *Scope* paragraph, and
  `spec/CHANGELOG.md` records it with the R2-07 correction.
Editorial; no wire change. — Implemented in
`paper/main.tex`, `paper/sections/{design,evaluation,intro,conclusion,limitations}.tex`,
`spec/layer.md` §7.4 and `spec/CHANGELOG.md`.

RULED 2026-09-27 (review finding R2-06, roles 3/4 — Methodology, Claims
auditor) — the paper omitted items the pre-registration committed to reporting,
while saying it reports everything. These were checkpoint byte overhead ("the
paper states this honestly"), B1's zero feedback cost ("stated as a
limitation"), the E1 time-to-detect and all-rounds metrics, the 16-core E3b
("the paper reports both"), and the E3d message-rate crossover (also RT-14). —
RULING: **ACCEPT-EDITORIAL**. A new `eval/analysis/prereg_extra.py` reads only
the committed CSVs and writes `eval/results/5bc7702/PREREG-EXTRA.md`, and
`run_all.ps1` now runs it. The paper now reports each item:
- time to detect: Ours-L reaches α/4 in 26–33 rounds (W2) and 27–38 (W1) in
  34 of 35 cells, and never under camouflage at α = 0.5; B2 never reaches it;
  B1 never does in 18 cells and B2L in 20; Ours-L+C is below it from round 0;
- all-rounds share: cold start roughly triples Ours-L's share;
- bytes: 38.5 B per Part at c = 8, 4.8× an 8-byte Part, plus 4.25 B of
  voucher digest and base64 overhead;
- the 16-core E3b also holds the whole shard;
- the E3d crossover: pull costs fewer messages only while q < N/(1.6·K·TTL),
  i.e. TTL < 0.63 s in E3d;
- B1's zero-fee assumption is stated in Limitations.
RT-14 is resolved by the crossover sentence. Editorial; no wire change. —
Implemented in `paper/sections/{evaluation,limitations}.tex`,
`eval/analysis/prereg_extra.py`, `eval/results/5bc7702/PREREG-EXTRA.md`,
`eval/run_all.ps1`, the provenance notes in `paper/main.tex`,
`paper/README.md` and `eval/README.md`.

RULED 2026-09-27 (review finding R2-10, role 1 — PC) — contribution 4
presented "three negative results that we believe generalize", but all
three are known: a global ledger collapsing under free Sybil slander follows
from the threat model; propagation failing against bought attack edges is
the classic weakness the paper itself cites; grinding with free identities
follows from Douceur. — RULING: **ACCEPT-EDITORIAL**. Contribution 4 now
reads "measured confirmations of three known weaknesses in the agent
setting", with citations to SybilGuard and Douceur. It says two of them
changed the specification: the default weighting became W1, and
epoch-salted sharding became NOT RECOMMENDED as a mitigation. The slander
result only confirmed T-3, which dates from v1, so it is not claimed as a
change. The minimax reversal is promoted to a new contribution 5: a
pre-registration should fix where a decision is tested, not only how it is
made. The conclusion's negative-results paragraph is rewritten to match.
Editorial; no wire change. — Implemented in `paper/sections/intro.tex` and
`paper/sections/conclusion.tex`.

RULED 2026-09-27 (review finding R2-05, role 1 — PC, novelty) — the closest
prior art was missing. Checked against primary sources:
- TrustChain (FGCS 107, 2020): its IETF draft says "transaction injection can
  only be done with two valid signatures, meaning a Sybil attacker can only
  create trust with itself", which is MAP's R-4/T-3 argument;
- Credence (NSDI '06, HTML text): per-client weights from correlations of
  voting histories plus a "PageRank-style" flow computation, the analogues of
  W1 and W2;
- PeerTrust (TKDE 16(7), 2004): the personalized similarity measure;
- DSybil (IEEE S&P 2009, pp. 283–298): provable loss bounds against unlimited
  Sybils.
— RULING: **ACCEPT**. Changes:
- all four are in `references.bib`;
- related work has a TrustChain paragraph with MAP's three differences
  (standalone receipts rather than per-agent chains; agreement weighting
  rather than contribution flow; checkpoints, resumption and the voucher
  binding), and Table 3 gains TrustChain and Credence rows;
- the classic-reputation paragraph now says neither W1 nor W2 is new;
- design §W1 credits PeerTrust and Credence;
- the security G2 argument engages DSybil: "no evidence-based scheme can tell
  them apart" becomes "evidence cannot tell them apart but can bound the
  damage", and a loss-bounded weighting for receipts is named as future work;
- the intro's "What we do not claim" now names TrustChain and
  agreement-weighted local trust.
Credence entry has no pages because USENIX lists none for NSDI '06. Editorial;
no wire change. — Implemented in `paper/references.bib` and
`paper/sections/{related,design,security,intro}.tex`.

RULED 2026-09-27 (review finding R2-09, roles 1/4 — PC, Claims auditor) — the
paper had no generative-AI disclosure. Of the 90 commits up to its first
draft, 81 carry a Claude co-author trailer, and review 1 was run by the model
that drafted the paper. Yet the Limitations section said "the same person
wrote MAP, both implementations, the baselines and the simulator", and the
v1 review was called "an external review". `MAP Review Results/` shows that
review was eight AI-model sessions. — RULING: **ACCEPT**. Changes:
- a new `acks` section discloses the use of Claude (Claude Code) for code,
  specification and paper text, and all the reviews, and gives the commit
  count. ACM requires disclosure in the work; arXiv holds authors
  responsible for all content;
- Limitations now reads "One author, one model family" and names the
  correlated-error risk: the same model lineage built the system and the
  baselines and reviewed both;
- the design, security, evaluation and implementation sections say the
  earlier review was model sessions, and they no longer say one person
  "wrote" the code.
The author's own attestation ("directed the work, reviewed the results,
takes full responsibility") is listed in `paper/README.md` for the author to
confirm before posting. Editorial; no wire change. — Implemented in
`paper/main.tex`, `paper/sections/{limitations,design,security,evaluation,implementation}.tex`
and `paper/README.md`.

RULED 2026-09-27 (review finding RT-08, roles 1/3 — PC, Methodology; widened
in register 2) — the abstract and conclusion quoted the favourable half of H1
(27 of 28 against the canary baseline B2), not the pre-registered verdict
(PARTIAL, 18 of 28). They also did not name the weighting or say that H1 covers
α ≤ 0.3 only; the prereg excluded α = 0.5, where W2 is 2.2× B2 under
camouflage. — RULING: **ACCEPT-EDITORIAL**. Both now state H1 as
pre-registered: local trust with the pre-registered weighting at least halves
the attacker share of both B1 and B2, over seven attacks at α ≤ 0.3. It is
partially supported at 18 of 28, and 27 of 28 remains as the canary-only
figure. Both lists of losses now include camouflage relative to a global
ledger (H1 fails against B1 at α = 0.2 and 0.3). The abstract was tightened to
1,905 characters to fit arXiv's 1,920-character limit. That cut the sentence
"Emerging trust layers answer them with global reputation ledgers or
single-signer receipts", which also resolves **RT-09**: the claim
contradicted the related-work section (Signet co-signs). Editorial; no wire
change. — Implemented in `paper/main.tex` (abstract) and
`paper/sections/conclusion.tex`.

RULED 2026-09-27 (review finding R2-28, role 4 — Claims auditor; raised while
fixing R2-02) — Appendix A says `eval/run_all.ps1` reproduces every table and
figure. It did not run the A8/A9 sweeps (`e1_camo_explore`, `e1_minimax`),
`minimax.py` or `b2l.py`. Without `e1-minimax.csv`, `make_figs.py` silently
skipped Fig. 4, so Table 2 (minimax) and Fig. 4 came from one-off commands. —
RULING: **ACCEPT**. `run_all.ps1` now runs both sweeps after `e1_main`, in fast
mode as the committed runs did, and then `minimax.py` and `b2l.py`. `-Quick`
skips the two scripts because they require 20 seeds per cell. Verified: the
sweeps re-run at HEAD reproduce the committed CSV SHA-256s. The only simulator
change since 5bc7702 is the additive `ProjParams::recommended()` constructor.
`minimax.py` and `b2l.py` reproduce the committed MINIMAX.md and B2L.md byte
for byte. Tooling only; no paper text or wire change. — Implemented in
`eval/run_all.ps1` and `eval/README.md`.

RULED 2026-09-27 (review finding R2-02, roles 1/3/4 — PC, Methodology, Claims
auditor) — E1 ran B2L, the canary plus provider-signed ledger that Wang et al.'s
prose describes, and Fig. 3 plotted it, but the text never reported it. B2L
beats Ours-L under whitewashing and slander at every α. — RULING: **ACCEPT**,
reported as exploratory because H1 did not pre-register it. The new script
`eval/analysis/b2l.py` reads only `e1-main.csv`, bootstraps the arms
independently (they do not share random streams, RT-12) and writes
`eval/results/5bc7702/B2L.md`. Before use, the numpy 2.4.6 environment was
checked to reproduce the committed MINIMAX.md byte for byte. Result, over 35
cells: Ours-L+C is lower than B2L in 25 and overlaps it in 10 (whitewashing and
slander); it is never higher. Ours-L alone is higher than B2L under
whitewashing and slander at every α, and under on-off at 4 of 5. Editorial; no
wire change. — Implemented in `paper/sections/evaluation.tex` (new paragraph
and Table `tab:b2l`), `eval/analysis/b2l.py`, `eval/results/5bc7702/B2L.md`,
and the provenance notes in `paper/README.md`, `paper/main.tex` and
`eval/README.md`.

RULED 2026-09-27 (review finding R2-01, roles 3/4 — Methodology, Claims
auditor) — the paper said "the ablations confirm both admissibility rules", but
A-unilateral matches Ours-L in every cell. — RULING: **ACCEPT-EDITORIAL**, and
the review's explanation is corrected. The finding said T-3 zeroes the
unilateral positive receipts. In fact E1 never produces one: `e1.rs` emits
dual-signed receipts for honest successes, farming, collusion and camouflage
(the attacker holds both keys), and requester-only receipts only for failures
and slander. The arm therefore admits a receipt type that never occurs, and
its largest difference from Ours-L (0.0023) is noise between independent
random streams (B2L.md §2). E1 measures T-3, not R-4, and R-4 rests on the G1
argument. Editorial; no wire change. — Implemented in
`paper/sections/evaluation.tex` (Arms: "requester-only positive receipts"; new
Ablations paragraph) and `paper/sections/security.tex` (reviewer-findings
bullet on receipt farming).

RULED 2026-09-27 (review finding R2-07, role 4 — Claims auditor) — the paper
and the spec quoted MAP's signing overhead as 0.086% of task time at c = 8 and
0.69% at c = 1, and the paper quoted a transcript append as 1.4 µs. These
figures come from the superseded evaluation run `9a4a968`. The declared source
run `5bc7702` gives 0.0794% and 0.6306% (REPORT.md, H5) and 1.33 µs
(`bench/transcript_push_root`), so `paper/README.md`'s provenance statement
was false for these three numbers. — RULING: **ACCEPT-EDITORIAL**. The numbers
change and the conclusions do not: the stale values were higher, and H5's
"< 1% at c ≥ 8" holds either way. Editorial; no wire change. — Implemented in
`paper/sections/evaluation.tex` (0.079%, 0.63%),
`paper/sections/implementation.tex` (1.3 µs), `spec/layer.md` §7.5 C-6
rationale, and the C-6 row of `spec/CHANGELOG.md`.

---

## External review, 2026-09-18 — Track 0 rulings

Eight role-based external reviews (`map-spec/REVIEW-PROMPTS.md`) were run
against HEAD `56bce96`. Rulings below follow `REVIEW-PROCESS.md` Stage 2.
Track 0 is the claim-vs-reality pass only; Track 1 (removals) and Track 2
(normative fixes) are ruled separately as they are executed.

RULED 2026-09-18 (review finding R8-06, role 8 — Prior-Art & Positioning) —
the Independent Submission stream cannot create new IANA registries (RFC 8726
§4), will not appoint designated experts and therefore cannot grant
Specification Required or Expert Review policies (§5), and cannot grant
Standards Action (§2). All ~20 registries in wire §4 use exactly those three
policies. — RULING: **ACCEPT**, and it is decisive. Relabeling `std` → `info`
does not fix it; `SUBMISSION.md` never named this blocker. The family's
registries move to the self-hosted steward process ops §3.5 already describes,
as the permanent arrangement, and only narrow registry-free extracts are
submitted. RFC 8726 §4 explicitly contemplates this ("registries are hosted by
organizations other than IANA"). Editorial; no wire change. — Implemented in
`SUBMISSION.md` (Stream section rewritten; the registry blocker is now
MUST-resolve item 0) and `draft-map-index-01` §4.

RULED 2026-09-18 (review finding role 3 — Formal-Methods Auditor) — three of
the six claimed formal properties are vacuous, and two of those are proved over
an empty trace set according to the repository's own stored logs
(`results/pay-dispute.pv.log:64`, `results/pay-meter.out:194` both record the
reachability event as unreachable while `formal/README.md`'s table claims the
opposite). P2 as shipped is true by symbolic term structure. Eight planted
security-check removals — including an initiator accepting an unsigned
HELLO_ACK and a payer charging on an unsigned release — changed no query
result. — RULING: **ACCEPT**. The sentence "P1–P6 are machine-checked" was
false and is removed everywhere it appeared. P4/P5/P6 come out of the claim set
rather than being rebuilt; sound versions need a real state model, an actual
watermark, and bounded arithmetic, which is months of work. The P2 repair (one
responder nonce plus an injective query, verified TRUE by the reviewer) is
scheduled for Track 2 because it converts the weakest claim into the strongest
for an afternoon's work. Editorial. — Implemented in `ops` §6.3,
`SUBMISSION.md`, `README.md`, `draft-map-intro-01` §6, `formal/README.md`
(results table + the "not a vacuous result" sentence).

RULED 2026-09-18 (review finding R8-08, role 8) — `draft-map-intro-01` §6 and
`SUBMISSION.md` claim "two independent, separately-authored implementations";
`git log` shows 63 commits by one author, covering both. — RULING: **ACCEPT**.
In IETF usage "independent" means different parties. The source-isolation rule
(Go written from the drafts and frozen vectors without reading Rust source) is
real and produced at least one genuine catch — Go implemented the foreign-
SessionID check first and Rust was patched — but it is not two parties. The
ops §6.1 criterion is now stated as met mechanically and not substantively,
in both the criterion's own section and every document that cites it.
Editorial. — Implemented in `ops` §6.1, `draft-map-intro-01` §6,
`draft-map-index-01` §4, `README.md`, `SUBMISSION.md`.

RULED 2026-09-18 (review findings role 5 and role 6) — `README.md` cited a
16/16 interop matrix in three places against a recorded 26/26, contradicted
itself on the TV/NV counts between its own lines, and claimed the protocol is
"implemented twice" when several registered message classes have no codec in
either implementation and `map-go` additionally lacks the entire trust-receipt
layer. `ops` §6.1 additionally asserted that cross-language task and payment
flows "remain future work" — false since 2026-07-17. — RULING: **ACCEPT**.
The interop figure is corrected to 26/26 everywhere and `README.md`'s Status
section now states implementation coverage in three explicit tiers rather than
as one claim. Editorial. — Implemented in `README.md`, `ops` §6.1,
`draft-map-index-01` §4, `draft-map-intro-01` §6.

RULED 2026-09-18 (review finding R8-02, role 8) — `arch` §1 cited "the attached
design work" and `ops` §7.2 was titled "From HEP-0.1 drafts (the attached
preprint)". Both point at unpublished assistant transcripts under
`input-designs/`; `01-design-critique.md`'s C1–C15 findings critique that same
private material and are cited ~25–30 times across nine drafts. — RULING:
**ACCEPT**. A standalone document cannot cite material its readers cannot
obtain, and the drafts meanwhile cite no actual prior art at all. The "attached"
wording is removed now. The `Cn` citations and ops §7.1–§7.3 are ruled for
removal in Track 1, and an editorial note in ops §7.2 records that pending the
edit. Editorial. — Implemented in `arch` §1 and `ops` §7.2; Track 1 carries the
rest.

RULED 2026-09-18 (review finding role 6, seeded item 8) — `CLAUDE.md` stated
"This is a documentation/spec repository, not a software project — there is no
build, lint, or test command." — RULING: **ACCEPT**. False since the
implementations landed, and actively misdirecting: it is the first file a new
session reads. Rewritten to name both implementations, the render pipeline and
the interop harness, and to point at `AGENTS.md` for commands. Editorial, out
of spec scope. — Implemented in `CLAUDE.md`.

RULED 2026-09-18 (review finding role 6, seeded item 7) — `README.md` states
"License: Apache-2.0" while `map-rs/Cargo.toml` declares
`license = "MIT OR Apache-2.0"`, and no MIT text exists in the repository. —
RULING: **DEFER** — author decision, not a copy-edit. An IETF submission cares
about IPR precision and the two statements cannot both stand. Resolve before
either draft is submitted, by adding the MIT text or by narrowing the manifest.
— No change made.

RULED 2026-09-18 (review finding role 1 — ISE Process & Editorial Gatekeeper)
— `draft-map-wire-01` and `draft-map-ops-01` have no `## Abstract` section and
render `md2rfc.py`'s generic family placeholder, while `build/nits.py` reports
them green because it greps for the word "Abstract" anywhere in the rendered
text. A document-specific abstract is an RFC-Editor requirement. — RULING:
**ACCEPT**, and the tooling defect is the more important half: a check that
cannot fail is worse than no check, because it is trusted. `nits.py` now
detects the placeholder (matched against whitespace-collapsed text, since
xml2rfc re-wraps it) and hard-fails; verified to go red for exactly those two
drafts before the abstracts were written, and green after. `render.py` now
prints xml2rfc's stderr on successful runs as well as failed ones — discarding
warnings from a zero-exit run is how the dropped `submissionType` attribute
stayed invisible. Editorial. — Implemented in `draft-map-wire-01`,
`draft-map-ops-01`, `build/nits.py`, `build/render.py`.

RULED 2026-09-18 (review findings role 2, role 6, role 8 — converging) — all
eleven Status-of-This-Memo preambles asserted "Two independent interoperating
implementations pass the full live interop matrix". — RULING: **ACCEPT**, same
reasoning as the R8-08 ruling above; corrected in all eleven. Editorial. —
Implemented family-wide.

RULED 2026-09-18 (review findings role 4 R4-01/R4-03/R4-04, role 7 A1/A2/A3,
role 2 A1) — six exploits were demonstrated or measured against unmodified
code: a negative `Amount` moves funds from payee to payer; dispute claim 0 is
rebutted by one self-signed COMPLETE and claim 1 is unilaterally forgeable;
receipt farming reaches score 0.998 at confidence 501 in 154 ms; tool-grant
caveats have no assigned code points, so a conforming grant must be rejected by
every conforming validator; publishing to all 256 shards makes one agent the
majority of every shortlist; and the identity-pair channel binding is not a
channel binding under RFC 5056 and is normatively authorized, not merely
implemented. — RULING: **ACCEPT**, scheduled across Tracks 1 and 2, with
disposition per item: disputes, escrow, the x402 method definition, crypto
suite 3 and tool invocation are removed rather than repaired; non-negative
amounts, shard integrity, issuer weighting, the channel binding and the
injection treatment are fixed. Wire-affecting; each gets its own ruling as it
lands. Nothing is submitted while any of these stands. — Track 1/2.

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
