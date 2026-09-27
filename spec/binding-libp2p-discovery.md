# MAP discovery over libp2p Kademlia, version 2.0-draft

**Status:** Draft. Normative for this binding. Defines how the two-plane
discovery model (layer spec, *Descriptors and discovery binding*) sits on
libp2p Kademlia. Written against v1 `map-v1-ietf:map-spec/draft-map-discovery-01.md`
§5-§8, `map-v1-ietf:map-spec/draft-map-core-01.md` §10, and the current
`map-rs/crates/map-net/src/node.rs` implementation, which this binding
tracks rather than contradicts.

BCP 14 (RFC 2119, RFC 8174) keywords apply as in the other binding
documents.

## 1. Scope

DHT-1: This binding covers **discovery and status traffic only**. The work
plane — task exchange, evidence, payment — runs over A2A/HTTPS
(`binding-a2a.md`); nothing here carries task content. This mirrors ADR-0001
("libp2p: Discovery plane only") and v1 discovery §1's stable/volatile
split, kept unchanged in substance.

## 2. Record layout

DHT-2: Two libp2p Kademlia mechanisms carry the two kinds of stable-plane
data, matching `map-net`'s current `MapNode` behavior (`node.rs`):

- **Provider records** (`StartProviding`/`GetProviders`) advertise *who*
  offers a capability, keyed by the sharded capability key
  `K'(cap, s) = multihash_sha256("map:idx:1:" || CapURI || ":" || s)`
  (discovery §6's shard preimage, unchanged: `s` is unpadded base-10
  ASCII). `MapNode::publish_descriptor` calls `StartProviding` once per
  `(capability, own shard)` pair, capped at 32 keys per agent (discovery
  §5).
- **Value records** (`PutRecord`/`GetRecord`) carry the descriptor envelope
  itself, keyed by `desc_record_key(cid) = multihash_sha256("map:desc:" ||
  cid)` where `cid = multihash_sha256(envelope-bytes)` — i.e. content
  addressed by the signed envelope's own digest, per discovery §5. Kad
  provider lookups return `AgentId`s (recovered from each providing
  `PeerId` per `agent_id_from_peer`, which requires the peer be
  identity-hashed so the Ed25519 key is embedded — `node.rs` §"Recover an
  AgentId"); the querier then fetches the actual descriptor with
  `GetRecord` at `desc_record_key` and verifies it (DHT-6).

DHT-3: `PeerId` MUST equal `AgentID` (v1 core §10's libp2p binding
requirement, `MapNode::new`'s FR-3 assertion, enforced both at construction
and per-dial via `DialOpts::peer_id`). A `PeerId` that does not
identity-hash to a recoverable Ed25519 key MUST be treated as having no
AgentID and dropped from provider-lookup results.

## 3. TTL and republish

DHT-4: Unchanged from v1 discovery §5: descriptor validity ≤ 24 h
(`Descriptor` key 13, layer spec's descriptor type); provider/value records
carry `ttl ≤ 6 h`; agents republish every 1-6 h. These are the same numbers
because nothing about moving the task protocol to A2A changes the cost
model that set them (discovery §9's scalability analysis: ~278 puts/s per
million agents mesh-wide, absorbed by the DHT).

## 4. ADS compatibility note

DHT-5: AGNTCY's Agent Directory Service (`draft-mp-agntcy-ads-02`, 6 July
2026) uses the same *shape* — a Kademlia-indexed capability key resolving to
a content-addressed record (`"natural_language_processing" → [CID, CID,
...]`, CIDs computed as content digests of a signed record artifact) — but
differs from this binding in three specific ways, stated precisely rather
than as a vague "compatible" claim:

1. **No sharding.** ADS's skill key is a single global Kademlia key per
   taxonomy path; this binding shards every capability key 256 ways
   (§2, discovery §6). A plain ADS client querying the unsharded key
   `multihash("map:idx:1:" || CapURI)` (i.e. `K(cap)` without a shard
   suffix, discovery §5) will find nothing under this binding — it MUST
   additionally sample shards (discovery §6) to see providers. This
   binding is therefore *layout-compatible* (same two-level
   capability→pointer→record structure, same use of Kademlia) but not
   *query-compatible* without shard awareness.
2. **Second-level indirection differs.** ADS's CID resolves to a *registry
   endpoint* (a second network hop to an external registry service) rather
   than to the record itself. This binding's `desc_record_key` stores the
   signed descriptor envelope directly as the Kademlia record value — one
   hop, no external registry.
3. **No volatile plane.** ADS's own text gives no status/liveness
   mechanism; it is a stable-record index only. This binding adds
   `StatusQuery`/`StatusReport` (§5) with no ADS analog.

DHT-6: In both systems, correctness rests on the client independently
verifying the fetched record's signature — the DHT is untrusted storage in
both designs (discovery §5's "data ownership" rule: DHT records are
derived, rebuildable, never load-bearing for correctness).

## 5. STATUS_QUERY transport

DHT-7 (decision): this binding defines a **new** stream protocol ID,
`/map/status/2.0.0`, rather than reusing `/map/1.0.0`. Justification: v1's
`/map/1.0.0` multiplexed the full session state machine (HELLO/HELLO_ACK,
the whole task protocol) over one substream type; that entire task
protocol is now A2A's job (`binding-a2a.md`), so the only thing left
running natively over libp2p is the lightweight, single-round-trip,
out-of-session `StatusQuery`/`StatusReport` exchange (discovery §3).
Reusing `/map/1.0.0` for that alone would force a status-only peer to
implement session negotiation it will never use, and would force any
future evolution of the (now much smaller) session/negotiation surface to
version-lock against status-query evolution. A dedicated, narrowly-scoped
protocol ID versions independently.

DHT-8: `/map/status/2.0.0` carries exactly one exchange per substream:
requester opens the stream, writes a length-prefixed `StatusQuery`
(discovery §3 CDDL, unchanged), reads one length-prefixed `StatusReport`,
closes. No session, no HELLO. Framing: 4-byte big-endian length prefix,
then message bytes (core §10's framing rule, unchanged).

## 6. Rate limiting

DHT-9 (REQUIRED): every agent serving `/map/status/2.0.0` MUST apply a
per-peer token-bucket rate limit to inbound `StatusQuery`s, independent of
and in addition to any libp2p-level connection limits. RECOMMENDED starting
parameters: bucket capacity 10, refill 1/s per peer (discovery §3's
existing guidance — queriers SHOULD query ≤10 candidates per routing
decision — sets the expected legitimate load this must comfortably clear).
A peer exceeding its bucket MUST have further `StatusQuery`s on that
substream protocol dropped (stream reset), not answered with a slow or
empty `StatusReport` — a silent drop is cheaper to enforce and does not
leak whether the limit or a real failure is the cause.

## 7. Querier-side shard and signature checks (restated as binding requirements)

DHT-10: A querier using this binding MUST discard any provider record
recovered under shard `s` whose own `shard(agent)` (discovery §6's
definition: first byte of the *digest* inside `multihash(agent)`, not the
multihash's constant prefix) does not equal `s`. This is a restatement of
discovery §6's existing rule as a hard requirement of this binding, because
libp2p's Kademlia implementation does not itself know about MAP's shard
semantics and cannot enforce it — the check is only meaningful at the MAP
layer, after `GetProviders` returns.

DHT-11: A querier MUST verify a fetched descriptor's signature (and, per
`binding-a2a.md` §11, the embedded Agent Card's JWS) and expiry before
using or caching it. An unverified or expired record MUST be dropped, never
cached, and never surfaced as a discovery result (discovery §2's validation
rule, restated here as a binding-level MUST since it is this binding's
`GetRecord` path that returns the raw, unverified bytes).

## 8. Anti-eclipse and anti-grinding

DHT-12: Identity and descriptor fetches SHOULD use disjoint-path Kademlia
lookups (d = 3 disjoint paths). This is layer D-9. Evaluation E3c
supports it: at N = 10⁵ with 30% malicious routers, lookup success rises
from 0.755 to 0.930, for 1.3–1.7× the messages. This binding does not
mandate a particular libp2p Kademlia configuration for achieving
disjointness. `map-net`'s current `kad::Behaviour<MemoryStore>` uses the
library's default query logic, which exposes no disjoint-path control, so
the reference implementation does not meet DHT-12 yet. That is known
future work.

DHT-13: The epoch-salted shard variant (`shard_e = SHA-256(epoch_salt ||
AgentID)[0]`, layer D-8) is NOT RECOMMENDED as a defence against
node-ID grinding. Evaluation E3b found that it gives no protection
(layer §11.5). `map-net` does not implement it. A conformant querier MUST
NOT assume a mesh uses it unless the mesh negotiates it separately.

## 9. Channel-binding caveat

DHT-14: The channel binding derived for `/map/status/2.0.0` (or any future
substream this binding defines) covers **only discovery and status
traffic**. It provides no guarantee about, and is entirely independent of,
whatever channel binding (or lack of one) protects a subsequent A2A/HTTPS
work-plane session with the same peer (`binding-a2a.md`). A requester MUST
NOT treat a successfully channel-bound `/map/status/2.0.0` exchange as
authenticating the later A2A connection — the two planes are separate
transports with separate trust establishment, by design (§1). This is the
same caveat v1 core §14 already states for the libp2p exporter's
identity-pair-not-connection-instance weakness, narrowed here to make
explicit that it does not "carry over" to the work plane at all, because
under this rework there is no session spanning both planes for it to carry
over into.

## References

`map-v1-ietf:map-spec/draft-map-discovery-01.md` §5-§9 (v1, informative — the
sharding, TTL, and validation rules this binding restates or narrows are
defined there in full). `map-v1-ietf:map-spec/draft-map-core-01.md` §10 (v1 libp2p
transport binding, informative). `map-rs/crates/map-net/src/node.rs`
(current implementation, the source of truth for DHT-2/DHT-3's described
behavior). AGNTCY Agent Directory Service, `draft-mp-agntcy-ads-02`, IETF
Independent Submission, 6 July 2026,
https://datatracker.ietf.org/doc/draft-mp-agntcy-ads/02/. [RFC2119]
[RFC8174] BCP 14.
