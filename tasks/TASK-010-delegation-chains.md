# TASK-010 — Delegation Certificates and Envelope Chain Verification

Status: READY (after TASK-005) · Priority: P2 · Target: `map-identity` + `map-core` + vectors
Depends on: TASK-005 (session), TASK-004 (vector process) · Blocks: TASK-016 (gateway), tool grants hardening

---

## 1. Problem Statement

Identity §4 defines the key hierarchy (RIK→DK→SK) and DelegationCert;
core ProtHdr key 33 carries the chain — but `envelope::open()` verifies
only direct kid signatures, and no DelegationCert code exists. This task
implements certificate issuance/validation, chain verification in the
envelope path, and caveat attenuation (tighten-only), with test vectors.

## 2. Functional Requirements

- FR-1 `map-identity::delegation`: `DelegateBody` codec per identity §4
  CDDL {1 issuer AgentID, 2 subject AgentID, 3 not-before, 4 not-after,
  ?5 [CapURI] scope, ?6 [ZoneLabel] zone scope, ?7 max-depth,
  ?8 caveats map}; a DelegationCert IS an envelope (msg-type 0x0502,
  out-of-session, empty aad) whose kid is the ISSUER; `issue(parent_kp,
  body) -> Vec<u8>` and `validate_cert(bytes, now_ms) -> Result<Cert>`
  (signature, time window, issuer==kid).
- FR-2 Chain validation `validate_chain(chain: &[Vec<u8>], leaf_signer:
  &AgentId, now_ms) -> Result<EffectiveScope>`: order leaf..root (core
  ProtHdr key 33 comment); each cert's subject == next-lower issuer;
  chain terminates at a ROOT AgentID (returned — the accountable
  identity); depth ≤ every cert's max-depth remaining; **attenuation**:
  effective scope = intersection of all cap scopes (prefix-segment
  matching via `map_discovery::cap_matches`), effective zones =
  intersection (and never wider than any link — tighten-only, decision
  5); `EffectiveScope { root: AgentId, caps: Option<Vec<CapURI>>,
  zones: Option<Vec<u8>>, depth_left: u32 }`.
- FR-3 `map-core::envelope`: new `open_delegated(env_bytes, external_aad,
  now_ms) -> Result<OpenedDelegated>` — if ProtHdr key 33 present:
  verify the envelope signature against the LEAF key (the kid MUST be
  the leaf subject), validate the chain, return Opened + EffectiveScope
  + root AgentID. Existing `open()` is UNTOUCHED (byte-frozen behavior;
  conformance guards). An envelope whose kid ≠ chain leaf subject →
  error 2003-family (delegation error range; verify against errors.rs).
- FR-4 Caveats (identity §4 key 8): implement TWO registered caveats
  end-to-end — `1: task-pinning (value = TaskID bytes)` and
  `2: audience (value = AgentID bytes)`; unknown caveat keys: if listed
  in the cert body's crit-style semantics… identity §4 doesn't define
  crit for caveats — ARCHITECT RULING: unknown caveat keys make the
  chain INVALID (fail closed; a caveat is a restriction — ignoring one
  widens scope). Record this ruling as an identity §4 spec patch in the
  same change.
- FR-5 Session integration: `SessionConfig` gains optional
  `delegation_chain: Vec<Vec<u8>>`; when set, handshake envelopes carry
  key 33 and the responder validates the chain and records the ROOT as
  the accountable peer (receipts would accrue to the root); the
  transport-authenticated key check (core §7.3) then applies to the
  LEAF. Test: session established with a 2-link chain (RIK→DK→SK),
  peer's accountable id == RIK's AgentID.
- FR-6 Vectors: extend `gen_vectors.py` with TV-011 (a DelegationCert,
  RIK-A→DK derived from seed SHA-256("MAP-TEST-VECTOR-KEY-A-DK")) and
  TV-012 (an envelope signed by the DK carrying the 1-link chain);
  NV-006 (chain with widened cap scope — MUST reject); regenerate the
  markdown (TV-001..010 bytes frozen, in-script regression assert);
  extend map-conformance accordingly.

## 3. Non-Functional Requirements

Pinned deps; zero warnings; `open()` byte-frozen; chain validation pure
(injected now_ms). Chain length hard cap 8 (DoS bound; record in spec
patch).

## 4. System Architecture

delegation.rs in map-identity (owns certs); open_delegated in map-core
(owns envelopes); session integration in map-session; vectors in
map-spec + conformance. Dependency graph unchanged.

## 5. Component Design

As FR-1..FR-5. `EffectiveScope::permits(cap: &str, zone: u8) -> bool`
helper (used later by tool grants/gateway).

## 6. Interface Specifications

`validate_chain` never panics; error enum distinguishes: BadSignature,
Expired{index}, NotYetValid{index}, BrokenLink{index}, DepthExceeded,
WidenedScope{index}, UnknownCaveat{index, key}, TooLong. Complexity
O(links × scope terms).

## 7. Data Structures

Caveat registry constants in map-identity (`CAVEAT_TASK_PIN: u64 = 1`,
`CAVEAT_AUDIENCE: u64 = 2`) — these ARE the registry assignments
(architect-approved; record in identity §11 registry table in the same
change).

## 8. Algorithms

Scope intersection: a chain link may only NARROW: for caps, every URI in
link N+1's scope must prefix-match some URI in link N's effective scope
(absent scope = inherit, not widen); zones: set intersection; depth:
min(remaining−1). Audience caveat: chain valid only when validated by
the named audience (validator passes its own AgentID). Task-pin: chain
valid only for envelopes whose Body content references that TaskID —
enforced by the CALLER (open_delegated returns the pin; taskrun checks)
— document the split.

## 9. Edge Cases

Empty chain with key 33 present → malformed (1000). Single cert chain.
Cert not-yet-valid. Root == leaf (self-delegation) → BrokenLink.
Duplicate cert in chain → TooLong/Broken. Zone scope absent everywhere →
None (unrestricted). max-depth 0 cert followed by another link →
DepthExceeded.

## 10. Error Handling

Wire mapping: all chain failures → 2003 (delegation) family with detail
naming the failing index; session handshake with bad chain → ERROR 2003
+ CLOSE.

## 11. Security Considerations

Fail-closed unknown caveats (FR-4 ruling) and tighten-only intersection
are the load-bearing properties — NV-006 plus unit tests for every §6
error variant. Revocation is OUT of scope (identity §7 records exist but
no distribution mechanism yet) — document loudly; chains are
short-lived by not-after instead.

## 12. Performance Considerations

Chains ≤8 links; per-envelope chain validation is 8 Ed25519 verifies
worst case — cache validated chains by chain-bytes digest within a
session (invalidate at not-after).

## 13. Input Specifications / 14. Output Specifications

Pure library APIs per §5/§6; vectors regenerate deterministically.

## 15. Acceptance Criteria

- ✓ Workspace green; `open()` behavior byte-identical (conformance).
- ✓ TV-011/TV-012 produce+consume byte-exact in conformance; NV-006
  rejected with WidenedScope.
- ✓ FR-5 session-with-chain test passes; accountable peer == root.
- ✓ Every §6 error variant unit-tested.
- ✓ identity §4/§11 spec patches recorded (caveat registry ids,
  fail-closed ruling, chain cap 8).

## 16. Comprehensive Test Plan

Unit: cert codec roundtrip; validate_cert time windows; every chain
error; intersection algebra (caps prefix cases, zones, depth); caveat
task-pin/audience honored; unknown caveat fails.
Integration: FR-5; envelope via open_delegated happy + leaf/kid
mismatch.
Vectors: TV-011/012 byte-exact; NV-006; TV-001..010 regression.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/` + `map-spec/`, implement §FR-1..FR-6 per §4–§14.
> Identity §4 CDDL (`draft-map-identity-01.md`) is authoritative for
> DelegateBody; core ProtHdr key 33 order is leaf..root. Do NOT modify
> `envelope::open()` — add `open_delegated`. Unknown caveats fail
> closed; scope intersection is tighten-only; both are normative. Apply
> the identity §4/§11 spec patches described in §7/§FR-4/§3 in the same
> change. Extend `gen_vectors.py` (TV-001..010 bytes frozen — keep the
> regression assert) and map-conformance. `cargo
> +stable-x86_64-pc-windows-gnu` from PowerShell; full suite per fix.
> Deliver: code, vectors, spec patches, tests per §16, report.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo +stable-x86_64-pc-windows-gnu test --workspace
cd ../map-spec/test-vectors && python3 gen_vectors.py && git diff --stat
git diff map-spec/draft-map-identity-01.md   # caveat ids + fail-closed + cap 8
```

## 19. Known Risks

R1: identity §4's caveat semantics are the thinnest part of the family
(memory §4 gap) — this package resolves the minimum (two caveats +
fail-closed); richer caveats (budget) stay future. R2: revocation
distribution absent — chains must stay short-lived; noted in spec patch.

## 20. Handoff Notes

Update memory.md §4 (gap partially closed), §5, §8. `EffectiveScope`
is the enforcement primitive for TOOL_GRANT hardening and the federation
gateway (TASK-016).
