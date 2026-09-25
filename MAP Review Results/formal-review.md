# MAP formal-methods review: are the ProVerif results meaningful or vacuous?

Reviewer role: formal methods. Scope: `formal/` (nine `.pv` models, `results/`, `README.md`), the claims about them in `draft-map-ops-01` §6.3, `draft-map-intro-01`, `README.md`, `SUBMISSION.md`, `memory.md`. Tool: ProVerif 2.05 under WSL2, re-run on HEAD 56bce96. All planted-bug work was done in a detached worktree; `git checkout -- .` after every experiment, baseline re-verified green before the next one; nothing committed or pushed.

## Headline

Of the six properties, **one is meaningful but proves less than claimed (P1), one is unsound as formulated but repairable and the repaired version proves more than claimed (P2), one is a finite truth-table check (P3), and three are vacuous (P4, P5, P6)**. Two of the three vacuous models prove their properties over an empty trace set: their own stored logs show the "reachability sanity" event is unreachable, contradicting the README table. The README, `SUBMISSION.md`, `README.md` (root) and intro draft all state "P1–P6 are machine-checked". That sentence must not go to the ISE as written.

Per-property verdicts:

| Property | Verdict | One-line reason |
|---|---|---|
| P1 downgrade resistance | **WEAK** | Q1 is `event ==> v = v2` where the event is emitted only inside `if chosen = v2`. Tautology by construction; what it verifies is a map-session implementation recheck, not the spec's TH mechanism. |
| P2 anti-relay / CB | **VACUOUS as shipped, repairable** | CB-collision query is true by symbolic term structure; the `suchthat otherMsgid <> msgidH` filter excludes the one collision that does exist in the model. The real property (injective anti-relay) is FALSE in the shipped model and TRUE once the ACK carries the responder's fresh msg-id that the spec and Rust both already have. |
| P3 delegation attenuation | **WEAK** | Tighten-only is a four-row hand-written table over three constants; root grants one scope; Q4 checks scope only. Removing the chain-link check or the fail-closed caveat check leaves Q4 green. |
| P4 escrow 2-of-2 | **VACUOUS** | All three antecedent events are emitted sequentially in the same process branch as the conclusion. Pause is a parallel branch, so `NotPaused` always fires. The control does not change any query result. Dropping the release-signature check leaves Q1 green. |
| P5 dispute watermark binding | **VACUOUS** | `DisputeResolved` is unreachable in the shipped model (arbiter checks the dispute signature against its own key). Q3 is proved over zero traces. No invoice, watermark, or agreement is modeled. |
| P6 metering arithmetic | **VACUOUS** | `Settled` is unreachable (honest provider mints a random `cb` that never equals `billed_capped(u)`). `rate` and `cap` are declared and never used. `billed_capped` is a public `[data]` tag, not arithmetic. Making the honest flow reachable falsifies Q-overbill. |

## Claim-vs-reality (checkable claims that are false)

| Claim | Where | Reality (from the author's own logs and my re-run) |
|---|---|---|
| `pay-dispute.pv`: "Q4 reachable" | `formal/README.md` Files table | `results/pay-dispute.pv.log:64` `RESULT not event(DisputeResolved(...)) is true.` Unreachable. |
| `pay-meter.pv`: "Q-reach reachable" | `formal/README.md` Files table | `results/pay-meter.out:194` `RESULT not event(Settled(...)) is true.` Unreachable. |
| `pay-auth-control-onesided.pv`: "Q1 **false** (trace)" | `formal/README.md` Files table | `results/pay-auth-control-onesided.pv.log:283` `Query ... is true.` Identical summary to the honest model. The README's own modeling note (§"Q1 (P4)") admits this and re-labels the honest model's *expected* reachability result as the "control failing". |
| "Formal properties P1–P6 are machine-checked" | `map-spec/SUBMISSION.md:89`; `README.md:184-190` ("machine-checked and proved") | P4–P6 are vacuous per above. |
| "Delivered for P1–P3 … payment binding chain … not yet modeled" | `draft-map-ops-01` §6.3 | Contradicts `formal/README.md` and `SUBMISSION.md`, which claim P4–P6. The ops text is stale but closer to the truth. |
| "Reproduce all eight in one pass" | `formal/README.md` | Nine files listed. NIT. |
| "each carries a mandatory negative control that fails with a concrete witnessing trace" | `draft-map-ops-01` §6.3; `formal/README.md` Results | True for P1, P2, P3 controls. False for P4 (control does not discriminate). No controls exist for P5, P6. |

## 1. Property-by-property analysis

### P1: downgrade resistance (`hello-cb.pv` Q1)

Informal property (core §6.2): an attacker cannot cause two honest peers that both support v2 to complete a session at v1; the spec's mechanism is TH bound into CB so that "every subsequent signature check fails".

Query: `event(NegotiatedByInitiator(pkA, pkB, v)) ==> v = v2` (`hello-cb.pv:181-182`).

Judgement: the event is emitted at line 113, three lines after `if chosen = v2 then` (line 108). The query cannot be false in any model with that guard, whatever the attacker does and whatever the cryptography is. The README concedes this ("would be near-tautological") and argues the control gives it meaning. That inverts the logic: the control shows the *guard* is load-bearing, not that the *query* measures downgrade resistance. Evidence that Q1 measures nothing about the handshake's security:

- E1p (initiator drops the HELLO_ACK signature check entirely): Q1, Q2, Q3 unchanged. An initiator that accepts an unsigned ACK is "downgrade resistant".
- E3p (responder drops the HELLO signature check): unchanged.
- E2 (responder drops the expected-peer pin): unchanged.

Second problem: the mechanism verified is not the spec's. The initiator-side `negotiate(&cfg.hello, &ack.as_hello())` recheck exists in `map-session/src/session.rs:196`, but core §6.1/§6.2 contains no MUST requiring the initiator to re-derive the choice. The spec's stated defence is TH. In the model TH plays no role in Q1 at all: the control still computes TH and still fails Q1. So Q1 verifies a Rust implementation choice and says nothing about whether the specified protocol (TH-in-CB) is downgrade resistant. As a spec reviewer I would ask: what happens in a conforming implementation that trusts the ACK's chosen version and relies on TH as §6.2 says? Answer from the control: it "completes" at v1 and then fails on the first CB-bound envelope. That is fail-safe but it is detection, not prevention, and the drafts describe it as prevention.

Third problem, found with an agreement query (E7): adding `NegotiatedByInitiator(A,B,v) ==> NegotiatedByResponder(A,B,v)` to the honest model is **FALSE**. Trace (abridged from my run): A sends HELLO(msgid m, sid s); the insider C sends HELLO(m, s) to B's C-session; B answers `sign(cat5(tagAck, pkB, v2, m, s), skB)`; the attacker delivers that ACK to A; A passes every check and fires `NegotiatedByInitiator(A, B, v2)` although B has only ever negotiated with C. This is faithful to the spec: handshake bodies carry no recipient (`session.rs:573-580` `recipient: None`; core §4.1 says key 4 is required only in-session), and SessionID is initiator-chosen and attacker-copyable. Consequence in the real protocol: TH differs, so A's first in-session envelope is rejected with 2000/1006; no authorisation break, but A's handshake "completes" without B's participation, and core §6.1's sentence "the HELLO exchange is therefore itself authenticated and MITM-resistant" is not true of the exchange alone. Severity: MAJOR against that sentence; MINOR as a protocol issue (fix: make Body key 4 recipient REQUIRED in HELLO_ACK, or state that handshake authentication completes only at the first CB-bound envelope).

Verdict: **WEAK**. Not vacuous (Q3 reachability is genuine, and the control fails for the stated reason), but the query is guard-equals-conclusion, verifies an implementation detail rather than the spec mechanism, and misses a real agreement failure in the same model.

### P2: anti-relay via channel binding (`hello-cb.pv` Q2)

Informal property (core §7.3): "an envelope recorded on one session cannot verify on another".

Query: `event(CbCollision(m1, m2, cb))` unreachable, where the responder fires `CbCollision` if a *different-msgid* prior session produced the same cb (`hello-cb.pv:156-159, 201-202`).

Is the substitution of a CB-uniqueness query for an authentication property sound? No, for three independent reasons.

1. **It is true by term structure.** `cb = kdf(h(cat2(helloEnv, ackEnv)), exporter)` and `helloEnv = sign(cat5(..., msgidH, ...), skA)`. In the symbolic model `h` and `kdf` are free constructors, so two cb terms are equal iff their arguments are syntactically equal, which forces equal `msgidH`. The query therefore proves exactly "msgidH occurs syntactically inside cb". The control (drop TH) removes the occurrence and the query flips. That is a syntax check, not a security proof; no attacker capability is involved.
2. **The filter hides the collision that does exist.** E6 (remove `suchthat otherMsgid <> msgidH`): `CbCollision` is **reachable** in the shipped honest model. The attacker replays A's HELLO to two responder instances; both produce identical ACKs (the model's ACK reuses the initiator's msgid, so two B-sessions are byte-identical) and identical CB. A sealed envelope from A verifies on both sessions. That is precisely the §7.3 property failing, and the query was scoped so as not to see it.
3. **Uniqueness is not authentication.** Even if CB were unique per session, the property §7.3 states is about envelope acceptance, and nothing in Q2 mentions `EnvelopeAccepted`/`EnvelopeSealed` (both declared, neither queried).

The README's explanation for abandoning the injective correspondence ("contaminated by the out-of-scope msg-id replay cache") is an artefact of the model, not of the protocol. The spec's TH covers whole HELLO_ACK envelope bytes (core §6.2), every envelope carries its own fresh MsgID (core §4.1 key 2), and the Rust responder mints one for HELLO_ACK (`session.rs:256` `ids.next_msg_id()`). The model instead echoes the initiator's msgid as the ACK's identity (`hello-cb.pv:148`). Results:

- E6d: shipped model + `inj-event(EnvelopeAccepted(pkA,pkB,cb,p)) ==> inj-event(EnvelopeSealed(pkA,pkB,cb,p))`: **FALSE** (as the README reports).
- E6c: same query, ACK carries one fresh responder nonce `nB`: **TRUE**, with no replay cache modelled.

So the correct P2 property is provable in ProVerif for the protocol as specified, and it is a strictly stronger result than what is currently claimed. The current claim ("no two distinguishable sessions ever derive the same CB") is both weaker than needed and, once "distinguishable" is defined honestly, false in the model.

Verdict: **VACUOUS as shipped; MEANINGFUL after the E6c change.** Recommended fix: (a) give the responder's ACK its own fresh nonce, mirroring the real msg-id; (b) replace Q2 with the injective correspondence scoped to the honest pair; (c) keep the relay control, which will still fail (`cb = kdf(zeroConst, exporter)` makes CB session-invariant, so the replayed envelope verifies on a second session). I did not run (c); it is a one-line check.

### P3: delegation attenuation (`delegation.pv` Q4)

Informal property (identity §4): a child cert's scope MUST be a subset of the parent's; unknown caveat keys MUST invalidate the chain; validity windows, max-depth, kid == leaf.

Query: `event(VerifierAccepted(pk(skRoot), leaf, scp)) ==> scp = capAB` (`delegation.pv:77-78`).

Judgement: the verifier's "tighten-only intersection" (`delegation.pv:126-131`) is a nested `if` enumerating four allowed (scope2, scope1) pairs over three free constants. Root grants only `capAB`. With `scope1 = capAB` the only accepted `scope2` is `capAB`, so Q4 reduces to checking one row of a table the author wrote. No prefix/segment matching (discovery §4), where real bugs live (`map:cap:a` vs `map:cap:ab`), is modelled. Nothing about depth, validity windows, max-depth (key 7), or `kid == leaf` is modelled.

Planted bugs:

- E8 (remove `if issuer2 = subject1`, the chain-link check): Q4 **stays true**. To show this is a real bug and not a harmless mutation: E8c0 (DK's key not leaked, DK issues nothing) makes `VerifierAccepted(root, ·, ·)` unreachable in the honest verifier; E8c1 (same, plus chain-link removed) makes it **reachable**. The attacker's self-signed cert2 is accepted as rooted at the real root, at scope `capAB`, and Q4 is satisfied because Q4 only inspects the scope value.
- E9 (remove both fail-closed caveat checks): Q4 **stays true**. The README acknowledges there is no caveat control; this confirms the honest query cannot see the caveat rule at all, so the claim in ops §6.3 "including fail-closed unknown caveats" is not backed by any query.

The control (`delegation-control.pv`) fails for the intended reason (verifier trusts scope2; DK issues `capA`).

Verdict: **WEAK**. Sound as far as it goes, but what it goes to is a four-row table; the query is blind to the two other chain-validation rules the model does implement.

### P4: escrow charge-authorisation symmetry (`pay-auth.pv` Q1)

Informal property (pay §5.2): a PHASE_RELEASE debit requires a prior payer hold and the payee's canonical release trigger (COMPLETE + verified transcript-root).

Query: `ChargeApplied(P,E,inv,amt) ==> PayerAuthorized(P,inv) && PayeeAuthorized(E,inv) && NotPaused(inv)` (`pay-auth.pv:111-115`).

Judgement: in the payer process, `PayerAuthorized` (line 76), `NotPaused` (line 80), `PayeeAuthorized` (line 87) and `ChargeApplied` (line 88) are emitted in sequence by the same process on the same branch with the same arguments. The correspondence holds by control flow; it would hold with every cryptographic check deleted. Specifically:

- `PayeeAuthorized` is emitted by the **payer**, not the payee. "Payee authorised" means "the payer decided to charge". A query whose antecedent and consequent are the same decision proves nothing.
- Pause: lines 77-89 put `GatePaused(inv)` and the charging branch in **parallel** (`|`), not in choice. Both branches run for every invoice; `NotPaused` fires unconditionally; the charge happens whether or not `GatePaused` fired. Pause is not modelled.
- The control (`pay-auth-control-onesided.pv`) leaves every query result identical to the honest model (both logs: Q1 true, reachability false-with-trace). The README's modeling note re-describes the reachability trace, which is the *expected* honest outcome, as the control "having teeth".
- E11p (payer drops the release-token signature check): Q1 **stays true**. A payer that charges on an unsigned release passes the "2-of-2" proof.

A sound formulation exists and I verified it. Add `event ReleaseIssued(pkey, bitstring)` in the **payee** process before `out(c, relEnv)`, and query `event(ChargeApplied(pk(skP), E, inv, amt)) ==> event(ReleaseIssued(E, inv))`. Results: honest model (E12a) **TRUE**; with E11p's bug (E12bp) **FALSE**. That query is what "charge implies payee authorised" means. It still would not model pause, amounts, hold/release phase ordering, or the ledger; those need a real state model.

Verdict: **VACUOUS**.

### P5: dispute watermark binding (`pay-dispute.pv` Q3)

Informal property (pay §6.1/§6.3): a resolution is bound to the disputed `(invoice-id, PayAgreeRef)` pair and to the agreement's watermark/rate table.

Query: `DisputeResolved(A, inv, ref) ==> Contested(inv, ref)` (`pay-dispute.pv:82-84`).

Judgement: `DisputeResolved` is **unreachable** in the shipped model (`results/pay-dispute.pv.log:64`, reproduced). Cause: `pay-dispute.pv:55` checks the dispute signature against `pk(skA0)`, the arbiter's own key, while the only disputant signs with `skDishonest`. Q3 is a theorem about zero traces, and the README's "Q4 confirms reachability, so Q3 is not vacuous" is contradicted by its own log. Even after fixing reachability (E13a), `insert; event Contested; get; event DisputeResolved` are consecutive statements in one process with identical arguments, so Q3 remains a control-flow tautology: E13b (resolve without the table lookup) keeps Q3 **true**. Beyond that, the model contains no invoice, no watermark, no agreement digest and no evidence, so the phrase "watermark binding" does not correspond to anything in the file. The "table-uniqueness pattern" imported from Q2 is misapplied: in Q2 the table compared a derived value across sessions; here it looks up a value the same process inserted one line earlier.

Verdict: **VACUOUS**. No control exists.

### P6: metering arithmetic and quote binding (`pay-meter.pv`)

Informal property (pay §3/§4): cumulative-billed = usage x rate with exact arithmetic, cumulative-billed <= cap, delivery gated on settlement, settle only the agreed quote digest.

Queries: Q-overbill, Q-underpay, Q-bind (`pay-meter.pv:154-170`).

Judgement: `Settled` is **unreachable** (`results/pay-meter.out:194`, reproduced), so all three "true" results are over an empty trace set. Cause: the honest provider mints `new cb` (line 106) and the requester accepts only `cb = billed_capped(u)` (line 139), which a random name never equals; the leaked insider provider cannot help because the requester hard-codes `pk(skPv)` (line 135). Further:

- `rate` and `cap` (lines 85-86) are declared `[private]` and **never referenced**. The comments at lines 61-65, 80-84, 126, 150-153 describing them as what makes over-billing unforgeable describe code that does not exist.
- `billed_capped` is declared `[data]` (line 66): a public constructor the attacker can apply to any term. It is a tag, not a function of rate or cap. There is no arithmetic and no comparison anywhere in the model; "over-bill" and "over-cap" are not expressible in it.
- E14 (requester drops the `cb = billed_capped(u)` check): Q-overbill flips to **FALSE**, but only because the honest provider's random `cb` now gets settled; this is the model becoming reachable, not an attack.
- E15a (make the honest provider emit `billed_capped(u)`, the obvious fix): `Settled` becomes reachable and Q-overbill is immediately **FALSE** with no bug planted. Reason: the provider's proof branch (line 118) emits `Settled(qd, uP, amt_dummy(uP))`, whose third argument can never equal `billed_capped(uP)`. The headline query is inconsistent with the model's own event shapes and was "proved" only because nothing was reachable.
- E15c (provider tags `cb` with a different usage than it invoices): everything returns to "true" because the requester rejects and nothing is reachable again. "Requester rejects" and "proved" are the same observation in this model.

Verdict: **VACUOUS**. No control exists.

## 2. Negative controls audit

| Control | Fails? | For the intended reason? |
|---|---|---|
| `hello-cb-control-downgrade.pv` | Yes, Q1 false | Yes: trace is the insider splice with copied msgid/SessionID. But note it also demonstrates that the *spec's* mechanism (TH) is not what Q1 measures, since TH is still computed in the control. |
| `hello-cb-control-relay.pv` | Yes, Q2 false | Yes, but trivially: removing TH removes msgid from the cb term, so the syntactic-equality query fires. It confirms a syntax dependency, not a security property. |
| `delegation-control.pv` | Yes, Q4 false | Yes: DK issues `capA`, verifier trusts scope2. Sound. |
| `pay-auth-control-onesided.pv` | **No.** Q1 true; only the reachability query is false-with-trace, exactly as in the honest model. | The control does not discriminate. |
| P5, P6 | No control files exist. | README table implies controls per property; ops §6.3 says "each carries a mandatory negative control". |

## 3. Planted-bug log

All in the disposable worktree, one mutation at a time, `git checkout -- .` after each, baseline re-run and compared (md5 of the RESULT lines) before the next. Restore-plus-green took 320-475 ms in every case (each model verifies in well under a second). Nothing committed.

Note on harness validity: my first versions of E1, E3 and E11 removed only the `if chk = body then` line and left `let chk = checksign(...) in`. In ProVerif a failing destructor in `let ... in` without `else` blocks the process, so those mutations did not disable the check. I detected this when E12b stayed green against prediction, redid them as E1p/E3p/E11p removing both lines, and report only the corrected runs below. The same trap applies to anyone extending these models.

| # | File | What I broke | Should be caught by | Actually caught by | Verdict |
|---|---|---|---|---|---|
| E1p | hello-cb | Initiator accepts HELLO_ACK without any signature check | P1/P2 | nothing (Q1 true, Q2 true, Q3 reachable) | **uncaught** |
| E2 | hello-cb | Responder drops expected-peer pin | P2 | nothing | **uncaught** |
| E3p | hello-cb | Responder accepts HELLO without any signature check | P1/P2 | nothing | **uncaught** |
| E1q | hello-cb | E1p + agreement query added | -- | agreement query false | a sound query would catch E1p |
| E6 | hello-cb | Query probe: remove the `<> msgidH` filter | -- | CbCollision reachable in the honest model | shipped filter hides a real collision |
| E6b | hello-cb | E6 + fresh responder nonce in ACK | -- | "cannot be proved" | inconclusive for the table form |
| E6d | hello-cb | Query probe: injective anti-relay on shipped model | -- | false | matches README |
| E6c | hello-cb | Injective anti-relay + fresh responder nonce | -- | **true**, no replay cache | correct P2 is provable |
| E7 | hello-cb | Query probe: initiator/responder agreement | -- | false; trace = insider splice | real agreement gap |
| E8 | delegation | Remove chain-link check | P3 | nothing | **uncaught** |
| E8c0/E8c1 | delegation | No DK leak: honest unreachable, chain-link removed reachable | -- | shows E8 is a real bug | -- |
| E9 | delegation | Remove fail-closed caveat checks | P3 (per ops §6.3 wording) | nothing | **uncaught** |
| E11p | pay-auth | Payer charges on unsigned release | P4 | nothing | **uncaught** |
| E12a | pay-auth | Payee-side ReleaseIssued query on honest model | -- | true | sound query candidate |
| E12bp | pay-auth | E11p + payee-side query | -- | false | sound query catches E11p |
| E13a | pay-dispute | Fix reachability (drop self-signature check) | -- | Q4 becomes reachable | confirms shipped model unreachable |
| E13b | pay-dispute | E13a + resolve without table lookup | P5 | nothing | **uncaught** |
| E14 | pay-meter | Requester drops arithmetic check | P6 | Q-overbill false, but only because model became reachable | misleading catch |
| E15a | pay-meter | Fix reachability (provider bills tagged amount) | -- | Q-overbill false with no bug planted | model internally inconsistent |
| E15b | pay-meter | E15a + drop arithmetic check | P6 | Q-overbill false | same as E15a; not attributable to the bug |
| E15c | pay-meter | E15a + provider mis-tags usage | P6 | nothing; model unreachable again | **uncaught** |

Bugs nothing catches: E1p, E2, E3p, E8, E9, E11p, E13b, E15c. Every model except the two P1/P3 controls admits at least one removed security check with no query change.

## 4. Attacker model

- **Network**: full Dolev-Yao on `c` in all models. Sound.
- **Insiders**: leaked third-party keys in hello-cb (C), delegation (DK), pay-auth (payer and payee insiders), pay-meter (both), pay-dispute (disputant). Sound in intent; in pay-meter the insider is useless because the honest parties hard-code `pk(skPv)`/`pk(skR)` instead of taking them as parameters, so the insider processes exercise nothing.
- **Compromise of honest long-term keys**: never modelled. No forward-secrecy or post-compromise query anywhere. The threat model (arch TH13, identity §7 rotation/revocation) says key compromise is in scope; the formal work does not touch it. Acceptable for a first pass if stated; currently unstated.
- **Replication**: honest roles are replicated (`!`) in all models. Sound.
- **Attacker knowledge**: `offerLowOnly` is public; `exporter` is public by design (correct for the /map/1.0.0 ruling). `billed_capped` and every `catN` are `[data]`, so the attacker can build any invoice shape; that is fine only if the honest checks are real, which in pay-meter they are not.
- **Trusted parties**: arbiter (P5) honest; ledger operator (P4) not modelled at all rather than "honest".

## 5. Abstraction gaps, ranked

Ranked by whether the gap hides a property failure the model would otherwise expose.

1. **HELLO_ACK freshness not modelled (not in the README list).** The ACK reuses the initiator's msgid. This single omission is why the injective anti-relay property fails in the model and why P2 was rewritten as a collision check. Not benign: it made the author replace the right property with the wrong one. Fix is one `new`.
2. **No replay cache / msg-id / timestamp.** Listed. After fixing item 1, it is benign for P2 (E6c proves injectivity without it). It remains the right next model for core §7.1 itself.
3. **Escrow, pause, ledger not modelled (P4).** Listed as "operator honest"; in fact there is no operator, no ledger state, no hold/release phase and no pause choice. Not benign: it is why P4 is vacuous.
4. **Metering arithmetic not modelled (P6).** Not listed (the README claims it IS modelled). ProVerif cannot do integer arithmetic natively; the honest way is a bounded enumeration of usage values or a move to Tamarin with natural-number reasoning. Not benign.
5. **Dispute evidence/claim codes out of scope (P5).** Listed. Not benign as stated, because nothing else in the dispute model is in scope either.
6. **Recipient binding in handshake.** Not listed. Handshake bodies have no recipient; E7 shows the initiator-side agreement failure. Real, low impact.
7. **Two-party HELLO only; no gateway.** Listed. Benign for P1/P2; the gateway (ops §5.1) is a separate model.
8. **Delegation depth bounded to 2; zone attenuation; prefix matching abstracted to a table.** Listed (except prefix). Depth bound is benign. The table abstraction is the reason P3 is weak, not a hidden attack.
9. **Suite/extension negotiation omitted.** Listed. Benign given the version query is itself tautological; would become relevant once a real agreement query exists.
10. **Sequence numbers, frame sizes.** Listed. Benign for these properties.
11. **Cancel accounting.** Listed as future. Benign; nothing claims it.

## 6. Claim calibration: paste-ready text

`draft-map-ops-01` §6.3, replace the first paragraph with:

> **Status.** ProVerif 2.05 models at `formal/` cover session establishment and delegation. `hello-cb.pv` checks that the initiator's negotiated version is the mutual maximum given the initiator-side recheck implemented in map-session, and that the channel-binding value depends on the handshake transcript; `delegation.pv` checks that a two-link chain rooted at an honest issuer is accepted only at the scope the root granted, over a fixed three-element capability lattice. The adversary has full network control plus one compromised insider key. No counterexample was found in the honest models, and the downgrade, relay and delegation-widening negative controls each fail with a witnessing trace. These models do not establish injective anti-relay (the ACK freshness needed for that is present in the protocol but was not modelled), do not verify the transcript-hash mechanism of §6.2 itself, and do not cover the fail-closed caveat rule, chain-link binding, validity windows or depth limits of identity §4. The escrow, dispute and metering models under `formal/` are drafts whose reachability guards fail (the properties are proved over an empty trace set) and are not relied on for any claim in this document.

`map-spec/SUBMISSION.md:89`, replace "Formal properties P1–P6 are machine-checked (ProVerif, ops §6.3)" with:

> Two session-layer properties (negotiation recheck, transcript-dependence of the channel binding) and one delegation property (scope attenuation over a fixed lattice) have ProVerif models with negative controls (ops §6.3); the payment-layer models are not yet sound and no payment property is claimed as machine-checked.

`README.md:184-190`, replace the "Formal model of the payment/escrow path" bullet with:

> **Formal model of the payment/escrow path: not yet.** `pay-auth.pv`, `pay-dispute.pv` and `pay-meter.pv` exist as drafts but their reachability queries show the guarded events are unreachable, so their positive results are vacuous; escrow symmetry, dispute binding and metering arithmetic remain unverified.

`draft-map-intro-01` line 172-174, replace "are machine-checked (ops §6.3)" with "have ProVerif models with negative controls; see ops §6.3 for exactly what each model establishes".

`formal/README.md`: delete the sentence "No counterexample was found for P1–P5 in the honest models … so this is not a vacuous result" and the P4 control row's "Q1 **false** (trace)"; change the P5 and P6 rows to "Q4 unreachable (vacuous)" and "Q-reach unreachable (vacuous)" until fixed.

Core §6.1: change "the HELLO exchange is therefore itself authenticated and MITM-resistant" to "each HELLO and HELLO_ACK is authenticated to its signer; binding of the exchange to a specific peer session is completed by the channel binding (§7.3) on the first in-session envelope" or make Body key 4 (recipient) REQUIRED in HELLO_ACK.

## 7. Models worth adding, prioritised

1. **Fix P2 (one afternoon).** Fresh nonce in the ACK; injective `EnvelopeAccepted ==> EnvelopeSealed` scoped to the honest pair; keep the relay control. Verified reachable-and-true in E6c. Turns the weakest headline claim into the strongest.
2. **Real P1.** Give B a configurable version set (a B that supports only v1 must legitimately negotiate v1), remove the hard-coded `= v2`, query agreement `NegotiatedByInitiator(A,B,v) ==> NegotiatedByResponder(A,B,v)` plus "v = max(offerA ∩ offerB)". Include recipient in the ACK, or record the E7 gap as a Security Consideration.
3. **P4 rebuilt as a state model.** Payee-side `ReleaseIssued` event (verified sound in E12a/E12bp), pause as a choice not a parallel branch, hold and release as distinct invoice-ids with phase ordering (pay §5.2.1), one-sided control that actually flips a query.
4. **P3 with the real matching rule.** Encode capability as a list of segments with a recursive prefix predicate (ProVerif handles lists via constructors), root granting a non-leaf scope so widening in both directions is exercised, plus queries on chain-link, caveat fail-closed and max-depth with one control each.
5. **P6 in Tamarin, or bounded in ProVerif.** Enumerate usage 0..3 and rate in {1,2}, encode `cb = u*rate` as a table, cap as a bound, and query settled-implies-correct and chunk-implies-settled with the provider's `Settled` event removed (it is the requester's event).
6. **P5 with an actual watermark.** Reuse the P6 invoice terms; the arbiter accepts a dispute only with an agreement digest and invoice from the transcript; query that resolution amount ≤ last countersigned watermark.
7. **Replay cache (core §7.1)** as a table keyed on (sender, msgid), then re-run the injective queries with a replayed envelope control.
8. **Key compromise.** Phase-based `out(c, skA)` after session completion; query that pre-compromise sessions keep integrity of accepted envelopes.

## 8. Not examined

- Tamarin was not tried; I did not assess whether any property needs it beyond the arithmetic remark above.
- I did not read the Go or Rust payment code (`escrow.rs`, `dispute.rs`, map-go/pay) to check whether the *implementations* enforce what the vacuous models fail to prove; that is a separate role.
- I did not audit ProVerif's abstraction of `get ... suchthat` for the E6b "cannot be proved" outcome beyond noting it is inconclusive.
- I did not run the interop matrix, conformance suites, or the render pipeline.
- I did not check the ERRATA rulings (evidence kind 9, checkpoint countersig encoding) against the dispute model, since the dispute model contains no evidence at all.
- I did not examine `01-design-critique.md` or `REVIEW-PROCESS.md` beyond confirming they do not alter the formal claims.
