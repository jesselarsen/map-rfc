# map-go — MAP implementation #2 (Go), Core Conformance scope

The second, independent MAP implementation (TASK-015; ops §6.1
two-implementation rule). Scope is **byte-exact conformance to the full
frozen test-vector set** — produce, consume, reject — not feature parity
with `map-rs`. Written from `map-spec/draft-map-*.md` and
`map-spec/test-vectors/` only (independence rule, TASK-015 §1); no
`map-rs` source was read, including the registry-mirror files (the code
points and error codes used here come from core §5/§9 directly).

## Build and test

Requires Go ≥ 1.24 (this repo's toolchain: go1.26.5). **The protocol core
has zero external dependencies** — the packages that define wire behavior
(`cbor`, `envelope`, `identity`, `session`, `hello`, `receipt`,
`discovery`, `conformance`) are pure stdlib (`crypto/ed25519`,
`crypto/hkdf`, `crypto/sha256`); base58 is implemented in-package. No
network, no clocks, no randomness in the conformance tests. This is the
independence marker: the byte-exact conformance surface reads the spec,
not a library. Verify it — this MUST print nothing:

```
go list -deps ./cbor ./envelope ./identity ./session ./hello ./receipt \
  ./discovery ./conformance | grep -E 'libp2p|multiformats|mr-tron'
```

External dependencies (`go-libp2p`, Kademlia DHT, multiformats) are
confined to the **transport binding and demo layer** added in the
TASK-020 interop extension: `net` (libp2p/QUIC `MapNode`), `dht` (the
Kademlia descriptor router — kept a SEPARATE package from `discovery`
precisely so the codec/conformance surface stays libp2p-free), and the
`cmd/*` demo agents. This mirrors `map-rs`, which confines libp2p to its
own `map-net` crate. The core §10 transport is a *binding*, not part of
the frozen wire format, so it does not touch the independence claim
above.

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
| `session` | the session layer (TASK-019): HELLO negotiation, channel binding, replay/seq, TCP framing (see below) |
| `conformance` | vector-file reader, transcript Merkle (tasks §8.1), vector regeneration, the conformance tests |

## Session layer (TASK-019)

`session` is the wire twin of the Rust `map-session` crate — everything
needed for a Go peer to run a live MAP session:

- **HELLO / HELLO_ACK** codecs and `Negotiate` (core §6.1): highest mutual
  version, first mutual suite in the *initiator's* preference order,
  extension intersection at min version, `max-message-size` = min of both.
  HELLO fields 1/2 are lists; HELLO_ACK's are singletons and its key 4 is
  the *responder's own* max (the initiator computes the min).
- **Transcript hash + channel binding** reused from `hello`; CB is the
  COSE `external_aad` on every in-session envelope (§7.3), so a frame
  recorded on one session cannot verify on another.
- **Anti-replay** (per-sender msg-id uniqueness, ±120 s skew, 10-min
  cache) and **dense sequence** tracking (core §7).
- **Framing + transport**: 4-byte big-endian length prefix, size-first
  reads (core §6.3, §10). `TCPTransport` over `net.Conn` and an in-process
  `InMemoryDuplex` for tests. The channel-binding exporter is
  `initiator-AgentID || responder-AgentID` — the same construction the
  core §10 libp2p binding uses. **Test-harness transport only**: raw TCP
  does not authenticate the static keys; deployment bindings are core
  §10's TLS/QUIC/libp2p.
- **State machine**: `Initiate`/`Respond` complete the handshake (with
  delegation-chain resolution — the accountable peer is the chain root);
  `Send` returns the sealed frame (the TASK-017 I1 seam); `Recv` runs the
  normative receive order (size → verify → sender → session-id → replay →
  seq), auto-answers PING with PONG, and surfaces CLOSE/ERROR. `Close`
  sends a CLOSE with a reason.

Per-frame session-id enforcement (core §6.1: a mismatch is a 1006 state
violation) is implemented here; the Rust reference does not yet enforce it
and is corrected in TASK-020. Zero external dependencies (net, crypto/rand,
crypto/hkdf, time — all stdlib). The cross-language interop matrix is
TASK-020; this package ends at a Go↔Go TCP loopback.

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
