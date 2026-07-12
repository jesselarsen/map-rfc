---
title: "MAP Discovery: Agent Descriptors, Capability Ontology, and Two-Plane Discovery"
docname: draft-map-discovery-00
category: std
---

# MAP Discovery and Service Advertisement

**Status:** preprint, work in progress. BCP 14 language applies (see
Requirements Language, below).

## Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document family are to be interpreted as described in
BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here. No adaptation or weakening of these terms applies.

## Abstract

Defines how agents describe themselves (AgentDescriptor), the governed
capability ontology that makes descriptions comparable, and a two-plane
discovery system: a *stable plane* of signed, content-addressed descriptors
indexed in a Kademlia DHT with hot-key sharding, and a *volatile plane* of
on-demand status queries and optional scoped presence gossip. Also
specifies bootstrap, the NAT-traversal profile, and discovery privacy.

## 1. Two-plane rationale (normative consequence)

Stable facts (identity, capabilities, models, transports, pricing policy)
change on the order of days; volatile facts (queue depth, free VRAM,
current TTFT) change on the order of seconds. Systems that push volatile
facts through global storage or gossip fail at scale and route on stale
data (critique C3). Therefore:

- R1: Volatile fields MUST NOT appear in descriptors or DHT records.
- R2: Routing decisions requiring volatile facts obtain them by direct
  STATUS_QUERY to a shortlist derived from the stable plane.
- R3: Latency is measured by the querier, never advertised (critique C6).
- R4: Trust/reputation fields MUST NOT appear anywhere in this document's
  structures (critique C1); trust lives in draft-map-trust-00.

## 2. AgentDescriptor

An out-of-session envelope (msg-type 0x0100), content-addressed by
`descriptor-cid = multihash(suite-hash, envelope-bytes)`.

```cddl
Descriptor = {
  1  => AgentID,                    ; subject (accountable identity)
  2  => uint,                       ; seq — strictly increasing per subject
  3  => [+ Version],                ; MAP versions spoken
  4  => [+ uint],                   ; suites supported
  5  => [+ tstr],                   ; multiaddrs / URLs per transport
  6  => [+ CapEntry],               ; capabilities offered
  ? 7  => [+ ModelEntry],
  ? 8  => HardwareClass,
  ? 9  => PricingHint,
  ? 10 => SecurityProfile,
  ? 11 => [+ MemoryTopic],
  ? 12 => [+ ZoneLabel],            ; zones served (default [2])
  ? 13 => uint,                     ; descriptor validity, ms (<= 24h)
  ? 14 => ExtMap
}

CapEntry = {
  1 => CapURI,                      ; from the ontology, §4
  ? 2 => { * int => any }           ; capability-specific parameters
}                                   ;  (registered per ontology branch)

ModelEntry = {
  1 => tstr,                        ; model name (informative)
  ? 2 => Multihash,                 ; weights digest — the verifiable id
  ? 3 => uint,                      ; max context tokens
  ? 4 => tstr,                      ; quantization label
  ? 5 => tstr                       ; runtime ("ollama","vllm","gateway")
}

HardwareClass = {                   ; COARSE BUCKETS ONLY (arch §8.2)
  ? 1 => uint,                      ; vram-class: 0=n/a,1<=8,2=16,3=24,
                                    ;  4=32,5=48,6=64,7=96,8=128,9>128 GB
  ? 2 => uint,                      ; ram-class (same scale)
  ? 3 => uint,                      ; accel: 0 none,1 consumer-gpu,
                                    ;  2 datacenter-gpu,3 npu,4 cpu-only
  ? 4 => uint                       ; bandwidth-class: 1<=100Mb,2<=1Gb,3>1Gb
}

PricingHint = {                     ; nonbinding; binding prices via
  ? 1 => tstr,                      ;  PAY_QUOTE (pay §3)
  ? 2 => [ int, int ],              ; per-1M-tokens [exp, mantissa] tag-4
  ? 3 => [ int, int ],              ; per-minute
  ? 4 => bool                       ; barter accepted
}

SecurityProfile = {
  ? 1 => uint,                      ; sandbox: 0 none,1 container,2 microVM,
                                    ;  3 wasm,4 tee
  ? 2 => bool,                      ; remote attestation available
  ? 3 => [* tstr]                   ; tool allowlist (ontology tool URIs)
}

MemoryTopic = {
  1 => tstr,                        ; domain label
  2 => tstr,                        ; embedding-model identifier (REQUIRED —
  3 => uint,                        ;  dimension        similarity across
  ? 4 => uint,                      ; vector count      models is undefined;
  ? 5 => Multihash                  ; index digest      critique "0.85")
}
```

Validation: signature per core §4.3; `seq` MUST exceed any cached seq for
the subject (else stale — keep newer); validity ≤ 24 h; descriptors with
trust/latency/load-shaped extension fields SHOULD be treated as
non-conformant by public-mesh policy. Model claims are *claims*; the
weights multihash is the only verifiable model identity (TH6) and SHOULD be
present for any model whose substitution matters.

## 3. STATUS — the volatile plane

```cddl
StatusQuery  = { ? 1 => [+ CapURI], ? 2 => bstr .size 8 /*echo*/ }
StatusReport = {
  1 => Timestamp,
  ? 2 => uint,        ; queue-length
  ? 3 => uint,        ; est-ttft-ms for the queried capability, self-est.
  ? 4 => uint,        ; accepting: 0 no, 1 yes, 2 yes-if-reserved
  ? 5 => uint,        ; free vram-class right now
  ? 6 => bstr .size 8 ; echo (lets querier compute its own RTT — R3)
}
```

Reports are hints: self-estimates that later become checkable against
receipt evidence (TTFT promised vs. observed — trust §8). Providers MUST
rate-limit STATUS_QUERY per core §8.3; queriers SHOULD query ≤10 candidates
per routing decision.

## 4. Capability Ontology

Capabilities are hierarchical URIs under `map:cap:`; matching is by exact
segment prefix (a provider of `inference/text-generation/code` matches a
query for `inference/text-generation`). Initial branches (registry with
Expert Review policy, ops §3):

```
map:cap:inference/text-generation[/code|/chat]
map:cap:inference/embedding
map:cap:inference/vision | /speech-to-text | /text-to-speech | /image-gen
map:cap:tool/git | /shell | /compiler/<lang> | /browser | /sql
map:cap:lang/<bcp47-or-programming-language-tag>   ; qualifier axis
map:cap:memory/<domain>                            ; retrieval domains
map:cap:relay      map:cap:gateway/<namespace>     ; infrastructure roles
map:cap:schedule/reservation                       ; tasks §6
```

Free-text tags MUST NOT be used as DHT keys (critique C12); descriptors
MAY carry informative free-text specialties in ExtMap for UI only.
Ontology versioning: additions are backward compatible; renames occur only
at protocol major versions with alias records.

Gateway provenance (ops §5, rule G-1). A CapEntry MAY carry a params map
(`CapEntry = {1 => CapURI, ?2 => { * uint => any }}`). For the `gateway/`
branch the following params key is registered (FINAL, immutable — ops §3.3):

| Key | Name | Value | Meaning |
|-----|------|-------|---------|
| 1 | gateway-of | tstr namespace | The fronted capability is re-advertised through this gateway namespace. |

When a gateway advertises a *fronted* capability (e.g.
`map:cap:inference/text-generation`) it SHOULD attach `{1: <namespace>}` to
that CapEntry, so a consumer sees the capability is proxied. The gateway's
descriptor subject remains the gateway's own AgentID — never the upstream
(G-1). (Reference-implementation note: the scaffold `Descriptor` codec does
not yet model CapEntry params, so the interim carrier is an out-of-band
`GatewayAdvert` wrapper; the wire assignment above is authoritative.)

## 5. Stable-plane publication (DHT)

Provider records map capability keys to descriptor pointers:

```
K(cap)        = multihash(suite-hash, "map:idx:1:" || CapURI)
ProviderRecord (signed, out-of-session envelope, ttl <= 6h):
  { 1 => AgentID, 2 => descriptor-cid, 3 => seq, 4 => Timestamp,
    ? 5 => [+ tstr] /* addr hints */ }
```

Agents publish one record per offered capability (bounded: MUST NOT exceed
32 capability keys; aggregate under parents instead), republish every
1–6 h, and store the full descriptor retrievably at
`H("map:desc:" || descriptor-cid)` and/or serve it on request
(CAP_RESPONSE). Rotation/revocation records live under the identity key
(identity §7).

Data ownership (normative): the agent is the sole source of truth for its
descriptor; DHT provider records and all peer caches are *derived* data —
rebuildable at any time by republication, repairable by re-fetch, and never
load-bearing for correctness. Loss or suppression of index records
degrades discoverability but cannot corrupt truth (integrity is end-to-end
via CID + signature, §8). Conflicts between copies resolve by `seq`
alone — last-writer-wins scoped per subject, with the subject as the only
writer.

## 6. Hot-key sharding

Popular capabilities would concentrate millions of provider records on one
DHT neighborhood. Therefore every capability key is sharded 256 ways:

```
shard(agent) = first byte of the DIGEST carried in
               multihash(suite-hash, AgentID)
               ; NOT the first byte of the multihash itself, which is the
               ; constant hash-code prefix and would put every agent in
               ; one shard
K'(cap, s)   = multihash(suite-hash, "map:idx:1:" || CapURI || ":" || s)
```

In the key preimage, `s` is rendered as its unpadded base-10 ASCII
decimal ("0".."255" — e.g. shard 141 contributes the three bytes
`31 34 31`), not as a raw byte or hex. (Pinned 2026-07-12 from the
map-go ambiguity report; the derived-constants test vector K′ already
froze this form.)

Providers publish only to their own shard `K'(cap, shard(agent))`.
Queriers sample `r` uniformly random shards (RECOMMENDED r = 3, doubling
until enough candidates or r = 16) and merge. Properties: publication load
per key drops 256×; sampling yields an unbiased provider sample (useful
against TH11 — an adversary must capture many shard neighborhoods to bias
results); the shard function is verifiable, so a record found in the wrong
shard is discarded.

## 7. CAP_QUERY (direct/relayed query)

For scoped meshes or privacy, a querier MAY send CAP_QUERY {constraints:
CapURI, params, zone, limits} directly to known peers or a rendezvous
relay instead of the DHT; peers answer CAP_RESPONSE with descriptor CIDs
they can vouch having verified. Constraint fields marked *hard* (context
size, zone, sandbox class) MUST be honored by responders — the normative
filter semantics that replace the rejected global score formula
(critique C5); ranking of survivors is querier policy.

Content maps (registry assignment; added 2026-07-05 from TASK-006):

```cddl
CapQuery    = { 1 => CapURI, ? 2 => uint /*shard*/, ? 3 => uint /*limit*/ }
CapResponse = {
  1 => [* bstr],      ; descriptor ENVELOPES (not bare CIDs) — the querier
                      ;  MUST verify each independently; the index is
                      ;  untrusted transport (§8)
  ? 2 => bool         ; more — the responder capped the batch (limit, ≤ 8)
}
```

The querier verifies every returned envelope (signature + self-signature
kid == subject + ttl) before caching; a forged or stale entry is dropped,
never cached. Responders SHOULD cap a CAP_RESPONSE at 8 descriptors and set
`more = true` when truncating. (A responder MAY answer with bare descriptor
CIDs for a follow-up fetch instead of inline envelopes; this profile inlines
the envelopes so a single round-trip yields verifiable descriptors.)

## 8. Anti-eclipse and record integrity

Queriers MUST use disjoint-path Kademlia lookups (≥3 disjoint paths) for
identity and descriptor fetches; DHT routing tables SHOULD enforce IP-prefix
diversity per bucket; descriptor integrity is end-to-end (CID + signature),
so the DHT is untrusted storage — an eclipse can *hide* providers (degrade)
but cannot *forge* them. Bootstrap lists MUST support multiple independent
sources (static config, DNS TXT under operator domains, cached peers);
`example.org` names are used in examples — `.local` is reserved by mDNS and
MUST NOT appear in public bootstrap configuration (critique C14).

## 9. Discovery privacy

Options, by increasing cost: (a) shard sampling already hides *which*
providers you saw; (b) route CAP_QUERY via a relay peer (`map:cap:relay`)
stripping the querier's address; (c) decoy queries on unrelated
capabilities; (d) Zone-3 ephemeral identity for the whole discovery
session. STATUS_QUERY inherently reveals requester→candidate interest;
privacy-sensitive requesters SHOULD status-query through the same relay.
Residual risks: arch §8.3.

## 10. NAT traversal and reachability profile

Public-mesh providers MUST support: AutoNAT v2 (reachability
self-diagnosis), Circuit Relay v2 (relayed listening when unreachable),
DCUtR (hole punching upgrade), and QUIC as first-choice transport.
Descriptors list relayed multiaddrs when direct addresses are absent.
Relays are ordinary agents advertising `map:cap:relay` with PricingHint;
relayed bytes are ciphertext (arch §5). Mobile/intermittent agents SHOULD
set short descriptor validity and rely on RESUME (tasks §8) rather than
session persistence.

## 11. Security considerations

Descriptor spoofing is prevented by signatures; descriptor *flooding* by
per-peer ingestion rate limits and admission costs (trust §5); stale-record
attacks by `seq` monotonicity; wrong-shard placement by shard verification
(§6); eclipse degradation bounded by §8. Fingerprinting surface is bounded
by bucket coarseness — implementers MUST NOT add high-entropy optional
fields to public descriptors (arch §8.2).

## 12. IANA-style considerations

Registries established: MAP Capability Ontology; MAP CapEntry Parameter
keys (per branch); MAP HardwareClass code points; MAP Sandbox Classes.

## References (selected)

[RFC2119] [RFC8174] BCP 14 requirement levels.
