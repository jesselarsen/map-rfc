# SECDIR-style review: draft-map-core-01, draft-map-identity-01, family Security Considerations

Reviewer role: IETF Security Directorate. Date: 2026-09-13. Repository HEAD 56bce96.
Read-only. Builds and tests were run; no repository files were modified.

Verification performed: full read of core and identity drafts; Security Considerations of all eleven; full read of map-rs map-core (envelope, cbor, replay, hello, delegation), map-identity, map-session/session.rs, map-net exporter derivation; map-go cbor, envelope, delegation, identity, session, hello, net transport; formal/README.md; interop/MATRIX.md; SUBMISSION.md; ERRATA.md; TV-011/TV-012/NV-007/NV-009. Tests: `go test` on cbor, envelope, identity, session, hello (pass; envelope has no in-package tests); `cargo test -p map-core -p map-identity -p map-session` (27 + 2 + 22 pass). Five probe tests were written in a throwaway module outside the repository against map-go; results are cited below as "probe".

Severity key: BLOCKING / MAJOR / MINOR / NIT.

---

## A. Channel binding (priority 1)

### A1. BLOCKING. The identity-pair "exporter" is not a channel binding, and the spec permits it normatively.

Text objected to. core §7.3: "for transports lacking an exporter, the concatenation of both peers' static public keys as authenticated by the transport." core §10: "the per-message msg-id replay cache and the monotonic in-session sequence numbers (§7.1) prevent cross-connection replay within the window regardless." core §14 repeats the same claim.

What the identity-pair CB actually does and does not do (derivation: CB = HKDF(salt=TH, ikm=A‖B); TH = H(HELLO env ‖ HELLO_ACK env); both handshake envelopes signed with empty external_aad, no recipient field, fresh 16-byte MsgID and SessionID):

- Prevents: replay of an in-session envelope from session s1 into session s2 between the same honest pair. Cause: TH differs because the HELLO and HELLO_ACK MsgIDs differ. The exporter contributes nothing to this; the ProVerif control `hello-cb-control-relay.pv` shows exactly that (dropping TH makes CB collide; the exporter alone never separates anything).
- Prevents: a peer M that holds live sessions with both A and B from forwarding A's in-session envelopes to B. Cause: B checks kid == transport-authenticated PeerID (identity B1), which M cannot satisfy. Again not the CB.
- Does not prevent: a transparent relay on any transport that does not itself authenticate the static keys. Both legs compute the identical CB because every input is public and identical on both legs. A true exporter (TLS, Noise handshake hash) differs per leg and would cause signature failure at the far end. The reference interop harness transport (framed TCP, MATRIX.md "raw TCP does not authenticate the static keys") is exactly this case, as is the WebSocket-through-TLS-terminator case in §10.
- Does not prevent: cross-connection replay when HELLO/HELLO_ACK MsgIDs repeat (RNG failure, VM snapshot restore, deterministic test id sources). TH then collides, CB collides, and the spec's stated backstop is absent: both implementations create the msg-id ReplayCache per session (map-rs session.rs:187/233, map-go session.go:240/286) and SeqTracker restarts at 0 per session. So the sentence "replay cache and sequence numbers prevent cross-connection replay" describes neither implementation.
- Provides no per-connection instance evidence for disputes. pay §6.2 discloses CB to an arbiter as "same-CB proves same-session provenance"; with the identity-pair exporter, anyone holding the two handshake envelopes can compute CB, so it proves transcript possession, not connection membership.

Is it honestly scoped? SUBMISSION.md and core §14 disclose it as an implementation-maturity limitation. That framing is not accurate, for three reasons:

1. The specification itself, in §7.3 clause 3, authorizes the construction for any "transport lacking an exporter". It is a normative option, not an implementation shortcut. Under RFC 5056 §2.1 a channel binding must be unique per channel; a concatenation of two long-lived public keys is by definition not.
2. The RECOMMENDED transport binding (libp2p, §10) is the only one implemented, and §14 says deployments on it "SHOULD treat the identity-pair fallback as Zone ≥2". The reference implementation therefore cannot lawfully operate at Zone 1 per the family's own zone rules (arch §8.1), which the drafts never say.
3. The formal claim is overstated. ops §6.3 states "P2 channel-binding session separation" and the family README calls it "anti-relay". formal/README.md says P2 is a CB-collision query with the exporter "modeled as PUBLIC information" and that the replay cache "is what prevents cross-connection replay ... this model does not include that cache". P2 proves TH freshness gives distinct CBs. It does not prove relay resistance on an unauthenticated leg, which is what "anti-relay" means to a reader.

Attack: any deployment where the transport does not bind static keys (harness TCP, WS behind a terminating proxy, a future binding that takes §7.3 clause 3 at its word). Precondition: attacker terminates or relays the transport. Impact: full transparent MITM with all envelopes verifying on both sides; no error is ever raised. On a correctly configured libp2p Noise transport the impact is nil, but the spec does not confine the construction to that case.

Minimal fix text:
- §7.3, replace clause 3 with: "Transports that do not expose a per-connection exporter MUST NOT be used for sessions at Zone ≤ 1. For such transports, `transport-exporter` is `initiator-AgentID || responder-AgentID` where both AgentIDs MUST have been authenticated by the transport handshake; implementations MUST reject the session otherwise. This construction is an identity binding, not a channel binding in the sense of RFC 5056."
- §6.1 add: "The MsgID of HELLO and of HELLO_ACK MUST be freshly generated from a CSPRNG for every connection; TH uniqueness, and therefore CB uniqueness, depends on it."
- §7.1 add: "The msg-id cache is keyed by sender AgentID and MUST persist across sessions with that sender for the full window" or delete the cross-connection claim from §10 and §14.
- §10 libp2p: change "SHOULD use [Noise handshake hash] instead" to MUST for Zone ≤ 1, and state that the identity-pair form is Zone ≥ 2 only.
- ops §6.3 / README: rename P2 to "CB uniqueness across distinguishable handshakes" and remove "anti-relay".

### A2. MAJOR. CB and exporter derivation is under-specified for every transport except the one that is wrong.

- §7.3 `HKDF(TH, transport-exporter, "map cb", 32)` does not say which argument is salt and which is IKM. Both implementations use salt=TH, IKM=exporter (map-rs hello.rs:291, map-go hello.go:26). Say so.
- §7.3 does not state the byte order of the two keys; only §10 does.
- TLS exporter: label "EXPORTER-map-binding" is given but RFC 8446 §7.5 exporters also need a context value and output length. Neither is stated. The label also needs registration in the IANA "TLS Exporter Labels" registry (RFC 5705 §7); §15 does not request it.
- Suite 2's KDF column is "BLAKE3-derive-key", which has no salt input, so the §7.3 HKDF formula is undefined for suite 2. Both implementations hard-code SHA-256 for TH and HKDF regardless of the negotiated suite (map-rs hello.rs:280-296, map-go hello.go:12-32), and both offer only suite 1, so this is untested.

Fix text: "CB = HKDF-Expand(HKDF-Extract(salt = TH, IKM = transport-exporter), info = "map cb", L = 32) using the negotiated suite's hash. For TLS 1.3 and QUIC, transport-exporter = TLS-Exporter("EXPORTER-map-binding", context = SessionID, 32)." Register the label. For suite 2 either define an HKDF-BLAKE3 or change the KDF column.

---

## B. Downgrade resistance (priority 2)

### B1. MAJOR. Two conflicting definitions of the transcript hash.

HelloAck key 8: "transcript-hash: H(HELLO bytes || this map fields 1..5 deterministically encoded)". §6.2: "TH = H("MAPv1-transcript" || HELLO-envelope-bytes || HELLO_ACK-envelope-bytes)". These are different values. Neither implementation emits or checks key 8 (map-rs hello.rs:129 "Key 8 (transcript-hash) is not modeled"). If a peer sends key 8 with a wrong value, nothing rejects it: fail-open by omission. Fix: delete key 8, or define it as H over the HELLO envelope only and add "a receiver MUST verify key 8 equals its own computation and MUST close with 1006 otherwise."

### B2. Sound, one line. TH covers the complete HELLO and HELLO_ACK envelope bytes, so versions, suites, extensions including unknown ones, limits, zones and cookies are all covered, and both handshake envelopes are signed over received bytes. A modifying attacker fails at signature verification before TH matters.

### B3. MINOR. The initiator cannot verify "highest mutually supported version". Both implementations re-run negotiate against the ACK singleton, which checks membership in the initiator's offer only (map-rs session.rs:196, map-go session.go:250). The responder's own set is never revealed, so §6.1's rule is a responder obligation that no peer can audit. Reword as such; it is not a downgrade path.

### B4. MINOR. §6.5 cookie retry is unimplemented in both, and the text does not say whether TH covers the first HELLO, the retried HELLO, or both. State "TH covers the HELLO that was answered by HELLO_ACK; the cookie-bearing HELLO replaces the first."

### B5. NIT. §4.3 rule 2 "protected.alg matches negotiated suite" has no meaning for HELLO itself, which precedes negotiation. State that pre-negotiation envelopes use the MTI suite's alg.

---

## C. Anti-replay (priority 3)

### C1. MAJOR. The spec instructs deployments to widen the skew window without stating the invariant that keeps replay impossible.

§14: "deployments with worse clocks MUST widen windows and accept the enlarged replay cache." Safety requires cache lifetime ≥ 2 × skew (plus processing delay). With defaults (600 s cache, ±120 s skew) it holds. Both implementations expose independent parameters with no check (map-rs replay.rs:43 `with_params(skew_ms, window_ms)`; map-go has constants only). Attack: operator sets skew = ±600 s per §14 and leaves the 600 s cache. Message with timestamp now+590 s is accepted at t0 and evicted at t0+600 s; at t0+601 s its timestamp is 11 s in the past and it is accepted again. Fix text in §7.1: "The cache MUST retain entries for at least twice the accepted clock-skew window plus the maximum processing delay; implementations MUST reject configurations that violate this."

### C2. Divergence report, replay.rs vs replay.go. Semantics are identical: (sender, msg-id) keyed, timestamp gated ±skew, arrival-time expiry, dense seq from 0 per direction. Differences: (a) both perform a full-map eviction scan on every check, O(n) per message; (b) neither is bounded except by §8.3 rate limits, which neither implements (grep: no rate-limiting code in either tree), so an authenticated peer can grow the cache without limit for the window; (c) both caches are per-session, see A1. Not a divergence between them, but a shared gap against §7.1 "per sender".

### C3. MINOR. Reconnect behaviour is unspecified. After CLOSE and a new HELLO, seq restarts at 0 and, as implemented, the msg-id cache is discarded. The only thing preventing acceptance of the previous session's frames is the CB change via TH. State this explicitly and see A1.

### C4. NIT. §7.2 "reject non-monotonic seq" reads as allowing gaps; both implementations require exactly next (dense), and on an ordered reliable transport that is the right rule. Say "seq MUST equal the previous accepted seq + 1".

### C5. MINOR. Pre-establishment ERROR envelopes use a fixed MsgID 0xEE…EE and zero SessionID in both implementations (map-rs session.rs:535, map-go session.go:136). They are replayable by construction; the spec says nothing about handshake-phase ERROR freshness. Harmless on an authenticated transport, but document it.

---

## D. Signature scope and deterministic CBOR (priority 4)

### D1. Sound, one line. Both implementations rebuild Sig_structure around the received `protected` and `payload` byte strings and never re-serialize (map-rs envelope.rs:291, map-go envelope.go:234). Delegated envelopes are the same path; the chain lives inside the signed protected header. Ed25519 verification is strict in both: dalek `verify_strict` and a hand-written equivalent in map-go identity/strict.go that rejects non-canonical A/R, small-order A/R, and non-canonical S.

### D2. Go hand-rolled CBOR audit: rejects every non-canonical form. Verified by reading decode.go: non-minimal heads, indefinite lengths (ai 31), reserved ai 28–30, unsorted or duplicate keys by bytewise comparison of encoded key, floats and all simple values except 0xf4/0xf5, invalid UTF-8, trailing bytes, nesting > 64. No path re-encodes before verifying. Two profile divergences from Rust, both interop not security: Go rejects CBOR null (Rust accepts `Value::Null`); Go rejects negative integers with argument > 2^62 (Rust accepts); Go depth limit 64, Rust 32. The spec bounds none of these. Fix: §4.2 add "null, undefined and simple values other than true/false MUST NOT appear" and a normative nesting limit.

### D3. MAJOR. `crit` (Body key 8) is fail-open in both implementations. Probe: an envelope with crit = [0xF123] and no such extension is accepted by map-go `Open`; map-rs `Body::from_value` never reads key 8 (envelope.rs:73 "the scaffold does not yet interpret them"). core §4.3 rule 4 is a MUST and is the only mechanism by which a sender can insist that a security-relevant extension (for example the rotation countersignature of identity §7) be understood. Impact: any future critical extension silently degrades to optional against these implementations. This does not change the spec text, but it falsifies the claim that two conforming implementations exist for §4.3.

### D4. MINOR. §4.1 says unprotected "MUST be empty map; receivers ignore". These conflict. The unprotected map is outside the signature, so a non-empty one is a malleability vector against §7.4 content-address dedup. Both implementations reject non-empty. Fix: "receivers MUST reject (1000) a non-empty unprotected header."

### D5. MINOR. Unknown protected-header keys: map-go rejects (envelope.go:295), map-rs ignores. Unknown DelegateBody keys: map-go rejects (delegation.go:218), map-rs ignores. §6.4 says minor revisions are additive and unknown optional keys MUST be accepted; Go therefore violates §6.4 while being the safer choice. The spec must pick one for protected-header and DelegateBody keys specifically.

### D6. NIT. map-go `identity.Parse` does not check the key decodes to a curve point; map-rs does at construction. A recipient field carrying an invalid point is accepted by Go and rejected by Rust as 1000. Interop-only.

---

## E. Delegation (priority 5)

### E1. MAJOR. The two implementations disagree on three security-critical chain rules, and nothing in the vector suite or interop matrix detects it. Probe results against map-go `OpenDelegated`:

| Rule (identity §4) | map-rs | map-go (probe) |
|---|---|---|
| audience caveat (key 8 → 2) checked against validator | fails closed, audience required | accepted with no audience check; root resolved |
| zone scope (key 6) attenuates only | rejects widening (WidenedScope) | accepted; key 6 parsed for shape only |
| child cert with absent cap scope under scoped parent | accepted, inherits parent scope | rejected 2003 "child scope widens" |
| task-pinning caveat (key 8 → 1) | returned in EffectiveScope; no caller enforces it (grep) | not surfaced at all |
| self-link / duplicate-link | rejected | not checked |

Attack for row 1: an org issues DK→SK with audience = provider P. Any Go provider Q accepts SK's chain as authorized by the org. Attack for row 2: a Zone-1-only delegated key operates at Zone 3 against a Go verifier. Attack for row 4: a tool-grant token (tasks §7 says these are task-pinned DelegationCerts) is usable for any task in both implementations. Row 3 is a pure interop failure: a chain one implementation accepts, the other rejects; the "delegated" cell of the interop matrix uses a one-link chain and cannot hit it. Fix: spec text for row 3 ("a certificate that omits key 5 under a parent that specifies key 5 is invalid; absent means all, and all is not a subset") plus NV vectors for audience, zones, absent scope and task-pin, plus a normative statement that the task-pin check is performed by the receiver of the envelope against Body content and is MUST.

### E2. MAJOR. Chain validity is evaluated once at HELLO and never again. Both session layers open in-session frames with plain `open`/`Open` and compare kid to the leaf (session.rs:348-361, session.go:426-437). identity §4 requires validity windows to "cover the verification time" and §2 justifies the hierarchy with "SK compromise burns hours". A session established one minute before not-after can run for days. Revocation mid-task is likewise never consulted, and neither implementation has ROTATE/REVOKE codecs or a 2002 path (grep). Fix text: "A session MUST be closed (2001) no later than the earliest not-after in the peer's handshake chain, and re-validated on any revocation notice for a key in that chain." Also state whether the chain must accompany every envelope or only HELLO; today I2 ("independently verifiable given only the sender's key material") is false for stored in-session envelopes from a delegated peer, which matters for tasks §8 transcripts and trust receipts.

### E3. MAJOR. Rotation countersignature is optional where the security argument needs it mandatory. identity §7: "SHOULD be countersigned by the new key (an ext field carries the second signature)". identity §10: "hence the countersignature requirement". No extension key is registered for it (core §5.1 lists 1–3 only). The attack §10 describes, stolen old key rotates the victim away, is exactly what the SHOULD leaves open. Fix: MUST; register the ext key with its byte layout; state that a ROTATE lacking it MUST be rejected.

### E4. MAJOR. The kid-vs-chain-root contradiction is fixed in identity but not in core. ERRATA says "identity §4 AND §3.2 B1/B3 patched"; verified. core §4.3 rule 7 still reads "or against the leaf key of a valid delegation chain terminating at `kid`'s root", and core §7.3 still reads "the envelope `kid`'s key or a delegated key chained to it", both of which describe kid as the root. core §4.1 and map-go cite "identity §6" for the chain; the chain is identity §4 and §6 is organizational identities. Fix: core rule 7 → "Signature verifies against `kid`; if ProtHdr key 33 is present, `kid` MUST equal the leaf certificate's subject and the chain MUST validate per identity §4, else 2003."

### E5. MINOR. Chain length: identity §4 "MUST NOT exceed 8", identity §10 "cap chain length (default 5)". Both implementations use 8.

### E6. MINOR. Depth counting is consistent between implementations (verified by tracing three cases) and matches the §4 ruling, but map-go/README.md item 3 still calls it ambiguous. TV-012 has one link and cannot distinguish readings; add the two-link vector the README asks for.

### E7. MINOR. §7.4 "store-and-forward objects ... MUST carry ttl-ms". DelegationCerts issued by both implementations omit ttl-ms (map-rs delegation.rs:291, TV-011). Either exempt objects with an intrinsic not-after or fix the implementations and vectors.

### E8. MINOR. Out-of-session verification time. §4 "all validity windows MUST cover the verification time" makes receipts and transcripts signed under an hours-long SK unverifiable after the SK's window closes. State that for stored objects the reference time is the object's timestamp, bounded by the verifier's freshness policy.

---

## F. Crypto agility (priority 6)

### F1. MAJOR. Agility is asserted (arch G7, core §2 suite 3, identity §3.1 "identity leg of cryptographic agility") but the wire format forecloses it.

- Envelope CDDL: `signature: bstr .size 64`. ML-DSA-44 signatures are 2420 bytes.
- ProtHdr CDDL: `1 => -8` fixed. Suites 1 and 2 share alg -8, so §4.3 rule 2 cannot distinguish them; the suite is bound only through TH.
- `AgentID = bstr .size (34..66)`. An ML-DSA-44 public key is 1312 bytes.
- identity B1 and core §10: PeerID MUST equal the AgentID key. libp2p has no PQ key type; the identity ≡ PeerID ≡ did:key equivalence that §1 calls a design feature holds for Ed25519 only.
- HELLO is signed before the suite is negotiated; no text says which signature algorithm signs HELLO when the MTI suite is not the one finally chosen.
- The KEM and AEAD columns of §2 are never used by any MAP mechanism; transport ciphersuites are chosen by TLS/Noise independently, so a "MAP-SUITE-1 session" may run over AES-GCM. The table implies a binding that does not exist.

Fix: either (a) remove suite 3 and the G7 claim, state that the family is Ed25519-only in this version and that a PQ migration is a major-version change, or (b) relax the CDDL sizes to per-suite values, make alg per-suite, and add "the HELLO envelope is signed with the MTI suite; HELLO_ACK is signed with the chosen suite". Option (a) is the honest one for -01.

### F2. NIT. Both implementations negotiate any suite number a peer lists but can only sign EdDSA and hash SHA-256; only suite 1 is offered in every shipped config, so the negotiation code for suites is untested.

---

## G. Other Security Considerations and claim-vs-reality

- MINOR. core §6.1 "Both peers MUST verify each envelope's signer matches the transport-authenticated identity". Both initiators return an ERROR to the caller before that check runs (map-rs session.rs:168-175, map-go session.go:222-229), and the ERROR drives §9 retry logic. Move the kid check ahead of the msg-type dispatch.
- MINOR. Body key 4 (recipient) is "REQUIRED in-session" but no receiver checks that it equals its own AgentID, and HELLO/HELLO_ACK omit it entirely. On the identity-pair exporter this is the one cheap check that would stop a forwarded HELLO reaching an unintended responder. Fix: "Receivers MUST reject (2000) an in-session envelope whose recipient is not their own AgentID; HELLO and HELLO_ACK MUST carry recipient."
- MINOR. §8.3 rate limits are MUST and are implemented nowhere; §6.5 cookie is implemented nowhere. SUBMISSION's "two independent interoperating implementations" therefore covers §4, §6.1–6.4, §7 and §12 only. Say so in the conformance statement.
- MINOR. MATRIX.md core cells run over unauthenticated TCP with pinned identities. The "relay" cell proves TH separation, not channel binding to a transport. The task and pay cells run over libp2p/QUIC but exercise no adversarial CB scenario. SUBMISSION should not cite the matrix as evidence for §7.3.
- MINOR. §10 WebSocket "CB falls back to static-key binding" is undefined: whose static keys, authenticated by what. As written it is A1's unauthenticated case.
- MINOR. §8.2 PING/PONG "opaque 8-byte payload echoed verbatim". map-rs PING sends an empty map; both PONGs echo the PING's 16-byte msg-id, not the payload. Interop works because neither checks; the text is wrong or the code is.
- MINOR. The formal program: `delegation.pv` bounds chains to two links and omits zones and caveat evaluation (README "abstraction gaps"), so P3 does not cover E1 rows 1, 2 or 4.
- NIT. trust §10, pay §7, discovery §11, tasks §15, ops §10 are consistent with core and raise nothing new for this scope. pay §7 "core replay rules (§7) cover the envelopes" inherits A1 and C1. index §5 and intro §7 correctly defer. wire §6 and arch §12 are pointers.
- NIT. core §14 "Deployments requiring instance-level binding MUST use an exporter-bearing transport" is a MUST on deployers that the reference implementation cannot satisfy, since it ships only the libp2p identity-pair binding.

---

## H. The three objections a SECDIR review will raise first

1. "This is not channel binding." §7.3 clause 3 defines the binding for exporter-less transports as public, long-lived identity material; the RECOMMENDED transport uses it; the mitigation stated in §10 and §14 is not what the implementations do; and the ProVerif P2 result proves transcript freshness, not relay resistance. Expect a request to remove clause 3, make Noise-handshake-hash or TLS-exporter MUST for the libp2p binding, and rewrite §14. See A1, A2.
2. "Cryptographic agility is claimed and structurally impossible." Fixed 64-byte signature, fixed alg -8, 66-byte AgentID cap, PeerID equivalence, and HELLO signed pre-negotiation. Expect a request to either drop suite 3 and G7 or re-specify the envelope. See F1.
3. "The delegation model is enforced differently by the two implementations on the rules that matter, and the core draft still contradicts the identity draft on what kid is." Audience caveats, zone attenuation, absent-scope inheritance and task pinning diverge or are unenforced; crit is fail-open in both; chains are checked once per session with no expiry or revocation follow-through; rotation countersignature is a SHOULD. Expect a request for NV vectors covering each rule, alignment of core §4.3/§7.3 with identity §3.2/§4, and MUST-level countersignature with a registered extension key. See D3, E1–E4.

## I. Not examined

Live libp2p or QUIC sessions were not run; the map-net and map-go/net transports were read only for exporter and peer-identity derivation. ProVerif models were read but not re-run. Conformance vector suites (map-rs conformance crate, map-go/conformance) were not executed beyond the unit tests listed above. The map-cli interop harness code, DHT record signing and freshness (discovery §5–§8), the tasks transcript and checkpoint countersignature, trust receipt semantics, pay settlement and dispute mechanics, gateway composition (ops §5.1), and the render pipeline were not reviewed beyond their Security Considerations sections. Timing side channels and memory-safety of the CBOR decoders were not fuzzed here; map-go/cbor ships a fuzz test that was not run.
