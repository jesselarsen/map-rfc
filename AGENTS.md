# AGENTS.md — quick ramp for MAP (Manolito Agent Protocol)

Spec-family repo (IETF-style Internet-Drafts). The product is the `draft-map-*.md`
files; `map-rs` and `map-go` are the two reference implementations that must
interoperate. `CLAUDE.md` is the authoritative guide — read it first. This file
is only the compact, hard-won context an agent would otherwise miss.

## Build / verify commands

Rust (`map-rs`, impl #1) — **on Windows prepend mingw and use the gnu triple**,
or `cargo` fails to link libp2p:
```powershell
$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
cargo +stable-x86_64-pc-windows-gnu build --workspace     # MUST be zero warnings
cargo +stable-x86_64-pc-windows-gnu test  --workspace     # unit + integration + conformance
cargo +stable-x86_64-pc-windows-gnu test -p map-cli --bin rfc-title-o-matic   # one bin
```
On Linux/macOS a plain `cargo build/test` works.

Go (`map-go`, impl #2):
```bash
cd map-go
gofmt -l . && go vet ./... && go test ./...
go test ./tasks/...        # one package
```
`gofmt -l` and `go vet` are mandatory acceptance gates — every committed map-go
change must be gofmt-clean.

Interop gate (ops §6.1): `pwsh interop/run-matrix.ps1` runs both peers and exits
non-zero on any failure. Current result: **26/26 — 16/16 core (TCP) + 6/6 task
+ 4/4 pay (libp2p/QUIC)**; `interop/MATRIX.md` is stamped to the commit hash
that produced it (currently `ed26742`, 2026-07-17). `MATRIX.md` is the
authoritative figure — cite it rather than this line. Re-run and re-stamp after
any change that could affect wire behavior.

Test vectors: `map-spec/test-vectors/gen_vectors.py` (deps: pynacl, cbor2,
base58). Vectors are **frozen/normative** — regenerate only additively; never
alter frozen bytes. Both impls must reproduce TV-001..020 + NV-001..012
byte-for-byte.

## Two-implementation independence rule (easy to violate)

The Go impl was written **without consulting Rust source** (two-impl rule). Keep
the protocol-core packages in each language self-contained:
- Go: `cbor`, `envelope`, `identity`, `session`, `hello`, `receipt`,
  `discovery` (codec), `conformance` are **pure stdlib**. `go-libp2p` is
  confined to `net`, `dht`, `cmd/*` (the transport/demo layer). Enforce:
  `go list -deps ./cbor ./envelope ./identity ./session ./hello ./receipt ./discovery ./conformance | grep -E 'libp2p|multiformats|mr-tron'`
  MUST be empty.
- Do **not** copy code or structure between `map-rs` and `map-go`. Resolve
  ambiguity by patching the spec and re-deriving both sides from the vectors.

## Protocol invariants (assumed everywhere in code)

- Signatures verify over **received bytes** of deterministic CBOR (COSE_Sign1) —
  never over a re-serialization. This is the #1 thing to preserve when touching
  envelope/session code.
- **No self-reported trust**: trust scores derive only from dual-signed receipts
  + local projection. Never introduce a self-asserted score.
- Determinism: no wall-clock or OS randomness in reproducible paths (vectors,
  keygen-from-seed, projection demos).
- Channel binding: the current libp2p/QUIC binding uses the core §7.3
  *public-key concatenation* stand-in (AgentID‖AgentID), **not** the real QUIC
  TLS exporter — a known TASK-012/014 gap, not endorsed keying material.

## Layout & ownership

- `map-spec/` — the drafts + `CHANGELOG.md` (editorial/additive/breaking,
  required by ops §4), `ERRATA.md` (historical record of every architect
  ruling), `test-vectors/`.
- `tasks/` — implementation-spec packages (`TASK-00N-*.md`) for downstream coding
  agents; `tasks/README.md` has the dependency graph + post-roadmap frontier.
- `interop/` — live matrix driver + `MATRIX.md` + `DEMOS.md`.
- `memory.md` — cross-session handoff (current state, roadmap, the "five
  decisions to know"). Update it when state changes materially.
- `build/` — **generated** kramdown/xml2rfc renderings of the drafts. Do not edit
  by hand; regenerate from source.
- `formal/` — ProVerif models (TASK-027/028), spec-adjacent, not a crate.

## Conventions that differ from defaults

- BCP 14 MUST/SHOULD/MAY used unweakened; one named source of truth per fact
  (cross-reference, don't restate normative definitions).
- Cargo convention: heavy/network deps are **feature-gated** (`real-backend`,
  `real-x402`) so the default `map-cli` build stays dep-light. Keep new heavy
  deps behind a feature.
- Spec-feedback loop: when an implementation exposes an ambiguity, patch the
  spec in the same change and record it. Don't leave the spec and code发散.
- `.opencode/` is gitignored (tooling artifact) — don't commit it.

## Before you finish

- Run the relevant build/test/vet gate above; Rust MUST be zero-warnings.
- If you touched wire behavior: re-run `interop/run-matrix.ps1` and update
  `MATRIX.md` (result + commit hash) + `memory.md`.
- Cross-cutting spec changes should be checked against every document that
  depends on the affected decision (CLAUDE.md "five decisions to know" +
  ops §2 engineering-policy table).
