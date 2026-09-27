# MAP accountability layer: changelog

Each change is classified as one of:
- **editorial**: wording only;
- **additive**: an optional field or structure, which a conformant peer of
  the previous version can ignore;
- **breaking**: the change would not interoperate.

The v1 family's changelog (the I-D era, through tag `map-v1-ietf`) is
frozen at `docs/history/CHANGELOG.md`.

---

## 2.0-draft: v1 family → accountability layer (2026-09)

**Breaking, a new major version.** The 11-document v1 family
(`draft-map-*-01`) is replaced by one normative document,
`spec/layer.md`, plus three bindings:
- `spec/binding-a2a.md`: the work plane over A2A v1.0.
- `spec/binding-libp2p-discovery.md`: the discovery plane over a
  Kademlia DHT.
- `spec/mapping-payment-rails.md` (informative): MPP sessions and x402 V2.

The rationale is in `docs/ADR-0001-thin-accountability-layer.md`. Retired
text stays in full at tag `map-v1-ietf` (`4107aaa`). The area-by-area
v1 → v2 table is `spec/layer.md` §13. This entry records what moved and
where to find it.

### v1 document → where its content went

| v1 document | Kept in | Retired |
|---|---|---|
| `draft-map-intro-01`, `draft-map-index-01` | layer §1 (scope and non-goals) | the family overview, reading order and submission status |
| `draft-map-arch-01` | layer §12 threat model; zones (arch §8.1) become layer §10, as attributable dual-signed labels with the refusal rule Z-1 | the MP2P framing and the Manolito name; zones as an enforcement mechanism (finding A7): they now give accountability, not confidentiality |
| `draft-map-core-01` | layer §4 envelope (deterministic CBOR + COSE_Sign1, invariant I1); HELLO and channel binding, for the discovery plane only (binding-libp2p DHT-14) | suite negotiation; the task/pay message types |
| `draft-map-identity-01` | layer §3: one Ed25519 AgentID, signing directly | the root/device/session key hierarchy, `DelegationCert` chains, rotation and revocation, hardware backing |
| `draft-map-wire-01` | the wire tags `"MAPReceipt"`, `map:cap:` and `map:idx:1:`, byte-stable | the family-wide code-point registry |
| `draft-map-discovery-01` | layer §11 two-plane records, sharded capability keys, the D-6 querier filter; descriptors now embed a signed A2A Agent Card | suite lists in descriptors, `MemoryTopic` |
| `draft-map-tasks-01` | layer §5 transcript Merkle root; §7 checkpoints (the new `CheckpointArtifact`) | the `OFFER`…`COMPLETE` task protocol, now A2A; `CHECKPOINT_ACK` and its `"MAPCheckpointAck"` signing input; the engine-state shortcut `ext-kvx` |
| `draft-map-trust-01` | layer §6 receipts (ReceiptCore keys 1–11 byte-stable, plus 12 zone and 13 resumed-from); §8 local projection with the rebuild rule | the Beta-prior default for unknown issuers (finding A1); the normative ±5% / ±2000 ms tolerance, now local policy (§6.5) |
| `draft-map-pay-01` | layer §9 `Amount`/`Asset` (uint mantissa), `Usage`, the `voucher-digest` binding; `mapping-payment-rails.md` (informative) | `PAY_*` messages, terms 0–3, escrow, the credit ledger, disputes and the `pay:x402` method. Money now moves on MPP sessions and x402 V2 |
| `draft-map-ops-01` | the evaluation methodology, now `eval/` with pre-registered hypotheses | the IETF submission track, IANA sections, the render pipeline |

New in v2, with no v1 counterpart: the A2A binding and its TaskID
derivation (`"MAPTaskID"`, binding A2A-8).

### Test vectors

- **Byte-stable:** TV-001..005, NV-001..005.
- **RETIRED(v1), never renumbered:**
  - TV-006..020 (including TV-010b);
  - NV-006..012.

  Their bytes stay in `map-test-vectors.md`, and `gen_vectors.py` still
  regenerates them.
- **Added:**
  - TV-021 (CheckpointArtifact), with NV-013 and NV-014;
  - TV-022 (signed A2A Agent Card);
  - TV-023 (receipt admissibility, 13 cases);
  - TV-024 (DescriptorV2);
  - TV-025 (A2A TaskID and Part leaves).

  Rust and Go both reproduce all of them byte-for-byte (interop matrix
  part a).

### Findings from the 2026-09-18 external review

The disposition of each finding is in layer §12.3.

| Finding | Fix |
|---|---|
| A1 receipt farming | R-2 through R-4 plus T-2, gated in `apply` (shared by rebuild), and T-3 |
| R4-01 negative amounts | M-4 |
| R4-02 unsigned checkpoint usage | `CheckpointArtifact` |
| A3 shard flooding | D-6 |
| A8 | descriptor TTL on ingest |

A4 (node-ID grinding) is **not** fixed. See the P8 entry below.

### Defaults set from the evaluation (phase P8)

These come from the pre-registered evaluation (`eval/HYPOTHESES.md`, tag
`prereg-v1`, Amendments A1–A9). The run is `eval/results/5bc7702/`,
20 seeds with bootstrap CIs.

| Change | Class | Where | Evidence |
|---|---|---|---|
| Default issuer weighting: W1, direct agreement. W2 is NOT RECOMMENDED where Sybils can outnumber honest requesters | additive (recommended policy) | layer §8.4 (was "Deferred") | The pre-registered minimax re-test (A9) over α 0.05–0.5 × 7 attacks gives worst-case attacker share W0 0.348, **W1 0.200**, W2 0.550 (W2 collapses under camouflage at α = 0.5). The earlier α = 0.2-only D5 had picked W2 by 0.2 points (0.0290 vs 0.0312) |
| Trust parameter table: decay 0.98/day, prior 1/1, future-skew 120 s, per-issuer cap 50 | additive (recommended policy) | layer §8.2 | E1 values, fixed before the runs and not tuned |
| C-6: default checkpoint interval c = 8 leaves | additive (SHOULD) | layer §7.5; binding-a2a `checkpointIntervalParts` | E2 / H2: at c = 8, resume wastes 0.0042× what restart wastes. E4 / H5: 0.079% time overhead; exposure ≤ price(c) |
| D-8 epoch-salted shard: from "experimental mitigation" to NOT RECOMMENDED as a mitigation; A4 becomes a stated limitation | normative downgrade (no wire change) | layer §11.5, §12.3; binding-libp2p DHT-13 | E3b: attacker holds 1.000 of the target shard in every configuration; the 1-core grind outpaces any epoch |
| D-9 disjoint-path lookups: from "experimental" to SHOULD, d = 3 | normative strengthening (no wire change) | layer §11.6; binding-libp2p DHT-12 | E3c: at N = 10⁵, f = 0.3, lookup success 0.755 → 0.930 |

No wire format changed in P8, so all vector bytes are unchanged.
### Paper red-team review (2026-09-27)

| Change | Class | Where | Evidence |
|---|---|---|---|
| C-6 rationale overhead figures corrected to run 5bc7702 (0.079% at c = 8, 0.63% at c = 1) | editorial | layer §7.5 | R2-07; `eval/results/5bc7702/REPORT.md` H5 |
| Resumption scope stated: complete only for tasks whose state is their delivered output; hidden state and external side effects are not carried, and this layer does not make side effects idempotent | editorial (informative) | layer §7.4 | R2-08 |
| W1 recommendation: informative Sybil-ratio caveat. W1 falls behind W0 at 6.4 camouflaging Sybils per honest requester | editorial (informative) | layer §8.4 | R2-04; evaluation A10, `eval/results/5bc7702/SYBIL-SWEEP.md` |
| §11.5 shard-capture text corrected: the trust layer can rank only what discovery returns. Informative measurement (A11) and advice to keep known-good providers on shortlists and run a canary; new requesters remain exposed | editorial (informative) | layer §11.5 | R2-03; evaluation A11, `eval/results/5bc7702/DISCOVERY-FLOOD.md` |
| C-6 default interval split by countersignature path: c = 8 asynchronous (A2A-20), c = 32 synchronous pause (A2A-21). The overhead rationale is relabelled CPU-only, with an estimated round-trip cost added | normative (recommended default; no wire change) | layer §7.5; binding-a2a `checkpointIntervalParts` | RT-01; E2 κ = 1 column (argmin c = 32 at the H2 point) |
| C-5 wording: overbilling is zero under the rule, not "bounded by one interval". The interval bounds the provider's unpaid work instead (fewer than c leaves per failure) | editorial | layer §7.5 | RT-06 |
| Attack table gains requester-side rows: free-riding (at most min(L, c) leaves per task, unbounded across fresh identities; no trust in requesters), receipt refusal and vindictive customers | editorial (informative) | layer §6.6 | RT-05 |
| Gaming table, badmouthing row: informative caveat that camouflaged issuers can aim their weight at chosen targets (A12: W1 cuts target traffic 68%, W2 88%) | editorial (informative) | layer §6.6 | RT-04; `eval/results/5bc7702/TARGETED-SLANDER.md` |
| §14.2 evidence-availability note (informative): no receipt dissemination is defined. A13 measurements show W1 falls behind W0 under camouflage at v ≤ 0.2, and a global ledger is worse than random there; prefer W0 when camouflage and limited disclosure are expected | editorial (informative) | layer §14.2 | RT-03; `eval/results/5bc7702/VISIBILITY.md` |
