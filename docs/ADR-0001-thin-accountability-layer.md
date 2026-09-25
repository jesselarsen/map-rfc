# ADR-0001: Rework MAP into a thin accountability layer; publish as an arXiv systems paper

- **Status:** Accepted (2026-09-24)
- **Supersedes:** the IETF Independent-Submission plan in `map-v1-ietf:map-spec/SUBMISSION.md`, tag `map-v1-ietf`
- **Deciders:** Jesse Larsen (author)

## Context

MAP v1 is 11 IETF-style drafts, plus a Rust and a Go implementation, 26/26
live interop, byte-exact test vectors and ProVerif models. It specifies a
complete stack: identity, envelope/session, discovery, a task protocol, trust,
payment/escrow/disputes, and operations.

Three reviews changed the picture.

1. **Process (2026-09-18, Roles 1 and 8).** The Independent Stream cannot create
   the ~20 registries the family needs (RFC 8726 §2, §4, §5), and relabelling
   the category does not fix that. The narrow two-draft fallback in
   `SUBMISSION.md` rested on a novelty finding that did not survive review 3.
2. **Security (2026-09-18, Roles 2, 3, 4 and 7).** The reviewers demonstrated
   exploits against the unmodified crates. They are preserved and reproduced in
   `attic/probes/`. The payment-layer exploits (negative amounts, forgeable
   dispute claims) sit in code that is secondary to MAP's contribution. The
   trust and discovery exploits (receipt farming, shard flooding, grinding,
   replay-cache cost) sit in code that is central to it.
3. **Prior art (2026-09-24).**
   - **The thesis is taken.** [arXiv:2603.03753](https://arxiv.org/abs/2603.03753)
     ("Agentic Peer-to-Peer Networks: From Content Distribution to Capability
     and Action Sharing", Mar 2026) frames the same "who has this file? → who
     can do this task?" move. It already has signed short-lived capability
     descriptors, provider-signed receipts, a hybrid local/federated reputation
     ledger, and a simulator evaluation.
   - **Each standalone leg of MAP has a strong incumbent:**
     - task exchange: [A2A v1.0](https://a2a-protocol.org/latest/announcing-1.0/),
       with signed Agent Cards, under the Agentic AI Foundation
     - decentralized indexing: [AGNTCY ADS](https://datatracker.ietf.org/doc/draft-mp-agntcy-ads/),
       a Kademlia capability → CID index
     - payment rails: [MPP sessions](https://mpp.dev/intents/session) and the
       [x402 V2](https://x402.org/x402-v2-launch/) `upto`/batch-settlement
       schemes, which already do cumulative, payer-signed, pause-until-paid
       metering
     - dialog management: the forming IETF [agentproto](https://datatracker.ietf.org/doc/charter-ietf-agentproto/) WG
   - **The combination is uncovered.** No surveyed system has all of:
     - execution evidence signed by *both* parties
     - trust computed *only* from the observer's own evidence
     - checkpoints *countersigned by the requester* so a task can resume on a
       different provider
     - metering bound to that evidence
     - an explicit data-sensitivity zone model

## Decision

**Scope.**
- Stop the IETF track. Freeze v1 at tag `map-v1-ietf`.
- On branch `accountability-layer`, reduce MAP to an **accountability layer
  for delegated agent work** that runs *over* existing ecosystems rather than
  replacing them.

**Deliverable.**
- Publish it as an **arXiv systems paper with a quantitative evaluation**
  (primary category cs.CR).
- Keep the specification as a versioned, self-hosted document set.

### Keep / replace / drop

| v1 component | Disposition | Rationale |
|---|---|---|
| Ed25519 AgentID, COSE_Sign1 over deterministic CBOR, verify-received-bytes | **Keep** | Sound foundation on stable RFCs (8949, 9052/9053, 8032) |
| Dual-signed ExecutionReceipt | **Keep** (core contribution) | 2603.03753 and ACTA receipts are signed by one party only |
| Local per-observer trust projection + rebuild rule | **Keep, fix A1** | ERC-8004/2603.03753 use global or federated ledgers |
| Transcript Merkle root + requester-countersigned checkpoints, resume on another provider | **Keep, finish** (R4-02: the codec exists only inside disputes) | Uncovered in the surveyed work |
| Metering (UsageWatermark) | **Keep, re-express** as a binding onto payer-signed cumulative vouchers (MPP/x402) via `voucher_digest` | MAP's own invoice protocol duplicates MPP |
| Data-sensitivity zones | **Keep, minimal** (label + refusal + auditability) | Useful and uncovered; the v1 enforcement messages were never defined |
| Two-plane discovery (stable DHT records / volatile STATUS_QUERY) | **Keep, fix A3/A4/A8**; descriptors embed signed A2A Agent Cards; ADS-compatible key layout | ADS has no volatile plane |
| OFFER/QUOTE/ASSIGN/CHUNK/COMPLETE task protocol | **Replace** with an A2A extension binding | A2A is where the ecosystem and agentproto converge |
| Settlement methods, escrow, credit ledger, disputes, `pay:x402` method definition | **Drop** (kept in the tag) | Duplicative of the rails; hosts the demonstrated R4 exploits |
| ±5% / ±2000 ms countersignature tolerance | **Demote** to a local policy parameter | Normative magic constant; telecom reconciliation practice |
| did:key, multiformats | **Informative** only | did:key is a CCG draft; the IETF MULTI WG was abandoned |
| libp2p | **Discovery plane only** (pending D3) | No versioned spec releases; the work plane runs over A2A/HTTPS |
| IETF render pipeline, registries, `SUBMISSION.md` | **Drop** (kept in the tag) | No IETF submission |
| "Manolito" name and MP2P heritage | **Drop** (rename pending D1) | Weak historical record; name collides with IETF MAP-E/MAP-T |

## Consequences

**Positive**
- The claimed contribution becomes defensible and specific.
- The layer composes with where adoption already is.
- The demonstrated exploits in kept code get fixed and *measured*, and those
  measurements become part of the evaluation.
- About half of the v1 surface area, including the riskiest payment code,
  leaves the build.

**Negative / costs**
- A2A binding work, plus canonical JSON (RFC 8785) for transcript leaves across
  A2A SDKs.
- A simulator has to be built from scratch; no harness exists.
- The v1 test vectors for tasks, pay and disputes are retired. They are never
  renumbered.
- Go shrinks to a verifier-only second implementation (pending D4).
- The paper must position explicitly against 2603.03753. The honest novelty
  claim is narrower than "a new agent network".

**Risks**
- The central result (E1) depends on the default issuer weighting (D5).
  Mitigation: pre-register the hypotheses in `eval/HYPOTHESES.md` and report
  the regimes where the approach loses.
- Voucher fields in MPP/x402 may not carry a free-form reference.
  Mitigation: the rail-agnostic reverse binding (`voucher_digest` in the
  checkpoint and the receipt).

## Alternatives considered

1. **Publish v1 on the Independent Stream as-is.** Not possible: the registry
   policies are unavailable (RFC 8726).
2. **Submit two narrow I-Ds (metering, tolerance band).** Rejected: both are
   duplicative (MPP/x402/Filecoin payment channels; telecom reconciliation).
3. **Keep the full stack; fix and evaluate it.** Rejected: the paper would
   compete head-on with A2A/MPP/ADS on their home ground.
4. **Paper only, no code changes.** Rejected: the novelty claim is weakest and
   there is no new evidence.
