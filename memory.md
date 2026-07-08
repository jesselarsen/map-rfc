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
- pay §3.1 (2026-07-04, from TASK-004 feedback): `PayQuoteRef` /
  `PayAgreeRef` / `PayRef` defined — refs are (quote-id, digest of
  received envelope bytes) pairs per I1; `PayRef` is a pre-quote pricing
  expectation. PayQuote gained optional key 9 (TaskID binding; MUST when
  quoting against an OFFER) closing quote replay across tasks. tasks §2
  comments now cite pay §3.1.
- discovery §7 (2026-07-05, from TASK-006): CAP_QUERY/CAP_RESPONSE content
  maps assigned — `CapQuery {1:CapURI, ?2:shard, ?3:limit}`,
  `CapResponse {1:[* descriptor-envelope bstr], ?2:more}` (envelopes inline,
  querier verifies each; batch ≤ 8). NOTE for the architect: TASK-006 FR-5
  wrote STATUS_QUERY as `{1: CapURI}` (single, required) but §3 already had
  `StatusQuery {?1: [+CapURI], ?2: echo}` (array, optional); map-mesh
  implemented §3's array form (draft-wins) — FR-5 text should be corrected.
- RFC 2119/8174 compliance pass (2026-07-05/06, architect-ruled, editorial-
  with-normative-effect): all seven remaining docs (core, discovery,
  identity, ops, pay, tasks, trust) gained the arch §2 boilerplate verbatim
  + a `## References (selected)` section citing `[RFC2119] [RFC8174]` —
  previously only arch-00 had them, the rest just pointed at it by
  reference. Capitalized every load-bearing lowercase MUST/MUST NOT/MAY
  found in a parallel construction next to an already-capitalized keyword
  or in a formal validation/registry/conformance list: core §9 (retryable
  MUST), discovery §2 (`seq` MUST exceed), identity §4 (validity-window/
  chain-root MUST ×2; REVOKE MAY), ops §2 rules 3+7 (idempotency/
  projections-rebuildable MAY/MUST), ops §3.1 registry table (settlement
  method MUST define...), ops §6.2 (7 instances — the entire "Required
  negative tests" list, incl. the I1 re-serialization test), tasks §2.2/§9
  (bidding-broadcast MAY, zone tighten-only MAY), trust §5/§8 (receipt-
  identity MUST match, holder-answers MAY, rebuild-write-path MAY).
  Prose-only: `gen_vectors.py` regenerated byte-identical (idempotent),
  full workspace suite green — no code/vector drift.
- tasks §2/§11 (2026-07-06, from TASK-007): Accept/Reject content maps
  (`Accept = {}`, `Reject = {1 code}`) and CANCEL field numbers
  (`{1 TaskID, 2 code}`) recorded — the negotiation diagram/§11 had shown
  them only inline (editorial/additive assignment FR-1 makes).
- **`map-spec/ERRATA.md` created (2026-07-05, PENDING-ARCHITECT queue).** Two
  TASK-007 items awaiting a future architect: (a) tasks §2 Quote key 2
  (PayQuoteRef) is draft-REQUIRED but the echo runtime omits it (propose
  `?2`); (b) frozen TV-007 ASSIGN uses `InputSpec {2:[multihash]}` but the
  codec models inline (key 1) only → TV-007 is Rust-consume-only, a
  frozen-vector parity hole for TASK-015. Handoff protocol: wire-affecting
  items go to ERRATA, not unilateral normative edits.
- trust §3 (2026-07-06, from TASK-008): countersignature tolerance assigned
  (usage ±5% of the OBSERVED value min ±1, timestamps ±2 s; band anchored to
  the trusted side — farming defense) + the task-terminal→outcome mapping
  table (Verified→0, Failed→1, TTFT-cancel→3). ERRATA gained two TASK-008
  items: (c) no core §9 code for a tolerance-rejected RECEIPT (interim 5000,
  propose a dedicated code); (d) trust §2 diagram shows `RECEIPT_ACK {sig_prov}`
  but the impl returns the full dual receipt (propose diagram fix).

Honest rev-00 gaps (spec side): extensions TBD-BID / TBD-KVX / TBD-GIT
have no code points and only sketch definitions; delegation `crit`/`ext`
processing thinly exampled; reservations (tasks §6) are a sketch; formal
analysis (ops §6.3) is a plan, not a result; two-independent-
implementations rule (ops §6.1) unmet — the Rust scaffold is impl #1.

## 5. Current state — Rust workspace (`map-rs/`) — **BUILDS + TESTS GREEN**

TASK-001 (2026-07-03) took the workspace from never-compiled to
`cargo build --workspace` + `cargo test --workspace` green, zero warnings.
The six pre-existing crates needed **no code changes** — the "small fix loop"
was empty; all their unit tests passed unmodified. Build toolchain note: this
machine has no MSVC C++ build tools installed, so the default
`stable-x86_64-pc-windows-msvc` fails at link (`link.exe` not found). Build/test
with the installed GNU toolchain: `cargo +stable-x86_64-pc-windows-gnu build`.
(Also, run cargo from PowerShell, not the Git-Bash shell — Bash's coreutils
`link` shadows any MSVC linker.)

| Crate | Status | Contents |
|---|---|---|
| crates/map-identity | written + unit tests | AgentId (validated ctor, multibase, did:key), Keypair, verify (verify_strict) |
| crates/map-core | written + unit tests | cbor.rs (canonical encoder + strict profile decoder), envelope.rs (seal/open, I1, channel-binding via external_aad), hello.rs (negotiate, TH, CB), replay.rs (ReplayCache ±120 s / 10 min, SeqTracker), msg_type.rs, errors.rs (codes 1000–6004 + ErrorMsg) |
| crates/map-discovery | written + unit tests | CapURI validation, cap_matches (prefix-segment), multihash_sha256, shard (digest byte), capability keys, minimal Descriptor (fields 1–6 + 12), DescriptorCache (seq-LWW) |
| crates/map-tasks | written + unit tests; +msg codecs (TASK-007) | TaskId, ChunkKind, TranscriptBuilder (patched Merkle), FlowAccount (dual-unit credits), requester state machine (ReqState/step). `msg` module (TASK-007): pure to_value/from_value for Offer/Quote/Assign/Accept/Reject/ChunkMsg/Complete/Cancel/FlowCredit + TaskConstraints/InputSpec/FlowInit/Usage. Complete key 3 = multihash(root); pay refs modeled `Option<Value>`, omitted (ERRATA); InputSpec inline-only |
| crates/map-trust | written + unit tests; +lifecycle (TASK-008) | ReceiptCore/Receipt (TV-002), role signing, idempotent ReceiptStore, TrustProjection (Beta, rebuild==incremental). TASK-008: `Receipt::to_value/from_value` (message content; encode/decode_bytes frozen-wire untouched) + `lifecycle::within_tolerance` (usage ±5% of OBSERVED value, ts ±2 s — anchored to the trusted side, farming defense) |
| crates/map-pay | written + unit tests | Amount (tag-4 decimal, Display), Asset validation, InvoiceId/QuoteId, SettlementLedger (settle-once idempotency) |
| crates/map-session | done (TASK-005; HelloAck refactor TASK-006) + 21 tests | in-memory session layer: HELLO/HELLO_ACK handshake, TH/CB, core §12 state machine, in-session send/recv with seq+replay+size enforcement, PING/PONG/CLOSE. `Transport`/`Clock`/`MsgIdSource` traits + `InMemoryDuplex` (mpsc). Uses `map_core::HelloAck` (private codec deleted, TASK-006). Deps: identity/core only. Handshake=empty aad, in-session=CB. Real libp2p exporter still future |
| crates/map-taskrun | done (TASK-007/008) + 29 tests | pump-driven runtimes: `ProviderTask` (Executor/EchoExecutor; credit-gated CHUNK stream, Starved-not-overrun, transcript, COMPLETE, exposes `observation()`) + `RequesterTask` (ReqState FSM, OFFER/direct-ASSIGN, QUOTE eval, regrant, TTFT×1.1 CANCEL, root verify). TASK-008 receipts.rs (`requester_issue`/`provider_countersign`/`requester_on_ack`, `make_receipt_core`, tolerance+I1 countersign) + evidence.rs (`serve_evidence`/`query_evidence`/`apply_verified_evidence` — dual-signed AND provider==subject gate). Deps: identity/core/session/tasks/trust (no pay/mesh) |
| crates/map-mesh | done (TASK-006) + 12 tests | discovery flows over sessions: `publish_descriptor`/`ingest_descriptor_envelope` (self-signed, ttl, seq-LWW), `MeshIndex` (in-process DHT stand-in; order-independent ingest ⇒ `rebuild==incremental`), CAP_QUERY/CAP_RESPONSE + STATUS over a live session (`serve_one`/`query_caps`/`query_status`). Index is untrusted transport — client verifies every envelope. Deps: identity/core/session/discovery (no tasks/trust/pay) |
| crates/map-cli | done (TASK-001) + 6 unit tests | `keygen` (seed/pubkey/AgentID hex+multibase/did:key; random or `--seed`), `projection-rebuild-demo` (synth dual-signed receipts → incremental vs `rebuild`, ops §8); hand-rolled argv, deps: identity/tasks/trust/sha2/hex/getrandom |
| adapters/ollama-provider | done (TASK-002) + 4 unit tests | `InferenceBackend` trait + deterministic `MockBackend`, `result_to_chunks`/`run_task` glue (kind-0 text + kind-3 usage CBOR, seq 0/1). No HTTP — real localhost:11434 backend still future |
| adapters/gateway | done (TASK-002) + 4 unit tests | provenance per ops §5: `GatewayAdvert` wrapper + `label_descriptor`/`is_gateway_advert` (G-1), `effective_zone` tighten-only (G-3), `valid_namespace` DNS/mesh (G-4). Descriptor wire format untouched |
| conformance/map-conformance | done (TASK-003/004) + 30 cases | reproduces TV-001..010 + NV-001..005 **byte-for-byte**; ops §6.2 negatives; `run_all() -> Report`. TV-003 upgraded (TASK-006) to a HelloAck PRODUCE-guard (from_value→to_value must reproduce the vector bytes — the tripwire for any HELLO_ACK codec regression), same discipline as the DESCRIPTOR round-trip. Encoder needed NO fix |

**Build-breaking fact (RESOLVED by TASK-001):** the workspace `Cargo.toml`
lists eleven members (map-session added by TASK-005); all exist and build.

Spec-feedback from TASK-005: RESOLVED 2026-07-04. (a) HELLO_ACK — core
§6.1 already had singleton HelloAck CDDL; the gap is code-side (map-core
has no HelloAck type; map-session hand-rolls the codec). A first-class
`HelloAck` in map-core is folded into TASK-006's prep work. **DONE
(TASK-006): `map_core::hello::HelloAck` added (to_value/from_value + `as_hello`
+ `Negotiated::to_ack`); map-session's private codec deleted; TV-003 bytes
unchanged (now a produce-guard).** (b) core §6.1
now mandates the responder MUST echo the initiator's SessionID (Body key
6); mismatch = 1006 + close — matching the implementation. (c) core §6.3
now says receivers SHOULD check the raw frame size before any decode/
signature work; TASK-005 doc carries an erratum fixing its FR-7 ordering
text (implemented order size→open→sender→replay→seq is correct).

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
`pip install pynacl cbor2 base58`; writes UTF-8 explicitly — don't drop
`encoding="utf-8"` or Windows cp1252 mangles the em-dashes). The script
self-tests (verifies every signature + a TV-001/TV-002 regression assert)
and is idempotent. Facts the conformance crate needs:

TASK-004 status: expanded to TV-003..TV-010 (HELLO_ACK/ERROR/DESCRIPTOR/
OFFER/ASSIGN/CHUNK/COMPLETE/PAY_QUOTE — one signed envelope per family),
NV-001..005 (MUST-reject bytes with core §9 class), and a Derived-constants
section (TH, CB, shard(B)=141, DHT keys, transcript root). The Rust
conformance crate consumes/rejects all of them (`extended.rs`, `vectors2.rs`;
29 cases total, all green). **Field-number gaps: RESOLVED 2026-07-04.**
TASK-004 §2's TV-004/005/006/010 layouts contradicted the drafts; the
vectors followed the drafts, and the task doc now carries errata blocks
correcting each item. The genuine draft gaps are patched: pay §3.1 defines
`PayQuoteRef`/`PayAgreeRef`/`PayRef`, and PayQuote gained optional key 9
(TaskID binding). TV-010 remains a valid *unbound standing quote*; a bound
variant is future vector work. TV-001..TV-010/NV-001..005 are now safe to
treat as the frozen interop contract.

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

1. Ensure toolchain: `rustup default stable` (but see §5 — this box builds
   only with the GNU toolchain; no MSVC C++ tools installed).
2. ~~Create the four missing members.~~ DONE (TASK-001): map-cli implemented,
   three stubs created.
3. ~~`cargo build/test --workspace` until green.~~ DONE (TASK-001): green,
   zero warnings, DDIA rebuild test passes. TASK-002 (adapters) and TASK-003
   (conformance) can now run in parallel.
4. ~~Conformance crate: reproduce TV-001/TV-002 byte-for-byte + ops §6.2
   negatives.~~ DONE (TASK-003): 16 tests green; the RFC 8949 encoder matched
   the cbor2-canonical vectors on the first run (no encoder fix needed),
   confirming the R2 ordering note below. ops §6.2 minimum negative suite for
   impl #1 is satisfied.
5. ~~Package/commit; note in ops §8 that the scaffold exists. TASK-004
   (test-vector expansion).~~ DONE (TASK-004, 2026-07-04): TV-003..010 +
   NV-001..005 + derived constants; `gen_vectors.py` extended, conformance
   crate mirrors it in `extended.rs`/`vectors2.rs` (29 cases green). All four
   packages TASK-001..004 committed/pushed to `master`.
6. **Next package (architect-suggested, 2026-07-05) — bound PAY_QUOTE
   coverage.** A small vector revision: add a *bound* PAY_QUOTE variant with
   the new optional **key 9 (TaskID binding)** present, plus a negative case
   where the task binding is mismatched and MUST be rejected — giving the new
   normative MUST in **pay §3.1** executable coverage. Extend `gen_vectors.py`
   (a bound TV-010b + an NV) and mirror in the conformance crate
   (`vectors2.rs`/`extended.rs`). TV-010 stays the valid *unbound standing
   quote*; the bound variant is additive (don't disturb frozen TV-001..010).

## 8. Roadmap after the scaffold is green (ops §8 MVP ordering)

1. **Session layer**: ~~HELLO/HELLO_ACK over an in-memory duplex first~~ DONE
   (TASK-005): `crates/map-session` handshake + state machine + seq/replay/size
   over `InMemoryDuplex`, 21 tests green. REMAINING: libp2p `/map/1.0.0`
   binding (rust-libp2p; PeerID must equal the AgentID key — identity §3.2 B1);
   wire real channel binding from the transport exporter/Noise hash (the
   `Transport::exporter()` seam is ready for it).
> 2026-07-04: EVERY remaining roadmap item below now has an authored
> implementation-spec package — TASK-006..TASK-016 in `tasks/` (see
> tasks/README.md for the dependency graph and recommended order:
> 006 → 007 → 008&009 ∥ → 012&013 ∥, 010/011 anytime, then 014/015,
> 016 last). The architect departs 2026-07-06; tasks/README.md
> "Operating without the architect" defines the PENDING-ARCHITECT
> protocol (map-spec/ERRATA.md) for anything the packages don't
> pre-decide. Mapping: step 1-remaining(libp2p)=TASK-012 ·
> step 2=TASK-006 · step 3=TASK-007 · step 4=TASK-008 ·
> step 5=TASK-009 · step 6: delegation=TASK-010, Ollama=TASK-013,
> ext code points+reservations=TASK-011, formal=TASK-014,
> Go=TASK-015, gateway=TASK-016.

2. **Descriptor publish/fetch**: ~~envelope-wrapped descriptors, provider
   records under sharded capability keys, republish ≤6 h.~~ DONE (TASK-006):
   `crates/map-mesh` — publish/ingest, `MeshIndex` (DHT stand-in), CAP_QUERY +
   STATUS over sessions, 12 tests. `map_core::HelloAck` landed (map-session
   refactored off its private codec). REMAINING: real Kademlia (provider
   records, disjoint-path lookups, anti-eclipse) replaces `MeshIndex` behind
   the same API in TASK-012.
3. **Echo task end-to-end**: ~~ASSIGN → CHUNK stream under FlowAccount →
   COMPLETE with transcript root; TTFT ×1.1 cancel rule.~~ DONE (TASK-007):
   `map-tasks::msg` codecs + `crates/map-taskrun` (pump-driven provider/
   requester runtimes), 19 tests incl. live-session E2E, tamper→5000,
   starved-not-overrun. TASK-008 consumes `ReceiptInputs`; TASK-009 wires the
   pay-ref codec fields (already present as `Option`).
4. **Receipts**: ~~dual-sign after verified COMPLETE; EVIDENCE_QUERY/SET.~~
   DONE (TASK-008): receipt lifecycle (requester issue → provider countersign
   → ack) + pull evidence over sessions, in `map-taskrun` (receipts/evidence)
   + `map-trust::lifecycle`. The trust loop is closed: work → dual-signed
   evidence → local score. Two enforcement gates proven (single-signed and
   wrong-provider receipts never reach a projection).
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
