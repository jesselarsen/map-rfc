# TASK-026 — Disputes (pay §6): codecs, evidence authentication (CB disclosure), mechanical claim verifier, runtime hooks, vectors

Status: READY (blocked by TASK-024) · Priority: P3 · Target: `map-rs`
(map-pay new module `dispute.rs`; map-taskrun small hooks; conformance;
gen_vectors.py) + `map-go` (codec parity + vectors only)
Depends on: TASK-024 (escrow artifacts are claim-2/4 evidence — REQUIRED
first), TASK-021 (billing arithmetic, claim 3), TASK-008 (receipts,
outcome 4), TASK-019/020 context (CB semantics live in the session layer)
· Blocks: nothing (last open pay-family surface).

Authored by the 2026-07-16 architect session (third pass). Normative
ground truth: **pay §6 (rewritten same session — READ IT FIRST; §6.1–§6.4
define every MUST)**, core §7.3 (CB derivation — the §6.2 disclosure rule
rests on it), tasks §8.1/§8.2 (transcript root recompute, countersigned
checkpoints), trust §3 (receipt outcome 4), pay §3.1/§4.1/§5.2 (the
verifiers claim 4 re-runs). The 2026-07-16 third-pass CHANGELOG entry is
the delta summary.

---

## 1. Problem Statement

Code points 0x0405/0x0406 have existed since -00 with no CDDL, no
verification procedure, and no implementation. The spec now defines them.
Build the dispute layer: message codecs, evidence-item authentication
including the §6.2 CB-disclosure rule (the one genuinely new mechanism —
third-party verification of in-session envelopes), the §6.3 per-claim
mechanical verifier with its three-valued verdict, minimal runtime hooks
(open/respond/record outcome 4), and additive vectors — reusing every
existing verifier rather than re-implementing any check.

## 2. Functional Requirements

### FR-1 codecs (map-pay `dispute.rs`)

`PayDispute { dispute_id: [u8;16], invoice_id: [u8;16], claim: u64,
agree_ref: PayAgreeRef, task: Option<[u8;32]>, statement: Option<String>,
timestamp_ms: u64 }`, `PayDisputeEvidence { dispute_id, items:
Vec<EvidenceItem>, more: Option<bool> }`, `EvidenceItem { kind: u64,
bytes: Vec<u8>, cb: Option<[u8;32]> }` — deterministic-CBOR per the §6.1
CDDL, `to_value`/`from_value` in the house codec style. Decode
fail-closed: claim ∉ 0..=4 and kind ∉ 0..=8 are decode errors (NV-012
pins the claim case); statement length > 1024 rejected. Constants:
`CLAIM_NON_DELIVERY..CLAIM_TERMS_BREACH`, `EV_QUOTE..EV_METHOD_ARTIFACT`.

### FR-2 evidence authentication (§6.2)

`authenticate_item(item, allowed_signers: &[AgentId]) ->
Result<AuthenticatedItem, EvidenceFault>`:

- Envelope kinds (0–4, 6): open the COSE_Sign1 with
  `external_aad = item.cb.unwrap_or(empty)` — reuse the existing
  map-session/map-core envelope-open seam; do NOT reimplement COSE. An
  in-session envelope with absent/wrong CB verifies false → the item is
  **unavailable evidence** (`EvidenceFault::Unverifiable`), never a
  panic and never a verified artifact. Check the signer (envelope kid)
  ∈ `allowed_signers` (the two disputants + the method operator); else
  `EvidenceFault::ForeignSigner` (discard, §6.3).
- Kind 5 (countersigned checkpoint): `bytes` = CBOR
  `[checkpoint-content, requester-countersig]`; verify the inner
  signature over (TaskID, chunk-seq, root) per tasks §8.2.
- Kind 7 (ExecutionReceipt): reuse the TASK-008 dual-signature receipt
  verifier verbatim.
- Kind 8 (method artifact): opaque here; surfaced to the claim engines
  that know how to check it (e.g. `credit_ledger::verify_entry`,
  release-auth checks from TASK-024).
- Same-session coherence: `AuthenticatedItem` records which CB verified
  it; the verifier exposes `sessions(&self) -> …` so a caller can see
  whether all in-session items share one CB (§6.2's provenance bonus —
  informational, not a verdict input).

### FR-3 claim verifier (§6.3)

`pub enum Verdict { Supported, Rebutted, Insufficient }`
`pub fn verify_dispute(dispute: &PayDispute, claimant: &[EvidenceItem],
respondent: &[EvidenceItem], parties: &Parties, now_ms: u64) -> Verdict`

Procedure exactly as §6.3: authenticate everything (FR-2), resolve the
agreement anchor (kind-1 item digest == `agree_ref.digest`, its kind-0
quote == the agreement's quote digest — reuse the 6006 digest machinery
over the item's VERBATIM bytes; missing/failing anchor → `Insufficient`),
then dispatch:

- claim 0: settled receipt/proof in claimant set AND no countersigned
  checkpoint or COMPLETE for the task in respondent set → Supported;
  respondent produces one → Rebutted; claimant shows no payment →
  Insufficient.
- claim 1: recompute the tasks §8.1 Merkle root over the claimant's
  presented transcript chunks (reuse the existing transcript-root
  implementation — do not re-derive the tree) and compare to the
  COMPLETE's root: mismatch → Supported, match → Rebutted, transcript or
  COMPLETE absent → Insufficient.
- claim 2: verifying proof/receipt/release covering the invoice in the
  respondent (payer) set → Rebutted; none, with the claimant's invoice +
  delivery artifacts present → Supported.
- claim 3: highest countersigned usage in evidence (kind-5 key 5) →
  `map_pay::billed(model, rates, usage)`; any presented invoice whose
  cumulative-billed / terms-1/2 amount exceeds it exactly (mantissa
  compare, §3 same-exponent — mismatched exponent is `Insufficient`,
  never converted) → Supported; all within → Rebutted; no countersigned
  checkpoint in evidence → Insufficient.
- claim 4: re-run the existing verifiers over the presented artifacts —
  digest binding (6006), cap (6005 via `verify_watermark`/escrow
  checks), watermark (6009 via `verify_watermark`), escrow/prepaid
  (6010 via TASK-024 `verify_escrow_invoice`), proof validity (6008 via
  the method verifiers). Any rule violation exhibited → Supported;
  everything checks → Rebutted; the rule's inputs absent →
  Insufficient. The dispute's `statement` field NEVER influences the
  verdict (it is not evidence — §6.1).

Determinism: the verdict is a pure function of (dispute, evidence sets,
parties); property to assert in tests — shuffling item order and
duplicating items does not change the verdict (content-address dedup,
§6.1).

### FR-4 runtime hooks (map-taskrun `disputes.rs` — deliberately thin)

- `open_dispute(...) -> Vec<Outgoing>` building 0x0405; idempotent by
  dispute-id.
- `on_dispute(inc) -> Vec<Outgoing>`: acknowledge by responding with
  0x0406 evidence drawn from what the runtime already stores (its
  received envelope bytes — the TASK-017 `Incoming.raw` seam is exactly
  the store; include the session CB per §6.2), chunked with `more` under
  a configurable max-items-per-message.
- `on_evidence(inc)`: accumulate (dedup by content address).
- Outcome-4 hook: expose the dispute state so the receipt path (TASK-008)
  can stamp outcome 4; do not rebuild receipt logic here.
- ERROR 6003 emission when a paused-pending-dispute flow receives further
  billing traffic (provider- or requester-side local policy flag).

### FR-5 vectors (additive; frozen bytes untouched)

- **TV-019** — a PAY_DISPUTE (claim 3, agree-ref to the TV-013 PAY_AGREE
  digest lineage if convenient, else self-contained fixture).
- **TV-020** — a PAY_DISPUTE_EVIDENCE carrying ONE kind-0 item whose
  bytes are the frozen TV-010b PAY_QUOTE envelope verbatim (elegant:
  the vector proves I1 verbatim-carry using bytes the suite already
  pins), `cb` absent (out-of-session).
- **NV-012** — TV-019's bytes with claim = 9, fresh msg-id, re-signed →
  both impls REJECT (fail-closed unknown claim; NV-010/011 recipe).

Rust conformance 44 → 47 (numbering assumes TASK-024 landed); map-go
30 → 33, codec parity only.

### FR-6 end-to-end tests (map-taskrun + map-pay)

1. Claim-3 overbilling, full loop: metered run (TASK-022 pair) with a
   countersigned checkpoint; provider then over-invoices; requester
   opens dispute; both sides exchange evidence; `verify_dispute` →
   Supported. Same run with correct invoice → Rebutted.
2. Claim-0/2 rebuttal shape: payee withholds checkpoint/COMPLETE →
   Supported; produces them → Rebutted; claimant with no payment
   artifact → Insufficient. Dual for claim 2 incl. an escrow release
   round (TASK-024 artifacts) as the rebutting evidence.
3. Claim-1: tampered transcript chunk → root mismatch → Supported.
4. §6.2 CB rules: an in-session envelope item WITH the correct CB
   verifies; with absent or wrong CB → Unverifiable (and a verdict that
   needed it → Insufficient); two items under one CB report one session.
   Drive with a real map-session loopback pair (TASK-020 machinery) so
   the CB is a genuine exporter-derived value, not a fixture.
5. Foreign-signer item discarded; verdict unchanged by its presence.
6. Determinism property: shuffle + duplicate items → identical verdict.
7. Statement-is-not-evidence: adversarial `statement` text flips
   nothing.
8. Griefing bound: evidence beyond the configured cap → Insufficient,
   never unbounded memory (cap enforced streaming, `more` honored).

## 3. Non-Functional Requirements

Zero new dependencies. Zero warnings. No panics on wire input (evidence
bytes are maximally hostile: fuzz-shaped negative tests for truncated/
recursive CBOR inside items). Frozen vectors byte-identical. Evidence
memory bounded by the FR-4 cap.

## 4. System Architecture

map-pay owns dispute wire truth + the verdict engine (pure, reusable by
any arbiter binary later); map-taskrun owns only message sequencing and
storage plumbing over seams that already exist (`Incoming.raw`, receipt
store, session CB accessor — if map-session does not currently expose
the CB post-handshake, add a getter; it is session-local state, not new
wire). Nothing changes in map-core except nothing at all: 0x0405/0x0406
are already registered. The verdict engine consumes existing verifiers
(billed, verify_watermark, verify_escrow_invoice, verify_entry,
transcript root, receipt verify) — NO check is re-implemented, which is
itself an acceptance criterion (grep-level review: the claim-4 arm calls
the existing functions).

## 5–9. Design / Data / Algorithms / Edge Cases

Edge cases to pin: dispute referencing an unknown agreement digest →
Insufficient (never an error path that stalls the responder); two
disputes, same dispute-id, different content → second rejected
(idempotency is by id AND bytes); evidence item whose envelope opens but
whose msg-type contradicts its declared kind → Unverifiable; claim 3
with countersigned usage but a rate-table exponent mismatch →
Insufficient (§3 rule: never convert); claim 1 where the COMPLETE is
in-session and CB-less → Insufficient (the §6.2 teeth); `more: true`
followed by nothing → verdict computed on what arrived (availability
doctrine — you rule on the evidence you have).

## 10. Error Handling

`EvidenceFault { Unverifiable, ForeignSigner, MalformedArtifact }`
internal; wire surface is only 6003 (flow refusal) and the codec rejects.
The verdict engine never errors — insufficiency IS its error channel.

## 11. Security Considerations

§6.2 is the substantive new surface: test CB disclosure both ways with a
REAL session-derived CB (FR-6 #4) — a fixture CB would not prove the
Sig_structure path. The foreign-signer discard prevents evidence
stuffing by third parties; the statement-never-evidence rule prevents
prompt-injection-shaped adjudication (an arbiter LLM reading disputes is
exactly the deployment to expect — the verdict engine gives it no text
channel). Dedup-by-content prevents amplification; the size cap bounds
memory. Disclosing CB does not weaken the session: assert in a comment
(with the core §7.3 cite) that CB is an HKDF output disclosed
post-session, and add a test that a disclosed CB does not verify
envelopes from any OTHER loopback session (session separation, the P2
property, now exercised at the evidence layer).

## 12–14. Performance / IO

Verdict is O(items × verify); irrelevant at dispute scale. All
in-process.

## 15. Acceptance Criteria

- ✓ Workspace + map-go green, zero warnings; frozen vectors
  byte-identical; Rust conformance 47 / map-go 33.
- ✓ FR-6 tests 1–8 + §5–9 edges green; §6.2 exercised with a genuine
  session CB (both accept and reject directions).
- ✓ Claim-4 arm demonstrably calls the existing verifiers (review grep);
  no re-implemented checks.
- ✓ No new deps, no new code points, no map-core changes.
- ✓ tasks/README.md row 026 + memory.md updated on completion.

## 16. Comprehensive Test Plan

Codec round-trips + fail-closed decode matrix (claim/kind/statement
bounds); TV-019/020 produce+consume both impls; NV-012 reject both
impls; FR-2 authentication unit tests per kind; FR-3 verdict matrix
(every claim × Supported/Rebutted/Insufficient — 15 cells, each a named
test); FR-6 integration suite; hostile-CBOR fuzz-shaped negatives.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-026 §FR-1..6 in `map-rs` (map-pay `dispute.rs`,
> map-taskrun `disputes.rs`, conformance, gen_vectors.py) and `map-go`
> (codec parity + TV-019/020/NV-012 only). PREREQUISITE: TASK-024 must be
> DONE (escrow artifacts and `verify_escrow_invoice` are consumed here) —
> if it is not, STOP. Read FIRST: map-spec/draft-map-pay-01.md §6 (all
> four subsections — every MUST is an acceptance test), core §7.3 (CB),
> tasks §8.1/§8.2, this package. REUSE the existing verifiers and the
> envelope-open/transcript-root/receipt code — the claim engines must
> call them, not copy them. Mirror the TASK-021 additive-vector recipe
> (frozen bytes untouched, verify in BOTH impls). The §6.2 CB tests must
> use a real map-session loopback CB, not a fixture. Do NOT add
> dependencies, code points, or an arbiter/remedy layer. Toolchain:
> PowerShell, `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"`,
> `cargo +stable-x86_64-pc-windows-gnu test --workspace`; Go via
> `C:\Program Files\Go\bin`, `go test ./...`. If §6 text is ambiguous or
> a needed seam (e.g. a session CB getter) forces a design choice beyond
> a trivial accessor, STOP and file PENDING-ARCHITECT in
> map-spec/ERRATA.md. Update memory.md §5/§8 + tasks/README.md row 026 on
> completion; commit + push.

## 18. Validation Checklist (reviewer)

```
cargo +stable-x86_64-pc-windows-gnu test --workspace     # green, 0 warn
cd map-go && go test ./...                                # green
python map-spec/test-vectors/gen_vectors.py               # frozen bytes stable
grep -n "verify_watermark\|verify_escrow_invoice\|verify_entry\|billed(" map-rs/crates/map-pay/src/dispute.rs   # reuse, not copies
grep -rn "Verdict::Insufficient" map-rs/crates/map-pay/src/dispute.rs | head  # three-valued, not boolean
```

## 19. Known Risks

R1: the CB getter may not exist on `Session` — adding it is trivial and
in-scope; anything beyond a getter is a STOP. R2: temptation to make the
verdict boolean — the three-valued shape IS the spec (rebuttal claims
depend on it); reviewers check the 15-cell matrix exists. R3: claim-1
transcript recompute can drift from tasks §8.1 if re-implemented — reuse
the existing Merkle code (acceptance-criterion grep). R4: evidence items
are attacker-controlled containers of attacker-controlled envelopes —
fuzz-shaped negatives are mandatory, and `authenticate_item` must be
panic-free by construction. R5: scope creep toward arbiter selection,
timeboxes, or remedies — mesh policy, permanently out.

## 20. Handoff Notes

This closes the last open pay-family surface: after 024 → 025 → 026 the
family is quote/agree/invoice/proof/receipt/metering/prepay/escrow/
disputes, implemented end-to-end with every claim mechanically checkable
by a third party. The verdict engine is deliberately a pure library —
a future `map-cli arbiter` subcommand or an ops-level arbitration
service composes from it without new protocol. Remaining unpackaged
frontier: method-generic payment runtimes (third structurally-different
rail), Go task runtime + metered/escrow/dispute interop scenarios, and
the long-standing non-pay items (NAT/eclipse defenses, streaming
inference, descriptor optional keys, revocation distribution).
