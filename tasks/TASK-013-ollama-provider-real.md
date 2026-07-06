# TASK-013 — Real Ollama Provider Adapter

Status: READY (after TASK-007) · Priority: P3 · Target: `map-rs/adapters/ollama-provider`
Depends on: TASK-007 (Executor trait usage) · Blocks: nothing (demo value)

---

## 1. Problem Statement

The adapter has a trait and a mock. To demonstrate MAP doing real work,
implement `InferenceBackend` against a local Ollama server
(http://127.0.0.1:11434), wire it as a TASK-007 `Executor`, and expose a
`map-cli` demo command. This makes the first genuine
`map:cap:inference/text-generation` provider.

## 2. Functional Requirements

- FR-1 **Dependency approval (architect-granted, exact):**
  `ureq = "2"` (blocking HTTP, small tree) and `serde_json = "1"` —
  confined to the `ollama-provider` crate, behind a cargo feature
  `real-backend` (default off, so the workspace default build stays
  network-dep-free). No other new deps.
- FR-2 `OllamaBackend { base_url, model, timeout }` implementing
  `InferenceBackend::infer`: POST `/api/generate` with
  `{"model": ..., "prompt": ..., "stream": false,
  "options": {"num_predict": max_tokens}}`; map response fields
  (`response` → text, `prompt_eval_count` → prompt_tokens,
  `eval_count` → completion_tokens, `total_duration` ns → millis).
- FR-3 Error mapping: connection refused → `Unavailable`; HTTP 404 with
  model error → `ModelNotFound(model)`; other non-200/parse failures →
  `Failed(msg)` (message MUST NOT include the prompt — Zone hygiene).
- FR-4 Executor wiring: `OllamaExecutor` adapting FR-2 through the
  TASK-007 `Executor` trait (prompt = InputSpec inline bytes as UTF-8,
  lossy-checked: invalid UTF-8 → task Failed, not a panic).
- FR-5 `map-cli` (feature `real-backend`): `serve-ollama --model <m>`
  — starts an in-process provider loop over... transport choice:
  if TASK-012 is merged, listen via MapNode; else run the two-terminal
  demo over a documented localhost TCP fallback is NOT in scope — in
  that case the command runs a single-process demo (requester +
  provider over InMemoryDuplex) sending `--prompt <text>` through a real
  Ollama call and printing the streamed chunks + verified transcript
  root + dual-signed receipt summary. Implement whichever matches the
  merged state of TASK-012 at execution time; state which in the report.
- FR-6 All tests that need a live Ollama are `#[ignore]` + feature-
  gated; a `MockServer`-free approach is fine (unit-test the request
  building and response/error mapping against canned JSON strings).

## 3. Non-Functional Requirements

Feature-gated deps (FR-1); zero warnings with and without the feature;
timeout default 120 s; no retries (a task is not idempotent at the
inference level — callers decide).

## 4–8. Architecture / Design / Interfaces / Data / Algorithms

One crate touched (+ map-cli). Request/response structs with serde;
canned-JSON unit tests. Usage chunk (kind 3) numbers from FR-2 mapping
feed the existing `result_to_chunks`.

## 9. Edge Cases

Empty prompt (valid); model name with slashes/tags (`llama3:8b`);
Ollama returning `done: false` on stream=false (treat as Failed);
num_predict 0 (omit options); non-UTF8 input (FR-4).

## 10. Error Handling

Per FR-3; timeout → Unavailable with elapsed ms in the message.

## 11. Security Considerations

Prompts/outputs are Zone-sensitive payloads: never log them at default
level; error strings exclude prompt content (FR-3). The backend talks
only to 127.0.0.1 by default; a non-loopback base_url logs a loud
warning (data leaves the machine — Zone ≥2 territory).

## 12. Performance Considerations

`stream: false` is deliberate (simplest correct); true token streaming
through CHUNKs is future work — note it.

## 13/14. Input & Output Specifications

Feature-gated CLI per FR-5; canned JSON for tests; live tests ignored.

## 15. Acceptance Criteria

- ✓ Default workspace build unchanged (no ureq/serde_json in default
  tree — verify with `cargo tree`).
- ✓ With `--features real-backend`: builds zero-warning; canned-JSON
  unit tests green; `#[ignore]` live test passes on a machine with
  Ollama running (document the model used).
- ✓ FR-5 demo prints text, root, receipt summary end-to-end.
- ✓ No prompt content in any error/log string (grep test).

## 16. Comprehensive Test Plan

Unit: request JSON building (all options); response mapping; each FR-3
error class from canned bodies; non-UTF8 input. Live (ignored): tiny
prompt against local Ollama; demo command smoke run.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/adapters/ollama-provider` (+ map-cli), implement §FR-1..6.
> Dependencies: exactly ureq 2 + serde_json 1, behind feature
> `real-backend`, default OFF — the default workspace tree must not
> change (prove with cargo tree in your report). Error strings must
> never contain prompt text. Check whether TASK-012 (map-net) is merged
> and implement the §FR-5 variant that matches; say which. `cargo
> +stable-x86_64-pc-windows-gnu` from PowerShell; full suite per fix
> (with and without the feature). Deliver: code, tests, report.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo +stable-x86_64-pc-windows-gnu tree | grep -c ureq   # 0
cargo +stable-x86_64-pc-windows-gnu test --workspace                   # green
cargo +stable-x86_64-pc-windows-gnu test -p ollama-provider --features real-backend
grep -rn "prompt" adapters/ollama-provider/src | grep -i "err\|log"    # inspect
```

## 19. Known Risks

R1: Ollama API drift between versions — pin the mapping to the
documented /api/generate v1 shape; unknown extra fields ignored.
R2: ureq TLS features pull more deps — use default-tls=off
(`ureq = { version = "2", default-features = false }` + gzip off) since
loopback HTTP needs none; verify tree stays small.

## 20. Handoff Notes

Update memory.md §5/§8 (Ollama done). Future: streaming inference
(Ollama stream=true → CHUNK-per-token under credit), model
advertisement via Descriptor ModelEntry (needs descriptor optional
keys 7+ modeling).
