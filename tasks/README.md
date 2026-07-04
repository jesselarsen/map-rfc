# tasks/ — Implementation-Specification Packages

This directory holds the engineering task packages that drive the MAP
reference-implementation program (ops §8) under a split-role model:

- **Architect** (Claude, per CLAUDE.md "Role model"): reviews the specs,
  performs the design analysis, and authors these packages. Does not write
  the production implementation.
- **Coding LLM / engineer**: executes one package at a time. Each package's
  §17 "Coding Task Prompt" is intended to be handed over verbatim; the rest
  of the package is the binding context it references.
- **Reviewer**: runs the package's §18 validation checklist and checks the
  §15 acceptance boxes before the work merges.

Every package follows the same 20-section template (problem statement,
functional/non-functional requirements, architecture, component and
interface design, data structures, algorithms, edge cases, error handling,
security, performance, I/O specs, acceptance criteria, test plan, coding
prompt, validation checklist, risks, handoff notes) and is self-contained:
wire constants, invariants, and vector references are embedded so the
implementer needs no clarifying questions.

## Standing rules (bind every package)

1. Pinned dependencies only (`map-rs/Cargo.toml` workspace list); adding a
   dependency is an architect decision, never an implementer one.
2. Invariant I1: signatures verify over received bytes — never
   re-serialize before verifying; `verify_strict` only.
3. Spec-feedback loop: when implementation exposes a spec ambiguity, STOP
   on that item and write it up (quote + proposed wording); the architect
   patches `map-spec/` in the same change. Never diverge silently.
4. Full-workspace test suite re-runs after every fix.
5. `memory.md` §5–§7 is updated whenever a package completes.

## Packages and execution order

| # | Package | Depends on | Status |
|---|---------|-----------|--------|
| 001 | [TASK-001-workspace-build-repair-and-map-cli.md](TASK-001-workspace-build-repair-and-map-cli.md) — make `map-rs/` build & test green; implement `map-cli` (keygen, projection-rebuild-demo); stub the 3 remaining members | — | READY |
| 002 | [TASK-002-adapter-stubs.md](TASK-002-adapter-stubs.md) — `ollama-provider` trait + mock, `gateway` provenance rules G-1..G-4 | 001 | blocked |
| 003 | [TASK-003-conformance-crate.md](TASK-003-conformance-crate.md) — TV-001/TV-002 byte-exact produce+consume, negative suite N1–N10 (ops §6.2) | 001 | blocked |
| 004 | [TASK-004-test-vector-expansion.md](TASK-004-test-vector-expansion.md) — TV-003..TV-010 + NV-001..005 via `gen_vectors.py` (spec-side, Python) | 003 (recommended) | blocked |

002 and 003 may run in parallel once 001 is green.

## Future packages (roadmap, memory.md §8 — not yet authored)

Session layer (in-memory duplex → libp2p `/map/1.0.0`) · descriptor
publish/fetch over Kademlia · echo task end-to-end · receipts flow ·
payments (`pay:credit-ledger`) · delegation-chain verification (ProtHdr
key 33) · extension code points (BID/KVX/GIT) + reservations CDDL/FSM ·
Tamarin/ProVerif formal models · Go implementation (impl #2, ops §6.1) ·
federation gateway (real).
