# map-go — MAP implementation #2 (Go)

The second MAP implementation. Since the 2026-09 pivot (decision D4) it is
a **full second implementation of the kept v2 layers**: identity, the COSE
envelope and session layer, discovery (v1 + v2 descriptors), receipts and
the observer-local trust projection, transcripts and countersigned
checkpoints, metering and the voucher mapping, and (P5b/P5c) the A2A
binding with a provider and a requester.

## Independence: what it does and does not claim

- The **v1 packages** (`cbor`, `envelope`, `identity`, `hello`, `session`,
  `discovery` v1) were written in TASK-015/019 from the v1 drafts and the
  test vectors only, **without reading `map-rs` source** (the original
  independence rule). The strict Ed25519 profile and the ambiguity report
  below come from that work.
- The **v2 packages** (`core`, `transcript` checkpoints, `receipt` v2,
  `trust`, `meter`, `discovery` v2, and the A2A binding) were written in
  P5 by the same author **with the Rust implementation open as the
  reference**. They are a second implementation, not an independent one.
  Their cross-checks against Rust therefore rest on artifacts neither
  codebase produced: the Python generator's frozen vectors
  (`spec/test-vectors/gen_vectors.py`: TV-021..TV-023, NV-013/014), an
  independent `cbor2` oracle for the MPP-Tempo digest, and the unmodified
  A2A Python SDK. Every such check was validated by planting a bug and
  confirming the test fails.

## Build and test

Requires Go ≥ 1.24. The protocol packages are pure stdlib; external
dependencies (`go-libp2p`, Kademlia DHT, multiformats) are confined to
`net` and `dht`, the libp2p discovery-plane binding (decision D3), exactly
as `map-rs` confines libp2p to `map-net`. Verify the protocol surface
links no libp2p (this MUST print nothing):

```
go list -deps ./cbor ./core ./envelope ./identity ./session ./hello ./receipt \
  ./trust ./transcript ./meter ./discovery ./conformance | grep -E 'libp2p|multiformats|mr-tron'
```

```
cd map-go
go vet ./... && gofmt -l . && go test ./...
go test ./cbor -fuzz FuzzDecode -fuzztime 60s   # optional: decoder fuzz
```

## Conformance

`conformance` reads `../spec/test-vectors/map-test-vectors.md` at test
time and:

- **produces** TV-001..TV-005, NV-001..NV-005 and TV-021 (the dual-signed
  CheckpointArtifact: root, core and artifact) byte-for-byte from the
  seeds;
- **consumes** TV-001/003/004/005 (open + strict verify), TV-002 (dual
  signed receipt) and TV-021 (a valid resumption point that resumes at
  seq 3);
- **rejects** NV-001→2000, NV-002→1000, NV-003→1000, NV-005→2000,
  NV-013 (corrupted countersignature → not a resumption point), NV-014
  (root mismatch on resume); NV-004 opens and loses on seq LWW;
- checks **TV-023**: each of 13 receipts reaches its pinned admissibility
  verdict (R-2 self-dealing, R-3 future-dated, R-4 unilateral receipts,
  cancelled), including the rule order;
- checks the **derived constants** (TH, CB, shard, K/K′, transcript root)
  and the AgentID multibase/did:key strings.

Vectors marked RETIRED(v1) (the v1 task/pay/dispute protocol and
delegation) are not consumed; their bytes stay in the file and in tag
`map-v1-ietf`.

## Packages

| package | contents |
|---|---|
| `cbor` | hand-rolled RFC 8949 §4.2.1 deterministic encoder + strict decoder; fuzzed |
| `identity` | AgentID, multibase base58btc (both directions) / did:key, **VerifyStrict** |
| `envelope` | COSE_Sign1 Seal/Open over exact received bytes (I1), core §9 error classes |
| `hello` | negotiation transcript hash, channel binding |
| `session` | HELLO negotiation, channel binding, anti-replay (amortized O(1) expiry, finding A8), dense seq, TCP framing |
| `core` | TaskID, cumulative Usage (layer §6.1) |
| `transcript` | transcript Merkle tree (layer §5), CheckpointArtifact, prefix verification and resumption (layer §7) |
| `receipt` | v2 ExecutionReceipt: optional per-party signatures, keys 12 zone / 13 resumed-from |
| `trust` | observer-local projection with the R-2..R-4 gates inside Apply, IssuerWeights (unknown = 0), issuer cap, rebuild, countersignature Tolerance, evidence subject filter |
| `meter` | non-negative Amount, Asset, exact Billed, voucher digests (RAIL-1/5), atomic units, M-6 ceiling |
| `discovery` | cap index keys, sharding, ShardEpoch (D-8), v1 Descriptor, v2 DescriptorV2 |
| `net`, `dht` | libp2p/QUIC MapNode and Kademlia router (discovery plane); shard-filtered lookups (finding A3) |
| `conformance` | vector reader and the conformance tests |
| `cmd/map-interop` | Go peer for the core interop matrix (`interop/run-matrix.ps1`) |

## Known divergence from map-rs

- An envelope protected header with an unknown key (for example the
  retired delegation key 33) is **rejected** by Go and **ignored** by
  Rust. No v2 peer sends one; Go's behavior is the conservative one.

## Strict Ed25519 verification (§FR-4)

Go's `ed25519.Verify` differs from dalek's `verify_strict` on edge
inputs. `identity.VerifyStrict` adds the strict checks:

1. **Canonical S** — already enforced by the Go stdlib (S < L).
2. **Canonical A and R encodings** — rejected when the encoded y (sign
   bit cleared) is ≥ p = 2^255−19.
3. **No small-order A or R** — the 8 canonical torsion-point encodings
   are rejected.
4. The stdlib's **cofactorless** verification equation matches
   verify_strict's.

## Ambiguity report (TASK-015, v1 drafts)

Recorded while implementing v1 from the drafts alone. Items 2 and 3
concern delegation, which v2 retired.

1. **Sharded-key text format**: the shard byte inside
   `map:idx:1:<cap>:<shard>` is *decimal, no padding* (pinned by the
   derived-constants vector).
2. *(retired)* Delegated-envelope kid: TV-012 used kid = leaf.
3. *(retired)* Max-depth counting of delegation chains.
4. **HELLO vs HELLO_ACK field shapes**: fields 1/2 are lists in Hello and
   singletons in HelloAck; TV-001/TV-003 are the guard.
5. **Policy-reject NVs** (NV-004): opens, rejected by policy, not a wire
   error.
