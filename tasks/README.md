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
| 023 | [TASK-023](TASK-023-x402-settlement-adapter.md) — `pay:x402` settlement adapter: feature-gated (`real-x402`) EIP-3009 signer + facilitator client, always-on codec/nonce/flow seams, map-cli demo | 009, 021 | READY (P4, periphery — implements the 2026-07-14 `pay:x402` registration in pay §5.1. First non-Ed25519 crypto dep in the tree: k256 stays HARD-gated; default build adds zero deps. No vectors, no Go work — settlement proofs are chain-dependent, like every rail method) |

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
