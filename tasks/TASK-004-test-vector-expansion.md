# TASK-004 — Test-Vector Expansion (TV-003 … TV-010 + negative vectors)

Status: READY (best after TASK-003 lands, so discrepancies are settled) ·
Priority: P1 · Target: `map-spec/test-vectors/` (spec-side; Python)
Depends on: TASK-003 recommended · Blocks: draft advancement (ops §6.1 matrix)

---

## 1. Problem Statement

The vector suite covers 2 of ~45 assigned message types. Ops §6.1 requires
interoperating implementations to pass a full matrix; independent
implementations (the Go impl, third parties) need normative bytes for at
least one message per protocol family, plus negative vectors that MUST be
rejected. This task extends `gen_vectors.py` and regenerates
`map-test-vectors.md` with eight new positive vectors and a negative
section.

## 2. Functional Requirements

New positive vectors, all as **complete signed envelopes** (COSE_Sign1,
tag 18, same construction as TV-001) except where noted. Reuse Keys A
(requester) and B (provider). All msg-ids are `TV_NNN` deterministic:
16 bytes = `bytes([n]*16)` where n = vector number (e.g. TV-003 → 0x03).
All timestamps = 1780000000000 unless the message semantics need a later
one (+42000 for completion-side messages).

- TV-003 `HELLO_ACK` (0x0002), signed by B. Content mirrors Hello (chosen
  version [0x0100]→0x0100, suite 1, extensions {}, max_message_size
  262144) per core §6 HELLO_ACK shape. Also print the resulting
  transcript hash `TH = SHA-256("MAPv1-transcript" || TV001-env ||
  TV003-env)` as a derived constant (implementations verify negotiation
  with it) and `CB = HKDF-SHA256(salt=TH, ikm=h'00'*32, info="map cb",
  L=32)` using an all-zero 32-byte stand-in exporter (label it clearly:
  stand-in, since no real transport exists in vectors).
- TV-004 `ERROR` (0x0006), signed by B: content per core §9 `ErrorMsg`
  {1: 1003 (replay), 2: false (retryable), 3: "replayed msg-id",
  4: h'000102030405060708090a0b0c0d0e0f' (offending MsgID)}.
  *[Erratum 2026-07-04: originally listed a (code, reason, msg-id)
  layout that contradicted core §9; the generated vector follows the
  draft — key 2 is the retryable bool, text is key 3, MsgID is key 4.]*
- TV-005 `DESCRIPTOR` (0x0100), signed by B: minimal Descriptor per
  discovery §2 {1: AgentID B, 2: seq 1, 3: [0x0100] versions,
  4: [1] suites, 5: ["/map/1.0.0"] multiaddrs,
  6: [{1: "map:cap:inference/text-generation"}] CapEntry list,
  12: [2] zones}.
  *[Erratum 2026-07-04: originally misnumbered fields (caps at 3,
  protocols at 4, issued-at/validity at 5/6); discovery §2 assigns
  3=versions, 4=suites, 5=addresses, 6=capabilities — the vector
  follows the draft.]*
  Also print `shard(B)` and the sharded capability DHT key hex for the
  capability (discovery §6: first byte of SHA-256 digest of AgentID).
- TV-006 `OFFER` (0x0200), signed by A: per tasks §2 `Offer` {1: task-id
  = TV-002's task-id constant, 2: cap URI as above,
  3: {} (empty TaskConstraints)}; PayRef (key 4) omitted.
  *[Erratum 2026-07-04: originally put an InputSpec digest and max_price
  inside OFFER; InputSpec belongs to ASSIGN (tasks §2.1), and pricing
  expectations ride `Offer.4 => PayRef` — now defined in pay §3.1. The
  vector follows the draft.]*
- TV-007 `ASSIGN` (0x0202), signed by A: same task, FlowInit credits
  {msgs: 64, bytes: 1_048_576}, per tasks §2/§3.
- TV-008 `CHUNK` (0x0205), signed by B: seq 0, kind 0, content
  `"Hello, MAP!"` (11 bytes UTF-8), per tasks §3.
- TV-009 `COMPLETE` (0x020D), signed by B: outcome 0, transcript-root =
  the Merkle root over exactly the TV-008 chunk (leaf
  `H(0x00 || enc([0, 0, b"Hello, MAP!"]))` — single leaf IS the root),
  usage {1:1523, 2:812, 3:431, 4:42000}, per tasks §4. Print the root hex
  separately (implementations test their TranscriptBuilder against it).
- TV-010 `PAY_QUOTE` (0x0400), signed by B: per pay §3 `PayQuote`
  {1: quote-id `bytes([0x10]*16)`, 2: 0 (flat pricing), 3: rate table
  {0: ["iso4217:USD", tag4([-6, 4_200_000])]}, 5: [{1:
  "pay:credit-ledger", 2: h''}], 6: expiry 1780000600000}.
  *[Erratum 2026-07-04: originally required "task-id bound", but
  PayQuote had no task-binding field at generation time — the vector is
  an unbound standing quote. Pay §3 has since gained optional key 9
  (TaskID binding, MUST when answering an OFFER); a future vector
  revision should add a bound variant.]*

Negative vectors (bytes that MUST be rejected; publish hex + reason +
required error class, per ops §6.2):

- NV-001 TV-001 with payload byte at offset 60 XOR 0xFF (bad signature).
- NV-002 TV-001 outer tag rewritten 18→17 (malformed).
- NV-003 TV-001 body re-encoded with indefinite-length body map
  (non-canonical; produce by hand-editing bytes, then re-sign with Key A
  so ONLY canonicality fails — this isolates the deterministic-profile
  check from signature checking).
- NV-004 TV-005 descriptor re-issued with seq 0 (stale vs seq 1).
- NV-005 TV-002 with sig fields 2 and 3 swapped (role binding).

## 3. Non-Functional Requirements

- Python 3, deps exactly: pynacl, cbor2, base58 (as today). blake2 via
  hashlib only.
- `gen_vectors.py` stays single-file, linear, printout style unchanged;
  every vector block in the .md must include: diagnostic notation, payload
  hex, Sig_structure hex (or signing-input hex), signature hex, full
  envelope hex with byte length.
- The script MUST verify every signature and every negative property
  before writing the file (assert; exit non-zero on failure), as it does
  today.
- Regenerating twice produces byte-identical output (no clock, no
  randomness).

## 4. System Architecture

Single generator script → single markdown artifact. The markdown remains
the single source of truth; Rust `vectors.rs` (TASK-003) mirrors it.

## 5. Component Design

Refactor `gen_vectors.py` minimally: extract
`def sealed(signer, body_map) -> bytes` (build protected for signer,
payload=enc(body), Sig_structure, sign, tag-18 envelope, verify, return)
and `def vector_block(title, ...) -> str`. Keep TV-001/TV-002 output
byte-identical to the current file (regression guard: assert the two
known envelope/receipt hexes before writing).

## 6. Interface Specifications

Script only; no public API. Exit 0 = file written + all checks passed.

## 7. Data Structures

Message bodies as Python dicts with INTEGER keys exactly as the drafts'
CDDL assigns; never string keys. Envelope body common keys: 0 version,
1 msg-type, 2 msg-id, 3 timestamp, 10 content (as in TV-001).

## 8. Algorithms

Merkle (TV-009): implement leaf/parent/empty exactly per tasks §8.1
(domain bytes 0x00/0x01, SHA-256, odd promotion); single-chunk root =
leaf. Cross-check expectation: independent hand-derivation in a comment.

## 9. Edge Cases

- TV-003 TH depends on the exact TV-001 bytes — compute from the freshly
  generated envelope variable, not a pasted constant.
- NV-003: after hand-editing to indefinite-length, lengths change;
  recompute Sig_structure over the NEW payload bytes and re-sign so the
  signature is VALID and only canonicality is violated.
- Amount encoding: cbor2 tag 4 `[exponent, mantissa]` — confirm the byte
  form matches pay §3's example before publishing.

## 10. Error Handling

Any assertion failure → script exits non-zero and writes NOTHING
(generate to a string, write once at the end — current pattern, keep it).

## 11. Security Considerations

These keys/vectors are test-only (header already says so — keep it).
NV vectors are "MUST reject" bytes: label each with the required error
class from core §9 codes (1000 MALFORMED, 2000 BAD_SIGNATURE, …) so
implementations can assert the class, not just rejection.

## 12. Performance Considerations

Not applicable.

## 13. Input Specifications

None (deterministic). Run from `map-spec/test-vectors/`.

## 14. Output Specifications

Regenerated `map-test-vectors.md`: existing TV-001/TV-002 sections byte
identical; new sections `## TV-003` … `## TV-010`, then
`## Negative vectors (MUST reject)` with NV-001..005; a closing
`## Derived constants` section (TH, CB, shard(B), DHT key, transcript
root) for cross-implementation spot checks.

## 15. Acceptance Criteria

- ✓ `python3 gen_vectors.py` exits 0; running twice → identical file.
- ✓ TV-001/TV-002 hex unchanged vs git (regression assert in-script).
- ✓ Every TV has diagnostic + payload + signing-input + signature + full
  hex + length; every NV has hex + reason + required error class.
- ✓ All field numbers verified against the owning draft (discovery §2,
  tasks §2–§4/§8.1, pay §3, core §6/§9) — cite section per vector block.
- ✓ TASK-003's crate extended with TV-003..TV-010/NV cases in the same
  change or an immediately following one (do not let the two drift).

## 16. Comprehensive Test Plan

In-script asserts (self-testing artifact): every signature verifies;
regression hexes match; TV-009 root equals hand-derived comment value;
NV-003 signature verifies but canonical re-encode differs (proving the
isolation property); file idempotence (generate twice, compare strings).
Downstream: TASK-003 crate consumes every new vector (produce+consume per
TV; reject per NV with the named error class).

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Work in `map-spec/test-vectors/`. Extend `gen_vectors.py` per §2–§14 of
> this document. BEFORE writing any vector, open the owning draft
> (`map-spec/draft-map-discovery-01.md` §2, `draft-map-tasks-01.md`
> §2–§4 and §8.1, `draft-map-pay-01.md` §3, `draft-map-core-01.md` §6 and
> §9) and copy the exact CDDL field numbers into a comment above that
> vector's dict; if the draft's field numbering is ambiguous or missing
> for any field you need, STOP for that vector and record the gap in your
> report (with proposed spec wording) instead of inventing a number.
> Preserve TV-001/TV-002 bytes exactly (in-script regression assert).
> Deps: pynacl, cbor2, base58, hashlib only. Deliver: the updated script,
> the regenerated markdown, and a report listing per-vector spec sections
> cited and any gaps found.

## 18. Validation Checklist (reviewer)

```
cd map-spec/test-vectors
python3 gen_vectors.py && git diff --stat map-test-vectors.md
python3 gen_vectors.py && git diff --quiet map-test-vectors.md && echo idempotent
git diff map-test-vectors.md | grep -E '^-.*(d2845828|a30158bd)'  # expect: no removed TV-001/002 lines
```

## 19. Known Risks

- R1 (high): drafts may under-specify field numbers for OFFER/ASSIGN/
  PAY_QUOTE content maps — this task is DESIGNED to surface that; expect
  spec patches (the feedback loop is the deliverable as much as the
  vectors).
- R2: cbor2 canonical (RFC 7049) vs RFC 8949 ordering divergence for the
  new maps — all keys remain small uints/short strings; verify tag-4 and
  text-key cases explicitly.

## 20. Handoff Notes

Spec gaps found here go to the architect for draft patches (same-change
rule). After landing, extend the conformance crate (TASK-003 §20) and
update `memory.md` §6. These vectors are the interop contract for the
future Go implementation (impl #2).
