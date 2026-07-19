# tasks/ — Implementation-Specification Packages

This directory holds the engineering task packages that drive the MAP
reference-implementation program (ops §8) under a split-role model:

- **Architect**: reviews the specs, performs the design analysis, and
  authors these packages. Does not write the production implementation.
  **A 2026-07-12 architect session resolved the entire PENDING-ARCHITECT
  queue (rulings + spec patches in `map-spec/ERRATA.md`) and authored
  TASK-017/018. The role is again unstaffed after that pass — the
  "Operating without the architect" protocol below stays in force for
  anything those packages don't pre-decide.**
- **Coding LLM / engineer**: executes one package at a time. Each package's
  §17 "Coding Task Prompt" is intended to be handed over verbatim; the rest
  of the package is the binding context it references.
- **Reviewer**: runs the package's §18 validation checklist and checks the
  §15 acceptance boxes before the work merges.

Every package follows the same 20-section template (problem statement,
functional/non-functional requirements, architecture, component and
interface design, data structures, algorithms, edge cases, error handling,
security, performance, I/O specs, acceptance criteria, test plan, coding
prompt, validation checklist, risks, handoff notes — adjacent sections may
share a heading where jointly specified) and is self-contained:
wire constants, invariants, and vector references are embedded so the
implementer needs no clarifying questions.

## Standing rules (bind every package)

1. Dependencies: only what the package's FR-1/§3 explicitly grants
   (TASK-012 and TASK-013 carry pre-approved grants); otherwise the
   pinned workspace list is closed. Never add a dependency on your own
   authority.
2. Invariant I1: signatures verify over received bytes — never
   re-serialize before verifying; `verify_strict` (or its documented
   strict equivalent, TASK-015 §FR-4) only.
3. Spec-feedback loop: when implementation exposes a spec ambiguity,
   STOP on that item and write it up (quote + proposed wording). See
   "Operating without the architect" for who patches what.
4. Full-workspace test suite re-runs after every fix. Toolchain:
   `cargo +stable-x86_64-pc-windows-gnu` from PowerShell on this
   machine (no MSVC linker; Git-Bash shadows `link`).
5. `memory.md` §4/§5/§7/§8 updated whenever a package completes.
6. Registry assignments (code points, ext keys, caveat ids, params
   keys) are made ONLY where a package states them as FINAL
   (TASK-006 §3, TASK-010 §7, TASK-011 §FR-1, TASK-016 §FR-1).
   Everything else: propose, don't assign. Assigned points are
   immutable (ops §3.3).

## Operating without the architect (handoff protocol)

The architect authored TASK-001..016 and departs 2026-07-06. The
packages are designed to be executable without further architectural
input. When something nonetheless needs a ruling:

- **Editorial / additive spec gaps** (missing field number the package
  already assigns, cross-reference fixes, recording an assignment a
  package granted): the coding agent patches `map-spec/` in the same
  change, per the package instructions. This is the normal loop.
- **Anything security-relevant, wire-breaking, or judgment-shaped**
  (a formal-analysis counterexample, a vector that can't be reproduced,
  a tolerance/semantic choice no package pre-decides): do NOT patch
  normative text. Record it in `map-spec/ERRATA.md` (create on first
  use) as `PENDING-ARCHITECT: <date> <package> <quote> <proposal>`,
  mark it in memory.md, and continue with other work. A future
  architect session resolves the queue — CLAUDE.md tells that session
  to check ERRATA.md first.
- **Conflict rule**: drafts win over task-doc text (precedent:
  TASK-004 errata); vectors win over both for byte-level questions;
  if a draft contradicts a frozen vector, that is PENDING-ARCHITECT.
- **Never**: assign unassigned code points, weaken BCP-14 language,
  touch `envelope::open()` byte behavior, or let self-reported trust
  or single-signed receipts near a projection.

## Packages and execution order

| # | Package | Depends on | Status |
|---|---------|-----------|--------|
| 001 | [TASK-001](TASK-001-workspace-build-repair-and-map-cli.md) — workspace green + map-cli | — | DONE 2026-07-03 |
| 002 | [TASK-002](TASK-002-adapter-stubs.md) — adapter trait/mock + gateway G-rules | 001 | DONE 2026-07-03 |
| 003 | [TASK-003](TASK-003-conformance-crate.md) — TV-001/002 byte-exact + N1–N10 | 001 | DONE 2026-07-03 |
| 004 | [TASK-004](TASK-004-test-vector-expansion.md) — TV-003..010 + NV-001..005 | 003 | DONE 2026-07-03 (errata 07-04) |
| 005 | [TASK-005](TASK-005-session-layer-in-memory.md) — map-session over in-memory duplex | 001, 003 | DONE 2026-07-04 (erratum FR-7) |
| 006 | [TASK-006](TASK-006-descriptor-publish-fetch.md) — HelloAck refactor + map-mesh (descriptor publish/fetch, CAP/STATUS flows) | 005 | READY |
| 007 | [TASK-007](TASK-007-echo-task-end-to-end.md) — task codecs + map-taskrun (echo task E2E, TTFT cancel, credits, transcript) | 005, 006 | blocked by 006 |
| 008 | [TASK-008](TASK-008-receipts-and-evidence.md) — receipt lifecycle + evidence exchange | 007 | blocked by 007 |
| 009 | [TASK-009](TASK-009-payments-credit-ledger.md) — pay codecs + credit-ledger method + payment flow | 007 | blocked by 007 (parallel with 008) |
| 010 | [TASK-010](TASK-010-delegation-chains.md) — DelegationCert, chain verify (key 33), caveats, TV-011/012 + NV-007 | 005 | DONE 2026-07-08 (delegation in map-core not map-identity — dep graph; negative is NV-007 not NV-006; kid==leaf per ERRATA) |
| 011 | [TASK-011](TASK-011-spec-extensions-and-vectors.md) — SPEC: ext code points (bid/kvx/git), reservations CDDL/FSM, vectors | 004 | DONE 2026-07-08 (bound-quote TV-013/NV-007 already existed as TV-010b/NV-006; new = TV-013 PAY_AGREE, TV-014 RESERVE, TV-015 LEASE, NV-008; added Offer.?6 ExtMap) |
| 012 | [TASK-012](TASK-012-libp2p-transport.md) — map-net: libp2p `/map/1.0.0`, kad-backed index, loopback interop | 006 | DONE 2026-07-08 (libp2p 0.54.1 + libp2p-stream 0.2.0-alpha; loopback tests pass; build needs mingw on PATH — memory §5. Deferred: cli node cmds, anti-relay/poisoned-record net tests) |
| 013 | [TASK-013](TASK-013-ollama-provider-real.md) — real Ollama backend (feature-gated ureq) + demo | 007 | DONE 2026-07-08 (OllamaBackend+OllamaExecutor, feature `real-backend` off by default; canned-JSON tests; map-cli serve-ollama demo. Deviations: Unavailable(String), send_string not ureq-json, infallible Executor→empty on error) |
| 014 | [TASK-014](TASK-014-formal-analysis.md) — ProVerif models P1–P3 + negative controls | 010 | DONE 2026-07-10 (formal/ at repo root: hello-cb.pv + delegation.pv, all 3 negative controls fail as designed, no BLOCKING finding. ProVerif only works via WSL2 on this box — see memory.md §7 step 12) |
| 015 | [TASK-015](TASK-015-go-implementation.md) — map-go impl #2 at frozen-vector parity (independence rule) | 011 | DONE 2026-07-11 (`map-go/`: zero-dep Go module — stdlib `crypto/hkdf`, no x/crypto needed; all 24 frozen vectors produced byte-exact + consumed + NVs rejected with required classes; FR-4 strict verify incl. non-canonical y=p and small-order rejection tests; TV-007 produce+consume, closing that ERRATA parity hole Go-side; 5-item ambiguity report in map-go/README.md; ops §8 status note added). **ALL packages complete.** Live pairwise interop matrix remains future — memory.md §7 step 13 |
| 016 | [TASK-016](TASK-016-federation-gateway.md) — running gateway: G-rules enforced, zone/scope gating, E2E demo | 008, 010, 013 | DONE 2026-07-08 (GatewayProvider Executor+admit gate, 15 tests incl. FR-6 E2E; discovery §4 gateway-of key. Reject codes 2004/2005/2003 not FR-2's 3xxx; gateway deps ollama-provider for the backend trait). **Completes the roadmap** — only 014 (formal) + 015 (Go) remain, both need external tooling |
| 017 | [TASK-017](TASK-017-envelope-byte-payrefs-and-error-codes.md) — I1 envelope-byte pay-ref digests (session raw-bytes seam) + codes 2006/6005–6008 | 008, 009 | DONE 2026-07-12 (`Session::send`→sealed bytes, `Incoming.raw`; requester digests `inc.raw`, provider binds via `bind_quote_envelope`; codes rewired 2006/6005-6008; FR-6 envelope-vs-content regression test green; whole workspace green, zero warnings) |
| 018 | [TASK-018](TASK-018-inputspec-parity-and-nv009.md) — InputSpec keys 2/3 (TV-007 Rust parity) + NV-009 max-depth vector, both impls | 010, 011, 015 | DONE 2026-07-12 (map-tasks InputSpec models keys 1/2/3 (≥1 required); Rust conformance TV-007 now produce+consume — closes the ERRATA parity hole; NV-009 added additively to gen_vectors.py (frozen bytes untouched, 803 B), reject guards + depth-boundary positives in BOTH impls. Rust conformance 39 cases; map-go 25 vectors. Rust max-depth logic already matched the ruling — no code change) |
| 019 | [TASK-019](TASK-019-go-session-layer.md) — Go session layer: hello/negotiate, replay/seq, CB-bound I/O, TCP framing (wire twin of map-session) | 015 | DONE 2026-07-13 (`map-go/session`: Hello/HelloAck codecs + Negotiate, ReplayCache/SeqTracker, 4-byte-BE framing + TCPTransport/InMemoryDuplex, full Initiate/Respond/Send/Recv/Close with normative receive order + delegated-handshake root resolution; envelope Body extended with in-session keys 4/5/6/7 + SealBody (frozen vectors untouched, 25 still byte-exact). 9 tests incl. TCP loopback + all negatives (replay/seq/tamper/wrong-sid/oversize/version-disjoint/delegated). Per-frame session-id 1006 check implemented ahead of Rust. Zero deps; vet+gofmt clean) |
| 020 | [TASK-020](TASK-020-live-interop-matrix.md) — LIVE Rust↔Go interop matrix: 8 scenarios × 2 orientations over TCP + MATRIX.md record | 019 | DONE 2026-07-13 (**16/16 PASS**, both orientations). Rust `TcpTransport` (std::net) + session-id 1006 fix (FR-2) in map-session; `map-cli interop` + `map-go/cmd/map-interop` peers; `interop/run-matrix.ps1` driver; `interop/MATRIX.md` record. ops §6.1 note updated to "live matrix delivered". Whole Rust workspace + map-go green, zero warnings. **The ops §6.1 gate is met** — only rev-01 editorial cut remains |
| 021 | [TASK-021](TASK-021-usage-watermark-codec-and-vectors.md) — UsageWatermark codec + exact billing arithmetic + TV-016/NV-010, both impls | 015, 018 | DONE 2026-07-14 (map-pay `UsageWatermark`/`Usage` codec + `billing::{billed, verify_watermark, AgreedTerms, BillingError}` exact checked arithmetic; `PayInvoice` key 4 optional (postpaid bytes unchanged, regression-tested); `codes::INVOICE_WATERMARK_VIOLATION=6009`. TV-016/NV-010 added additively to gen_vectors.py (both 301 B; NV-010 differs by msg-id + 1 mantissa nibble + sig). Rust conformance 39→41; map-go 25→27 vectors + independent `billedModel1` check. Whole workspace + map-go green, zero warnings) |
| 022 | [TASK-022](TASK-022-metered-incremental-runtime.md) — metered-incremental (terms 3) runtime: watermark invoicing, six-check verification, 6001 pause/top-up, checkpoint-anchored CANCEL | 021 | DONE 2026-07-14 (`map-taskrun::metered` — `Metered{Provider,Requester}Payment` on the pump architecture; provider bills kind-3 CHUNK + PAY_INVOICE at watermarks, requester runs the six pay §4.1 checks IN ORDER (meter-consistency vs its own received chunks is the new trust boundary) via `map_pay::billed`, pays idempotently. 12 tests incl. inflated-usage/off-by-one/over-cap/zero-delta/resent-idempotent + 6001 pause→top-up→resume + owed_at_checkpoint. Workspace green, zero warnings. **R1 resume-after-6001 signaling filed as PENDING-ARCHITECT (2026-07-14)** — implemented an INTERIM that only sequences spec'd messages, no new wire message) |
| 023 | [TASK-023](TASK-023-x402-settlement-adapter.md) — `pay:x402` settlement adapter: feature-gated (`real-x402`) EIP-3009 signer + facilitator client, always-on codec/nonce/flow seams, map-cli demo | 009, 021 | DONE 2026-07-14 (`adapters/x402-adapter`: always-on `X402Params`/`X402Proof` CBOR codec, `x402_nonce` (the §5.1 MUST), hand-rolled x402 PaymentPayload JSON, payer/payee flows over `EvmSigner`/`Facilitator` seams with canned impls — 16 always-on tests incl. all §9 edges + nonce-both-ways + 6008/6002. `real-x402` gates k256+sha3+ureq+serde_json (2 gated tests; EIP-3009 typehash anchored to the published `0x7c7c…2267`). `map-cli x402-demo` gated. **Default build adds ZERO deps** (cargo-tree-verified); map-go + vectors untouched. Payment runtimes NOT made method-generic — altitude) |
| 024 | [TASK-024](TASK-024-prepaid-escrow-runtime.md) — prepaid (terms 1) + escrow (terms 2): PayInvoice.7/PayReceipt.5 codecs, code 6010, credit-ledger hold/release/refund, runtime pairs, TV-017/018 + NV-011 both impls | 009, 021, 022 | DONE 2026-07-17 (map-pay: `PayInvoice.phase`/`PayReceipt.kind` fail-closed codecs + byte-regressions, `escrow::verify_escrow_invoice` (§5.2.3 order), `AgreedTerms.quoted_total`, credit-ledger `hold`/`release`/`refund` + `MAPLedgerReleaseAuth`/`RefundAuth` inner sigs + balance-conservation tests; `codes::ESCROW_VIOLATION=6010`; map-taskrun `escrow` module: `Prepaid{Provider,Requester}Payment` (execution gates on settled) + `Escrow{Provider,Requester}Payment` (hold/release rounds, payer-signed release-auth, checkpoint-anchored CANCEL) — 9 FR-7 flow tests. TV-017/018 + NV-011 additive (frozen bytes untouched: md diff 56 ins / 0 del); Rust conformance 44; map-go 30 vectors + `receiptKindRecognized` spec-written check. Whole workspace + map-go green, zero warnings) |
| 025 | [TASK-025](TASK-025-x402-gateway-composition.md) — paid-API gateway: x402 HTTP 402 payer loop + `PaidGatewayProvider` (admit → prepaid gate → bounded upstream spend, G-5/G-6/G-7) | 016, 023, **024** | DONE 2026-07-17 (x402-adapter `http402.rs`: `HttpUpstream` seam + 402-body parser (fail-closed brace-matching), `SpendPolicy` deterministic selection, `upstream_nonce` = SHA-256("MAP-x402-upstream-nonce"‖task‖be32(attempt)) domain-separated from the invoice nonce (tested on identical inputs), `pay_402_loop` (G-7 cap BEFORE signing; exactly one payment per attempt; every post-payment failure carries `PaymentEvidence`), 4-mode `CannedUpstream` with call/payment counters, `RealUpstream` gated; hand-rolled base64. gateway `paid.rs`: `PaidGatewayProvider` embeds the TASK-016 `GatewayProvider` for admit reuse, enforces **admit → settled → pay → execute** structurally (reject paths call-counted to 0), settled gate fed by TASK-024 `execution_permitted`, G-5 evidence = kind-4 chunk only (paid-then-failed degrades to the evidence chunk, never empty); E2E test runs the full prepaid MAP leg gating the rail leg with balance assertions both sides. `map-cli x402-gateway-demo` gated; gateway README documents the two-leg model + rule mapping. Default build zero new deps (cargo-tree-verified); no crates/ changes; workspace default + gated green, zero warnings) |
| 026 | [TASK-026](TASK-026-disputes.md) — disputes (pay §6): PAY_DISPUTE/PAY_DISPUTE_EVIDENCE codecs, §6.2 CB-disclosure evidence authentication, §6.3 three-valued claim verifier (reuses all existing verifiers), TV-019/020 + NV-012 both impls | 008, 021, **024** | DONE 2026-07-17 (map-pay `dispute.rs`: §6.1 codecs fail-closed (claim ≤4, kind ≤9, statement ≤1024, non-empty items) + content-address dedup; §6.2 `authenticate_item` — envelope kinds open with `external_aad = disclosed CB` (CB-less in-session = unavailable), kind-5 checkpoint countersig (bytes PINNED: `enc(["MAPCheckpointAck", task, seq, root])` — ERRATA 2026-07-17), kind-7 dual receipts via map-trust, foreign signers discarded; §6.3 `verify_dispute` PURE three-valued engine — claims 0/2 rebuttal-shaped, claim 1 = TranscriptBuilder recompute over kind-9 chunks (kind 9 ADDED to §6.1 — the 0–8 registry lacked chunks), claim 3 = `billed` vs highest countersigned usage, claim 4 re-runs `verify_watermark`/`verify_escrow_invoice`; §6.4 cap. map-taskrun `DisputeState` thin hooks (open/respond/accumulate from `Incoming.raw`, chunked `more`, 6003, outcome-4 flag); `Session::cb()` getter added. Full 15-cell verdict matrix + hostile-bytes/determinism/statement-never-evidence tests; §6.2 tested with a GENUINE loopback-session CB incl. P2 session-separation (finding: fully deterministic harness sessions derive identical CBs — distinguishability comes from the transcript, as spec'd). TV-019/020 (TV-020 carries frozen TV-010b VERBATIM) + NV-012 additive (59 ins/0 del); Rust conformance 47; map-go 33 + `disputeClaimRecognized`. map-pay gains map-tasks/map-trust deps (acyclic). Workspace + map-go green, zero warnings) |
| 027 | [TASK-027](TASK-027-pay-formal.md) — ProVerif models P4 (escrow authorization symmetry) + P5 (dispute watermark) | 024, 026 | DONE 2026-07-17 (landed in `ac9d4ce` spec-review Items 1–9; verified under ProVerif 2.05 via WSL2; row added retroactively 2026-07-18) |
| 028 | [TASK-028](TASK-028-pay-meter.md) — ProVerif model P6 (continuous metering: no settled over-bill, settled-not-submitted, PAY_AGREE digest binding) | 021, 022, 027 | DONE 2026-07-17 (landed in `ac9d4ce`; verified under ProVerif 2.05; row added retroactively 2026-07-18) |
| 029 | [TASK-029](TASK-029-go-pay-runtime-and-interop.md) — Go pay runtime (terms 0/1/3): pure-stdlib `map-go/pay` codec/billing/credit-ledger + pump FSMs, `pay-postpaid`/`pay-metered` libp2p cells both orientations (matrix 22→26) | 009, 021, 022, 024 | DONE 2026-07-18 (`map-go/pay`: codec byte-verified vs frozen TV-010b/013/016/017/018 + NV-010/011; `Billed`/`VerifyWatermark` six-check; `LedgerOperator`/`EntryProof`/`VerifyEntry`; postpaid/prepaid/metered raw-aware pump FSMs — stdlib-only, boundary-checked. Go `map-interop` + Rust gated `libp2p_task` pay scenarios (Rust reuses `map-taskrun::{payments,metered}`; map-pay optional under libp2p-interop). Matrix **26/26** (16 core + 6 task + 4 pay), both orientations. Rust workspace 33 suites + `--no-default-features` zero warnings; map-go green, gofmt/vet clean, frozen vectors untouched. Escrow/disputes Go port OUT — future) |
| 030 | [TASK-030](TASK-030-paid-demo.md) — paid demo: `rfc-title-o-matic --price` prepaid gate ("no pay, no work") × Go `map-requester --pay`, cross-impl live-verified, DEMOS.md paid/unpaid/over-budget transcripts | **029**, 024 | DONE 2026-07-18 (`rfc-title-o-matic --price <cents> --operator-seed`: reuses TASK-024 `PrepaidProviderPayment`, SEQUENTIAL pay-then-task gate — chosen over deferred-ASSIGN interleave, stronger + composes with Go `RunRequester`; prints SETTLED/RECEIPT. Go `map-requester --pay/--operator-seed/--budget`: hosts the ledger operator, over-budget → clean refusal before agreeing. **Live cross-impl verified**: paid (`OK paid: 500` + title root 7bd6…5137), over-budget (refused, no money), unpaid (gate holds, provider `GATE-HELD 0x0200 not PAY_AGREE`), free (byte-identical). Free path additive (no pay object when `--price`/`--pay` absent). DEMOS.md paid-demo section w/ real transcripts. Stretch pay-prepaid matrix cell SKIPPED (demo's pay-first flow ≠ cells' task-first; not trivial). Rust default+minimal + map-go green, zero warnings) |
| 031 | [TASK-031](TASK-031-id-submission-readiness.md) — I-D submission readiness: author/ipr frontmatter (all 11 drafts), pinned reproducible render, `build/nits.py` validation pass, `map-spec/SUBMISSION.md` checklist | none (parallel-safe with 029/030) | READY (authored 2026-07-18; scope = [PLAN-D-milestones](PLAN-D-milestones.md) §TASK-031) |

Recommended order: **006 → 007 → 008 & 009 (parallel) → 012 & 013
(parallel)**, with **010 and 011 anytime after their deps** (they touch
different files than 006–009 — safe in parallel), then **014, 015**, and
**016 last**. TASK-016 §20 lists the post-roadmap frontier for a future
architect.

## After TASK-016

The memory.md §8 roadmap is complete. Remaining frontier (needs a new
architect pass): NAT/eclipse defenses, metered/escrow/dispute payments,
streaming inference, descriptor optional keys 7–14, Go session layer +
live cross-impl matrix (the real ops §6.1 gate), revocation
distribution, rev-01 document cut per ops §4.
