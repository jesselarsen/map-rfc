# TASK-002 — Adapter Stubs: `ollama-provider` and `gateway`

Status: READY (after TASK-001) · Priority: P2 · Target: `map-rs/adapters/*`
Depends on: TASK-001 (build green) · Blocks: nothing (future session-layer work builds on it)

---

## 1. Problem Statement

`draft-map-ops-01.md` §8 plans two adapters around the core crates: a local
inference provider bridging MAP tasks to an Ollama HTTP endpoint, and a
federation gateway that re-advertises external capabilities under the
provenance rules of ops §5. Neither may pull in network dependencies yet
(the workspace is deliberately I/O-free). This task defines the **traits,
types, and provenance logic** — the seams the real adapters will fill —
replacing the empty stubs created in TASK-001.

## 2. Functional Requirements

- FR-1 (`ollama-provider`): define trait `InferenceBackend` abstracting a
  local model server, plus a pure `MockBackend` implementation for tests.
  No HTTP client. The real backend (localhost:11434) is FUTURE work.
- FR-2 (`ollama-provider`): provide `run_task(backend, assign) ->
  Vec<Chunk>` glue that turns a backend response into transcript-eligible
  chunks (kind 0 output-text, kind 3 usage) with correct `seq` numbering
  starting at 0, so the tasks-crate `TranscriptBuilder` can commit them.
- FR-3 (`gateway`): implement provenance labeling per ops §5:
  - G-1: a gateway signs **as itself**; re-advertised capability
    descriptors MUST carry ext field `gateway-of: <namespace>`; the crate
    exposes `label_descriptor(desc, namespace)` doing exactly this and
    `is_gateway_advert(desc) -> Option<&str>` reading it back.
  - G-3: `effective_zone(upstream_zone: u8, allowlisted: bool) -> u8`
    returning max(upstream_zone, 2) unless allowlisted → max(upstream, 1);
    zones never decrease through a gateway (tighten-only).
  - G-4: `valid_namespace(&str) -> bool`: DNS-anchored
    (`gateway/<dns-name>` where dns-name is lowercase LDH labels joined by
    dots) or mesh-registered (`gateway/z<base58>` AgentID form).
- FR-4 (`gateway`, G-2 note): evidence accrual to the gateway's AgentID is
  a property of the receipt flow (trust crate), not new code — add a doc
  comment stating this; do not duplicate logic.

## 3. Non-Functional Requirements

Same as TASK-001 §3: pinned deps only, no HTTP/async/network crates, no
unsafe, zero warnings, edition 2021.

## 4. System Architecture

`ollama-provider` depends on `map-tasks` (Chunk, ChunkKind) and `map-core`.
`gateway` depends on `map-discovery` (Descriptor) and `map-identity`.
Neither is depended on by any crate. Both are `lib` crates.

## 5. Component Design

```
adapters/ollama-provider/src/lib.rs
  pub struct InferenceRequest { pub model: String, pub prompt: String,
                                pub max_tokens: u32 }
  pub struct InferenceResult  { pub text: String, pub prompt_tokens: u64,
                                pub completion_tokens: u64, pub millis: u64 }
  pub trait InferenceBackend {
      fn infer(&self, req: &InferenceRequest) -> Result<InferenceResult, BackendError>;
  }
  pub enum BackendError { Unavailable, ModelNotFound(String), Failed(String) }
  pub struct MockBackend { .. }   // returns deterministic text: "echo: " + prompt
  pub fn result_to_chunks(r: &InferenceResult) -> Vec<Chunk>
      // chunk 0: kind 0 (output-text), content = r.text bytes
      // chunk 1: kind 3 (usage), content = deterministic CBOR map
      //          {1: prompt_tokens, 2: completion_tokens, 4: millis}
      //          (same key meanings as ReceiptCore.metrics; encode with
      //           map_core::cbor::encode)
adapters/gateway/src/lib.rs
  pub const GATEWAY_OF_EXT: &str = "gateway-of";
  pub fn valid_namespace(ns: &str) -> bool
  pub fn label_descriptor(..) / is_gateway_advert(..)
  pub fn effective_zone(upstream_zone: u8, allowlisted: bool) -> u8
```

Descriptor ext-field mechanics: the scaffold `Descriptor` covers fields
1–6 + 12 only. If it has no ext map, add the MINIMAL extension: an
`ext: Vec<(String, String)>` optional field mapped to descriptor CDDL key
14 semantics is NOT yet modeled — instead store `gateway-of` via a wrapper
type `GatewayAdvert { pub inner: Descriptor, pub namespace: String }` and
leave Descriptor untouched. (Do not modify map-discovery's wire format —
that would be a spec-feedback event.)

## 6. Interface Specifications

All functions above: pure, no I/O, no panics on any input; errors via
`Result`/`Option`. `effective_zone` precondition: `upstream_zone <= 3`
(else clamp to 3 and debug_assert). Complexity: all O(len(input)).

## 7. Data Structures

As in §5. `InferenceResult.millis` is duration, not a timestamp — no
clock reads anywhere in these crates (reproducibility rule).

## 8. Algorithms

`valid_namespace`: must start with `"gateway/"`; remainder either (a)
DNS: labels 1–63 chars of `[a-z0-9-]`, no leading/trailing `-`, at least
one dot, total ≤253; or (b) `z`-prefixed base58btc string that decodes via
bs58 to exactly 34 bytes starting `0xED 0x01`.

## 9. Edge Cases

- Empty prompt → MockBackend returns "echo: " (valid, 0 completion tokens
  is fine).
- `valid_namespace("gateway/")` → false; uppercase DNS → false (require
  pre-lowercased, do not normalize silently); punycode (`xn--`) labels →
  true (they are LDH).
- `effective_zone(0, false)` → 2; `effective_zone(3, true)` → 3.

## 10. Error Handling

`BackendError` variants as in §5; `ModelNotFound` carries the model name.
No logging framework — these are libraries; callers decide.

## 11. Security Considerations

- The wrapper-type approach (§5) prevents accidental fabrication of
  upstream-signed descriptors (G-1): a `GatewayAdvert` is unsigned data
  until the gateway seals it under ITS key — enforce by not exposing any
  API that re-signs with a foreign AgentID.
- Zone tighten-only (G-3) is load-bearing for arch §8.1; test exhaustively
  (all 4×2 input combos).

## 12. Performance Considerations

Not applicable (pure functions, tiny inputs).

## 13. Input Specifications

Library crates only: inputs are the function arguments per §5–§6. No CLI,
no files, no env vars, no network.

## 14. Output Specifications

Return values per §5–§6 only; no side effects of any kind.

## 15. Acceptance Criteria

- ✓ `cargo build --workspace` / `cargo test --workspace` green, zero
  warnings.
- ✓ No new dependencies; no modification to any existing crate's source.
- ✓ All §16 tests present and passing.
- ✓ Doc comments cite ops §5 rule IDs (G-1..G-4) at the item they enforce.

## 16. Comprehensive Test Plan

ollama-provider: mock infer roundtrip; `result_to_chunks` seq numbering
(0,1) and kinds (0,3); usage-chunk CBOR decodes back to the three values
via `map_core::cbor::decode`; chunks accepted by `TranscriptBuilder` and
produce a stable root across two runs (determinism).
gateway: `valid_namespace` positive/negative table (≥10 cases incl. §9);
`effective_zone` all 8 combos; `label_descriptor`→`is_gateway_advert`
roundtrip; `is_gateway_advert` on a plain Descriptor → None.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, replace the stub crates `adapters/ollama-provider` and
> `adapters/gateway` with the libraries specified in §5–§11 of this
> document. Constraints: pinned workspace deps only, no network/async
> crates, no changes to any other crate, zero warnings. Implement every
> test in §16. Run `cargo build --workspace && cargo test --workspace`
> after each component; re-run the full suite after every fix. Deliver a
> report: what you built, test summary, and any point where this spec was
> ambiguous (quote it; propose replacement wording). Do not implement any
> HTTP client — the real Ollama call is explicitly out of scope.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo test -p ollama-provider -p gateway
git diff --name-only | grep -v '^adapters/' # expect: empty (plus tasks/ docs)
grep -rn "reqwest\|hyper\|tokio" adapters/   # expect: no hits
```

## 19. Known Risks

- R1: temptation to extend Descriptor's wire format for ext fields —
  forbidden here; the proper descriptor ext-key modeling is future
  spec-side work (descriptor optional keys 7–14).
- R2: DNS validation subtleties (IDN); scoped down to LDH-only on purpose.

## 20. Handoff Notes

Future work (separate packages): real Ollama HTTP backend behind the
trait; gateway descriptor ext-key once map-discovery models optional keys
7–14; session-layer wiring so `run_task` is driven by a live ASSIGN.
Update `memory.md` §5 crate table on completion.
