# TASK-014 — Formal Analysis: HELLO, Channel Binding, Delegation Attenuation

Status: READY (after TASK-010) · Priority: P3 · Target: new top-level `formal/` directory
Depends on: TASK-005 (protocol as implemented), TASK-010 (delegation semantics) · Blocks: draft-map-core leaving draft (ops §6.3)

---

## 1. Problem Statement

Ops §6.3 commits the family to machine-checked models before beta. The
three properties that carry the security story are: (P1) downgrade
resistance of HELLO/HELLO_ACK via the transcript hash; (P2) anti-relay
via channel binding (an in-session envelope verifies only on the session
whose CB it was sealed under); (P3) delegation attenuation (a chain can
never widen capability/zone scope). This task builds the models and
either proves the properties or produces counterexample traces — a
counterexample is a SUCCESS OUTCOME for the task (it feeds the
spec-feedback loop), not a failure.

## 2. Functional Requirements

- FR-1 Tool: **ProVerif** (preferred: better fit for the Dolev-Yao
  modeling of P1/P2 and available cross-platform without a JVM;
  Tamarin acceptable if ProVerif's equational theory blocks P3 — record
  the choice and why). Windows note: ProVerif via opam is painful on
  native Windows — running it under WSL2 is APPROVED; document the
  setup so it is reproducible.
- FR-2 `formal/hello-cb.pv`: model agents A/B with Ed25519 signatures
  (standard sign/checksign theory), the HELLO/HELLO_ACK exchange with
  version+suite lists, TH = hash over both envelope messages, CB =
  kdf(TH, exporter), and in-session envelopes signed over
  (CB, payload). Adversary: full Dolev-Yao network control, may also
  open its OWN sessions with both parties. Queries:
  - Q1 (P1): if A and B complete negotiation with versions vA/vB
    believed, then vA == vB == the honest maximum (no downgrade event
    reachable).
  - Q2 (P2): an envelope accepted by B in session s was sealed by its
    kid FOR session s (injective correspondence on (msg, CB)).
  - Q3 sanity/reachability: sessions DO complete (guards against
    vacuous proofs — MUST be a reachability check that succeeds).
- FR-3 `formal/delegation.pv` (or .spthy): model DelegationCert chains
  up to depth 3 with capability scopes as a small lattice (e.g.
  `cap:a`, `cap:a/b`, `cap:c`) and the TASK-010 intersection rule incl.
  fail-closed unknown caveats. Query Q4 (P3): no reachable state where a
  verifier accepts (root R authorizes leaf L for capability c) with c
  outside the intersection of the chain scopes.
- FR-4 Negative controls (mandatory, one per property): deliberately
  weaken the model (drop TH from CB; accept unknown caveats open) and
  show the query then FAILS with a trace — proving the queries have
  teeth. Commit the weakened variants as `*-control.pv` with comments.
- FR-5 `formal/README.md`: tool version pin, run commands, expected
  output per file (true/false per query), property-to-spec mapping
  table (Q1→core §6.2, Q2→core §7.3, Q4→identity §4/TASK-010), and the
  abstraction gaps (what the model does NOT capture: replay windows,
  seq, sizes, real HKDF structure).
- FR-6 Ops §6.3 updated from "plan" to "delivered for P1–P3" with
  results; any counterexample → spec patch proposal in the report
  (do NOT patch normative text unilaterally for a security finding —
  this one class of change needs human review; put it in the report and
  memory.md as BLOCKING).

## 3. Non-Functional Requirements

Models small and commented (target <300 lines each); every `query`
line cross-referenced to the spec section it encodes; no CI integration
required (manual run documented).

## 4–8. Architecture / Design / Interfaces / Data / Algorithms

`formal/` at repo root (spec-adjacent, not Rust). Standard applied-pi
encodings; exporter modeled as a per-session fresh name known to both
endpoints (matches the TASK-012 ruling's strength — model THAT, not a
stronger exporter we don't have; this makes Q2's result honest for the
deployed binding).

## 9. Edge Cases

Model the attacker as a LEGITIMATE registered agent too (insider) — P2
must hold against a peer replaying its own received envelopes into
other sessions.

## 10. Error Handling

N/A (analysis artifacts). Non-termination: bound sessions (!N with
N=3) if needed; record any bounding as an abstraction gap.

## 11. Security Considerations

The FR-4 controls are the guard against vacuous verification — a "true"
on a query whose control also says "true" is meaningless; the README
table must show control results diverging.

## 12–14. Performance / Input / Output

Runtime minutes-grade; artifacts are the .pv files + README + captured
tool output logs committed under `formal/results/`.

## 15. Acceptance Criteria

- ✓ All three property files run with pinned tool version; Q1–Q4 results
  recorded (true, or counterexample trace committed).
- ✓ Q3 reachability true (non-vacuous).
- ✓ All FR-4 controls produce failing queries with traces.
- ✓ formal/README.md complete per FR-5; ops §6.3 updated per FR-6.
- ✓ Any counterexample flagged BLOCKING in memory.md.

## 16. Comprehensive Test Plan

The controls ARE the test plan (self-validating analysis); plus a
from-clean-checkout reproduction run following only the README.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Create `formal/` at the repo root per §FR-1..6. Model the protocol AS
> SPECIFIED in map-spec/draft-map-core-01.md §6–§7 and the TASK-010
> delegation rules — where the implementation and spec differ, the spec
> wins and the difference is a finding. The exporter is modeled per
> §4–8 (per-session fresh name shared by endpoints). Every query maps
> to a named spec clause. Mandatory negative controls per §FR-4 — a
> proof without a failing control is not deliverable. If a real query
> fails: commit the trace, mark it BLOCKING in memory.md, propose (do
> not apply) the spec fix. WSL2 for ProVerif is approved; document the
> exact install. Deliver: models, controls, results logs, README, ops
> §6.3 update, report.

## 18. Validation Checklist (reviewer)

```
cat formal/README.md                      # pins, commands, mapping table
proverif formal/hello-cb.pv               # per README expected output
proverif formal/hello-cb-control.pv       # must FAIL queries
git diff map-spec/draft-map-ops-01.md     # 6.3 plan→delivered
```

## 19. Known Risks

R1: tool setup friction on Windows (mitigated: WSL2 approved). R2:
modeling gap — the per-session-fresh-name exporter abstraction is
GENEROUS to the protocol; if Q2 only holds under it, the README must
say the real-world guarantee is exactly as strong as the transport
authentication (this is already the §19 R2 note in TASK-012). R3:
attractive nuisance of over-modeling — scope is P1–P3 only.

## 20. Handoff Notes

Update memory.md §4 (formal gap closed or BLOCKING finding), §8.
Future formal work: receipts non-repudiation, payment binding chain
(quote→agree→invoice→proof), reservation obligations.
