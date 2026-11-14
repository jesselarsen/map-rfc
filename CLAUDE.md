# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@memory.md

## What this project is

**MAP, the Mutual Attestation Protocol, version 2.0-draft.** It is an
accountability layer for delegated agent work, running over A2A (tasks), an
ADS-compatible Kademlia DHT (discovery), and MPP / x402 V2 (payment rails).
It provides:
- dual-signed receipts
- observer-local trust
- requester-countersigned checkpoints with cross-provider resumption
- metering bound to evidence
- data-sensitivity zones

The goal is an **arXiv systems paper with a quantitative evaluation**.

The v1 IETF-style family (the "Manolito Agent Protocol", a full stack in 11
drafts) is frozen at tag `map-v1-ietf`; `main` stays at that snapshot.
**All work happens on branch `accountability-layer`.**
- Why the project changed: `docs/ADR-0001-thin-accountability-layer.md`.
- Resume point: `memory.md` §00 (development suspended 2026-11-13).
  Phase plan and decisions: `memory.md` §0.

Retired v1 text is still readable with `git show map-v1-ietf:<path>`, for
example `git show map-v1-ietf:map-spec/draft-map-trust-01.md`.

## Workflow

For planning, implementation, verification and review, read the project workflow
guide: [docs/agent-engineering.md](docs/agent-engineering.md). It maps the current
authorities and lists the pending decisions.

## Layout

- `spec/layer.md`: the normative layer. Rule IDs R-*, C-*, T-*, M-*, Z-*,
  D-* are citable from tests and code comments.
- `spec/binding-a2a.md` (A2A-*), `spec/binding-libp2p-discovery.md` (DHT-*),
  `spec/mapping-payment-rails.md` (RAIL-*, informative).
- `spec/test-vectors/`: `map-test-vectors.md` plus `gen_vectors.py` (pynacl,
  cbor2, base58). Vectors are frozen: regenerate only additively, retire but
  never renumber. **Test vectors outrank the spec text, and the spec text
  outranks everything else.**
- `map-rs/`: the Rust workspace. `map-go/`: the Go second implementation.
  `interop/`: the live matrix. `formal/`: ProVerif (runs only under WSL2).
- `attic/probes/`: the 2026-09-18 reviewers' exploit crates. The workspace
  ports them as `#[ignore]` `attack_*` tests.
- `docs/history/`: v1 ERRATA, CHANGELOG, the design critique, and the review
  process (historical record). `tasks/` holds the v1 implementation packages
  (historical).
- `AGENTS.md`: build commands and hard-won gotchas. Read it before building.

## Working conventions

- **Signatures cover the received bytes** of deterministic CBOR (COSE_Sign1
  or the `enc([tag, role, core-bytes])` form). Never re-serialize before
  verifying.
- **No self-reported trust, ever.** Trust is a local projection over
  receipts. The admissibility rules (layer R-1..R-4) live in the projection's
  apply step, identically for incremental use and for rebuild.
- **Two-implementation rule:** `map-go` is written from `spec/` and the
  vectors, never from `map-rs` source. Resolve an ambiguity by patching the
  spec and re-deriving both implementations.
- **Spec-feedback loop:** when code exposes a spec ambiguity, patch the spec
  in the same change.
- Use the BCP 14 keywords unweakened. Keep one source of truth per fact, and
  cross-reference instead of restating.
- Claims in the paper or docs must be checkable. Mark anything unimplemented
  or unevaluated as experimental or deferred. Don't overclaim: v1 had to
  retract "independent implementations" and vacuous formal results.
