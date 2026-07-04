# TASK-001 — Workspace Build Repair + `map-cli`

Status: READY · Priority: P0 (blocks everything) · Target: `map-rs/` Rust workspace
Depends on: nothing · Blocks: TASK-002, TASK-003

---

## 1. Problem Statement

The `map-rs/` Cargo workspace contains six carefully written crates
(`map-identity`, `map-core`, `map-discovery`, `map-tasks`, `map-trust`,
`map-pay`) with unit tests, but **`cargo build` has never been run** — the
code was authored while the toolchain was still installing. Additionally,
`map-rs/Cargo.toml` lists four members that do not exist on disk
(`crates/map-cli`, `adapters/ollama-provider`, `adapters/gateway`,
`conformance/map-conformance`), so the workspace cannot even begin to
compile. This task makes the whole workspace build and test green, and
implements the first missing member, `map-cli`.

## 2. Functional Requirements

- FR-1: `cargo build --workspace` succeeds from `map-rs/`.
- FR-2: `cargo test --workspace` passes; **all pre-existing unit tests must
  pass unmodified in intent** (mechanical fixes to make them compile are
  allowed; changing an assertion's expected value is NOT, except as governed
  by §10 Error Handling / the spec-feedback rule in §20).
- FR-3: `map-cli` provides subcommand `keygen`: generates an Ed25519 keypair
  (or derives from `--seed <64-hex-chars>`), prints seed hex, pubkey hex,
  AgentID hex, AgentID multibase (`z…`), and did:key string.
- FR-4: `map-cli` provides subcommand `projection-rebuild-demo`: constructs
  N synthetic dual-signed receipts in memory, ingests them into a
  `ReceiptStore`, computes a `TrustProjection` incrementally, then rebuilds
  it from scratch via `TrustProjection::rebuild`, and asserts/prints that
  incremental == rebuilt (the DDIA "derived state is rebuildable" property,
  ops §8). Exit code 0 on equality, 1 with a diff report otherwise.
- FR-5: Placeholder members for the other three missing crates are created
  so the workspace resolves (see Scope: minimal `lib.rs` stubs are created
  HERE; their real content is TASK-002/TASK-003).

## 3. Non-Functional Requirements

- Rust edition 2021, stable toolchain.
- **No new dependencies** beyond the pinned workspace set: ed25519-dalek 2,
  sha2 0.10, hkdf 0.12, bs58 0.5, getrandom 0.2, hex 0.4. For CLI argument
  parsing, use hand-rolled `std::env::args` matching — do NOT add clap.
- No `unsafe`. No async. No network I/O.
- Compiles with zero warnings under `cargo build --workspace` (fix or
  `#[allow]` with a one-line justification comment only where a fix would
  change behavior).

## 4. System Architecture

Workspace layout (already declared in `map-rs/Cargo.toml`, resolver 2):

```
map-rs/
  crates/map-identity     AgentId, Keypair, verify (exists)
  crates/map-core         cbor, envelope, hello, replay, msg_type, errors (exists)
  crates/map-discovery    cap URIs, sharding, Descriptor, cache (exists)
  crates/map-tasks        TaskId, transcript Merkle, FlowAccount, FSM (exists)
  crates/map-trust        ReceiptCore/Receipt, ReceiptStore, TrustProjection (exists)
  crates/map-pay          Amount, Asset, SettlementLedger (exists)
  crates/map-cli          NEW — this task (binary)
  adapters/ollama-provider  NEW — stub only here; real content TASK-002
  adapters/gateway          NEW — stub only here; real content TASK-002
  conformance/map-conformance  NEW — stub only here; real content TASK-003
```

Dependency direction (must be preserved): identity ← core ← {discovery,
tasks, trust}; pay depends only on core's CBOR. `map-cli` depends on
identity + trust (+ core transitively). Nothing depends on `map-cli`.

## 5. Component Design

`map-cli` (binary crate, `crates/map-cli`):

- `main.rs`: dispatch on first arg → `keygen` | `projection-rebuild-demo` |
  anything else → usage text to stderr, exit 2.
- `keygen`: optional `--seed <hex>` (32 bytes hex = 64 chars). With seed →
  `Keypair::from_seed`; without → `Keypair::generate()`.
- `projection-rebuild-demo`: optional `--receipts <N>` (default 50).
  Deterministic synthesis: derive requester/provider keypairs from fixed
  seeds (`SHA-256("MAP-CLI-DEMO-A")`, `SHA-256("MAP-CLI-DEMO-B")`);
  receipt *i* gets task-id = SHA-256 of the 8-byte big-endian encoding of
  *i*, capability `"map:cap:inference/text-generation"`, started
  `1_780_000_000_000 + i*1000`, ended = started + 42_000, outcome cycling
  {0,0,0,1,2} (mostly ok), rating `(50 + (i*7) % 50)`. Do NOT use
  wall-clock time or OS randomness — output must be reproducible.

## 6. Interface Specifications

Reuse existing public APIs (do not change signatures):

- `map_identity::Keypair::{generate, from_seed(seed: [u8;32]), agent_id, sign}`
- `map_identity::AgentId::{to_multibase, did_key, as_bytes}`
- `map_trust::{ReceiptCore, Receipt, sign_receipt(kp, role, core_bytes) -> [u8;64],
  ROLE_REQUESTER, ROLE_PROVIDER, ReceiptStore::{new, ingest, iter, len},
  TrustProjection::{new, apply, score, rebuild}, ProjParams}`

New (internal to map-cli, suggested):

```
fn cmd_keygen(args: &[String]) -> Result<(), String>
  // Side effects: writes report to stdout. No files written.
fn cmd_projection_demo(args: &[String]) -> Result<(), String>
  // Side effects: stdout report. Returns Err → main exits 1.
fn parse_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str>
  // "--seed abc" and "--seed=abc" both accepted.
```

Preconditions: seed hex must decode to exactly 32 bytes, else print
`error: --seed must be 64 hex chars` to stderr and exit 2.
Postconditions: keygen with the same `--seed` prints identical output on
every run (assert in a test).

## 7. Data Structures

No new wire structures. `keygen` output format (exact, one field per line,
lowercase hex):

```
seed:      <64 hex>
pubkey:    <64 hex>
agent-id:  <68 hex>            # ed01 || pubkey
multibase: z...
did:key:   did:key:z...
```

## 8. Algorithms

- Build repair: run `cargo build --workspace`; for each error, apply the
  **smallest** correction that preserves the documented invariants (§11).
  Expected error classes: missing trait imports (`Signer`/`Verifier` from
  ed25519-dalek 2), borrow-checker fixes, integer-type mismatches, missing
  `Cargo.toml` deps in a member crate that uses a workspace dep.
- Projection demo: ingest all receipts → `apply` each verified receipt with
  `now_ms` = max(ended)+1 day and `trust_in_issuer` = 1.0 → snapshot scores;
  `rebuild` with same params/now → compare per (provider, capability) pair
  with exact f64 equality first; if that fails only by float association
  order, compare with tolerance 1e-9 and note it in the report.

## 9. Edge Cases

- `keygen --seed` with uppercase hex: accept (decode case-insensitively),
  output lowercase.
- `projection-rebuild-demo --receipts 0`: valid; both projections empty;
  exit 0.
- Duplicate receipt ingestion (same content address): `ReceiptStore.ingest`
  must return the idempotent variant; demo must ingest one duplicate on
  purpose and assert store length is unaffected.
- Non-UTF8 / missing args: usage text, exit 2 (never panic).

## 10. Error Handling

- All CLI errors → single-line `error: <msg>` on stderr; exit codes:
  0 success, 1 demo mismatch/verification failure, 2 usage error.
- Build-repair rule: if a unit test FAILS (not merely doesn't compile), do
  not "fix" the test — first re-derive the expected value from the spec
  section named in the test's comment (deterministic CBOR: core §4.2;
  Merkle: tasks §8.1 — leaf `H(0x00 || enc([seq,kind,content]))`, parent
  `H(0x01 || l || r)`, odd node promoted, empty root `H(0x00 || enc([]))`;
  shard: first byte of the **digest**, not the multihash prefix). If code
  and spec disagree, fix the CODE. If the spec is ambiguous, STOP and record
  the ambiguity in Handoff Notes (§20) — do not guess silently.

## 11. Security Considerations

Invariants that MUST survive the fix loop (from memory.md §3 / core spec):

- I1: signatures verify over **received bytes**; never re-serialize before
  verifying (`envelope::open` keeps `Opened.payload` as bytes — preserve).
- `verify_strict` (not `verify`) for Ed25519 verification.
- AgentID = `0xED 0x01 || pubkey`, 34 bytes; constructor validation stays.
- Receipt signing input is exactly `enc(["MAPReceipt", role, core-bytes])`,
  roles 1/2.
- `keygen` prints private seeds by design (test tooling): print the warning
  line `WARNING: test tooling — do not use for production identities.`

## 12. Performance Considerations

Irrelevant at this scale (N≤10⁴ receipts). Do not optimize; do not add
parallelism. ReplayCache's O(n) purge is accepted for the scaffold.

## 13. Input Specifications

CLI argv only (documented above). No files read, no env vars, no network.

## 14. Output Specifications

stdout as specified in §7/§5; stderr for errors/usage. No files written.

## 15. Acceptance Criteria

- ✓ `cargo build --workspace` exits 0, zero warnings.
- ✓ `cargo test --workspace` exits 0; no pre-existing test deleted or
  weakened.
- ✓ `cargo run -p map-cli -- keygen --seed <64x'a'>` deterministic across
  two runs (byte-identical stdout).
- ✓ `cargo run -p map-cli -- keygen` twice → different agent-ids.
- ✓ `cargo run -p map-cli -- projection-rebuild-demo` exits 0 and prints
  `rebuild == incremental` (or tolerance note).
- ✓ Three stub members exist and compile (empty `lib.rs` + `Cargo.toml`
  with workspace inheritance; one doc comment naming the follow-up TASK).
- ✓ No dependency added beyond the pinned set.

## 16. Comprehensive Test Plan

Unit (in map-cli, `#[cfg(test)]`):
- T1 keygen determinism: `from_seed([0xaa;32])` → assert multibase starts
  `z6Mk`, agent-id hex starts `ed01`, did:key = `did:key:` + multibase.
- T2 parse_flag: both `--seed X` and `--seed=X`; absent → None.
- T3 seed validation: 63 chars, 65 chars, non-hex → Err.
- T4 demo receipts deterministic: synthesize N=5 twice → identical encoded
  receipt bytes.
- T5 duplicate-ingest idempotency (as in §9).
- T6 rebuild equality: N=50 → scores equal per key.

Integration: run the two binaries via `cargo run` in CI script (see §18).
Regression: the entire pre-existing suite across 6 crates is the
regression suite. Negative: T3, usage-error exit code 2.
Concurrency/stress/property tests: not applicable in this task.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> You are working in the Rust workspace `map-rs/` of the MAP protocol repo.
> Toolchain: stable, edition 2021. Read sections 1–16 of this document; they
> are binding. Steps:
> 1. Create minimal compiling stubs for `adapters/ollama-provider`,
>    `adapters/gateway`, `conformance/map-conformance` (lib crates,
>    workspace-inherited package fields, empty `lib.rs` with a doc comment
>    `//! Stub — implemented in TASK-002/TASK-003.`).
> 2. Create `crates/map-cli` per §5–§7 (binary `map-cli`), depending only on
>    `map-identity`, `map-core`, `map-trust`, `sha2`, `hex` via workspace
>    deps.
> 3. Run `cargo build --workspace`; fix compile errors with minimal diffs,
>    honoring §10 and §11. Then `cargo test --workspace`; same rule.
> 4. Add the tests in §16. Re-run the FULL suite after every fix.
> 5. Produce a final report: root causes of every fix, test summary, any
>    spec ambiguity encountered (verbatim quote + proposed spec patch), and
>    confirmation of each §15 checkbox.
> Never change a public API signature of the six existing crates; never
> re-serialize bytes before signature verification; never replace
> `verify_strict` with `verify`.

## 18. Validation Checklist (reviewer runs)

```
cd map-rs
cargo build --workspace 2>&1 | grep -c warning   # expect 0
cargo test --workspace                            # expect all pass
cargo run -q -p map-cli -- keygen --seed $(python3 -c "print('aa'*32)") > a.txt
cargo run -q -p map-cli -- keygen --seed $(python3 -c "print('aa'*32)") > b.txt
diff a.txt b.txt                                  # expect empty
cargo run -q -p map-cli -- projection-rebuild-demo; echo exit=$?   # expect exit=0
git diff --stat crates/map-identity crates/map-core ...            # inspect: minimal diffs only
```

## 19. Known Risks

- R1: never-compiled code may hide a design-level error (not just typos);
  if a fix would require changing wire bytes, STOP — that is a spec-feedback
  event (escalate via §20), not a build fix.
- R2: float non-determinism in projection rebuild ordering — mitigated by
  tolerance fallback (§8) and by BTreeMap-ordered iteration if HashMap
  iteration order is the cause.
- R3: ed25519-dalek 2 API drift vs what the code assumes (e.g.
  `SigningKey::from_bytes` taking `&[u8;32]`).

## 20. Handoff Notes

- The final report's "spec ambiguities" section feeds the spec-feedback
  loop: each entry must quote the spec text, describe the ambiguity, and
  propose concrete replacement wording. The architect (not the coding LLM)
  patches `map-spec/` and `memory.md`.
- After this task is green, TASK-002 (adapters) and TASK-003 (conformance)
  can run in parallel.
- Update `memory.md` §5 table (crate statuses) as part of the same commit.
