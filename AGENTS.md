# AGENTS.md: quick ramp for MAP (Mutual Attestation Protocol)

MAP v2 is an accountability layer over A2A, an ADS-style DHT and MPP/x402.
The spec lives in `spec/`; `map-rs` and `map-go` are two implementations
that must interoperate. `CLAUDE.md` is the authoritative guide; read it
first. This file is only the compact, hard-won context an agent would
otherwise miss. Work on branch `accountability-layer`. v1 is at tag
`map-v1-ietf`.

## Build / verify commands

Rust (`map-rs`). **On Windows, prepend mingw and use the gnu triple**, or
`cargo` fails to link libp2p:
```powershell
$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
cargo +stable-x86_64-pc-windows-gnu build --workspace     # MUST be zero warnings
cargo +stable-x86_64-pc-windows-gnu test  --workspace     # unit + integration + conformance
cargo +stable-x86_64-pc-windows-gnu test  --workspace -- --ignored   # attack_* regression tests
```
On Linux/macOS a plain `cargo build` / `cargo test` works. Cargo stops at the
first failing test binary, so add `--no-fail-fast` to see them all.

Go (`map-go`):
```bash
cd map-go
gofmt -l . && go vet ./... && go test ./...
```
`gofmt -l` and `go vet` are mandatory acceptance gates.

Interop: `pwsh interop/run-matrix.ps1` runs both peers and exits non-zero on
any failure. `interop/MATRIX.md` is stamped with the commit that produced it
and is the authoritative figure. The v1 matrix (26/26) is being replaced
during the rework; re-run and re-stamp after any wire-affecting change.

Test vectors: `spec/test-vectors/gen_vectors.py` (deps: pynacl, cbor2,
base58). Vectors are **frozen**. Add new ones, and mark retired ones
`RETIRED(v1)`; never alter frozen bytes or renumber.

Reviewer exploit probes: `attic/probes/<name>` are standalone crates, run
with `cargo run --release --manifest-path attic/probes/<name>/Cargo.toml`.
Two crates both define a `grind` bin, so don't share one `CARGO_TARGET_DIR`
between them.

## Two-implementation independence rule (easy to violate)

`map-go` is written **without consulting Rust source**. Resolve ambiguity by
patching the spec and re-deriving both sides from the vectors. The Go
protocol-core packages are **pure stdlib**; libp2p is confined to the
transport/demo packages. Check:
`go list -deps ./cbor ./envelope ./identity ./receipt ./discovery ./conformance | grep -E 'libp2p|multiformats|mr-tron'`
MUST be empty.

## Protocol invariants (assumed everywhere in code)

- Signatures verify over **received bytes** of deterministic CBOR. Never
  re-serialize.
- **No self-reported trust.** Receipt admissibility (layer R-1..R-4)
  lives in the projection's apply step, shared by incremental use and
  rebuild.
- Determinism: no wall-clock or OS randomness in reproducible paths
  (vectors, keygen-from-seed, projections, the simulator).
- Channel binding on libp2p covers discovery and status traffic only. It is
  the AgentID‖AgentID stand-in, not a TLS exporter (DHT-14).

## Gotchas

- PowerShell: pass multi-line commit messages with `git commit -F <file>`.
  Piping a here-string to `git commit -F -` does not work.
- Text replacement in PowerShell: a nested single-pair array `@(@("a","b"))`
  flattens to `@("a","b")`. Iterating it then yields strings, and `$pair[0]`
  is a *character*. Use Python, or the Edit tool, for bulk replacements.
- The repo `.gitignore` ignores `bin/`, which includes `src/bin/`. Force-add
  (`git add -f`) Rust `src/bin/*.rs` files.
- Cargo convention: heavy/network deps are **feature-gated**, so the default
  build stays dep-light.
- `.opencode/` is gitignored tooling output. Don't commit it.

## Before you finish

- Run the relevant build/test/vet gate above. Rust MUST be zero-warnings.
- If you touched wire behaviour: re-run `interop/run-matrix.ps1`, update
  `MATRIX.md`, and update `memory.md` §0.
- If code exposed a spec ambiguity: patch `spec/` in the same change.
