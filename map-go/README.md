# map-go — MAP implementation #2 (Go), Core Conformance scope

The second, independent MAP implementation (TASK-015; ops §6.1
two-implementation rule). Scope is **byte-exact conformance to the full
frozen test-vector set** — produce, consume, reject — not feature parity
with `map-rs`. Written from `map-spec/draft-map-*.md` and
`map-spec/test-vectors/` only (independence rule, TASK-015 §1); no
`map-rs` source was read, including the registry-mirror files (the code
points and error codes used here come from core §5/§9 directly).

## Build and test

Requires Go ≥ 1.24 (this repo's toolchain: go1.26.5). **Zero external
dependencies** — pure stdlib (`crypto/ed25519`, `crypto/hkdf`,
`crypto/sha256`); base58 is implemented in-package. No network, no
clocks, no randomness in tests.

```
cd map-go
go vet ./... && gofmt -l . && go test ./...
go test ./cbor -fuzz FuzzDecode -fuzztime 60s   # optional: decoder fuzz
```

The conformance suite reads the single source of truth,
`../map-spec/test-vectors/map-test-vectors.md`, at test time and:

- **produces** every frozen positive vector (TV-001..015, TV-010b) AND
  every negative vector byte-for-byte from the deterministic seeds
  (failures report vector id + first-diff offset);
- **consumes** every positive vector from its frozen bytes (envelope
  open + strict signature verify; TV-002 dual-signed receipt; TV-012
  delegated open resolving the accountable root);
- **rejects** every NV with the required class: NV-001→2000, NV-002→1000,
  NV-003→1000 (canonicality isolated from signatures), NV-005→2000,
  NV-007→2003; NV-004/006/008 open (policy rejections, per the file);
- checks the **derived constants**: TH, CB, shard(B), K/K′ DHT keys,
  transcript root, and the exact AgentID multibase/did:key strings.

Note: TV-007 (ASSIGN with content-addressed `InputSpec` key 2) is
produce+consume here — this closes, on the Go side, the parity hole
recorded in `map-spec/ERRATA.md` (2026-07-05) where the Rust codec is
consume-only for TV-007.

## Packages

| package | contents |
|---|---|
| `cbor` | hand-rolled RFC 8949 §4.2.1 deterministic encoder + strict decoder (rejects non-minimal heads, indefinite lengths, unsorted/duplicate keys, floats/simples, trailing bytes); fuzzed for accept⇒canonical stability |
| `identity` | AgentID (34-byte value type), multibase base58btc / did:key formatting, **VerifyStrict** (§FR-4) |
| `envelope` | COSE_Sign1 Seal/Open over exact received bytes (I1), delegation-chain validation (identity §4/§6), core §9 error classes |
| `hello` | negotiation transcript hash (core §6.2), channel binding (core §7.3) |
| `receipt` | dual-signed ExecutionReceipt, role-bound MAPReceipt signatures (trust §3) |
| `discovery` | cap index keys, sharding (discovery §6), descriptor LWW freshness (§5) |
| `conformance` | vector-file reader, transcript Merkle (tasks §8.1), vector regeneration, the conformance tests |

## Strict Ed25519 verification (§FR-4)

Go's `ed25519.Verify` differs from dalek's `verify_strict` on edge
inputs. `identity.VerifyStrict` documents and adds the strict checks:

1. **Canonical S** — already enforced by the Go stdlib (S < L,
   RFC 8032 §5.1.7).
2. **Canonical A and R encodings** — rejected when the encoded y
   (sign bit cleared) is ≥ p = 2^255−19 (RFC 8032 §5.1.3 decoding
   domain); tested with the non-canonical y = p encoding.
3. **No small-order A or R** — the 8 canonical torsion-point encodings
   (libsodium blacklist / dalek `is_small_order`) are rejected; with
   check 2 in force the non-canonical torsion encodings are unreachable.
4. The stdlib's **cofactorless** verification equation matches
   verify_strict's.

## Ambiguity report (TASK-015 §17 — what a fresh implementer hits)

Recorded while implementing from the drafts without Rust source to lean
on. None is a spec-text change; none is security-relevant beyond what
ERRATA already tracks.

1. **Sharded-key text format** (discovery §6): the shard byte's textual
   form inside `map:idx:1:<cap>:<shard>` is not pinned by the draft
   prose; the derived-constants vector (K′ for shard 141) resolves it to
   *decimal, no padding*. Worth one normative sentence.
2. **Delegated-envelope kid** (identity §4): the draft's "chain root
   MUST equal the envelope kid" contradicts TV-012, where kid = leaf DK.
   Already flagged PENDING-ARCHITECT in the spec's editorial note; the
   Go implementation follows the note (verify against leaf, resolve root
   as accountable), frozen by TV-012 bytes.
3. **Max-depth counting** (identity §4 key 7): "max further delegation
   depth" does not say whether the count is certs below the *declaring
   cert* or chain length overall. Implemented as: cert at chain index i
   (leaf = 0) tolerates i further certs below it (`i <= max-depth`).
   TV-012 (depth 1, one link) passes either reading; a 2-link vector
   would disambiguate.
4. **HELLO vs HELLO_ACK field shapes** (core §6.1): field 1/2 are
   *lists* in Hello and *singletons* in HelloAck under the same field
   numbers. The vectors make it unambiguous, but a fresh reader can miss
   it; TV-001/TV-003 are the guard.
5. **Policy-reject NVs** (NV-004/006/008): the vector file is explicit
   that these OPEN and are rejected by layer-specific policy, not wire
   errors — but the required *error class* for NV-006/008 is "reject",
   with no code. Implemented as boolean policy checks
   (`discovery.DescriptorFresher`, binding/window comparisons in the
   conformance tests) rather than inventing codes.

## Deviations from the task package (documented)

- §FR-2 allows `golang.org/x/crypto`; not needed — Go ≥ 1.24 provides
  `crypto/hkdf` in the stdlib, so the module has zero external deps
  (stricter than the requirement).
- §FR-1 names a `hello` package for TH/CB; the transcript Merkle
  (tasks §8.1) lives in `conformance` (with its consumers) rather than a
  package outside the §FR-1 list.

## Future (out of scope here, TASK-015 §FR-5/§20)

Go session layer + cross-implementation live handshake against
`map-net` (the real ops §6.1 interop matrix), then Go task/receipt
flows.
