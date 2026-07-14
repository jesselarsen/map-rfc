# TASK-003 — Conformance Crate: Byte-Exact Vectors + Negative Suite

Status: READY (after TASK-001) · Priority: P1 · Target: `map-rs/conformance/map-conformance`
Depends on: TASK-001 · Blocks: any claim of spec conformance; feeds TASK-004

---

## 1. Problem Statement

Ops §6 requires conformance evidence: an implementation must reproduce the
normative test vectors **byte-for-byte** and reject a defined catalog of
malformed/hostile inputs. The vectors exist
(`map-spec/test-vectors/map-test-vectors.md`: TV-001 HELLO envelope,
TV-002 dual-signed ExecutionReceipt) but no code checks them. This task
builds `conformance/map-conformance`, the crate whose green test run IS
the conformance statement for impl #1.

## 2. Functional Requirements

- FR-1: Derive test keys in Rust from the documented seeds and assert they
  match the published hex EXACTLY:
  - seed_A = SHA-256("MAP-TEST-VECTOR-KEY-A") =
    `6dbbc0077f020e6085d0db9d5662db32158e2eda8521a348ebc3643b1ff877e5`
    → pubkey `65c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1`
    → AgentID multibase `z6MkmJaZFLtdW8yn2t9b8An5D3bbYvziY7XabCVQ6xSCxah6`
  - seed_B = SHA-256("MAP-TEST-VECTOR-KEY-B") =
    `ce1fba32e140ef080bc9523218892aa845b6d37a887f5518b7355cbdc21e6ff2`
    → pubkey `c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d`
    → AgentID multibase `z6Mkszo9kizvbjPPLe5KnKHedt9pCieVg3NCPoFdzD7446EG`
- FR-2 (TV-001, produce): build the HELLO body
  `{0: 0x0100, 1: 0x0001, 2: h'000102030405060708090a0b0c0d0e0f',
  3: 1780000000000, 10: Hello{versions:[0x0100], suites:[1],
  extensions:{}, max_message_size:262144}}`, seal with Key A and empty
  `external_aad`, and assert the resulting envelope equals the 166-byte
  vector hex (full hex in the vectors file; ends `...30840d0f`).
- FR-3 (TV-001, consume): `envelope::open` on the vector bytes succeeds;
  sender AgentID, msg_type 0x0001, timestamp, and payload bytes match.
- FR-4 (TV-002, produce): build ReceiptCore with task-id =
  `d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25`
  (**hardcode this hex — it is BLAKE2b-256("example-task"); do NOT add a
  blake2 dependency**), requester = AgentID A, provider = AgentID B, cap
  `"map:cap:inference/text-generation"`, started 1780000000000, ended
  1780000042000, outcome 0, metrics {1:1523, 2:812, 3:431, 4:42000},
  rating 92. Sign role 1 with A and role 2 with B over
  `enc(["MAPReceipt", role, core-bytes])`; assert the assembled
  `{1: core, 2: sig_req, 3: sig_prov}` equals the 327-byte vector hex.
- FR-5 (TV-002, consume): `Receipt::decode_bytes` + `verify()` on the
  vector bytes succeeds and yields the same core fields.
- FR-6 (negative suite, per ops §6.2 — every case MUST be rejected with
  the listed error class):
  - N1 tampered byte: flip one byte of TV-001 payload → open() = bad
    signature.
  - N2 wrong external_aad: open TV-001 with aad `b"x"` → bad signature.
  - N3 non-canonical CBOR: re-encode TV-001 body with (a) an indefinite-
    length map, (b) a non-minimal length encoding (e.g. 0x19 0x00 0x01 for
    1), (c) unsorted map keys — decoder must reject all three.
  - N4 truncated envelope: TV-001[..165] → decode error.
  - N5 wrong outer tag: replace tag 18 with 17 → decode error.
  - N6 stale descriptor seq: DescriptorCache ingest seq=5 then seq=4 →
    second returns the Stale/rejected variant.
  - N7 wrong-shard key: `sharded_capability_key(cap, shard(agent_A))` ≠
    the same for a shard byte ≠ shard(agent_A); and shard() must equal the
    first byte of the SHA-256 **digest** of the AgentID bytes (not 0x12).
  - N8 duplicate receipt ingest: TV-002 twice into ReceiptStore → second
    is the Duplicate variant, len stays 1.
  - N9 swapped roles: verify TV-002 with sig_req/sig_prov swapped → verify
    error.
  - N10 wrong role domain: signature over `enc(["MAPReceipt", 2, core])`
    placed in field 2 → verify error (role binding).
- FR-7: expose `fn run_all() -> Report` (counts, failures with case IDs)
  so a future CLI/CI can call it; the `#[test]`s wrap the same functions.

## 3. Non-Functional Requirements

Pinned deps only (sha2, hex, bs58 via existing crates). Vector hex strings
live in `vectors.rs` as `const &str`, copied EXACTLY from
`map-spec/test-vectors/map-test-vectors.md` (single source of truth is
the spec file; the constants must cite it in a comment). Zero warnings.

## 4. System Architecture

Lib crate + tests. Depends on map-identity, map-core, map-discovery,
map-trust. Never a dependency of anything else.

## 5. Component Design

```
conformance/map-conformance/src/
  vectors.rs   // const SEED_A_HEX, PUBKEY_A_HEX, AGENT_A_MB, TV001_HEX,
               // TV001_PAYLOAD_HEX, TV002_HEX, TV002_CORE_HEX,
               // TV002_TASK_ID_HEX, SIG hex constants
  positive.rs  // tv001_produce/consume, tv002_produce/consume (FR-1..5)
  negative.rs  // n1..n10 (FR-6)
  lib.rs       // Report { passed: Vec<&'static str>, failed: Vec<(id, why)> }, run_all()
```

## 6. Interface Specifications

`run_all() -> Report`: no I/O, no panics (catch case failures as data).
Each case: `fn case_<id>() -> Result<(), String>` (Err = human-readable
mismatch, including first differing byte offset for byte-compare cases).

## 7. Data Structures

Byte comparison helper: `fn assert_bytes(expected_hex: &str, got: &[u8])
-> Result<(), String>` reporting `len exp/got` and first-diff offset.

## 8. Algorithms

For N3, construct the malformed encodings by hand as literal byte arrays
(do not build an "evil encoder"); each with a comment showing the
diagnostic notation and why it violates RFC 8949 §4.2.1.

## 9. Edge Cases

- The Hello extensions map is EMPTY — deterministic encoding is `0xa0`;
  ensure the Hello builder doesn't omit the field (key 3 present, empty).
- msg-id is the literal bytes 00..0f, not random — the producer path must
  accept a caller-supplied MsgId.
- Timestamps exceed u32 — must be u64 (`1b 0000019e 70448800`).

## 10. Error Handling

A failing case never aborts `run_all`; all cases always run; Report lists
every failure. `#[test]` wrappers assert individual case success so
`cargo test` pinpoints the case.

## 11. Security Considerations

This crate is the executable statement of invariant I1 (verify received
bytes) and the deterministic-encoding profile. If ANY positive case can
only pass by re-serializing before verification, that is a critical bug
in map-core — stop and escalate, do not adapt the test.

## 12. Performance Considerations

Not applicable.

## 13. Input Specifications

No runtime inputs; all data is compiled-in vector constants.

## 14. Output Specifications

`cargo test` results, and the `Report` struct from `run_all()` (§6) for
programmatic consumption.

## 15. Acceptance Criteria

- ✓ All FR-1..FR-6 cases green under `cargo test -p map-conformance`.
- ✓ TV-001 produced envelope is byte-identical (166 bytes) — assert length
  AND content.
- ✓ TV-002 produced receipt is byte-identical (327 bytes).
- ✓ Every N-case rejects with the expected error CLASS (not just "some
  error"): match on the error enum variant.
- ✓ No blake2 (or any new) dependency.
- ✓ Zero warnings; workspace still green.

## 16. Comprehensive Test Plan

Positive: FR-1 key derivation (4 assertions), FR-2..FR-5 (4 tests).
Negative: N1–N10 (10 tests).
Regression: whole-workspace suite.
Property (optional, only if zero-dep): roundtrip `encode(decode(x)) == x`
for the two vector byte strings — verifies the decoder is
canonical-preserving.
Each test documents: purpose, inputs (vector name), expected outcome,
failure condition (first-diff offset reporting).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> In `map-rs/`, replace the stub `conformance/map-conformance` with the
> crate specified in §5–§11. The authoritative vector bytes are in
> `map-spec/test-vectors/map-test-vectors.md` — copy the full hex strings
> from that file into `vectors.rs` (the excerpts in this document are
> abbreviated; the file is the source of truth). Hardcode the TV-002
> task-id hex; do not add a blake2 dependency. Implement FR-1 through
> FR-7 and all tests in §16. Byte-compare failures must report length and
> first-diff offset. If a positive case fails, debug map-core/map-trust
> rather than adjusting the expected bytes — the vectors are normative; if
> you conclude the VECTORS are wrong, stop and write up the evidence
> (exact bytes, spec citations) instead of changing anything. Run the full
> workspace suite after every fix. Deliver: test report + any
> spec/vector discrepancy write-ups.

## 18. Validation Checklist (reviewer)

```
cd map-rs && cargo test -p map-conformance -- --nocapture
grep -rn "blake2" conformance/                 # expect: comment only, no dep
grep -n "d2845828a2" conformance/src/vectors.rs  # TV-001 hex present, matches spec file
python3 ../map-spec/test-vectors/gen_vectors.py  # optional cross-check if deps available
```

## 19. Known Risks

- R1 (likely): the never-compiled canonical CBOR encoder disagrees with
  the Python cbor2 canonical output on some detail — the vectors decide;
  fixing the encoder is in scope and is precisely the point of this task.
- R2: cbor2 used RFC 7049 ordering; the vectors' keys are small uints
  where RFC 8949 order coincides — noted in memory.md §6; a mismatch
  elsewhere would be a vector-generation issue → escalate, don't patch.

## 20. Handoff Notes

- A discrepancy write-up (R1/R2) triggers the spec-feedback loop: the
  architect patches spec/vectors, TASK-004 regenerates.
- On green: this satisfies ops §6.2's minimum negative suite for impl #1;
  note it in `map-spec/draft-map-ops-01.md` §8 and `memory.md` §5/§7.
- TASK-004 will extend `gen_vectors.py`; this crate's `vectors.rs` must
  then be extended in lockstep (one new const block per new TV).
