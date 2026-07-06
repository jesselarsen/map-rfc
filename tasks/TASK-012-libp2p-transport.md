# TASK-012 — libp2p Transport Binding (`/map/1.0.0`)

Status: READY (after TASK-006) · Priority: P2 · Target: new crate `map-rs/crates/map-net`
Depends on: TASK-005, TASK-006 · Blocks: real-network interop; TASK-015 cross-impl testing

---

## 1. Problem Statement

Everything runs over `InMemoryDuplex`. The protocol's deployment target
is libp2p (core §10, arch): QUIC/TCP+Noise transports, protocol id
`/map/1.0.0`, PeerID ≡ AgentID (identity §3.2), channel binding from
real transport keying material. This task adds the first network
transport behind the existing `Transport` trait, plus Kademlia-backed
descriptor publication replacing `MeshIndex` behind its API.

## 2. Functional Requirements

- FR-1 **Dependency approval (architect-granted, exact):**
  `libp2p = "0.53"` (or the latest 0.5x at implementation time — record
  the chosen pin in memory.md) with features
  `["tokio","tcp","quic","noise","yamux","kad","identify","dns"]`, plus
  `tokio = "1"` with `["rt-multi-thread","macros","sync","time"]`.
  These are the ONLY new dependencies. All async stays INSIDE map-net;
  the public API is blocking (channel-bridged), so no other crate
  changes its programming model.
- FR-2 `Libp2pTransport implements map_session::Transport`: one MAP
  session per libp2p stream on protocol `/map/1.0.0`; frames are
  length-prefixed (u32 BE, max = negotiated size; the duplex's implicit
  framing note in TASK-005 §13 becomes explicit here).
- FR-3 Identity binding: the libp2p keypair IS the MAP keypair
  (ed25519); assert PeerID bytes == AgentID multihash identity per
  identity §3.2 B1 at startup; refuse to run otherwise.
- FR-4 Channel binding: `Transport::exporter()` returns keying material
  derived from the Noise session. libp2p does not expose a TLS-exporter
  equivalent on Noise streams portably — ARCHITECT RULING per core §7.3
  third clause ("transports lacking an exporter"): exporter =
  `initiator-AgentID || responder-AgentID` AS AUTHENTICATED BY libp2p
  (i.e., the PeerIDs the connection authenticated). Record this as the
  normative `/map/1.0.0` binding in a new core §10 paragraph (spec
  patch, same change). If the chosen libp2p version DOES expose Noise
  handshake-hash material, use it instead and record that.
- FR-5 Node runtime `MapNode`: start/listen/dial by multiaddr;
  accept inbound streams → `Session::respond`; `dial(addr, expected
  AgentID) -> Session`; Kademlia provider records under sharded
  capability keys (`provide`/`providers` mapping MeshIndex's API onto
  kad), descriptor envelopes as kad records at
  `H("map:desc:" || descriptor-cid)` (discovery §5); republish ≤6 h
  (tokio timer inside map-net only).
- FR-6 Two-node loopback integration test (127.0.0.1, OS-assigned
  ports): handshake, echo task (reuse map-taskrun), receipt dual-sign,
  CAP_QUERY via kad-backed lookup. Gate network tests behind
  `#[ignore]` + a `net-tests` cargo feature so CI without network
  stays green; document how to run them.

## 3. Non-Functional Requirements

Zero warnings incl. under the feature; no async types in the public
API; graceful shutdown (drop closes streams). NAT traversal (AutoNAT/
relay/DCUtR), discovery privacy, and anti-eclipse defenses are OUT of
scope (future package; discovery §7–§9).

## 4. System Architecture

`map-net` deps: identity, core, session, discovery, mesh (for the index
API it implements) + libp2p/tokio. Nothing else may depend on libp2p —
enforce by keeping map-net a leaf that only the demo/CLI uses.
`map-cli` gains `node` subcommands (`listen`, `dial`, `publish`,
`query`) behind the same feature for manual testing.

## 5. Component Design

```
crates/map-net/src/
  transport.rs  Libp2pTransport (blocking bridge over tokio mpsc)
  node.rs       MapNode (swarm task, kad, stream acceptor)
  framing.rs    u32-BE length prefix codec
  lib.rs
```

## 6. Interface Specifications

`MapNode::new(keypair) -> Result<MapNode>` (asserts FR-3);
`listen(multiaddr) -> ListenerId`; `dial(addr, peer: AgentId) ->
Result<Session<Libp2pTransport, SystemClock, RandomMsgIds>>`;
`incoming() -> Receiver<Session<...>>`; `publish_descriptor(desc)`;
`find_providers(cap) -> Vec<AgentId>`; `fetch_descriptor(cid) ->
Option<Vec<u8>>`. All blocking, all Send.

## 7. Data Structures

Kad record key layout per discovery §5/§6 (sharded capability key for
provider records; `H("map:desc:"||cid)` for descriptor bytes). Values
are the SIGNED envelope bytes — consumers re-verify (untrusted
transport, same rule as MeshIndex).

## 8. Algorithms

Framing read: length, bounds-check against session max BEFORE reading
body (core §6.3 size-first rule). Republish loop: 5.5 h interval.
Dial-verify: after Noise, compare authenticated PeerID to `expected`;
mismatch → close + PeerMismatch (2000 semantics).

## 9. Edge Cases

Dial timeout; peer closes mid-handshake; oversize length prefix
(reject before allocation); kad lookup empty; concurrent inbound
sessions from the same peer (allowed — sessions are independent);
node restart re-publishes (derived-data rebuild property).

## 10. Error Handling

`NetError` wraps transport/dial/kad errors; session-level errors stay
`SessionError`. No panics on network input, ever (fuzz the framing
reader with the seeded-LCG pattern).

## 11. Security Considerations

FR-3 identity binding and FR-4 exporter ruling are the critical items;
the anti-relay test from TASK-005 MUST be re-run across two REAL
connections (record→replay across sockets fails). Kad records: verify
everything, trust nothing (test: poisoned provider record for an agent
that never signed a descriptor yields no cached descriptor). DoS: the
oversize-prefix rejection test. Document loudly: no NAT traversal, no
eclipse defenses yet — LAN/loopback grade only.

## 12. Performance Considerations

Not a goal; one tokio runtime inside MapNode; measure nothing yet.

## 13. Input Specifications

Multiaddrs, keypairs; network I/O via libp2p only.

## 14. Output Specifications

Blocking API per §6; `map-cli node` subcommands print human-readable
status lines.

## 15. Acceptance Criteria

- ✓ Workspace green (default features, no network) zero warnings; and
  green with `--features net-tests -- --ignored` locally.
- ✓ FR-6 loopback test passes end-to-end (handshake→echo→receipt→kad
  CAP query).
- ✓ Anti-relay-across-real-connections test passes.
- ✓ PeerID/AgentID assertion test (mismatched key refused).
- ✓ Poisoned-kad-record test passes.
- ✓ core §10 spec patch recording the `/map/1.0.0` binding + exporter
  ruling landed in the same change; libp2p pin recorded in memory.md.
- ✓ No crate outside map-net/map-cli references libp2p/tokio.

## 16. Comprehensive Test Plan

Unit: framing codec (roundtrip, oversize, truncated, fuzz); FR-3
assertion. Integration (feature-gated): FR-6; anti-relay; poisoned
record; dial-wrong-peer; restart-republish. Concurrency: two
simultaneous sessions A↔B, both complete echo tasks.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, create workspace member `crates/map-net` per §4–§14.
> The ONLY new dependencies allowed are those in §FR-1 (record the
> exact libp2p pin you use in memory.md §5). Public API is blocking;
> async confined to map-net internals. PeerID must equal AgentID
> (identity §3.2) — assert at construction. Implement the §FR-4
> exporter ruling and patch `map-spec/draft-map-core-00.md` §10 in the
> same change (if your libp2p version exposes Noise handshake-hash
> material, prefer it and record which). Network tests behind a
> `net-tests` feature + `#[ignore]`. Re-run the TASK-005 anti-relay
> property across two real connections. `cargo
> +stable-x86_64-pc-windows-gnu` from PowerShell (expect longer builds;
> if the GNU toolchain fights libp2p's ring/quic deps, report the exact
> error before attempting toolchain changes — do NOT install MSVC
> silently). Deliver: code, tests, spec patch, report.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo +stable-x86_64-pc-windows-gnu test --workspace
cargo +stable-x86_64-pc-windows-gnu test -p map-net --features net-tests -- --ignored
cargo tree -i libp2p | head          # only map-net, map-cli paths
git diff map-spec/draft-map-core-00.md   # §10 binding paragraph
```

## 19. Known Risks

R1 (HIGH): libp2p on windows-gnu — ring/aws-lc-sys build friction is
plausible; the report-first rule in §17 exists for this; fallback is
TCP+noise-only feature set (drop quic) — acceptable degradation, record
it. R2: exporter ruling weaker than a true exporter (binds identities,
not the specific connection) — acceptable because msg-id replay + seq
still prevent cross-connection replay within the window; note this
residual in the core §10 patch text. R3: version churn in libp2p APIs —
pin exactly, record.

## 20. Handoff Notes

Update memory.md §5/§8 (roadmap step 1-remaining + 2 fully done).
Future: NAT profile (AutoNAT v2, relay v2, DCUtR), discovery privacy
options, anti-eclipse (disjoint-path lookups) — one package, after
cross-network experience. TASK-015 (Go) interops against this node.
