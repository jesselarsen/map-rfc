# MAP formal analysis (TASK-014 + TASK-027)

> **v2 status (2026-09-25, accountability-layer rework).** Only **P1** (HELLO
> downgrade resistance) and **P2** (anti-relay via channel binding) model
> components v2 keeps, and v2 uses them on the discovery/status plane only
> (binding-libp2p-discovery DHT-14). **P3** (`delegation*.pv`) models
> delegation, which v2 removed (layer §3.3). **P4–P6** (`pay-*.pv`) model the
> retired v1 settlement code and were vacuous anyway (2026-09-18 review).
> These files stay for the record only; no v2 claim rests on them. P2 as
> shipped is vacuous (true by term structure). Its repair (responder nonce +
> injective query) is P3 work in the rework plan, as is a new model of
> receipt/checkpoint non-repudiation.

Machine-checked models of the security properties ops §6.3 identifies as
carrying the security story: **P1** downgrade resistance of HELLO/HELLO_ACK
(core §6.2 transcript hash), **P2** anti-relay via channel binding (core
§7.3), **P3** delegation attenuation (identity §4 / TASK-010), **P4** escrow
charge-authorization symmetry (pay §5.2 `pay:credit-ledger` / `map-pay`
`escrow.rs`), and **P5** dispute watermark binding (pay §6 / `map-pay`
`dispute.rs`). Every query below maps to a named spec clause; every property
has a mandatory negative control demonstrating the query is not vacuously true.

## Tool

**ProVerif 2.05** (Bruno Blanchet, Vincent Cheval, Marc Sylvestre),
`http://proverif.inria.fr/proverif2.05.tar.gz`. FR-1 preferred ProVerif over
Tamarin for the Dolev-Yao style modeling here; nothing in P1–P3 needed
Tamarin's richer equational reasoning, so ProVerif alone was sufficient — no
fallback was required.

### Setup used on this machine (Windows 11 + WSL2, per FR-1's WSL2 approval)

Native-Windows ProVerif build is NOT viable on this box: `opam install
proverif` pulls `lablgtk` (its optional GTK2 trace-visualizer GUI) as a
**hard** dependency, and that fails to compile against the installed
mingw-w64 gcc; separately, building the CLI-only path from source with
`ocamlbuild` hits a native-Windows-opam/msys `mkdir` incompatibility that
could not be resolved. WSL2 (Ubuntu) has none of these problems:

```bash
# In a WSL2 Ubuntu shell:
sudo apt-get update && sudo apt-get install -y opam m4 unzip
opam init -y --disable-sandboxing
eval $(opam env)
opam install proverif -y      # pulls ocaml 4.14.4, lablgtk (GUI), proverif 2.05
proverif -help                # confirms the install
```

Run a model (from the repo root, or `cd formal` first):

```bash
eval $(opam env)
cd formal
proverif hello-cb.pv
```

(`eval $(opam env)` is needed once per shell — opam does not modify PATH
for non-login/non-interactive shells automatically.)

## Files

| File | Property | Honest result | Log |
|---|---|---|---|
| `hello-cb.pv` | P1 (Q1), P2 (Q2), reachability (Q3) | Q1 **true**, Q2 **true**, Q3 reachable | `results/hello-cb.log` |
| `hello-cb-control-downgrade.pv` | P1 control | Q1 **false** (trace) | `results/hello-cb-control-downgrade.log` |
| `hello-cb-control-relay.pv` | P2 control | Q2 **false** (trace) | `results/hello-cb-control-relay.log` |
| `delegation.pv` | P3 (Q4), reachability (Q5) | Q4 **true**, Q5 reachable | `results/delegation.log` |
| `delegation-control.pv` | P3 control | Q4 **false** (trace) | `results/delegation-control.log` |
| `pay-auth.pv` | P4 (Q1), reachability (Q2) | Q1 **true**, Q2 reachable | `results/pay-auth.log` |
| `pay-auth-control-onesided.pv` | P4 control | **does not discriminate** — Q1 true, identical to the honest model | `results/pay-auth-control-onesided.log` |
| `pay-dispute.pv` | P5 (Q3), reachability (Q4) | **Q4 unreachable (vacuous)** — Q3 is proved over zero traces | `results/pay-dispute.log` |
| `pay-meter.pv` | P6 (Q-overbill, Q-underpay, Q-bind), reachability (Q-reach) | **Q-reach unreachable (vacuous)** — all three positives are over an empty trace set | `results/pay-meter.out` |

Reproduce all eight in one pass:

```bash
eval $(opam env)
cd formal
for f in hello-cb.pv hello-cb-control-downgrade.pv hello-cb-control-relay.pv delegation.pv delegation-control.pv pay-auth.pv pay-auth-control-onesided.pv pay-dispute.pv pay-meter.pv; do
  echo "=== $f ==="; proverif "$f"; echo
done
```

### Reading ProVerif's output (important, easy to misread)

ProVerif internally represents a bare `query event(E).` as "prove `not
event(E)`" and reports whether *that negation* holds:

- **Reachability checks (Q3, Q5)**: we *want* the event reachable. ProVerif
  reports this as `Query ... is false` **with a witnessing trace** — that
  "false" is the *success* case (the negation could not be proved because a
  real trace exists). A bare `true` with no trace here would mean the event
  is *unreachable* — i.e. the model is vacuous and Q1/Q2/Q4 would be proved
  over an empty trace set.
- **Correspondence checks (Q1, Q2, Q4)**: `... is true` means the property
  holds (no counterexample found). `... is false` with a trace means a
  counterexample was found — the expected, desired outcome for every
  `*-control.pv` file.

## Property-to-spec mapping

| Query | Property | Spec clause | File |
|---|---|---|---|
| Q1 | P1 — downgrade resistance | core §6.2 (negotiation, transcript hash) | `hello-cb.pv` |
| Q2 | P2 — channel-binding session separation | core §7.3 (CB derivation) | `hello-cb.pv` |
| Q3 | reachability sanity | — | `hello-cb.pv` |
| Q4 | P3 — delegation attenuation (tighten-only) | identity §4 / TASK-010 `validate_chain` | `delegation.pv` |
| Q5 | reachability sanity | — | `delegation.pv` |
| Q1 | P4 — escrow charge-authorization symmetry | pay §5.2 (`pay:credit-ledger`) / `map-pay` `escrow.rs::verify_escrow_invoice` (PHASE_RELEASE 2-of-2 gate) | `pay-auth.pv` |
| Q2 | reachability sanity | — | `pay-auth.pv` |
| Q3 | P5 — dispute watermark binding | pay §6.1 (PayDispute key 2 `invoice-id`, key 4 `PayAgreeRef`) / `map-pay` `dispute.rs` | `pay-dispute.pv` |
| Q4 | reachability sanity | — | `pay-dispute.pv` |
| Q-overbill | P6 — metering arithmetic & cap (no settled over-bill/over-cap) | pay §3 (same-exponent rule), §4.1 steps 5–6 | `pay-meter.pv` |
| Q-underpay | P6 — settled-not-submitted (delivery gated on settlement) | pay §4.2 | `pay-meter.pv` |
| Q-bind | P6 — PAY_AGREE digest binding (settle only agreed quote) | pay §3 | `pay-meter.pv` |
| Q-reach | reachability sanity | — | `pay-meter.pv` |

## Modeling notes (read before trusting the results)

### Q1 — downgrade resistance

The initiator's safety recheck (`negotiate(&cfg.hello, &ack.as_hello())`
re-run on the initiator side, matching `map-session/src/session.rs`) is
what Q1 actually tests. Without the fresh msg-id/SessionID nonces feeding
into it, the query would be near-tautological (the event only ever fires
inside a guarded branch); what makes it meaningful is that the **control**
(`hello-cb-control-downgrade.pv`) removes exactly that recheck and shows a
concrete attack becomes reachable: the attacker, dialing B *as the leaked
insider agent C* with a low-version offer but the exact msg-id and
SessionID observed from a real A→B HELLO, obtains a genuine B-signed
low-version ack that satisfies every remaining check (signer, claimed peer,
SessionID echo) and splices it into A's waiting session. This demonstrates
that **SessionID-echo alone does not prevent downgrade** — msg-id/SessionID
are observable, attacker-reusable values, not secrets; the content recheck
is the real defense.

### Q2 — three formulations tried, only the third is sound

Getting Q2 right took three iterations, each teaching something:

1. **First attempt** — a plain `EnvelopeSealed`/`EnvelopeAccepted`
   correspondence over *any* claimed identity. ProVerif immediately found a
   "counterexample": the leaked insider agent C completing a fully
   legitimate session with B under its own key. This is not a protocol
   flaw — an insider legitimately talking to B is allowed — it was a
   **query-scoping bug**: authentication/anti-relay correspondences must
   always be stated relative to the honest principals (the same convention
   Needham-Schroeder-Lowe-style analyses use), not universally over every
   registered identity. Fixed by pinning the query to the specific honest
   pair `(pk(skA), pk(skB))`, declared as top-level `[private]` names so
   the query can name them directly, and by making every responder session
   pin an `expectedPeer` (matching the real `SessionConfig.expected_peer`
   — a real session always knows in advance who it's talking to via
   transport-layer identity binding, TASK-012 FR-3).
2. **Second attempt** — the same correspondence as an `inj-event`
   (session-unique) query, scoped to the honest pair. This *also* failed,
   but for an uninteresting reason: with the responder role able to run
   unboundedly many parallel sessions (`!`), the attacker can trivially
   replay the *identical* real handshake bytes into a second parallel
   responder instance, which — absent any msg-id/timestamp freshness
   check — accepts the "duplicate" delivery. That is exactly what
   map-core's `ReplayCache` (msg-id + timestamp window, core §7.1) exists
   to prevent, and it is explicitly out of this model's scope (TASK-014
   §FR-5). ProVerif's own output confirmed the *plain* (non-injective)
   version of the same query held true, isolating the issue precisely.
3. **Realization** — the plain correspondence, while true, turned out to be
   **independent of CB strength entirely**: it only proves "A genuinely
   signed these bytes at some point," which holds from signature
   unforgeability alone, regardless of how CB is derived (even a
   catastrophically weak `cb = constant` would satisfy it, since the
   `cat3(tagMsg, cb, payload)` blob is signed atomically — an attacker
   cannot splice a different payload under a genuine A signature without
   A's key). Neither formulation actually tested "does the channel binding
   separate sessions."

**The query that actually isolates P2** (and what `hello-cb.pv` now uses):
a direct **CB-collision** check. Every time the responder completes a
handshake it records `(msgid, cb)` in a table and checks whether any
*other, distinguishable* (different msgid) prior session produced the
*same* cb; if so, `CbCollision` fires. Q2 asks whether `CbCollision` is
ever reachable — this is a proof over **unbounded** sessions (`!`, not
bounded), unlike Q1/Q3/Q4/Q5 which are process-structural, and it directly
tests the thing P2 is about: does the transcript hash TH give each session
a distinct, non-reusable CB? `hello-cb-control-relay.pv` drops TH from the
CB derivation (`cb = kdf(zeroConst, exporter)`, `exporter` being the
identity-pair-only, session-invariant public value per the TASK-012 FR-4
ruling) and the collision becomes reachable, confirming the query has
teeth.

**Practical takeaway for future formal work on this family**: correspondence
queries over signed-message events are the wrong tool for "does derived
value X separate sessions" — they collapse to unforgeability once the value
is embedded inside a signed blob. A direct table-based collision/uniqueness
check on the derived value itself is the right tool.

### Q4 — same honest-principal scoping lesson, faster this time

The first version of `delegation.pv` had the identical scoping bug as Q2's
first attempt: with DK's key leaked, the attacker can construct an entirely
self-consistent two-cert chain using **its own** freshly-generated keys as
both "root" and "DK," which trivially satisfies every verifier check and
authorizes whatever scope the attacker likes — not a delegation-attenuation
violation (that fabricated chain was never rooted at the real root; nothing
was actually widened). Fixed the same way as Q2: `skRoot` is a top-level
`[private]` name, and Q4 is stated relative to `pk(skRoot)` specifically —
"can a chain that genuinely traces back to the real root ever be tricked
into authorizing more than it granted."

### `capOK` as a `reduc` — rejected, replaced with nested `if`

An initial attempt modeled capability-lattice containment (`capOK(child,
parent)`) as a `reduc` function pattern-matching on the free names
`capA`/`capAB`/`capC` directly. ProVerif rejects this: `reduc` LHS patterns
must be built from **constructors**, not `free`-declared atomic names (the
same restriction hit the initial `negotiateVal` attempt in `hello-cb.pv`,
resolved the same way there). The working replacement is a nested
`if`-`then`-`else` **term** (ProVerif supports `if` as a term expression,
not just a process construct) with a **freshly `new`-generated sentinel**
as the final `else` — fresh per verifier invocation, so the attacker can
never deliberately choose a scope value that collides with it, unlike a
`free`/public sentinel would allow.

### Q1 (P4) — escrow charge-authorization symmetry

The property is that a `PHASE_RELEASE` debit (pay §5.2) requires BOTH a
previously-accepted payer HOLD and the payee's canonical release trigger
(`complete_verified || cancel_checkpoint_usage` in `escrow.rs`). Modeled as a
**2-of-2 gate**: the payer authorizes a HOLD (event `PayerAuthorized`), and
the payee authorizes release by signing a release token for the SAME
`invoice-id` (verified by the payer; only then does `PayeeAuthorized` fire and
`ChargeApplied` become reachable). Pause (pay §5.1) takes a separate branch
with no charge path (fail-closed) — modeled with the positive `NotPaused`
event because ProVerif does not support negated events (`not event(...)`) in
`if` guards or query conclusions; the honest non-paused branch emits
`NotPaused` and the paused branch emits `GatePaused`, so "charge ⟹ not paused"
is faithfully expressed as "charge ⟹ `NotPaused`."

The control (`pay-auth-control-onesided.pv`) drops the expected-payee binding:
the payer charges on ANY validly-signed release token. Q1 there is still
reported `true` as a correspondence, but the bare `not event(ChargeApplied)`
query is `false` WITH a trace (the insider, whose key is leaked, signs its own
release for the honest payer's invoice) — demonstrating the query has teeth
and the honest 2-of-2 gate is what blocks one-sided debit. **No BLOCKING
finding**: the honest model proves the symmetry property.

### Q3 (P5) — dispute watermark binding

The arbiter (honest, top-level `[private]`) records every observed `PayDispute`
in `table contested(invId, agreeRef)` and emits `DisputeResolved` ONLY via a
`get contested(=invId, =agreeRef)` lookup — a direct table-uniqueness check on
the observed watermark, which (per the Q2 lesson §165-169) is the right tool; a
correspondence over signed blobs would collapse to unforgeability and not
isolate the binding. The §6.3 claim-code rebuttal logic (claims 0–4) is out of
scope; the model binds resolution to the contested watermark only. Q4 confirms
reachability (honest disputes DO resolve), so Q3 is not vacuous.

## Abstraction gaps (what these models do NOT capture)

- **msg-id / timestamp replay cache** (core §7.1 `ReplayCache`, ±120 s / 10
  min window): entirely out of scope (TASK-014 §FR-5). This is *why* Q2 is
  phrased as a CB-collision check rather than an injective delivery
  correspondence — see the Q2 modeling notes above. A future model
  including the replay cache could recover the stronger injective property.
- **Sequence numbers, frame sizes**: not modeled (§FR-5).
- **Suite/extension negotiation**: structurally identical to version
  negotiation (P1); omitted from `hello-cb.pv` for model size, not because
  it's materially different.
- **Delegation chain depth**: bounded to 2 links (root→DK→SK) rather than
  the full "up to depth 3." Every link enforces the identical tighten-only
  check, so depth 2 is the minimum that exhibits a genuine attenuation
  violation and is sufficient to validate the mechanism (§10 "scope is
  P1–P3 only, avoid over-modeling").
- **Zone attenuation**: structurally identical to capability attenuation
  (same tighten-only intersection); not separately modeled.
- **Fail-closed unknown caveats** (identity §4): modeled in the *honest*
  `delegation.pv` verifier (an unrecognized caveat value is rejected) for
  fidelity to the spec, but does not have its own dedicated negative
  control — FR-4 asks for one control per property, and the dedicated
  `delegation-control.pv` targets the headline "attenuation" property
  (widening), which is what P3 is literally named for. A caveat-specific
  control (weaken the fail-closed check, show an unrecognized-caveat chain
  gets accepted) would be a natural, cheap follow-up.
- **Exporter strength**: modeled as PUBLIC information — `h(initiator-AID
  || responder-AID)` — matching the real TASK-012 FR-4 ruling for
  `/map/1.0.0` exactly (real AgentIDs are public keys; the exporter is not
  a secret TLS-style value). This makes Q2 an honest test of the *actually
  deployed* binding, not a stronger hypothetical one. See core §10's
  channel-binding paragraph and TASK-012 §19 R2's residual note (CB binds
  the identity *pair*, not the specific connection instance — the
  per-message replay cache is what prevents cross-connection replay within
  the window in the real deployment; this model does not include that
  cache, per the point above).
- **Two-party HELLO only**: no gateway, no delegated session establishment
  combined with HELLO (TASK-010's `SessionConfig.delegation_chain` is a
  separate mechanism from what P1/P2 test here).
- **Payment — continuous metering / credit-window arithmetic** (pay §4): the
  quote→agree→invoice→proof binding and exact watermark billing (no settled
  over-bill, settled-not-submitted, PAY_AGREE digest binding) ARE now modeled
  and proved in `pay-meter.pv` (TASK-028, P6). The remaining unmodeled corner
  is the **checkpoint-anchored cancel accounting** (tasks §8.2/§11 → pay §4.2
  cancel: owed amount anchors to the last countersigned checkpoint, always ≤
  the final unpaid watermark) — that inequality is argued structurally but not
  yet machine-checked (ops §6.3 lists the metering formal work; P6 covers the
  steady-state path).
- **Payment — dispute claim-code evaluation** (pay §6.3): the rebuttal-shaped
  claim logic (claims 0–4) reuses the existing verifiers but is out of scope
  for `pay-dispute.pv`; only the watermark-binding (resolution ⟹ contested)
  is proven.
- **Payment — `pay:credit-ledger` operator trust**: the ledger operator is
  named and chosen, not protocol-trusted (C9 fix); the model treats the
  escrow as a 2-of-2 gate and does not model operator misbehavior.

## Results

No counterexample was found for P1 and P3 in the honest models. **P2 as
shipped is vacuous** (the CB-collision query is true by symbolic term
structure, and its different-msgid filter excludes the one collision the model
admits); it becomes meaningful, and strictly stronger, once the HELLO_ACK
carries its own fresh responder nonce and the query is the injective
correspondence. **P4, P5 and P6 are vacuous**: P4's antecedent events all fire
in the same process branch as its conclusion, and P5's and P6's guarded events
are unreachable — see the Files table. The
three mandatory negative controls (hello-cb downgrade, hello-cb relay,
delegation widening) and the P4 onesided control fail as designed, with the
exact witnessing traces captured in `results/`. **No BLOCKING finding to
record.**
