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
./map-go/                    Go module — impl #2 (vectors + session layer)
./interop/                   live Rust↔Go interop matrix: driver + MATRIX.md (TASK-020)
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
- pay §5.1 (2026-07-06, from TASK-009): `pay:credit-ledger` method defined
  (operator-signed entry format `enc(["MAPLedgerEntry",…])`, proof payload
  `[entry-bytes, sig]`, verify_strict verification, instant/irreversible
  finality). ERRATA gained three TASK-009 items: content-vs-envelope digest
  binding (§3.1 wants envelope bytes; session doesn't expose them → content-
  bind, price-switch defense intact); no distinct 6xxx codes for cap-exceeded/
  digest-mismatch/binding-mismatch/bad-proof (interim 6002); PayInvoice.4 usage
  watermark shape unspecified (omitted, postpaid flat).
- trust §3 (2026-07-06, from TASK-008): countersignature tolerance assigned
  (usage ±5% of the OBSERVED value min ±1, timestamps ±2 s; band anchored to
  the trusted side — farming defense) + the task-terminal→outcome mapping
  table (Verified→0, Failed→1, TTFT-cancel→3). ERRATA gained two TASK-008
  items: (c) no core §9 code for a tolerance-rejected RECEIPT (interim 5000,
  propose a dedicated code); (d) trust §2 diagram shows `RECEIPT_ACK {sig_prov}`
  but the impl returns the full dual receipt (propose diagram fix).

Honest rev-00 gaps (spec side): extensions bid/kvx/git now have code points
(ext keys 1/2/3, TASK-011 — bid is full, kvx/git are reserved stubs w/ container
+ zone rule, full semantics deferred); delegation `crit`/`ext`
processing thinly exampled (delegation CERTS + chain verify now implemented,
TASK-010 — two caveats task-pin/audience, deeper caveats e.g. budget still
future; revocation distribution still absent); reservations (tasks §6) now
normative (CDDL+FSM+MUSTs, codecs only — no runtime yet, TASK-011); formal
analysis (ops §6.3) delivered for P1–P3 (TASK-014); two-independent-
implementations rule (ops §6.1): impl #1 = Rust scaffold, impl #2 = `map-go/`
at frozen-vector parity (TASK-015, 2026-07-11) — the LIVE pairwise interop
matrix is still unmet, so no document leaves draft yet.

## 5. Current state — Rust workspace (`map-rs/`) — **BUILDS + TESTS GREEN**

**TASK-017 DONE 2026-07-12** (implements the architect rulings of §7 step
14): `map-session` `Session::send` now returns the SEALED envelope bytes and
`Incoming` carries `raw: Vec<u8>` (the exact received frame, I1). Payment
digests re-bound to envelope bytes — requester digests `inc.raw`; provider
calls `ProviderPayment::bind_quote_envelope(sealed)` with what `send`
returned (the pump feeds it back; session-less unit tests use the `synth_env`
stand-in in tests.rs). This fixes an interop BUG: the old content-re-encoding
digest never matched frozen TV-013 or map-go. Pay/receipt reject codes split
per core §9: 2006 countersign-refused (`RECEIPT_REJECT_CODE`, was 5000),
6005 cap-exceeded, 6006 agreement-digest-mismatch (price switch), 6007
task-binding-mismatch, 6008 invalid-payment-proof; 6002 stays generic. New
test `payagree_binds_received_envelope_bytes_not_content` (FR-6 b/d) proves
envelope≠content digest. Whole workspace green, zero warnings.

TASK-001 (2026-07-03) took the workspace from never-compiled to
`cargo build --workspace` + `cargo test --workspace` green, zero warnings.
The six pre-existing crates needed **no code changes** — the "small fix loop"
was empty; all their unit tests passed unmodified. Build toolchain note: this
machine has no MSVC C++ build tools installed, so the default
`stable-x86_64-pc-windows-msvc` fails at link (`link.exe` not found). Build/test
with the installed GNU toolchain: `cargo +stable-x86_64-pc-windows-gnu build`.
(Also, run cargo from PowerShell, not the Git-Bash shell — Bash's coreutils
`link` shadows any MSVC linker.)

**CRITICAL toolchain fact (TASK-012, 2026-07-08):** since `map-net` joined the
workspace, building/testing the **whole workspace** requires the mingw-w64
toolchain on PATH — `map-net` pulls libp2p → `windows-sys` (needs `dlltool.exe`
to make Windows import libs on the -gnu target) and `ring` (needs `gcc` for
quic). A complete MSYS2 mingw64 is already installed at **`C:\msys64\mingw64\bin`**
(gcc 16.1, binutils 2.46) but is NOT on PATH by default. Every build/test
command must prepend it, e.g. in PowerShell:
`$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"; cargo +stable-x86_64-pc-windows-gnu test --workspace`.
Without it you get `error calling dlltool 'dlltool.exe': program not found` on
`parking_lot_core`/`windows-sys`. The pure-Rust crates (everything except
map-net) still build without mingw.

| Crate | Status | Contents |
|---|---|---|
| crates/map-identity | written + unit tests | AgentId (validated ctor, multibase, did:key), Keypair, verify (verify_strict) |
| crates/map-core | written + unit tests; +delegation (TASK-010) | cbor.rs (canonical encoder + strict profile decoder), envelope.rs (seal/open, I1, channel-binding via external_aad; TASK-010: `seal_delegated`/`open_delegated` key 33 — `open()` byte-frozen), hello.rs (negotiate, TH, CB), replay.rs (ReplayCache ±120 s / 10 min, SeqTracker), msg_type.rs, errors.rs (codes 1000–6004 + ErrorMsg). `delegation.rs` (TASK-010): DelegateBody codec, issue/validate_cert/validate_chain→EffectiveScope, Caveat task-pin/audience, tighten-only + fail-closed, chain cap 8 |
| crates/map-discovery | written + unit tests | CapURI validation, cap_matches (prefix-segment), multihash_sha256, shard (digest byte), capability keys, minimal Descriptor (fields 1–6 + 12), DescriptorCache (seq-LWW) |
| crates/map-tasks | written + unit tests; +msg codecs (TASK-007); +reservations (TASK-011) | TaskId, ChunkKind, TranscriptBuilder (patched Merkle), FlowAccount (dual-unit credits), requester state machine (ReqState/step). `msg` module: pure to_value/from_value for Offer/Quote/Assign/Accept/Reject/ChunkMsg/Complete/Cancel/FlowCredit + TaskConstraints/InputSpec/FlowInit/Usage. TASK-011: `Reserve`/`Lease`/`LeaseRelease` codecs (tasks §6) + `Lease::granted_window_within`. Complete key 3 = multihash(root); pay refs modeled `Option<Value>`, omitted (ERRATA); InputSpec inline-only |
| crates/map-trust | written + unit tests; +lifecycle (TASK-008) | ReceiptCore/Receipt (TV-002), role signing, idempotent ReceiptStore, TrustProjection (Beta, rebuild==incremental). TASK-008: `Receipt::to_value/from_value` (message content; encode/decode_bytes frozen-wire untouched) + `lifecycle::within_tolerance` (usage ±5% of OBSERVED value, ts ±2 s — anchored to the trusted side, farming defense) |
| crates/map-pay | written + unit tests; +codec/ledger (TASK-009) | Amount (tag-4), Asset, InvoiceId/QuoteId, SettlementLedger. TASK-009: `codec` (PayQuote incl. ?9 task-binding, PayAgree, Pay{Quote,Agree}Ref, PayRef, PayInvoice/Proof/Receipt, MethodOffer, AssetAmount, `pay_ref_digest`) + `credit_ledger` (LedgerOperator: signed entry `enc(["MAPLedgerEntry",…])`, idempotent `pay`, `verify_entry` verify_strict). deps += identity/sha2 |
| crates/map-session | done (TASK-005; HelloAck refactor TASK-006; delegation FR-5 TASK-010) + 21 tests | in-memory session layer: HELLO/HELLO_ACK handshake, TH/CB, core §12 state machine, in-session send/recv with seq+replay+size enforcement, PING/PONG/CLOSE. `Transport`/`Clock`/`MsgIdSource` traits + `InMemoryDuplex` (mpsc). Uses `map_core::HelloAck`. TASK-010: `SessionConfig::new()` + optional `delegation_chain` (handshake carries key 33 via seal_delegated), `accountable_peer()` records the chain ROOT (leaf still = transport-auth peer); bad chain → 2003. Deps: identity/core only. Handshake=empty aad, in-session=CB. Real libp2p exporter still future |
| crates/map-taskrun | done (TASK-007/008/009) + 40 tests | pump-driven runtimes: task (Provider/RequesterTask), receipts.rs + evidence.rs (TASK-008: dual+provider==subject gate), payments.rs (TASK-009: `ProviderPayment`/`RequesterPayment` — PAY_QUOTE(task-bound)→AGREE→INVOICE→pay via LedgerOperator→PROOF→verify+settle_once→RECEIPT; cap/digest/binding/forged-proof defenses; retries move money once). Deps: identity/core/session/tasks/trust/pay (no mesh) |
| crates/map-mesh | done (TASK-006) + 12 tests | discovery flows over sessions: `publish_descriptor`/`ingest_descriptor_envelope` (self-signed, ttl, seq-LWW), `MeshIndex` (in-process DHT stand-in; order-independent ingest ⇒ `rebuild==incremental`), CAP_QUERY/CAP_RESPONSE + STATUS over a live session (`serve_one`/`query_caps`/`query_status`). Index is untrusted transport — client verifies every envelope. Deps: identity/core/session/discovery (no tasks/trust/pay) |
| crates/map-cli | done (TASK-001; +serve-ollama TASK-013) + 6 unit tests | `keygen`, `projection-rebuild-demo`; hand-rolled argv. TASK-013: `serve-ollama --model --prompt [--url --max-tokens]` (feature `real-backend`) — single-process demo: real Ollama call → prints output + transcript root + dual-signed receipt (needs live server). deps: identity/tasks/trust/sha2/hex/getrandom + optional ollama-provider |
| crates/map-net | done (TASK-012, 2026-07-08) + 4 unit tests + 2 gated net tests | **First real network transport.** `framing` (u32-BE length prefix, size-first), `Libp2pTransport` (blocking `map_session::Transport` bridged over tokio mpsc), `MapNode` (blocking API over a background tokio swarm: listen/dial/accept/publish_descriptor/find_providers/fetch_descriptor). Behaviour = libp2p-stream + kad(MemoryStore) + identify. PeerID≡AgentID asserted (FR-3); dial uses DialOpts(peer-id) so libp2p enforces the expected peer. Exporter = init-AID‖resp-AID (FR-4, core §10). Deps: libp2p 0.54.1 (tcp+quic+noise+yamux+kad+identify+dns+ed25519), **libp2p-stream 0.2.0-alpha** (raw substreams; umbrella has no `stream` feature — separate alpha crate), futures 0.3, tokio 1. Net tests behind `net-tests` feature + `#[ignore]` (2-node loopback handshake/echo/kad + dial-wrong-peer, both PASS). NOT-done (deferred, noted in report): map-cli `node` subcommands; anti-relay-across-two-connections + poisoned-kad-record network tests; NAT/relay/eclipse (all future) |
| adapters/ollama-provider | done (TASK-002; real backend TASK-013 2026-07-08) + 13 tests (feature) | `InferenceBackend` trait + deterministic `MockBackend`, `result_to_chunks`/`run_task` glue. TASK-013: `OllamaExecutor<B>` (adapts any backend to `map_taskrun::Executor`; input=prompt UTF-8, errors/invalid→empty output never panic) always-on; **`OllamaBackend`** (real HTTP `/api/generate` stream:false, FR-2 field map, FR-3 errors, no prompt in error strings) behind feature **`real-backend`** (default OFF; deps `ureq` default-features-off + `serde_json`, NOT in default tree). `BackendError::Unavailable` now carries a msg (FR-10). Canned-JSON unit tests; live test `#[ignore]`. `Unavailable`→`Unavailable(String)` |
| adapters/gateway | done (TASK-002; running gateway TASK-016 2026-07-08) + 15 tests | provenance primitives (TASK-002): `GatewayAdvert`/`label_descriptor` (G-1), `effective_zone` (G-3), `valid_namespace` (G-4). TASK-016: **`GatewayProvider<B>`** — a running gateway that IS a `map_taskrun::Executor` (composes `OllamaExecutor`, fronts any `InferenceBackend`) + `admit(cap,zone,now)` gate: FR-5 scope (its own `EffectiveScope::permits` from a delegation chain) → FR-2/G-3 zone (`effective_zone > task_zone` → reject); `GatewayReject{Zone→2004,Scope→2005,BadChain→2003}`. `build_gateway_descriptor` (subject=gateway, G-1/FR-4). FR-6 E2E (task→chunks→COMPLETE→dual-signed receipt naming the gateway) via shuttle. deps: +map-core/tasks/taskrun/ollama-provider (acyclic; InferenceBackend reuse). Discovery §4 patch: gateway/ params key 1=gateway-of (codec doesn't model params yet → GatewayAdvert interim) |
| conformance/map-conformance | done (TASK-003/004; +bound-quote, delegation, spec-round-3 2026-07-08) + 38 cases (+run_all) | reproduces TV-001..015 + NV-001..008 **byte-for-byte** (adds TV-010b bound quote, TV-011 cert, TV-012 delegated OFFER, TV-013 PAY_AGREE, TV-014 RESERVE, TV-015 LEASE; NV-006 task-binding, NV-007 widened scope, NV-008 out-of-window lease); gained a map-pay dep (real PayQuote/PayAgree) + uses map_core delegation + map_tasks Reserve/Lease; ops §6.2 negatives; `run_all() -> Report`. TV-003 upgraded (TASK-006) to a HelloAck PRODUCE-guard (from_value→to_value must reproduce the vector bytes — the tripwire for any HELLO_ACK codec regression), same discipline as the DESCRIPTOR round-trip. Encoder needed NO fix |

**Build-breaking fact (RESOLVED by TASK-001):** the workspace `Cargo.toml`
lists all members (map-session added by TASK-005, map-net by TASK-012); all
exist and build (map-net only with mingw on PATH — see the CRITICAL note above).

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
- `envelope::open()` verifies only direct kid signatures (byte-frozen); ProtHdr
  key 33 delegation chains are handled by `open_delegated` (TASK-010), which
  layers chain validation on top of `open()`.
- Body keys 8/9 (crit/ext) are not modeled; byte-exact preservation for
  store-and-forward (invariant I3) is achieved by keeping `Opened.payload`
  bytes, not by re-encoding — preserve that pattern.
- Descriptor omits optional keys 7–11/13/14 (models, hardware buckets,
  pricing, security profile, memory topics).
- ReplayCache purges via O(n) retain per call — fine for scaffold.

## 6. Test vectors (ground truth for conformance)

**Counter at NV-009 (TASK-018, 2026-07-12).** NV-009 = a 2-link delegation
chain whose PARENT declares max-depth 0 yet DK delegates further; identical
scopes isolate the identity §4 depth bound → BAD_DELEGATION (2003). Added
additively to gen_vectors.py (TV-001..015 + NV-001..008 frozen bytes
untouched — the script's in-line asserts + a git diff of only-additions are
the guard). **TV-007 is now full produce+consume parity in BOTH impls**: the
Rust map-tasks `InputSpec` codec models keys 1/2/3 (≥1 required), and the
Rust conformance TV-007 case round-trips the frozen ASSIGN byte-exact — this
CLOSES the long-standing ERRATA 2026-07-05 parity hole (Rust was
consume-only). Rust conformance = 39 cases; map-go = 25 vectors. Next NV is
NV-010.

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
6. ~~**Next package (architect-suggested, 2026-07-05) — bound PAY_QUOTE
   coverage.**~~ DONE (2026-07-08): added a *bound* **TV-010b** (PAY_QUOTE with
   key 9 = the TV-006 OFFER's TaskID, pay §3.1 MUST) + **NV-006** (key 9 present
   but mismatched → §3.1 policy REJECT, cf. NV-004 — envelope is valid and OPENS;
   the binding check rejects). Both are additive: frozen TV-001..010 / NV-001..005
   bytes are byte-for-byte undisturbed (markdown diff = 33 insertions, 0 deletions;
   gen_vectors.py re-verified idempotent). Conformance mirrors them via the REAL
   `map_pay::codec::PayQuote` (new dep on map-pay) — `case_tv010b_pay_quote_bound`
   asserts `task_binding == Some(offer_task_id)`, `case_nv006_task_binding_mismatch`
   asserts the §3.1 reject, exactly as `map-taskrun::payments` does. run_all now
   31 cases (was 29); workspace green, zero warnings. COMMITTED + PUSHED to `main`
   as f99f83c (2026-07-08; the commit also folded in the README license-line edit
   MIT OR Apache-2.0 → Apache-2.0). Files:
   `map-spec/test-vectors/gen_vectors.py` + regenerated `map-test-vectors.md`;
   `map-rs/conformance/map-conformance/{Cargo.toml,src/vectors2.rs,src/extended.rs,src/lib.rs}`.

> NOTE (2026-07-08): the repo default branch was renamed `master` → `main`
> (origin/HEAD → origin/main; remote `master` deleted). Any `master` references
> elsewhere in this file are historical.

7. ~~**TASK-010 — delegation certificates + envelope chain verification.**~~ DONE
   (2026-07-08). `map-core::delegation` (NEW module — hosted in map-core, NOT
   map-identity as TASK-010 §4 suggested, because a cert IS an envelope and
   map-identity has no CBOR/envelope dep; hosting it there would invert the dep
   graph, NFR §3. Wire format is spec-exact; only the host crate differs):
   `DelegateBody` codec, `issue`/`validate_cert`, `validate_chain` →
   `EffectiveScope{root,caps,zones,depth_left,task_pin}`, `Caveat`
   (task-pin=1/audience=2), tighten-only attenuation, fail-closed unknown
   caveats, chain cap 8. `envelope::open_delegated` (key 33; verifies LEAF sig,
   returns scope+root; `open()` untouched/byte-frozen) + `seal_delegated`.
   map-session FR-5: `SessionConfig::new()` + `delegation_chain` field +
   `Session.accountable_peer()` (records the root when the peer delegates; leaf
   still satisfies transport-auth). Vectors TV-011 (cert)/TV-012 (delegated
   OFFER, chain in key 33)/**NV-007** (widened scope → 2003); conformance 31→34
   cases; gen_vectors idempotent, TV-001..010b frozen (57 additive md lines).
   Spec patches: identity §4 (caveat registry, fail-closed, cap 8 + editorial
   note) and §11 (caveat table). **ERRATA:** identity-§4 "chain root MUST equal
   kid" clause contradicts the crypto (leaf signs) — logged PENDING-ARCHITECT;
   impl follows kid==leaf-subject, root returned as accountable. NV numbering:
   TASK-010's planned "NV-006" became **NV-007** (NV-006 taken by the bound-quote
   pass); TASK-011 should continue from NV-008. Whole workspace green, zero warns.

8. ~~**TASK-011 — spec extensions + reservations + vector round 3.**~~ DONE
   (2026-07-08). Registry (FINAL): msg-types RESERVE 0x020F / LEASE 0x0210 /
   LEASE_RELEASE 0x0211 (core §5 + msg_type.rs); extension keys 1=ext-bid /
   2=ext-kvx / 3=ext-git (new core §5.1 Extension Keys registry). Spec: tasks §2.2
   ext-bid FULL (BidExt {1 bid-deadline, 2 queue-wait-ms}; needed a new
   `Offer.?6 => ExtMap` — additive), §6 reservations normative (Reserve/Lease/
   LeaseRelease CDDL + FSM Requested→Granted→Active→Released/Expired/Revoked +
   MUSTs: rsv-id idempotency, granted-window⊆requested, missed-lease=outcome-1 +
   squatting/PayRef Security), §8.4 ext-kvx & §9 ext-git STUBS (container
   {1 version,2 bstr} + zone rule + "deferred"). Code: map-tasks `Reserve`/`Lease`/
   `LeaseRelease` codecs (+`granted_window_within`); msg_type consts+name().
   Vectors: TV-013 PAY_AGREE (binds TV-010b envelope digest), TV-014 RESERVE,
   TV-015 LEASE, NV-008 out-of-window lease; conformance 34→38. **Renumbering
   (per check-first rule):** the package's planned TV-013 bound-quote & NV-007
   mismatch ALREADY existed as TV-010b/NV-006 (skipped); its TV-014/15/16→our
   TV-013/14/15, its NV-008 stays NV-008. TV-001..012/NV-001..007 frozen (70
   additive md lines, idempotent). Workspace green, zero warnings. Remaining
   READY packages: TASK-012 (libp2p), TASK-013 (real Ollama); TASK-014/016 need
   their deps; rev-01 doc cut now possible (ops §4, additive changelog).

9. ~~**TASK-012 — libp2p transport `/map/1.0.0`.**~~ DONE (2026-07-08). New crate
   `crates/map-net` (see §5 table). Real two-node loopback proven: TCP handshake +
   in-session echo + Kademlia publish/find/fetch/re-verify, and FR-3 dial-wrong-
   peer refusal — both gated net tests PASS. Core §10 gained the `/map/1.0.0`
   channel-binding ruling (FR-4: exporter = init-AID‖resp-AID; residual noted).
   **Deps decided (FR-1 allowed "latest 0.5x"):** libp2p **0.54.1** + libp2p-stream
   **0.2.0-alpha** (raw substreams — the umbrella crate has NO `stream` feature at
   any 0.5x; libp2p-stream ships only as alpha) + futures 0.3 + tokio 1.
   **Signature deviation:** `MapNode::new(seed, hello)` takes a 32-byte seed, not a
   `Keypair`, because map-identity deliberately doesn't export its secret; both the
   MAP and libp2p keypairs derive from that seed (FR-3). **Build now needs mingw on
   PATH** (see §5 CRITICAL note). NOT-done (deferred, in report): map-cli `node`
   subcommands; anti-relay-across-two-connections + poisoned-kad-record net tests;
   NAT/relay/DCUtR + anti-eclipse (explicitly out of scope, future package).

10. ~~**TASK-013 — real Ollama provider adapter.**~~ DONE (2026-07-08).
    `ollama-provider` gained `OllamaExecutor<B>` (any backend → `map_taskrun::Executor`)
    always-on, and `OllamaBackend` (HTTP `/api/generate` stream:false) behind feature
    `real-backend` (default OFF — `ureq` default-features-off + `serde_json` stay out
    of the default tree, verified `cargo tree`=0). FR-2 field map + FR-3 error classes
    + no-prompt-in-errors (enforced by a test). `map-cli serve-ollama` demo (same
    feature) prints output+root+dual-signed receipt. **Decisions:** enum
    `Unavailable`→`Unavailable(String)` (only def referenced it) to carry FR-10 elapsed;
    used `send_string`+manual serde_json rather than ureq's `json` feature (keeps tree
    minimal, no TLS); Executor is infallible so backend errors/invalid-UTF8 degrade to
    empty output (a fallible Executor to surface COMPLETE outcome=1 is a noted follow-up);
    chose the single-process InMemoryDuplex-free demo (TASK-012 merged, but a networked
    `serve` needs a separate client to show the full loop). Live test `#[ignore]`.
    Default + feature builds both zero-warning; full workspace green.

11. ~~**TASK-016 — federation gateway (running).**~~ DONE (2026-07-08) — **the last
    package; memory.md §8 roadmap is now COMPLETE.** `adapters/gateway` gained
    `GatewayProvider<B>` (a `map_taskrun::Executor` that fronts any `InferenceBackend`,
    composing `OllamaExecutor`) + `admit()` gate enforcing FR-5 scope (own EffectiveScope
    from a delegation chain) then FR-2/G-3 zone (tighten-only). `build_gateway_descriptor`
    keeps the subject = gateway (G-1/FR-4). 15 tests incl. FR-6 E2E (task→receipt naming
    the gateway) + zone matrix + FR-5 scope reject/admit + bad-chain. Discovery §4 spec
    patch: gateway/ params key 1 = gateway-of (FINAL). **Decisions:** gateway deps
    ollama-provider for the `InferenceBackend` trait (acyclic; a shared `map-backend`
    crate is the noted cleanup); reject codes use core §9's precise auth codes
    (2004/2005/2003) not FR-2's imprecise "3xxx"; CapEntry params aren't in the codec so
    provenance rides the GatewayAdvert wrapper (spec assigns the key for later). Whole
    workspace green (30 suites), zero warnings.

## 8. Roadmap after the scaffold is green (ops §8 MVP ordering)

1. **Session layer**: ~~HELLO/HELLO_ACK over an in-memory duplex first~~ DONE
   (TASK-005): `crates/map-session` handshake + state machine + seq/replay/size
   over `InMemoryDuplex`, 21 tests green. ~~REMAINING: libp2p `/map/1.0.0`
   binding.~~ DONE (TASK-012): `crates/map-net` — `Libp2pTransport` behind the
   `Transport` seam, PeerID≡AgentID (FR-3), channel binding = init‖resp AgentIDs
   (FR-4, core §10). Two-node loopback validated.
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
   refactored off its private codec). ~~REMAINING: real Kademlia replaces
   `MeshIndex`.~~ DONE (TASK-012): `map-net::MapNode` provides kad-backed
   `publish_descriptor`/`find_providers`/`fetch_descriptor` (libp2p-kad,
   MemoryStore). Disjoint-path lookups + anti-eclipse still future (out of
   TASK-012 scope).
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
5. **Payments last**: ~~`pay:credit-ledger` as the first settlement method.~~
   DONE (TASK-009): postpaid flat happy path (PAY_QUOTE→AGREE→INVOICE→PROOF→
   RECEIPT) + `LedgerOperator` in `map-pay`, flows in `map-taskrun::payments`,
   11 tests (incl. §11 security: forged entry, proof replay, digest switch,
   cap exceeded, task-binding). Metered terms-3 / escrow / disputes future.
6. Then: ~~DelegationCert implementation + envelope chain verification~~ DONE
   (TASK-010); ~~Ollama adapter for real~~ DONE (TASK-013); ~~extension code
   points (BID/KVX/GIT)~~ DONE (TASK-011); ~~formal models (ProVerif) of HELLO
   + channel binding + delegation attenuation~~ DONE (TASK-014, 2026-07-10 —
   see §7 step 12 below); ~~Go implementation (impl #2 for the two-impl rule,
   TASK-015)~~ DONE (2026-07-11 — see step 13 below); ~~federation gateway~~
   DONE (TASK-016). **The ops §8 roadmap is COMPLETE.**

13. ~~**TASK-015 — Go implementation (impl #2), ops §6.1.**~~ DONE
    (2026-07-11). New top-level `map-go/` Go module (go1.26.5; requires
    ≥1.24 for stdlib `crypto/hkdf` — ZERO external deps, x/crypto not even
    needed). Packages: `cbor` (hand-rolled RFC 8949 §4.2.1 deterministic
    encoder + strict decoder, fuzzed 60s/25M execs clean — the fuzzer DID
    catch and fix one pre-allocation panic on huge length heads), `identity`
    (AgentID + in-package base58 + `VerifyStrict`: canonical A/R/S +
    small-order blacklist, FR-4 tests incl. non-canonical y=p rejection),
    `envelope` (COSE_Sign1 Seal/Open over received bytes, delegation-chain
    validation), `hello` (TH/CB), `receipt`, `discovery`, `conformance`.
    Conformance parses `map-spec/test-vectors/map-test-vectors.md` at test
    time and PRODUCES all 24 frozen vectors (TV-001..015 + TV-010b +
    NV-001..008) byte-exact from seeds, consumes all positives, rejects all
    NVs with required classes (NV-007→2003 via full identity §4 chain
    validation). **Go is produce+consume on TV-007** (InputSpec key 2
    modeled), closing the ERRATA 2026-07-05 parity hole on the Go side —
    Rust remains consume-only there. Independence rule honored: written from
    drafts+vectors only, no map-rs source read (not even the registry
    mirrors). Ambiguity report (5 items, none blocking) in map-go/README.md;
    ops §8 status note updated ("impl #2 at vector parity — live pairwise
    interop matrix remains future; do not overclaim"). Toolchain: prepend
    `C:\Program Files\Go\bin` to PATH in each PowerShell command; no mingw
    needed for Go.

14. **ARCHITECT SESSION 2026-07-12 — PENDING-ARCHITECT queue fully
    resolved.** All 7 queued items + the 5 map-go ambiguities ruled;
    ERRATA.md rewritten as a RESOLVED record (read it for the rulings'
    rationale). Spec patches landed in the same change: tasks §2 Quote.2
    now OPTIONAL; trust §2 RECEIPT_ACK = full dual Receipt + 2006 on
    refusal; core §9 adds 2006 countersign-refused and 6005/6006/6007/6008
    pay codes; identity §3.2 B1/B3 + §4 kid clause corrected (kid = chain
    LEAF subject, root = accountable — wire already frozen by TV-012) and
    max-depth counting pinned (cert admits ≤ d certs leafward of it);
    discovery §6 shard rendered as unpadded decimal; core §6.1
    Hello-vs-HelloAck shape caution. KEY RULING: pay §3.1 envelope-byte
    digest binding STANDS (TV-013 froze it; Rust's content-binding is an
    interop BUG). Two implementation packages authored, NOT yet executed:
    **TASK-017** (P1: map-session raw-bytes seam — `send` returns sealed
    bytes, `Incoming.raw`; re-bind pay digests to envelope bytes; rewire
    RECEIPT_REJECT_CODE→2006 and split PAY_REJECT→6005-6008) and
    **TASK-018** (P2: InputSpec keys 2/3 + TV-007 Rust conformance guard;
    NEW vector NV-009 max-depth-exceeded chain, additive to
    gen_vectors.py, guards in BOTH impls). Still open in ERRATA: PayInvoice
    key 4 watermark CDDL (needs a metered-terms package).
    **TASK-017 + TASK-018 both DONE 2026-07-12 (see §5, §6).** The entire
    PENDING-ARCHITECT queue is now resolved AND implemented; only the
    PayInvoice-watermark item remains (deferred to a metered-terms package).

15. **ARCHITECT SESSION 2026-07-12 (second pass) — interop-matrix packages
    authored.** Line-level extraction of the Rust wire surfaces
    (map-session handshake/recv-order, hello negotiate rules, replay/seq
    constants, framing, exporter construction) distilled INTO the packages
    so the Go implementer never reads Rust (independence rule).
    **TASK-019 DONE 2026-07-13**: `map-go/session` — Hello/HelloAck codecs
    (list-vs-singleton shapes, ACK key 4 = responder's OWN max), Negotiate
    (highest mutual version, first mutual suite in INITIATOR order, ext
    min-version intersection), ReplayCache (±120 s, 10-min window),
    SeqTracker (dense from 0), 4-byte-BE TCP framing with size-first reads,
    exporter = init-AID||resp-AID (68 B), full FR-5 receive order, handshake
    ERROR msg-id 0xEE×16, session-id echo + per-frame 1006 check. envelope
    Body extended with in-session keys 4/5/6/7 + SealBody/SealBodyDelegated
    (buildBody unchanged → frozen 25 vectors still byte-exact). identity
    gained AgentID.Bytes(). 9 tests: TCP + in-mem loopback, PING auto-PONG,
    CLOSE, replay/seq-gap/tamper/wrong-sid/oversize/version-disjoint/
    delegated-handshake (root resolves to RIK). Zero deps, vet+gofmt clean.
    **TASK-020** (BLOCKED on 019, P1):
    the live matrix — 8 scenarios (echo/replay/seq-gap/tamper/cross-session
    relay/version-disjoint/wrong-sid/delegated) × 2 orientations over TCP,
    peer binaries (`map-cli interop`, `map-go/cmd/map-interop`), PowerShell
    driver, `interop/MATRIX.md` as the ops §6.1 record. **FINDING pinned in
    both packages: Rust `Session::recv` never enforces the core §6.1
    per-frame session-id check (1006) — Go implements it in 019 (DONE), Rust
    fixed in 020 FR-2.** **TASK-020 DONE 2026-07-13: the live matrix is
    16/16 PASS (see §5).** on a 16/16 matrix the only remaining spec action
    is the rev-01 editorial cut.

16. **TASK-020 DONE 2026-07-13 — LIVE Rust↔Go interop matrix, 16/16 PASS.**
    The ops §6.1 gate is MET. Added: Rust `TcpTransport` (map-session,
    std::net, 4-byte-BE framing, identity-pair exporter) + `TransportError::
    TooLarge`; the FR-2 **session-id 1006 fix** in `Session::recv` (was
    silently skipped post-handshake — Go had it first) + its unit test;
    `Session::send_raw` (harness hook); `map-cli interop` subcommand
    (crates/map-cli/src/interop.rs; map-cli gained map-core + map-session
    deps) and `map-go/cmd/map-interop` + Go `Session.SendRaw/ChannelBinding/
    SessionID` accessors + `identity.AgentID.Bytes()`. 8 scenarios (echo,
    replay, seqgap, tamper, relay=cross-session anti-relay, versiondisjoint,
    wrongsid, delegated) × 2 orientations, over framed TCP loopback. Driver
    `interop/run-matrix.ps1` (builds both, runs 16 cells, exits nonzero on
    any fail); record `interop/MATRIX.md`; `interop/README.md`. ops §6.1
    status note updated to "live matrix delivered 16/16". Scenario seeds:
    A=01×32 (dialer/root), B=02×32 (listener), DK=03×32 (delegated leaf).
    Whole Rust workspace + map-go green, zero warnings. NEXT: rev-01
    editorial cut (fold ERRATA resolutions into normative text) — the last
    spec action before leaving -00.

12. ~~**TASK-014 — formal analysis (P1/P2/P3), ops §6.3.**~~ DONE (2026-07-10).
    New top-level `formal/` (spec-adjacent, not Rust): ProVerif 2.05 models.
    `hello-cb.pv` — Q1 (P1 downgrade resistance, core §6.2) **true**; Q2 (P2
    channel-binding session separation, core §7.3) **true**; Q3 reachability
    confirmed. `delegation.pv` — Q4 (P3 tighten-only attenuation, identity §4
    / TASK-010 `validate_chain`, incl. fail-closed caveats) **true**; Q5
    reachability confirmed. **No BLOCKING finding.** Every property has a
    mandatory FR-4 negative control that correctly FAILS with a witnessing
    trace: `hello-cb-control-downgrade.pv` (drops the initiator's
    negotiate-recheck → downgrade to v1 becomes reachable via a sid/msgid-
    reuse attack through the leaked insider agent), `hello-cb-control-relay.pv`
    (drops TH from the CB derivation → CB collides across distinguishable
    sessions), `delegation-control.pv` (drops the tighten-only intersection
    check → an arbitrary/widened scope gets accepted). ops §6.3 updated from
    "plan" to "delivered for P1–P3" with results.
    **Toolchain note (significant time sink, now resolved):** native-Windows
    `opam install proverif` fails (`lablgtk` GTK2 GUI is a HARD dep, doesn't
    compile against the installed mingw gcc); a from-source CLI-only
    (`-nointeract`) build via `ocamlbuild` also fails (native-Windows-opam +
    msys `mkdir` incompatibility, unresolved). **ProVerif works ONLY via
    WSL2** (Ubuntu): `sudo apt-get install -y opam m4 unzip && opam init -y
    --disable-sandboxing && eval $(opam env) && opam install proverif -y`.
    Also hit and fixed: WSL2's network adapter was stuck down post-reboot
    (Cloudflare WARP was blocking WSL2 traffic — disabling WARP + a full
    Windows reboot fixed it, `wsl --shutdown` alone was NOT enough).
    **Non-trivial modeling lessons (see formal/README.md "Modeling notes"
    for the full account — read before extending these models):** (a) `reduc`
    patterns can't match `free` names directly, only constructor structure —
    use nested `if-then-else` as a TERM instead; (b) multi-clause `reduc`
    needs `otherwise`, and the function needs a `fun NAME(...): type` line
    immediately before it with NO trailing period (combined declaration);
    (c) correspondence queries MUST be scoped to the honest principals (top-
    level `[private]` free names referenced directly in the query) — an
    unscoped query lets the Dolev-Yao attacker "attack" by just running a
    fully self-consistent session/chain using its own leaked key, which is
    not a real violation; (d) **the deepest lesson**: an event-correspondence
    over SIGNED-message events is the wrong tool for "does derived value X
    (like CB) separate sessions" — it collapses to plain signature
    unforgeability once the value is embedded in a signed blob, and the
    injective variant gets contaminated by the (correctly out-of-scope)
    msg-id replay-cache gap. A direct `table`-based collision/uniqueness
    check on the derived value itself (`get TBL(...) suchthat ... in ...
    else insert TBL(...)`) is the right tool — this is what Q2 uses.

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
