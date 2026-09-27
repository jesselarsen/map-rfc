# MAP over A2A: extension binding, version 2.0-draft

**Status:** Draft. Normative for this binding. MAP (Mutual Attestation
Protocol) is an accountability layer for delegated agent work; this document
binds it onto [A2A v1.0](https://a2a-protocol.org/latest/specification/) as
an A2A *extension*. It does not define the layer's own artifacts, signing
rules, or state machine — those are defined by **the layer spec**, cited
here by section name (e.g. "the layer spec, *Checkpoints and cross-provider
resumption*"). Section names for the layer spec are illustrative pending
that document's final headings; align at integration time if they differ.

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD
NOT, RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described in
BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in all
capitals.

## 0. Sources verified

This binding was written against the following, fetched and cross-checked
directly against the `a2aproject/A2A` GitHub repository (branch `main`,
commit `43e0c874d3baba68ed84b98678d7f2268438e69f`, 2026-09-22; tags present
up to `v1.0.1`):

- `docs/specification.md` (the prose specification)
- `specification/a2a.proto` (`package lf.a2a.v1`; the proto file's own
  comment states it is "the single authoritative normative definition of
  all protocol data objects" — the JSON artifact is a non-normative build
  product)
- https://a2a-protocol.org/latest/specification/ and
  https://a2a-protocol.org/latest/topics/extensions/ (rendered form of the
  same repository)

Every field name and header name below was read from `a2a.proto` or
`docs/specification.md` directly, not inferred. Where a detail could not be
confirmed against either source, it is marked **UNVERIFIED** rather than
guessed. One piece of third-party prior art — the x402 Foundation's
`transports-v2/a2a.md` (an x402-over-A2A binding written by others) — still
documents the *old* 0.x header name `X-A2A-Extensions`; this binding uses
the current v1.0 name confirmed against the primary source (§2).

## 1. Extension declaration

Extension URI (**PLACEHOLDER** — not registered): `https://map-protocol.org/ext/accountability/v2`.

A2A-1: A provider offering this binding MUST declare it in its Agent Card
under `AgentCapabilities.extensions[]` as an `AgentExtension` (`a2a.proto`
lines 424-432: fields `uri` string, `description` string, `required` bool,
`params` `google.protobuf.Struct`):

```json
{
  "capabilities": {
    "extensions": [
      {
        "uri": "https://map-protocol.org/ext/accountability/v2",
        "description": "MAP accountability layer: dual-signed receipts, countersigned checkpoints, zone labels, metering evidence.",
        "params": {
          "agentId": "z6MkPLACEHOLDERAgentIdBase58btc",
          "supportedZones": ["1", "2", "3"],
          "checkpointIntervalParts": "8",
          "acceptedVoucherRails": ["pay:x402", "pay:mpp-tempo"]
        }
      }
    ]
  }
}
```

A2A-2: `required` MUST be `false` by default. Because `required` is a
proto3 implicit-presence field, the value `false` is then omitted from the
card's JSON altogether (A2A §8.4.1 rule 1), as in the example above. An A2A client that does not
activate the extension MUST still receive a fully functional A2A
interaction from a MAP-capable provider (§10) — MAP rides *alongside* A2A,
never gating it. A deployment MAY run a `required: true` profile for a
closed mesh that mandates accountability evidence; that is a local policy
choice, not this binding's default.

A2A-3: `params` fields (all advertisement, non-binding until echoed in a
session — analogous to a descriptor's `PricingHint`, layer spec *Descriptors
and discovery binding*):

| Field | Type | Meaning |
|---|---|---|
| `agentId` | string | This agent's AgentID, multibase `z...` form (layer spec, *Identity*) |
| `supportedZones` | array of tstr (decimal uint) | Zone labels this provider will accept tasks under (layer spec, *Zones and data sensitivity*) |
| `checkpointIntervalParts` | tstr (decimal uint) | Default number of transcript leaves (§4) between checkpoint proposals. A session-local policy default, overridable per task (§3). RECOMMENDED value 8 on the primary path (A2A-20) and 32 on the fallback path (A2A-21) (layer C-6) |
| `acceptedVoucherRails` | array of tstr | Rail URIs (mapping-payment-rails.md §1) this provider can bind metering evidence to |

A2A-3a: `params` MUST NOT contain JSON numbers; unsigned integers are
carried as decimal strings (`"32"`, no sign, no leading zeros). `params` is
a `google.protobuf.Struct`, in which every number is a double, and a signer
or verifier that round-trips the card through protobuf (as the reference
Python SDK does) re-serializes `32` as `32.0`. RFC 8785 serializes it as
`32`. The two would then sign different bytes (A2A-31).

## 2. Extension activation

A2A-4: Activation uses the v1.0 service parameter, confirmed as
`A2A-Extensions` (not the pre-1.0 `X-A2A-Extensions`; `docs/specification.md`
§14.2.2 "Header field name: A2A-Extensions", and §4.6.1). A client that
wants MAP evidence for this task MUST send:

```http
A2A-Extensions: https://map-protocol.org/ext/accountability/v2
```

on every request of the task (JSON-RPC and REST bindings: HTTP header, per
`docs/specification.md` §14.2.2; gRPC binding: the equivalent metadata entry
under key `a2a-extensions`, §2534 of the same document). A2A-5: A provider
that activates the extension SHOULD echo it back on responses it sends for
that task, per the same section ("the response SHOULD include the
`A2A-Extensions` header, listing all extensions that were successfully
activated").

A2A-6: Because `required` is `false` (A2A-2), a provider that does not
recognize the URI or chooses not to run the extension MUST simply omit it
from its own `A2A-Extensions` response value and proceed as a plain A2A
interaction; it MUST NOT reject the request on that basis alone (A2A-2's
converse).

## 3. Where MAP artifacts ride

All MAP artifacts are carried as base64url-without-padding of their
deterministic-CBOR bytes (layer spec, *Wire conventions*), inside `metadata`
under one namespaced key, `"map/v2"`, with fixed sub-keys. This diverges
from the pattern shown in `docs/specification.md` §4.6.2 (which keys
`metadata` directly by the full extension URI, e.g.
`"https://example.com/extensions/geolocation/v1": {...}`); this binding
instead uses the short literal `"map/v2"` because the extension URI itself
is a PLACEHOLDER subject to change before registration, and a URI-keyed
metadata map would force every implementation to update a JSON object key
whenever the URI is finalized. This is a deliberate, documented departure
from the example convention, not an oversight (recorded as decision
D-A2A-1, §12).

```json
"metadata": {
  "map/v2": {
    "agentId": "requester AgentID, multibase z... form",
    "taskNonce": "base64url, 16 raw bytes",
    "zone": 2,
    "receipt": "base64url of CBOR Receipt",
    "checkpoint": "base64url of CBOR CheckpointArtifact",
    "voucherDigest": "base64url of CBOR Multihash",
    "error": { "code": 2, "detail": "root mismatch on prefix replay" }
  }
}
```

All sub-keys are OPTIONAL and present only on the message/event that
carries that particular artifact, except that the session-opening
`Message` MUST carry `agentId`, `taskNonce` and `zone`. `agentId` names
the requester: the provider verifies every countersignature on the task
against that key, and a countersignature by any other key is refused (code
1). `zone` is a plain JSON integer (not a
base64url CBOR blob): it is advertised early — before any signed artifact
exists to carry it — so non-MAP-aware intermediaries and observability
tooling can see it without decoding CBOR. It mirrors `ReceiptCore` key 12
and `CheckpointCore` key 7 once those exist; a mismatch between the plain
`zone` value and a later-carried Receipt/CheckpointArtifact's own zone field
is a protocol error: code 7 `zone-mismatch` (§9). The receiver MUST NOT
countersign the mismatching artifact.

A2A-7 (carriage table):

| MAP artifact | A2A carrier | Direction |
|---|---|---|
| `agentId`, `taskNonce`, `zone` (session opening) | First `Message` of the task, `metadata."map/v2"` | requester → provider |
| Checkpoint proposal (provider-signed `CheckpointArtifact`) | `TaskStatusUpdateEvent.metadata`, state `WORKING` unchanged, or transient `INPUT_REQUIRED` (§7) | provider → requester |
| Checkpoint countersignature | Follow-up `Message` with `taskId` set to the running task, `metadata."map/v2".checkpoint` = dual-signed `CheckpointArtifact` | requester → provider |
| Final receipt proposal (provider-signed `Receipt`) | Terminal `TaskStatusUpdateEvent.metadata` (state `COMPLETED`/`FAILED`/`CANCELED`) | provider → requester |
| Countersigned receipt | `Message` with `referenceTaskIds: [taskId]`, no `taskId` of its own, `metadata."map/v2".receipt` = dual-signed `Receipt` | requester → provider |
| Metering evidence | `voucherDigest` alongside a checkpoint or the final receipt, same carrier | either, as applicable |
| Binding errors | `metadata."map/v2".error` on any of the above, or a standalone `Message` | either |

The countersigned-receipt row does not attach to the original `taskId`
because A2A tasks are not generally reopenable once terminal
(`docs/specification.md` §4.1.3 lists `COMPLETED`/`FAILED`/`CANCELED`/
`REJECTED` as terminal). `Message.referenceTaskIds` (`a2a.proto` line 276,
`repeated string reference_task_ids`) exists precisely to let a later
message refer back to a task without reopening it, so the countersigned
receipt travels as an ordinary `SendMessage` call in the same `contextId`,
referencing the closed task. This works uniformly under both A2A
request/response (`SendMessage`) and streaming (`SendStreamingMessage`)
modes, satisfying A2A-2's non-gating requirement — a plain client simply
never sends this message. The provider is then left holding only its own
provider-signed half, which by layer rule R-4 contributes nothing to any
trust projection. It is kept only as the provider's own record.

## 4. TaskID derivation (D6)

A2A-8: `TaskID = SHA-256(enc(["MAPTaskID", requesterNonce, providerAgentID, a2aTaskId]))`
where `enc()` is deterministic CBOR (RFC 8949 §4.2.1) of the 4-element
array, `requesterNonce` is the 16 raw bytes from `metadata."map/v2".taskNonce`
(§3), `providerAgentID` is the provider's 34-byte AgentID, and `a2aTaskId`
is the UTF-8 bytes of the A2A `Task.id` string (`a2a.proto` line 170,
`string id`).

A2A-9: The nonce is chosen and sent by the requester in the first `Message`
of the exchange — before any A2A `Task` object, and hence before any
`Task.id`, exists. It is what lets the requester commit to *its own* half
of the TaskID binding before the provider has allocated anything, closing
the same "who names the task" race the layer spec's TaskID design exists to
prevent (layer spec, *Task identity*): a provider cannot mint a `Task.id`
that collides with a value the requester precommitted to, because the
requester's contribution (the nonce) is independent of the provider's
(the allocated id), and TaskID is a hash of both plus the provider's own
AgentID, binding the value to a specific provider so two different
providers can never compute the same TaskID for numerically identical
inputs by accident.

A2A-10: The provider allocates `Task.id` as usual (A2A's normal behavior on
`SendMessage`/`SendStreamingMessage`) and returns it in the first `Task` or
`TaskStatusUpdateEvent` response. TaskID is only computable, by either
party, once both the nonce (requester-supplied) and `Task.id`
(provider-supplied) are known — which happens after that first round trip.
A2A-11: No Receipt, CheckpointArtifact, or other TaskID-bearing MAP artifact
MAY be produced before that point; a checkpoint or receipt proposal carrying
a TaskID that does not recompute correctly from the nonce, provider AgentID,
and `Task.id` the requester actually received MUST be rejected with error
code 5 (`task-id-mismatch`, §9).

## 5. Transcript mapping

A2A-12: Transcript leaves are the `Part`s delivered by the provider, across
all `Artifact`s of the task, in delivery order. `seq` starts at 0 for the
first `Part` of the first `TaskArtifactUpdateEvent` (or the `Artifact`
list of a non-streaming terminal `Task` response) and increments by one per
`Part`, regardless of which `Artifact` it belongs to or whether the event
that carried it had `append = true` (`a2a.proto` line 317) — an appended
chunk is still newly-delivered bytes at a new sequence point, and the whole
purpose of the transcript commitment is to capture what was delivered
incrementally, not the logically-merged final artifact. `lastChunk`
(`a2a.proto` line 319, `last_chunk`) is informational only ("no more parts
coming for this `artifactId`") and does not affect `seq` assignment.

A2A-13: `kind = 0` for every leaf produced by this rule ("artifact Part").
Kinds 1-255 are reserved for future MAP-over-A2A leaf types (for example, a
future revision that also commits requester input `Message` `Part`s into
the transcript) and are UNASSIGNED by this document.

A2A-14: `content` = the RFC 8785 (JCS) canonical JSON bytes of the `Part`
object, taken from whichever protocol binding actually carried it
(`a2a.proto` lines 224-241: `oneof content { text | raw | url | data }` plus
`metadata`, `filename`, `media_type`), **excluding the `metadata` member**.
Metadata is excluded because it is where MAP's own artifacts (checkpoint
proposals, etc.) ride (§3) — including it in the leaf content would make
the transcript root depend on the accountability evidence itself,
circularly.

A2A-15 (canonicalization risk — flagged, not resolved): A2A's canonical
data model is Protocol Buffers (`docs/specification.md` §"Layer 1"); JSON is
one binding's wire form, produced by each SDK's own protobuf-JSON mapper.
Two conformant A2A SDKs are not guaranteed to produce byte-identical JSON
for the same `Part` (field ordering, `google.protobuf.Struct` numeric
representation, and `google.protobuf.Value` oneof encoding are all mapper
choices before JCS ever runs) even though JCS itself is deterministic once
its *input* JSON value is fixed. **This is a genuine, unresolved interop
risk for this binding**, no different in kind from the one A2A itself
already accepts for Agent Card signing (`docs/specification.md` §8.4.1,
which is why it additionally normalizes proto field-presence before JCS).
Implementations of this binding MUST use the same field-presence
normalization A2A defines for Agent Card canonicalization (§8.4.1) when
canonicalizing a `Part` for §A2A-14, and SHOULD cross-test transcript roots
between their own requester and provider implementations as part of
conformance, since this binding does not have A2A's own reference JSON
artifact to fall back on for `Part`.

A2A-16 (out-of-band content, §content-by-reference): a `Part` whose `oneof`
variant is `url` (line 226) commits to the transcript only the URL string,
not the bytes it points to. Where integrity of the referenced content
matters to the accountability claim, the provider SHOULD prefer the `raw`
variant (inline bytes) for anything transcript-critical, or, if bulk
out-of-band transfer is required, encode a content digest into the `data`
variant instead of `url` so the digest — not just an address — is what
gets committed.

## 6. State mapping

A2A-17: `TaskState` values, confirmed against `a2a.proto` lines 187-207
(`TASK_STATE_UNSPECIFIED = 0` through `TASK_STATE_AUTH_REQUIRED = 8`):

| A2A state | Layer event |
|---|---|
| `SUBMITTED` | No MAP event. TaskID is not yet computable if `Task.id` has just been allocated and not yet echoed back (§4); if it has, checkpointing may begin once `WORKING` starts. |
| `WORKING` | Checkpoint proposals at the negotiated cadence (§3 `checkpointIntervalParts`, default one checkpoint per N transcript leaves — a local policy parameter per the layer spec, *Checkpoints and cross-provider resumption*, not a protocol minimum). |
| `INPUT_REQUIRED` | MAY carry a checkpoint proposal that also functions as the input-required prompt (§7); otherwise no MAP event. |
| `AUTH_REQUIRED` | No MAP event; this is an A2A-native authentication interruption, not a MAP checkpoint. |
| `COMPLETED` | Terminal. Receipt proposal, `outcome = 0` (ok). |
| `FAILED` | Terminal. Receipt proposal, `outcome = 1` (failed). |
| `CANCELED` | Terminal. Receipt proposal, `outcome = 3` (cancelled), usage anchored to the last **countersigned** checkpoint, never to unconfirmed work past it (layer spec, *Metering and voucher binding*). |
| `REJECTED` | Terminal. **No Receipt.** A `REJECTED` task never entered execution (`docs/specification.md` describes it as the agent declining the task outright) — no accountable work occurred, so producing an ExecutionReceipt would misleadingly assert an attempt was made. This is a binding-level decision (§12, D-A2A-2), since the interface contract's outcome enum has no "never started" value and none is needed: the correct signal is the absence of a Receipt. |

A2A-18 (`timeout`, outcome `2`): A2A has no `TIMEOUT` task state. `timeout`
is a **requester-local** determination: the requester gives up waiting past
its own configured deadline while the task is still `SUBMITTED` or
`WORKING` (never having reached a terminal state at all). In that case the
requester MAY still send `CancelTask`, but the receipt it records locally
(there being no provider-countersigned receipt for a task the provider may
still consider live) uses `outcome = 2`. If the provider later completes
and reports a terminal state after the requester's local timeout, the
requester's own bookkeeping reconciles per local policy (layer spec,
*Receipts and evidence*); this binding does not require the two to agree.

## 7. Checkpoint delivery during `WORKING`: two paths

A2A-19: Vanilla A2A `SendMessage`/`SendStreamingMessage` is server-streaming
only: the client sends one `Message`, the server streams `Task`/
`StreamResponse` events back; there is no protocol-level provision for the
client to push a new `Message` mid-stream except by calling `SendMessage`
again with `taskId` set. Whether a given provider's implementation accepts
such a call while its own state for that task is still `WORKING` (rather
than `INPUT_REQUIRED`) is **implementation-defined** — nothing in
`a2a.proto` or `docs/specification.md` forbids it (a `Message`'s `taskId`
is simply "task reference... only if a task was created," §4.1.4), but
common server designs only expect client input during `INPUT_REQUIRED`.

This binding therefore defines two paths and lets providers advertise which
they support via a boolean sub-key omitted from the normative surface here
(implementation detail, not wire-visible — negotiate out of band or default
to the fallback):

- A2A-20 **Primary (RECOMMENDED where supported):** the provider emits a
  `TaskStatusUpdateEvent` with `state = WORKING` (unchanged) carrying the
  checkpoint proposal in `metadata."map/v2".checkpoint`; the requester
  calls `SendMessage` with `taskId` set to the running task and its
  countersignature in the same metadata shape, asynchronously, without the
  provider blocking on it. This keeps the task's own progress unblocked —
  matching A2A's streaming mode — but the requester-countersigned
  checkpoint arrives some (bounded) time after the provider's proposal,
  never before.
- A2A-21 **Fallback (MUST be supported by every conformant provider of this
  binding):** the provider transitions briefly to `INPUT_REQUIRED`,
  carrying the checkpoint proposal on that `TaskStatusUpdateEvent`; the
  requester's `Message` reply (the natural way to satisfy
  `INPUT_REQUIRED`) carries the countersignature; the provider resumes
  `WORKING` on receipt. This uses A2A's state machine exactly as designed
  ("the agent needs something from the client before continuing") and
  works under strict request/response semantics with no implementation
  ambiguity, at the cost of a synchronous pause.

A2A-22: A provider MUST support the fallback (A2A-21) even if it also
offers the primary path, so that a conformant requester never needs
out-of-band knowledge of which path a given provider prefers beyond
observing whether checkpoints arrive on `WORKING` or `INPUT_REQUIRED`
events.

## 8. Resumption over A2A

A2A-23: To resume on provider B after a countersigned checkpoint from
provider A, the requester opens a **new** A2A task at B. B's first
`Message` (from the requester) carries:

- `referenceTaskIds: [<A's task id>]`;
- the countersigned `CheckpointArtifact` in `metadata."map/v2".checkpoint`;
- the transcript prefix through `through-seq` — either inline as one or
  more `DataPart`s (`oneof content = data`) or as a `FilePart`-style `url`
  Part plus a content digest (§5, A2A-16) — the requester's choice, traded
  off against message-size limits. An inline prefix is one `DataPart`
  `{"data": {"mapTranscriptPrefix": [<Part>, ...]}}` listing the prefix
  Parts in `seq` order, each exactly as originally delivered (the leaf rule
  of §5 ignores Part `metadata`, so re-serialization of that field is
  harmless).

A2A-24: B recomputes the transcript root over the received prefix per the
layer spec's leaf/parent rule and MUST reject with error code 2
(`root-mismatch`, §9) if it disagrees with `CheckpointCore` key 5. B's own
first delivered `Part` (§5) is assigned `seq = through-seq + 1`, continuing
the same sequence space rather than restarting at 0 — the transcript is one
logical structure spanning both providers, only the signer of new leaves
changes. B's eventual Receipt carries `ReceiptCore` key 13
(`resumed-from`) set to the Multihash of A's countersigned
`CheckpointArtifact` envelope.

A2A-25: B independently derives its own TaskID for the new A2A task exactly
as in §4 (fresh nonce, B's own AgentID, the new `Task.id` B allocates) — it
is a distinct TaskID from A's, deliberately: the checkpoint is the
cryptographic link between the two tasks, not a shared identifier.

## 9. Error carriage

A2A-26: This binding defines a small error-code table carried as
`metadata."map/v2".error = {"code": uint, "detail"?: tstr}` on whichever
`Message` or event the error pertains to. This table is scoped to this
binding — it is not assumed to be, and does not need to be, the same
numbering as any error registry the layer spec or a retained core document
defines, since the interface contract given to this binding guarantees only
the existence of one named condition ("countersignature refused") and
leaves its carriage to the binding.

| Code | Name | Meaning |
|---|---|---|
| 1 | `countersignature-refused` | The receiver declines to countersign a checkpoint or receipt (values outside local tolerance, disagreement on usage, etc.) |
| 2 | `root-mismatch` | A recomputed transcript root (resumption, §8, or a countersignature check) does not match the claimed root |
| 3 | `zone-violation` | The task's zone label is not one the provider accepts (descriptor key 5 / `supportedZones`), or it widens the zone of a checkpoint being resumed (layer Z-3). A requester never needs this code for its own Z-1 check: it simply does not delegate. |
| 4 | `bad-checkpoint` | A `CheckpointArtifact` fails structural or signature validation |
| 5 | `task-id-mismatch` | A TaskID does not recompute from the nonce/AgentID/`Task.id` triple (§4) |
| 6 | `unsupported-extension-version` | The extension URI's version is not one this implementation speaks |
| 7 | `zone-mismatch` | A Receipt's or CheckpointArtifact's zone field differs from the `zone` the task was opened with (§3) |

A2A-27: An error MAY also be surfaced through A2A's own error mechanisms
(a JSON-RPC error object, a gRPC status, or an HTTP error status per
`docs/specification.md` §3.3.2) when it also constitutes an A2A-level
failure (for instance, `bad-checkpoint` on the message that would have
satisfied `INPUT_REQUIRED` legitimately fails that A2A request too); the
`map/v2` error code is the binding-specific detail riding alongside, not a
replacement for A2A's own error surface.

## 10. Interop requirement

A2A-28: An A2A client with no knowledge of this extension, talking to a
provider that supports it, MUST receive ordinary, fully functional A2A
behavior: normal `Task`/`Message`/`Artifact` flow, no unexplained
`INPUT_REQUIRED` stalls attributable to MAP (a provider that only offers
the fallback checkpoint path, §7, MUST NOT use it against a requester that
never activated the extension — such a requester gets no checkpoints at
all, not a stalled task), and no metadata it needs to understand to make
progress (it may see an opaque `"map/v2"` key in `metadata` and is free to
ignore it, per A2A's own metadata contract, §"flexible key-value map").

## 11. Descriptor / Agent Card rules

A2A-29: The MAP descriptor (layer spec, *Descriptors and discovery
binding*; see also `binding-libp2p-discovery.md`) embeds the signed A2A
Agent Card as RFC 8785 (JCS) canonical JSON bytes, plus the Agent Card's
own JWS (`AgentCardSignature`, `a2a.proto` lines 457-466: `protected`,
`signature`, optional `header`). The JWS `kid` in the protected header MUST
identify the same Ed25519 key as the AgentID (`alg` MUST be `"EdDSA"` per
RFC 8037 — `docs/specification.md` §8.4.2 gives `ES256`/`RS256` only as
*examples*, not an exhaustive `alg` list; EdDSA is a standard JOSE algorithm
and is REQUIRED by this binding for consistency with the AgentID's own
Ed25519 key).

A2A-30: A querier that has both the Agent Card's JWS and the descriptor's
own outer signature (layer spec, *Descriptors and discovery binding*) MUST
verify both; if the Agent Card JWS `kid` resolves to a key that does not
match the descriptor's subject AgentID, the whole descriptor MUST be
rejected (mismatch — one of the two claims is lying about who this is).

A2A-31 (Agent Card canonicalization interop profile): a MAP provider's
Agent Card MUST avoid every construct on which A2A §8.4.1 and deployed A2A
SDKs canonicalize differently, so that its signature verifies under both.
Concretely, the card as published MUST NOT contain:

- empty strings, empty arrays or empty objects anywhere (§8.4.1 keeps
  REQUIRED empties, the reference SDK drops all of them);
- implicit-presence fields set to their default (`AgentExtension.required:
  false`, `AgentInterface.tenant: ""`), which §8.4.1 omits;
- JSON numbers inside any `google.protobuf.Struct` (A2A-3a);
- non-ASCII characters (the reference SDK escapes them to ASCII escape
  sequences, while RFC 8785 emits raw UTF-8).

A verifier of this binding applies §8.4.1 as written: it drops the
`signatures` member, non-REQUIRED empty arrays and implicit-presence
defaults, then applies RFC 8785. Cards within this profile produce the same
bytes under that rule and under the SDK's rule.

*Evidence.* The profile is derived from a live test, not from reading. a2a-sdk
1.0.3 (`a2a.utils.signing`) canonicalizes by `MessageToDict`, then recursive
empty-value removal, then `json.dumps(sort_keys=True)`. The first MAP card
failed that SDK's EdDSA verifier on exactly two constructs: `"required":
false` and integers in `params`. Once both were removed it verified, and a
modified card was rejected (`interop/a2a-sdk/check_sdk.py`).

## 12. Judgment calls recorded in this binding

- **D-A2A-1** (§3): artifacts ride under the literal key `"map/v2"`, not
  under the extension URI as A2A's own examples do, because the URI is a
  PLACEHOLDER.
- **D-A2A-2** (§6): `REJECTED` produces no Receipt at all, rather than
  `outcome = 1`, because no execution occurred to attest.
- **D-A2A-3** (§3, §8): the countersigned receipt and the resumption
  handshake both travel via `Message.referenceTaskIds` rather than by
  reopening a terminal task, since A2A tasks are not generally reopenable.
- **D-A2A-4** (§7): two checkpoint-delivery paths are defined (async during
  `WORKING`, and a mandatory `INPUT_REQUIRED` fallback) because A2A does not
  guarantee client-initiated mid-stream messages are accepted by every
  server implementation.
- **D-A2A-5** (§9): a binding-local error-code table, independent of any
  numbering the layer spec's own error registry uses, because the
  interface contract only guarantees carriage of one named error.

## 13. Security considerations

- **Metadata stripping by intermediaries.** A2A permits proxies and
  gateways; nothing prevents one from dropping unrecognized `metadata`
  keys. A `"map/v2"` payload that is stripped in transit is
  indistinguishable, to the receiving party, from one that was never sent.
  This binding cannot detect that on its own; a requester that requires
  evidence SHOULD treat its absence over N consecutive checkpoints as
  equivalent to `countersignature-refused` at the transport level, per
  local policy.
- **Replay of `Message`s.** This binding relies on A2A's own transport
  security (TLS at minimum) plus the TaskID binding (§4) — a replayed
  `Message` from a different task cannot produce a valid artifact for
  *this* task because the artifact's own signed content (the CBOR
  `ReceiptCore`/`CheckpointCore`) carries the TaskID, and A2A-11 requires
  recomputation to match.
- **JCS pitfalls.** See A2A-15: this is the binding's largest open risk.
  Implementations SHOULD publish their canonical-JSON test vectors for
  `Part` alongside their conformance suite.
- **Out-of-band content.** See A2A-16: `url`-variant `Part`s commit only an
  address, not content, to the transcript.

## 14. Worked example

Placeholder cryptographic material below (`sig`, hashes, keys) is marked
`PLACEHOLDER` and is not a real signature or digest.

**Agent Card extension block** (see §1 for the full card fragment).

**First `Message` of a task** (opens the TaskID binding, §4):

```json
{
  "message": {
    "role": "ROLE_USER",
    "parts": [{ "text": "Summarize this 40-page report." }],
    "extensions": ["https://map-protocol.org/ext/accountability/v2"],
    "metadata": {
      "map/v2": {
        "agentId": "z6MkPLACEHOLDERRequesterAgentId",
        "taskNonce": "PLACEHOLDER_base64url_16_bytes",
        "zone": 2
      }
    }
  }
}
```

**`TaskArtifactUpdateEvent` carrying a checkpoint proposal** (§7, primary
path, `WORKING` unchanged):

```json
{
  "taskId": "task-abc123",
  "contextId": "ctx-789",
  "artifact": {
    "artifactId": "art-1",
    "parts": [{ "text": "...partial summary chunk 9..." }]
  },
  "append": true,
  "lastChunk": false,
  "metadata": {
    "map/v2": {
      "checkpoint": "PLACEHOLDER_base64url_CBOR_CheckpointArtifact_provider_signed_only"
    }
  }
}
```

**Terminal status event with receipt proposal**:

```json
{
  "taskId": "task-abc123",
  "contextId": "ctx-789",
  "status": {
    "state": "TASK_STATE_COMPLETED",
    "message": {
      "role": "ROLE_AGENT",
      "parts": [{ "text": "Summary complete." }]
    }
  },
  "metadata": {
    "map/v2": {
      "receipt": "PLACEHOLDER_base64url_CBOR_Receipt_provider_signed_only"
    }
  }
}
```

**Requester's countersigned receipt** (§3, §8; a standalone `SendMessage`
referencing the closed task):

```json
{
  "message": {
    "role": "ROLE_USER",
    "parts": [{ "text": "Countersigned receipt attached." }],
    "referenceTaskIds": ["task-abc123"],
    "metadata": {
      "map/v2": {
        "receipt": "PLACEHOLDER_base64url_CBOR_Receipt_dual_signed"
      }
    }
  }
}
```

## References

[RFC2119] [RFC8174] BCP 14. [RFC8949] CBOR. [RFC8785] JSON Canonicalization
Scheme (JCS). [RFC7515] JSON Web Signature (JWS). [RFC8037] EdDSA in JOSE.
A2A Protocol Specification, v1.0.1,
https://a2a-protocol.org/latest/specification/ ; source verified against
`a2aproject/A2A` commit `43e0c874d3baba68ed84b98678d7f2268438e69f`
(2026-09-22), `docs/specification.md` and `specification/a2a.proto`. A2A
Extensions topic, https://a2a-protocol.org/latest/topics/extensions/. x402
Foundation, `transports-v2/a2a.md` (`coinbase/x402`, informative — cited in
§0 for the stale `X-A2A-Extensions` header name it still documents).
