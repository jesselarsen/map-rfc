# TASK-016 — Federation Gateway (Real)

Status: READY (after TASK-010 + TASK-013) · Priority: P4 (last) · Target: `map-rs/adapters/gateway`
Depends on: TASK-007/008 (task+receipt flows), TASK-010 (EffectiveScope), TASK-013 (backend pattern) · Blocks: nothing

---

## 1. Problem Statement

The gateway crate holds provenance primitives (G-1..G-4) but no running
gateway. Ops §5 defines gateways as anti-corruption layers: an ordinary
agent that advertises `map:cap:gateway/<namespace>`, executes tasks by
calling an external upstream (mesh or hosted API), and remains fully
accountable — signing as itself, accruing evidence to its own AgentID,
never laundering zones. This task builds a working gateway provider
that bridges MAP tasks to an upstream `InferenceBackend` (the TASK-013
trait — so the same gateway fronts Ollama today and any HTTP API later)
with all four G-rules enforced in code.

## 2. Functional Requirements

- FR-1 `GatewayProvider`: a TASK-007 `Executor` implementation wrapping
  an `InferenceBackend` + `namespace` + configured `upstream_zone`
  (default 2 per G-3); it advertises TWO capabilities in its
  descriptor: `map:cap:gateway/<namespace>` and the fronted capability
  (e.g. `map:cap:inference/text-generation`) — the latter carried as a
  `GatewayAdvert` (G-1 provenance) once descriptor ext modeling exists;
  until then the gateway-of provenance rides the CapEntry params map
  (`{1: <namespace tstr>}` under a params key assigned here: **key 1 =
  gateway-of** in the `gateway/` ontology branch — record in discovery
  §4 registry as a spec patch).
- FR-2 G-3 zone enforcement at ASSIGN time: reject (REJECT 3xxx) any
  task whose TaskConstraints ZoneLabel is stricter (lower) than
  `effective_zone(upstream_zone, allowlisted)`; the check uses the
  existing `gateway::effective_zone`. Test: Zone-1 task vs
  non-allowlisted upstream → rejected.
- FR-3 G-2 accountability: receipts for gateway-executed tasks name the
  GATEWAY's AgentID as provider (this falls out of TASK-008 — assert it
  in a test, don't reimplement).
- FR-4 G-1 no-fabrication: the gateway NEVER signs a descriptor,
  receipt, or proof naming the upstream as subject; enforce by API
  shape (no such function exists) + a doc-comment contract + a test
  that its descriptor's subject == its own AgentID.
- FR-5 Delegated operation (composes TASK-010): the gateway MAY run
  under a DelegationCert chain from an org RIK; when configured with a
  chain, its sessions carry it and `EffectiveScope.permits(cap, zone)`
  gates every ASSIGN in addition to FR-2. Test: chain scoped to
  `map:cap:gateway` only → fronted inference ASSIGN rejected.
- FR-6 End-to-end demo test: requester discovers the gateway via
  map-mesh, runs a task against the fronted capability with a
  MockBackend upstream, receives chunks + COMPLETE + dual-signed
  receipt; provenance visible in the descriptor; all under fixed
  clocks.

## 3. Non-Functional Requirements

No new deps (backend trait already feature-gates HTTP via TASK-013);
pump-driven; zero warnings.

## 4. System Architecture

`adapters/gateway` gains deps on map-tasks/map-taskrun/map-mesh/
map-identity (+ ollama-provider for the trait — consider moving
`InferenceBackend` into map-taskrun or a tiny `map-backend` crate if
that dep feels wrong; implementer's call, report it). Dependency
direction stays acyclic.

## 5. Component Design

```
adapters/gateway/src/
  provider.rs   GatewayProvider (Executor impl + zone/scope gate)
  descriptor.rs build_gateway_descriptor(agent, namespace, fronted_caps,
                zones) — provenance per FR-1
  (existing)    effective_zone, valid_namespace, GatewayAdvert
```

## 6. Interface Specifications

`GatewayProvider::new(backend, namespace, upstream_zone, allowlisted,
chain: Option<Vec<Vec<u8>>>)`; namespace MUST pass `valid_namespace`
(constructor error otherwise). Executor::execute returns the upstream
result mapped through `result_to_chunks` (unchanged).

## 7. Data Structures

CapEntry params `{1: namespace}` per FR-1 (registered). Config struct
serializable-friendly but no file I/O in this package.

## 8. Algorithms

Gate order at ASSIGN: valid namespace (construction) → EffectiveScope
.permits (FR-5, if chained) → zone check (FR-2) → execute. First
failure wins; each maps to a distinct REJECT code (reuse 3xxx range;
report if a dedicated code is warranted).

## 9. Edge Cases

Upstream Unavailable mid-task → COMPLETE outcome 2 (failed) with
partial transcript (chunks already sent stay in the root — the
transcript commits what was delivered); allowlisted=true with
upstream_zone 3 → still 3 (tighten-only); namespace collision with DNS
anchor (G-4) is registry policy, not runtime — doc note only.

## 10. Error Handling

Distinct errors for zone-reject vs scope-reject vs upstream failure;
none leak upstream URLs or prompt content into wire errors.

## 11. Security Considerations

The tests for G-1 (FR-4), G-3 (FR-2 + tighten-only edge), and FR-5
scope gating are the substance. Zone laundering is THE gateway attack —
the FR-2 test plus the §9 allowlist edge are mandatory. Note: upstream
response content is untrusted input; it flows into chunks (payload) but
must never influence gateway control flow beyond sizes.

## 12. Performance Considerations

None.

## 13/14. Input & Output Specifications

Library + tests; demo test per FR-6 is the executable documentation.

## 15. Acceptance Criteria

- ✓ Workspace green, zero warnings, no new deps.
- ✓ FR-6 end-to-end test passes.
- ✓ G-1/G-2/G-3/FR-5 tests pass (incl. tighten-only edge).
- ✓ discovery §4 spec patch (gateway/ branch params key 1 =
  gateway-of) recorded.
- ✓ No upstream/prompt content in wire-visible errors (grep test).

## 16. Comprehensive Test Plan

Unit: constructor namespace validation; gate order (each rejection
class); zone matrix (upstream 0–3 × allowlisted × task zone). 
Integration: FR-6; FR-5 scoped-chain rejection; upstream-failure
mid-task partial COMPLETE; receipt accrues to gateway (FR-3).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/adapters/gateway`, implement §FR-1..6 per §4–§14. Reuse
> effective_zone/valid_namespace/GatewayAdvert, TASK-007's Executor +
> runtimes, TASK-008's receipt flow, TASK-010's EffectiveScope — fork
> nothing. If moving the InferenceBackend trait to break a dep cycle is
> needed (§4), do the smallest move and report it. Patch
> `map-spec/draft-map-discovery-00.md` §4 with the gateway/ params
> assignment in the same change. No upstream identifiers or prompt
> content in wire errors. `cargo +stable-x86_64-pc-windows-gnu` from
> PowerShell; full suite per fix. Deliver: code, tests, spec patch,
> report.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo +stable-x86_64-pc-windows-gnu test --workspace
cargo test -p gateway -- --list | wc -l    # >= 12
git diff map-spec/draft-map-discovery-00.md
```

## 19. Known Risks

R1: dep-graph friction around the backend trait (§4) — small-move rule.
R2: partial-COMPLETE semantics on upstream failure are the architect's
reading of tasks §4 (outcome enum has no "upstream-failed" — 2 failed
is correct); if implementation reveals tension, PENDING-ARCHITECT per
the handoff protocol.

## 20. Handoff Notes

This completes the memory.md §8 roadmap. Remaining unpackaged frontier
(needs a future architect): NAT/eclipse defenses, metered+escrow+
disputes, streaming inference, descriptor optional keys 7–14, Go
session layer + live interop matrix, revocation distribution,
rev-01 cut of the family per ops §4.
