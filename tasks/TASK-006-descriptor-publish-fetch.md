# TASK-006 — Descriptor Publish/Fetch over Sessions

Status: READY · Priority: P1 · Target: extend `map-rs/crates/map-discovery` + new integration tests
Depends on: TASK-005 · Blocks: TASK-007 (echo task discovers its provider)

---

## 1. Problem Statement

Discovery exists only as pure functions (cap URIs, sharding, a seq-LWW
cache). Nothing publishes a signed descriptor, answers a CAP_QUERY, or
serves STATUS over a live session. This task implements the discovery
message flows (discovery §2–§5) over `map-session`, with an in-process
`MeshIndex` standing in for the Kademlia DHT (the real DHT arrives with
libp2p, TASK-012). Prep item folded in: give map-core a first-class
`HelloAck` type (core §6.1 singleton shape) and switch map-session to it,
deleting its hand-rolled codec (TASK-005 feedback item (a)).

## 2. Functional Requirements

- FR-1 (prep, map-core): `hello::HelloAck { version: u16, suite: u16,
  extensions: Vec<(u64,u64)>, max_message_size: u32, max_tasks:
  Option<u64> }` with to_value/from_value per core §6.1 CDDL (keys 1,2,3,
  4,?5; key 8 transcript-hash optional, ignored on read for now);
  `negotiate` gains a `Negotiated::to_ack()` or equivalent; map-session
  drops its private codec. TV-003 bytes MUST still reproduce (conformance
  suite is the guard).
- FR-2 Descriptor envelopes: `publish_descriptor(kp, descriptor) ->
  Vec<u8>` produces an out-of-session envelope (msg-type 0x0100, empty
  external_aad, ttl-ms REQUIRED per core §7.4, ttl ≤ validity ≤ 24 h);
  `ingest_descriptor_envelope(bytes, cache, now_ms)` verifies signature,
  checks subject == envelope kid (a descriptor is self-signed — mismatch
  is invalid), enforces staleness via seq-LWW `DescriptorCache`.
- FR-3 `MeshIndex` (in-process DHT stand-in): stores
  (sharded-capability-key → set of provider AgentIDs) and
  (descriptor-cid → descriptor envelope bytes). API:
  `provide(cap, shard, agent)`, `providers(cap, shard) -> Vec<AgentId>`,
  `put_desc(cid, bytes)`, `get_desc(cid)`. Index entries are DERIVED data
  (discovery data-ownership rule): a `rebuild()` from a set of descriptor
  envelopes must reproduce the index exactly — test required.
- FR-4 CAP_QUERY/CAP_RESPONSE (0x0103/0x0104) over a session: querier
  sends {1: CapURI, ?2: shard, ?3: limit}; responder answers {1:
  [* descriptor-envelope bstr], ?2: bool more}. Responder serves from its
  MeshIndex; querier verifies EVERY returned envelope independently
  (index is untrusted transport) and ingests into its own cache.
- FR-5 STATUS_QUERY/STATUS_REPORT (0x0101/0x0102) over a session:
  query {1: CapURI, ?2: bstr .size 8 echo}; report per discovery §3
  StatusReport {1 timestamp, ?2 queue-length, ?3 est-ttft-ms, ?4
  accepting 0/1/2, ?6 echo}. Volatile: MUST NOT enter DescriptorCache or
  MeshIndex (two-plane rule, decision 3).
- FR-6 Republish policy object: `needs_republish(last_published_ms,
  now_ms) -> bool` (interval 6 h max per discovery §5), pure function.

## 3. Non-Functional Requirements

Pinned deps only; no network; zero warnings; `MeshIndex` is std
collections. Message shapes: where discovery §2–§5 does not pin exact
content-map field numbers for CAP_QUERY/CAP_RESPONSE/STATUS_QUERY, the
numbers in FR-4/FR-5 ARE the assignment (architect-approved); the
implementer MUST also patch `map-spec/draft-map-discovery-01.md` §3/§5
to record them (spec-feedback same-change rule) and note it in the
report.

## 4. System Architecture

`map-discovery` gains a `flows` module depending on `map-session` — wait:
dependency direction. `map-session` must not depend on discovery, and
discovery depending on session inverts the doc graph. DECISION: new
module lives in a new crate `crates/map-mesh` (deps: identity, core,
session, discovery) so both existing crates stay pure. `MeshIndex`,
publish/ingest, and the query/status responders live in `map-mesh`.

## 5. Component Design

```
crates/map-mesh/src/
  index.rs      MeshIndex (+ rebuild)
  publish.rs    publish_descriptor / ingest_descriptor_envelope
  flows.rs      serve_one(session, index, ...) — single-message service
                loop step; query_caps(session, cap) / query_status(...)
                client calls
  lib.rs
```
`serve_one` processes exactly one Incoming and replies (testable without
threads); a `serve_until_close` loop wraps it.

## 6. Interface Specifications

All functions return `Result<_, MeshError>`; `MeshError` wraps
Session/Envelope/Discovery errors plus `InvalidDescriptor(reason)`.
`ingest_descriptor_envelope` postcondition: cache and index both updated
or neither (no partial ingest). Verification uses received bytes (I1);
descriptor-cid = `multihash_sha256(envelope-bytes)` (discovery §5).

## 7. Data Structures

Content maps per FR-4/FR-5 exactly. Descriptor: reuse the scaffold's
minimal `Descriptor` (fields 1–6 + 12); optional fields 7–11/13/14 remain
out of scope.

## 8. Algorithms

Shard = `map_discovery::shard(agent)` (digest first byte). Query path:
exact-key lookup first, then `cap_matches` prefix aggregation client-side.
Rebuild: fold `ingest_descriptor_envelope` over stored envelopes ordered
by (agent, seq).

## 9. Edge Cases

Descriptor from kid ≠ subject → InvalidDescriptor + NOT cached. Stale seq
→ ingest returns Stale, index unchanged. CAP_RESPONSE containing one bad
envelope among good ones → good ones ingested, bad one reported, no
abort. ttl expired at ingest time (`now_ms > timestamp + ttl`) → reject
(stale). Empty providers → empty CAP_RESPONSE, not an error. STATUS echo
absent → report omits key 6.

## 10. Error Handling

Wire errors during serve: reply ERROR with the mapped core §9 code and
continue serving (a bad query must not kill the responder loop).

## 11. Security Considerations

Index is untrusted transport — client-side verification of every envelope
is the load-bearing check; a test MUST show a forged index entry cannot
produce a cached descriptor. Volatile/stable plane separation (FR-5) is
normative (decision 3). Self-signature rule (FR-2) prevents descriptor
forgery for third parties.

## 12. Performance Considerations

In-process maps; nothing to optimize. Republish timer is a pure function;
no background threads in this package.

## 13. Input Specifications

Library; inputs are sessions, envelopes, clocks (injected `now_ms` —
NO wall-clock reads).

## 14. Output Specifications

Library API per §5; no I/O beyond the session transport.

## 15. Acceptance Criteria

- ✓ Workspace green, zero warnings, no new external deps; `map-mesh`
  added to workspace members.
- ✓ HelloAck refactor done; conformance suite (incl. TV-003) still green;
  map-session's private codec deleted.
- ✓ End-to-end test: B publishes → index → A CAP_QUERYs over a live
  session → A's cache holds B's descriptor, verified.
- ✓ Rebuild test: index rebuilt from envelopes == incrementally built.
- ✓ Forged-index-entry test per §11 passes.
- ✓ discovery §3/§5 patched with the assigned content-map field numbers.

## 16. Comprehensive Test Plan

Unit: publish/ingest roundtrip; kid≠subject reject; stale seq; ttl
expiry; needs_republish boundary (5h59m no, 6h01m yes); index
provide/providers; rebuild equality.
Integration (two threads, fixed clocks): CAP_QUERY happy path; mixed
good/bad CAP_RESPONSE; STATUS query/report with echo RTT computable;
volatile data provably absent from cache after STATUS.
Security: forged index entry; descriptor replayed after newer seq seen.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, (1) add `hello::HelloAck` to map-core per §FR-1 and
> refactor map-session to use it — conformance tests are the regression
> guard, TV-003 bytes must not change; (2) create workspace member
> `crates/map-mesh` implementing §5–§14. Do not add dependencies; do not
> make map-session or map-discovery depend on each other. The content-map
> field numbers in FR-4/FR-5 are the approved registry assignments — also
> patch `map-spec/draft-map-discovery-01.md` §3/§5 to record them in the
> same change. Injected clocks only in tests. Build with
> `cargo +stable-x86_64-pc-windows-gnu` from PowerShell. Full workspace
> suite after every fix. Deliver: code, tests per §16, spec patch, report
> (fixes, ambiguities with quotes + proposed wording).

## 18. Validation Checklist (reviewer)

```
cd map-rs
cargo +stable-x86_64-pc-windows-gnu test --workspace       # green
cargo test -p map-conformance                              # TV-003 unchanged
cargo tree -p map-mesh | grep -E "map-(tasks|trust|pay)"   # no hits
git diff map-spec/draft-map-discovery-01.md                # field numbers recorded
```

## 19. Known Risks

R1: HelloAck refactor could shift TV-003 bytes if the singleton codec
differs subtly — conformance is the tripwire; if it fires, the CODE is
wrong, not the vector. R2: CAP_RESPONSE size can exceed
max_message_size with many descriptors — cap the batch (limit default 8)
and set `more`.

## 20. Handoff Notes

Update memory.md §5 (map-mesh row) and §8 (step 2 done). TASK-007 builds
directly on `map-mesh` for provider discovery. The DHT semantics
(provider records, disjoint-path lookups, anti-eclipse) stay future —
TASK-012 replaces `MeshIndex` behind the same API.
