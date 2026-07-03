# Critical Evaluation of the Attached Design Documents

Documents reviewed: `design1.txt` (capability-mesh concept), `design2.txt`
(three-layer starter kit), `design3.txt` (Hermes Protocol blueprint + MVP
plan), `design4.txt` / `rfc_preprint.txt` (HEP v0.1 preprint),
`design_goals.txt` and `design_prompt.txt` (requirements).

Verdict up front: the *architecture* is sound and genuinely novel in framing —
routing on a capability graph rather than a file or identity graph is the
right evolution of the MP2P idea. The *specifications*, however, contain
defects that would be fatal in standards review: a self-referential signature
definition, self-asserted trust scores, volatile data in the DHT, no replay
protection, normative "magic number" policy, and a serialization choice that
cannot support the signature semantics the documents rely on. Each finding
below carries a disposition into the unified MAP specification family.

---

## 1. Critical findings (cross-cutting)

### C1. Self-asserted trust scores — unverifiable by construction (SEVERITY: CRITICAL)
`NodeAdvertisement.trust: TrustScore` (preprint §4.1) and
`reputation_score` (design3) place a node's trust score, hallucination rate,
and tasks-completed count *inside the node's own signed advertisement*. A
signature by the subject proves only that the subject said it. Any rational
attacker advertises `trust.overall = 1.0`. The routing formula then weights
this self-claim at 0.35 — the largest single factor — so the preprint's
routing layer is steered primarily by an attacker-controlled input.

Reputation is only meaningful when derived from evidence signed by
*counterparties*. **Disposition:** descriptors in MAP carry no trust fields
at all; trust is computed locally by each consumer from dual-signed
`ExecutionReceipt`s (draft-map-trust-00). This also resolves the "who
computes the rolling average" ambiguity in design2 §3.3, where the node
updates its own reputation after each task.

### C2. Signature definition is self-referential and non-canonical (CRITICAL)
The preprint says `signature: Ed25519 over fields 1-10` in the message
comment but `Ed25519 over concatenation of fields 1–12 (UTF-8)` in the field
semantics — fields 1–12 *include the signature itself* (field 11), which is
impossible, and "UTF-8 concatenation" is undefined for `bytes` and `int64`
fields. Worse, all designs sign **Protobuf** structures, but Protobuf has no
canonical encoding: field order, packed/unpacked repeated fields, and
unknown-field retention all produce different bytes for the same message, so
"serialize and verify" fails across implementations, and "re-serialize to
verify" is exactly the malleability pattern that produced real-world
consensus and auth bugs (e.g., Protobuf-based signature bypasses).

**Disposition:** MAP signs deterministic CBOR (RFC 8949 §4.2.1) via
COSE_Sign1 (RFC 9052), and mandates *verify-the-received-bytes*: signatures
are computed over the exact transmitted `protected`/`payload` byte strings,
never over a re-serialization (draft-map-core-00 §4). Protobuf/gRPC remain
available as a non-normative application binding for bulk streams.

### C3. Volatile state in the DHT + advertisement storm — does not scale (CRITICAL)
The preprint stores load, latency, and queue length in DHT records
re-announced every 5–10 s with a 30 s TTL. Two problems:

*Churn math.* A Kademlia `PUT` is O(log N) messages to ~k=20 replicas. At
even 1 M nodes republishing every 7.5 s, the DHT substrate absorbs ~2.7 M
store operations/second before a single query is served. At the stated goal
of "billions of nodes" this is off by many orders of magnitude. Gossip is no
better: 1 M nodes × 1 KB ads / 7.5 s ≈ 133 MB/s aggregate that *every*
full subscriber must receive.

*Staleness math.* Load and queue length are stale within seconds; a 30 s TTL
guarantees routing on fiction. Meanwhile the preprint's own numbers conflict
(broadcast every 5–10 s, TTL 30 s, but design2 says cache TTL ≈ 5 min).

**Disposition:** MAP separates a *stable plane* (signed AgentDescriptors,
content-addressed, DHT provider records with ≥1 h TTL, republished hourly)
from a *volatile plane* (direct `STATUS_QUERY` to shortlisted candidates at
decision time, plus optionally scoped, rate-limited gossip presence beacons).
Hot capability keys are sharded (draft-map-discovery-00 §6). Latency is never
advertised at all — see C6.

### C4. No replay protection (HIGH)
No design includes nonces, sequence numbers, or replay windows. A signed
`TaskRequest`, `ToolAuthorization`, or payment message can be replayed
verbatim: re-executing paid work, re-authorizing tool calls, or double-
spending a receipt. Timestamps alone (present only in ads) are insufficient
without a defined skew window and a uniqueness cache.
**Disposition:** every MAP envelope carries a 128-bit `msg-id` and timestamp;
receivers enforce a ±120 s skew window and a (sender, msg-id) uniqueness
cache; session-bound messages additionally carry monotonic sequence numbers
and are bound to the transport channel via COSE `external_aad` (channel
binding), which also defeats cross-session relay (draft-map-core-00 §7).

### C5. Normative magic-weight routing formula (HIGH)
`score(p) = 0.35·trust + 0.25·(1/latency) + …` with min-max normalization
"over the last 60 s local window" is specified as protocol. Three defects:
(a) the weights are policy, not protocol — two conformant nodes need not
agree on them, so making them normative adds nothing to interop while
freezing bad constants into the standard; (b) min-max over a local window
makes scores non-comparable and trivially manipulable (one outlier peer
rescales everyone); (c) it mixes attacker-controlled inputs (C1) with
honest ones. The same applies to design3's bid cost function, which subtracts
milliseconds from dollars from reputation — dimensionally meaningless.
**Disposition:** MAP standardizes the *inputs* (verifiable evidence, quotes,
self-measured RTT, status reports) and the *constraint semantics* of queries
(hard filters MUST be honored). Ranking is explicitly local policy; a
reference policy is given as informative guidance only (draft-map-tasks-00
§5, draft-map-trust-00 §7).

### C6. "Latency" as a global advertisement field (MEDIUM)
`latency_ms` in an advertisement is meaningless: latency is a property of the
*path between two peers*, not of a node. Design2 even acknowledges it is
"measured round-trip to the node" — measured by whom? **Disposition:**
removed. Clients measure their own RTT during `STATUS_QUERY`/`PING`;
providers may advertise *service-time* statistics (TTFT under load) which are
then cross-checked against receipt evidence.

### C7. Privacy: fingerprinting, intent leakage, and code leakage (HIGH)
Advertisements carry exact GPU model, VRAM, RAM, CPU cores, bandwidth — a
high-entropy hardware fingerprint linked to a stable identity, broadcast
globally. Capability queries broadcast the user's intent ("kernel_driver",
"medical/research") to the whole DHT path. Design3 is the only document that
notices the gravest issue — proprietary source code routed to strangers — and
its Zone 0/1/2 model is the best idea in the set.
**Disposition:** zones are adopted and made normative (arch §8, tasks §9):
payload classes are labeled with a maximum zone and routing MUST NOT exceed
it. Hardware is advertised in coarse buckets; exact figures are optional and
zone-gated. Query privacy options (relayed queries, shard sampling) are
specified; metadata minimization is a stated design goal with concrete
requirements.

### C8. gRPC/Protobuf as the mandatory stack contradicts transport-agnosticism (MEDIUM)
The stated goal is transport agnostic (TCP, QUIC, WebSocket, future), yet the
designs hard-wire gRPC-over-QUIC, and use *both* Protobuf-over-gossipsub and
gRPC endpoints discovered via a `grpc_multiaddr` side channel — two parallel
identities/addresses for one node, with the gRPC endpoint outside the
libp2p security context (no Noise/TLS identity binding specified for it).
**Disposition:** MAP defines messages over any ordered, authenticated,
multiplexed byte stream, with normative bindings for libp2p (preferred),
raw QUIC, and WebSocket; one identity, one address book. gRPC remains an
optional application binding on a dedicated protocol ID (core §10).

### C9. Economic layer hand-waving (HIGH for that layer)
"Credits minted by the orchestrator" (design2/preprint) makes every
orchestrator a central bank — this reintroduces the central authority the
goals forbid, and "smart contract or off-chain ledger" is not a
specification. The 10-minute dispute window has no arbitration protocol,
no evidence format, and no binding on outcomes. Token supply, escrow custody,
and slashing are all named but not defined.
**Disposition:** MAP specifies payment *negotiation and evidence*
(quote → agreement → proof → receipt) with a settlement-method registry
(Lightning, card-processor intents, stablecoins, ACH, Interledger, local
ledger) and leaves settlement/custody to the chosen method, where mature
systems already exist (draft-map-pay-00). Disputes are defined as an
evidence-exchange protocol over Merkle-rooted transcripts; adjudication is
mesh policy, not core protocol.

### C10. KV-cache transfer for resumption is impractical (MEDIUM)
`kv_cache_delta` across heterogeneous nodes assumes identical model weights,
quantization, attention implementation, paging layout, and framework version;
in practice caches are engine-specific and enormous (GBs for long contexts).
Design3 partially concedes this. **Disposition:** normative resumption is
*transcript replay* (re-warm from the signed token transcript up to the last
verified checkpoint); KV-cache transfer is relegated to an optional extension
between homogeneous peers (tasks §8).

### C11. Checkpoint trust (MEDIUM)
Checkpoints are signed by the executing node — i.e., precisely the party
whose failure or malice triggers resumption. A poisoned `state_snapshot`
lets a failing/malicious node inject arbitrary state into its successor.
**Disposition:** the *requester* countersigns each checkpoint's transcript
Merkle root before it becomes a valid resumption point; opaque state blobs
are only accepted within Zone ≤1 or from TEE-attested executors (tasks §8.3).

### C12. Taxonomy free-for-all (MEDIUM)
Design3 correctly identifies that hashing free-text capability strings
("rust" vs "rust-lang" vs "systems_programming") fragments the DHT keyspace.
Its `capabilities.json` idea is right but under-specified (no governance, no
versioning, no extension path).
**Disposition:** a governed capability ontology of hierarchical URIs with an
IANA-style registry and Expert Review policy (draft-map-discovery-00 §4,
draft-map-ops-00 §3).

### C13. Identity format inconsistencies (LOW, but corrosive)
Across the documents a node ID is variously SHA-256(pubkey) hex (design2),
libp2p PeerID (design3), and `did:hep:<pubkey_base58>` (preprint) — three
incompatible identifier spaces for the same key. The preprint also invents a
`did:hep` DID method, forfeiting existing tooling.
**Disposition:** one identifier: `AgentID = multicodec(ed25519-pub) || key`,
multibase-encoded — byte-compatible with both libp2p Ed25519 PeerIDs
(identity multihash) and `did:key`, so existing resolvers and libp2p work
unchanged (draft-map-identity-00 §3).

### C14. Requirements-language weakening and editorial defects (LOW)
"RFC 2119, *adapted for open-source implementation maturity*" (preprint §2)
guts the normative force of MUST/SHOULD — reviewers will reject it.
`bootstrap.hep.local:443` uses `.local`, which RFC 6762 reserves for
mDNS and which cannot resolve on the public Internet. "No proprietary
telephony" appears as a core principle (a stray artifact). Naming the
protocol after the Hermes *implementation* invites protocol/implementation
confusion and collides with several established projects named Hermes.
**Disposition:** proper RFC 8174 boilerplate; example domains from
`example.org`; the protocol family is renamed **MAP** (Manolito Agent
Protocol), honoring the MP2P lineage while keeping "Hermes" free for the
user's orchestrator implementation.

### C15. Sybil-resistance claims without mechanisms (HIGH)
"Sybil attacks mitigated by reputation decay & resource anchoring" is an
assertion, not a mechanism — decay does nothing against fresh identities that
are free to create. Design2's "bond a small stake in a smart-contract-like
ledger" imports an undefined global ledger. Design3's proof-of-compute and
graduated-tier bootstrap ideas are the right direction but unquantified.
**Disposition:** an explicit, layered admission-cost framework (join tickets
via PoW/VDF, optional stake, social vouching), plus the structural defense
that trust in MAP is *per-observer and evidence-based*, so a Sybil army has
no shared reputation to exploit (draft-map-trust-00 §5, with an honest
statement of residual risk — no decentralized system fully solves Sybil).

### Additional findings, briefly
- **No error taxonomy** anywhere; TaskResponse has a 3-value enum. MAP
  defines a full numeric error space with retryability semantics (core §9).
- **No flow control**: token streams over gRPC rely on transport
  backpressure only; a slow client stalls provider VRAM indefinitely. MAP
  adds credit-based application flow control (core §8) so providers can bound
  committed resources and bill for reserved-but-unread output.
- **No version negotiation procedure**: `protocol_version = 1; drop
  otherwise` (design3) prevents any compatible evolution. MAP negotiates
  versions/suites/extensions in HELLO (core §6).
- **Embedding-model blindness**: "route where cosine_similarity > 0.85" is
  meaningless across different embedding models; 0.85 in one space ≠ 0.85 in
  another. Memory advertisements in MAP carry an embedding-model identifier
  and dimension, and similarity thresholds are query parameters, not
  protocol constants (tasks §10).
- **Git workspace sync** (design3) is a good Zone ≤1 mechanism but leaks the
  full repository; adopted as an extension with shallow, path-scoped, token-
  gated clones, prohibited in Zone 2.
- **Receipts published to a global gossip channel** (design3 §7) creates a
  public log of who computed what for whom — a metadata gold mine. MAP makes
  receipt disclosure selective (pull-based, per-audience) with optional
  blinded ratings.

---

## 2. Per-document assessment

**design1** — Strongest conceptual document. The reframing "who can solve
this problem?" and the observation that the network becomes a capability
graph are the foundation of MAP. Weaknesses: everything quantitative
(trust 0.94, latency 180 ms) presumes global, self-reported metrics (C1,
C6); "no scheduler required" underestimates incentive problems; swarm
merge is asserted, not designed.

**design2** — Best engineering pragmatism (milestones M0–M6, concrete
protos, integration table). Weaknesses: C1 (node updates own reputation),
C2 (signed Protobuf), C3 (5 min ad cache vs 10 s updates), per-field
Protobuf errors (`double pricing_per_token = 0.0;` assigns a default where a
field number is required — would not compile), stake "in a smart-contract-
like ledger" undefined (C15). Its risk table is honest and several rows are
adopted.

**design3** — The most security-aware document: zones (adopted), tool
authorization tokens (adopted, generalized to capability tokens), taxonomy
normalization (adopted, C12), TTFT guarantees with aggressive fallback
(adopted), bidding (adopted as an extension), OpenRouter gateway-as-peer
(adopted as the Federation gateway pattern). Weaknesses: dual addressing
(C8), receipts-on-global-gossip privacy leak, KV-cache sync optimism (C10),
cost function units (C5).

**design4 / rfc_preprint** — Most complete artifact and closest to RFC
form, which makes its normative defects the most consequential: C1, C2, C3,
C5, C9, C14 all originate here. Its layering diagram, state-machine sketch,
Merkle-rooted checkpoints, and phased acceptance criteria are retained in
restructured form. The phase table's metric "trust-driven routing reduces
hallucination by >30%" is unmeasurable as stated (no baseline, no
benchmark) — replaced by falsifiable interop criteria (ops §6).

---

## 3. Disposition summary

| Source idea | Disposition | Where |
|---|---|---|
| Capability graph routing | **Adopted** (core framing) | arch, discovery |
| Zones 0/1/2 | **Adopted, normative** | arch §8, tasks §9 |
| Tool authorization tokens | **Adopted** → capability tokens (UCAN-style caveats) | tasks §7, identity §6 |
| Capability taxonomy | **Adopted** → governed URI ontology + registry | discovery §4, ops §3 |
| Micro-bidding | **Adopted** as optional extension | tasks §4.2 |
| Gateway peers (OpenRouter) | **Adopted** → federation gateways w/ provenance | ops §5 |
| Merkle-rooted transcripts | **Adopted**, + requester countersignature | tasks §8 |
| Checkpoint/resume | **Adopted** via transcript replay; KV-cache = extension | tasks §8 |
| Graduated trust bootstrap | **Adopted** | trust §6 |
| Self-reported TrustScore | **Rejected** (C1) | trust §2 |
| Latency in ads | **Rejected** (C6) | discovery §5 |
| Volatile metrics in DHT | **Rejected** → two-plane discovery (C3) | discovery §5–6 |
| Normative score weights | **Rejected** → local policy (C5) | tasks §5 |
| Signed Protobuf control plane | **Rejected** → deterministic CBOR + COSE (C2) | core §4 |
| Orchestrator-minted credits | **Rejected** → settlement-method registry (C9) | pay §3 |
| KV-cache wire transfer | **Rejected** as normative (C10) | tasks §8.4 |
| Global receipt gossip | **Rejected** → selective disclosure | trust §4 |
| `did:hep` method | **Rejected** → did:key-compatible AgentID (C13) | identity §3 |
