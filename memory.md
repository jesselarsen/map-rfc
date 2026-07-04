# memory.md — MAP (Manolito Agent Protocol) Project Memory

Handoff file for continuing this project in a new context (e.g., Claude
Code). Read this first; it states what exists, what is binding, what is
unverified, and what to do next. Tip: in Claude Code, either rename this to
`CLAUDE.md` or add the line `@memory.md` to your `CLAUDE.md` so it loads
automatically.

## 1. What this project is

A standards-track-style specification family ("MAP") for a decentralized,
permissionless AI-agent network — evolving the Manolito/MP2P capability-
routing idea onto a modern stack — plus a Rust reference-implementation
scaffold. Produced from a critical evaluation of earlier design documents
(HEP v0.1 preprint and three design sketches); every rejected/adopted idea
is traceable in `map-spec/01-design-critique.md` (findings C1–C15 with a
disposition table).

Naming: the protocol is **MAP**; "Hermes" is reserved for an orchestrator
*implementation*. Protocol ≠ product.

## 2. Expected repository layout

```
./memory.md                  this file
./map-spec/                  the specification family (11 markdown docs)
./map-spec/test-vectors/     TV-001/TV-002 + gen_vectors.py (regenerator)
./map-rs/                    Rust workspace (unzip map-rs.zip here)
```

The two engineering-policy documents from the original project
(`designing-data-intensive-applications.md`,
`implementing-domain-driven-design.md`) are **binding** if present; their
protocol-specific distillation lives in `map-spec/draft-map-ops-00.md` §2
(bounded-context/ownership table + 7 rules). Changes violating that table
are rejected in review — that rule is itself in ops §2.

## 3. Non-negotiable protocol decisions (do not drift)

1. **Signatures cover received bytes** of deterministic CBOR (RFC 8949
   §4.2.1) via COSE_Sign1 (tag 18). Never re-serialize before verifying
   (core invariant I1). Protobuf is at most an optional bulk carrier.
2. **No self-reported trust** anywhere (critique C1). Trust = local,
   rebuildable projection over dual-signed ExecutionReceipts.
3. **Two-plane discovery**: stable signed descriptors in a sharded DHT;
   volatile load/TTFT pulled on demand via STATUS_QUERY. Latency is never
   advertised (C3/C6).
4. **Hard constraints are protocol; ranking is local policy** (C5).
5. **Zones 0–3** gate payload flow, normatively (arch §8.1; tighten-only on
   delegation).
6. **Payment negotiation is protocol; settlement is a method registry**
   (no native token/central credits — C9).
7. Envelope crypto facts: protected header `{1: -8 (EdDSA), 4: kid=AgentID}`;
   `Sig_structure = ["Signature1", protected, external_aad, payload]`;
   `external_aad` = channel binding in-session, empty out-of-session;
   `TH = SHA-256("MAPv1-transcript" || HELLO-env || HELLO_ACK-env)`;
   `CB = HKDF(salt=TH, ikm=transport-exporter, info="map cb", L=32)`.
8. AgentID = `0xED 0x01 || ed25519-pubkey` (34 bytes), multibase `z…`
   base58btc — byte-compatible with did:key and libp2p Ed25519 PeerIDs.
9. Receipt signatures: `Sign(enc(["MAPReceipt", role, core-bytes]))`,
   role 1 = requester, role 2 = provider (matches TV-002).
10. Transcript commitment (tasks §8.1, as patched):
    `leaf = H(0x00 || enc([seq, kind, content]))`,
    `parent = H(0x01 || l || r)`, odd node promoted,
    `empty = H(0x00 || enc([]))`; transcript kinds are 0/1/4.
11. Message-type code points: single source of truth is core §5, mirrored
    in `map-rs/crates/map-core/src/msg_type.rs`. Code points are immutable
    once assigned (ops §3.3); 0x0206 is deliberately unassigned.

## 4. Current state — specification family (COMPLETE as rev-00)

All eleven documents written, cross-referenced, BCP-14 language:
critique, arch (goals/threats TH1–TH15/zones/scalability), identity,
core, discovery, tasks, trust, pay, ops, README, test vectors.

Patches already applied during implementation feedback (keep this loop —
it found real bugs):
- discovery §6: shard = first byte of the **digest** (the multihash's
  first byte is the constant codec prefix 0x12 — original text collapsed
  all agents into one shard).
- tasks §8.1: Merkle construction fully specified (see decision 10; the
  original `H(seq || kind || content)` concatenation was ambiguous).
- tasks §1 and discovery §5: explicit data-ownership paragraphs (DDIA
  policy).

Honest rev-00 gaps (spec side): extensions TBD-BID / TBD-KVX / TBD-GIT
have no code points and only sketch definitions; delegation `crit`/`ext`
processing thinly exampled; reservations (tasks §6) are a sketch; formal
analysis (ops §6.3) is a plan, not a result; two-independent-
implementations rule (ops §6.1) unmet — the Rust scaffold is impl #1.

## 5. Current state — Rust workspace (`map-rs/`) — **NEVER COMPILED**

Written carefully but **no `cargo build` has ever run** (toolchain was
still installing when the session ended). Expect a small fix loop.

| Crate | Status | Contents |
|---|---|---|
| crates/map-identity | written + unit tests | AgentId (validated ctor, multibase, did:key), Keypair, verify (verify_strict) |
| crates/map-core | written + unit tests | cbor.rs (canonical encoder + strict profile decoder), envelope.rs (seal/open, I1, channel-binding via external_aad), hello.rs (negotiate, TH, CB), replay.rs (ReplayCache ±120 s / 10 min, SeqTracker), msg_type.rs, errors.rs (codes 1000–6004 + ErrorMsg) |
| crates/map-discovery | written + unit tests | CapURI validation, cap_matches (prefix-segment), multihash_sha256, shard (digest byte), capability keys, minimal Descriptor (fields 1–6 + 12), DescriptorCache (seq-LWW) |
| crates/map-tasks | written + unit tests | TaskId, ChunkKind, TranscriptBuilder (patched Merkle), FlowAccount (dual-unit credits), requester state machine (ReqState/step; refines spec with explicit OfferSent) |
| crates/map-trust | written + unit tests | ReceiptCore/Receipt (TV-002 wire format), role signing, append-only idempotent ReceiptStore, TrustProjection (Beta, decay 0.98/day) with rebuild==incremental test |
| crates/map-pay | written + unit tests | Amount (tag-4 decimal, Display), Asset validation, InvoiceId/QuoteId, SettlementLedger (settle-once idempotency) |
| crates/map-cli | **MISSING** | planned: `keygen`, `projection-rebuild-demo` (ops §8 wants rebuild as first-class CLI op) |
| adapters/ollama-provider | **MISSING** | stub trait only for now (no HTTP deps yet; real impl talks to localhost:11434 later) |
| adapters/gateway | **MISSING** | stub: provenance labeling per ops §5 G-1..G-4 |
| conformance/map-conformance | **MISSING** | must reproduce TV-001/TV-002 **byte-for-byte** |

**Build-breaking fact:** the workspace `Cargo.toml` lists all ten members,
including the four missing ones — `cargo build` fails until they exist (or
members are temporarily trimmed). Create them first.

Workspace deps pinned: ed25519-dalek 2, sha2 0.10, hkdf 0.12, bs58 0.5,
getrandom 0.2, hex 0.4. Edition 2021. No async/network deps yet by design.

Known code caveats:
- `envelope::open()` verifies only direct kid signatures; ProtHdr key 33
  delegation chains (identity §4) are unimplemented.
- Body keys 8/9 (crit/ext) are not modeled; byte-exact preservation for
  store-and-forward (invariant I3) is achieved by keeping `Opened.payload`
  bytes, not by re-encoding — preserve that pattern.
- Descriptor omits optional keys 7–11/13/14 (models, hardware buckets,
  pricing, security profile, memory topics).
- ReplayCache purges via O(n) retain per call — fine for scaffold.

## 6. Test vectors (ground truth for conformance)

File: `map-spec/test-vectors/map-test-vectors.md`. Regenerate with
`python3 gen_vectors.py` (run from its directory; needs
`pip install pynacl cbor2 base58`). Facts the conformance crate needs:

- Seeds: `seed_A = SHA-256("MAP-TEST-VECTOR-KEY-A")`,
  `seed_B = SHA-256("MAP-TEST-VECTOR-KEY-B")` — derive in Rust, then
  assert against the hex in the vectors file.
- TV-001: HELLO envelope, 166 bytes. Body: version 0x0100, msg_type
  0x0001, msg_id `000102030405060708090a0b0c0d0e0f`, timestamp
  1780000000000, content Hello{versions:[0x0100], suites:[1],
  extensions:{}, max_message_size:262144}, external_aad empty.
- TV-002: dual-signed receipt, 327 bytes. Core: task = 32-byte constant
  (BLAKE2b-256 of "example-task" — **hardcode the hex from the vectors
  file; do not add a blake2 dep**), cap
  "map:cap:inference/text-generation", started 1780000000000, ended
  1780000042000, outcome 0, metrics {1523, 812, 431, 42000}, rating 92.
- Python used cbor2 `canonical=True` (RFC 7049 ordering); all keys here
  are small uints where it coincides with RFC 8949 bytewise ordering —
  the Rust encoder is the RFC 8949 one and must reproduce the bytes.

## 7. Immediate next steps (in order)

> 2026-07-03: steps 2–4 are now packaged as implementation specifications
> in `tasks/` (TASK-001..004; see `tasks/README.md`). Split-role model:
> the architect authors packages, coding LLMs execute the §17 prompts,
> reviewers validate per §18. Do not implement ahead of a package.

1. Ensure toolchain: `rustup default stable`.
2. Create the four missing members (cli, two adapter stubs, conformance).
3. `cargo build --workspace && cargo test --workspace`; fix until green.
   All existing unit tests should pass, including the DDIA projection
   rebuild test in map-trust.
4. Conformance crate: reproduce TV-001 envelope and TV-002 receipt
   byte-for-byte from seeds; add negative tests per ops §6.2 (tampered
   byte, wrong external_aad, non-canonical CBOR variants, stale
   descriptor seq, wrong-shard key, duplicate ingest).
5. Package/commit; note in ops §8 that the scaffold exists.

## 8. Roadmap after the scaffold is green (ops §8 MVP ordering)

1. **Session layer**: HELLO/HELLO_ACK over an in-memory duplex first, then
   libp2p `/map/1.0.0` (rust-libp2p; PeerID must equal the AgentID key —
   identity §3.2 B1). Wire real channel binding from the transport
   exporter/Noise hash.
2. **Descriptor publish/fetch**: envelope-wrapped descriptors, Kademlia
   provider records under sharded capability keys, republish ≤6 h.
3. **Echo task end-to-end**: ASSIGN → CHUNK stream under FlowAccount →
   COMPLETE with transcript root; TTFT ×1.1 cancel rule; requester state
   machine driving it.
4. **Receipts**: dual-sign after verified COMPLETE; EVIDENCE_QUERY/SET.
5. **Payments last**: `pay:credit-ledger` as the first settlement method.
6. Then: DelegationCert implementation + envelope chain verification;
   Ollama adapter for real; extension code points (BID/KVX/GIT) via the
   registry process; formal models (Tamarin/ProVerif) of HELLO +
   channel binding + delegation attenuation; Go implementation (impl #2
   for the two-impl rule); federation gateway.

## 9. Working conventions

- BCP 14 language, unweakened. Registry code points never reused.
- Spec-feedback loop: when implementation exposes ambiguity, patch the
  spec in the same change (precedent: shard fix, Merkle fix) and record
  it in the doc.
- One source of truth per fact; events past-tense, commands imperative;
  idempotency wherever delivery is uncertain; ordering scoped and written
  down; projections rebuildable (full table: ops §2).
- Value types over bare byte strings at API boundaries.
- New wire structures: define CDDL in the spec first, then mirror in code
  with a roundtrip test; anything signed gets a vector.
