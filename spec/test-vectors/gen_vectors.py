#!/usr/bin/env python3
"""Generate reproducible MAP test vectors.

TV-001 HELLO + TV-002 ExecutionReceipt (unchanged, byte-for-byte), plus
TV-003..TV-010 (one signed envelope per protocol family), a negative-vector
section (bytes that MUST be rejected, with the required core §9 error class),
and a derived-constants section (transcript hash, channel binding, shard, DHT
keys, transcript root) for cross-implementation spot checks.

Field NUMBERS come from the owning drafts' CDDL (cited per vector); message
VALUES follow TASK-004 §2. Where the TASK-004 doc's field layout contradicts a
draft, the draft wins (see the report accompanying this change). No wall-clock,
no randomness: regenerating twice is byte-identical. Deps: pynacl, cbor2,
base58; blake2 via hashlib only.
"""
import cbor2, hashlib, hmac, base58, re
from nacl.signing import SigningKey
from nacl.exceptions import BadSignatureError

def agent_id(pub: bytes) -> bytes:
    # multicodec ed25519-pub (0xED varint -> 0xED 0x01) || raw pubkey
    return bytes([0xED, 0x01]) + pub

def b58(b: bytes) -> str:
    return "z" + base58.b58encode(b).decode()  # multibase base58btc

def enc(o) -> bytes:
    return cbor2.dumps(o, canonical=True)

def multihash_sha256(data: bytes) -> bytes:
    # 0x12 sha2-256, 0x20 length 32 (multiformats/multihash)
    return bytes([0x12, 0x20]) + hashlib.sha256(data).digest()

def hkdf_sha256(salt: bytes, ikm: bytes, info: bytes, length: int = 32) -> bytes:
    # RFC 5869 HKDF-SHA256; length<=32 so a single Expand block suffices.
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    return hmac.new(prk, info + b"\x01", hashlib.sha256).digest()[:length]

# --- Keys (deterministic seeds for reproducibility) ---
seed_a = hashlib.sha256(b"MAP-TEST-VECTOR-KEY-A").digest()
seed_b = hashlib.sha256(b"MAP-TEST-VECTOR-KEY-B").digest()
ka, kb = SigningKey(seed_a), SigningKey(seed_b)
pub_a, pub_b = ka.verify_key.encode(), kb.verify_key.encode()
aid_a, aid_b = agent_id(pub_a), agent_id(pub_b)

TS = 1780000000000          # fixed timestamp (ms)
CAP = "map:cap:inference/text-generation"
DELEG_EXPIRY = 1780000600000  # delegation not-after (shared with TV-010 expiry)

# Delegation key tiers derived from labelled seeds (RIK = Key A; DK/SK below),
# for TV-011/012 + NV-007 (TASK-010). Same derivation the Rust conformance uses.
seed_a_dk = hashlib.sha256(b"MAP-TEST-VECTOR-KEY-A-DK").digest()
seed_a_sk = hashlib.sha256(b"MAP-TEST-VECTOR-KEY-A-SK").digest()
ka_dk, ka_sk = SigningKey(seed_a_dk), SigningKey(seed_a_sk)
aid_a_dk = agent_id(ka_dk.verify_key.encode())
aid_a_sk = agent_id(ka_sk.verify_key.encode())

def sealed(signer, msg_type: int, msg_id: bytes, content, ts: int = TS, aad: bytes = b""):
    """Build+sign a COSE_Sign1 envelope exactly as TV-001 does (core §4).

    Returns the intermediate byte strings so each block can print them, and
    verifies the signature in-script (self-testing artifact, §16).
    """
    aid = agent_id(signer.verify_key.encode())
    protected = enc({1: -8, 4: aid})                       # alg EdDSA, kid=AgentID
    body = {0: 0x0100, 1: msg_type, 2: msg_id, 3: ts, 10: content}
    payload = enc(body)
    sig_struct = enc(["Signature1", protected, aad, payload])
    sig = signer.sign(sig_struct).signature
    signer.verify_key.verify(sig_struct, sig)              # in-script verify
    envelope = enc(cbor2.CBORTag(18, [protected, {}, payload, sig]))
    return {"aid": aid, "protected": protected, "payload": payload,
            "sig_struct": sig_struct, "sig": sig, "envelope": envelope}

def sealed_delegated(signer, msg_type: int, msg_id: bytes, content, chain, ts: int = TS, aad: bytes = b""):
    """Like `sealed`, but the protected header carries a delegation chain in
    key 33 (identity §6; order leaf..root). `chain` is a list of cert-envelope
    byte strings. The chain sits inside the signed `protected`, so the leaf
    signature covers it."""
    aid = agent_id(signer.verify_key.encode())
    protected = enc({1: -8, 4: aid, 33: chain})
    body = {0: 0x0100, 1: msg_type, 2: msg_id, 3: ts, 10: content}
    payload = enc(body)
    sig_struct = enc(["Signature1", protected, aad, payload])
    sig = signer.sign(sig_struct).signature
    signer.verify_key.verify(sig_struct, sig)
    envelope = enc(cbor2.CBORTag(18, [protected, {}, payload, sig]))
    return {"aid": aid, "protected": protected, "payload": payload,
            "sig_struct": sig_struct, "sig": sig, "envelope": envelope}

def msgid(n: int) -> bytes:
    return bytes([n]) * 16                                 # TV_NNN convention (§2)

# =====================================================================
# TV-001: HELLO envelope (COSE_Sign1, tag 18) — msg-id 00..0f (pre-convention)
# =====================================================================
hello = {1: [0x0100], 2: [1], 3: {}, 4: 262144}           # core §6.1 Hello (lists)
tv001 = sealed(ka, 0x0001, bytes.fromhex("000102030405060708090a0b0c0d0e0f"), hello)
protected, payload, sig_structure, sig1, envelope = (
    tv001["protected"], tv001["payload"], tv001["sig_struct"], tv001["sig"], tv001["envelope"])

# =====================================================================
# TV-002: ExecutionReceipt, dual-signed (requester A, provider B) — trust §3
# =====================================================================
core = {
    1: bytes.fromhex(hashlib.blake2b(b"example-task", digest_size=32).hexdigest()),
    2: aid_a, 3: aid_b, 4: CAP,
    5: 1780000000000, 6: 1780000042000, 7: 0,
    8: {1: 1523, 2: 812, 3: 431, 4: 42000}, 9: 92,
}
core_enc = enc(core)
sig_req = ka.sign(enc(["MAPReceipt", 1, core_enc])).signature
sig_prov = kb.sign(enc(["MAPReceipt", 2, core_enc])).signature
receipt = enc({1: core_enc, 2: sig_req, 3: sig_prov})
ka.verify_key.verify(enc(["MAPReceipt", 1, core_enc]), sig_req)
kb.verify_key.verify(enc(["MAPReceipt", 2, core_enc]), sig_prov)

# --- Regression guard (§5/§15): TV-001/TV-002 bytes are frozen. ---
assert envelope.hex() == (
    "d2845828a20127045822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bfa"
    "ce1a05835a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa401"
    "8119010002810103a0041a00040000584010307036a1d861d52cd830f729108aeda08654d455a5de07fe9"
    "8402c9504099f84a77527dd031731ab52b7fd902e7ee125a2ff7d4740767a7e5b158b30840d0f"), \
    "TV-001 envelope changed"
assert len(envelope) == 166
assert receipt.hex() == (
    "a30158bda9015820d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25025822"
    "ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1035822ed01c93c1b6a"
    "4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d0478216d61703a6361703a696e6665"
    "72656e63652f746578742d67656e65726174696f6e051b0000019e70448800061b0000019e70452c100700"
    "08a4011905f30219032c031901af0419a41009185c02584086e77545e95f12b67c227f0ec2327a12b29d74"
    "f2e10750b34b4c62ed16bcbe767892de0aa639cb41d4f19fa2c4699b5b395f2b96985477e48a6617fb277fd"
    "602035840debf7676fe7b664ac12f093ebf2519632d86ab4e1a87435aaa4a4877580c26aa6af442abd8bc3b"
    "535d06b2253d3150e3fd20015d5f159dc0f56033475e676d04"), "TV-002 receipt changed"
assert len(receipt) == 327

# =====================================================================
# TV-003..TV-010 — one signed envelope per protocol family
# =====================================================================
# TV-003 HELLO_ACK (0x0002), signed by B. core §6.1 HelloAck: singletons
# (1=>Version, 2=>uint), NOT lists; transcript-hash key 8 omitted (it is the
# session's derived TH, computed below per core §6.2, not a self-embedded field).
hello_ack = {1: 0x0100, 2: 1, 3: {}, 4: 262144}
tv003 = sealed(kb, 0x0002, msgid(3), hello_ack)

# TV-004 ERROR (0x0006), signed by B. core §9 ErrorMsg:
#   1=>code(uint) 2=>retryable(bool) ?3=>reason(tstr) ?4=>MsgID ?5=>detail.
#   (TASK-004 §2's {2:reason,3:msgid} contradicts the draft; draft wins.)
err = {1: 1003, 2: False, 3: "replayed msg-id",
       4: bytes.fromhex("000102030405060708090a0b0c0d0e0f")}
tv004 = sealed(kb, 0x0006, msgid(4), err)

# TV-005 DESCRIPTOR (0x0100), signed by B. discovery §2 Descriptor:
#   1 agent 2 seq 3 versions 4 suites 5 addrs 6 caps(CapEntry{1:CapURI}) 12 zones.
#   (TASK-004 §2's 3:caps/4:protocols/5:issued-at/6:validity contradicts the
#    draft; draft wins. validity key 13 omitted — the scaffold Descriptor does
#    not model it, so including it would break Rust round-trip.)
descriptor = {1: aid_b, 2: 1, 3: [0x0100], 4: [1],
              5: ["/map/1.0.0"], 6: [{1: CAP}], 12: [2]}
tv005 = sealed(kb, 0x0100, msgid(5), descriptor)

# TV-006 OFFER (0x0200), signed by A. tasks §2 Offer: 1=>TaskID 2=>CapURI
#   3=>TaskConstraints(all-optional => {}) ?4=>PayRef ?5=>expiry.
#   (max_price -> PayRef, an UNDEFINED type in the drafts: omitted+reported.
#    InputSpec belongs to ASSIGN, not OFFER: moved to TV-007.)
task_id = core[1]
offer = {1: task_id, 2: CAP, 3: {}}
tv006 = sealed(ka, 0x0200, msgid(6), offer)

# TV-007 ASSIGN (0x0202), signed by A. tasks §2.1 Assign: 1=>TaskID 2=>CapURI
#   3=>TaskConstraints 4=>InputSpec 5=>FlowInit. InputSpec key 2 = [+Multihash]
#   carries the MAP-TV-INPUT digest; FlowInit {1:msgs,2:bytes}.
assign = {1: task_id, 2: CAP, 3: {},
          4: {2: [multihash_sha256(b"MAP-TV-INPUT")]},
          5: {1: 64, 2: 1048576}}
tv007 = sealed(ka, 0x0202, msgid(7), assign)

# TV-008 CHUNK (0x0205), signed by B. tasks §3 Chunk: 1=>TaskID 2=>seq 3=>kind
#   4=>content. seq 0, kind 0 (output-text), content "Hello, MAP!" (11 bytes).
chunk_content = b"Hello, MAP!"
chunk = {1: task_id, 2: 0, 3: 0, 4: chunk_content}
tv008 = sealed(kb, 0x0205, msgid(8), chunk)

# TV-009 COMPLETE (0x020D), signed by B. tasks §4 Complete: 1=>TaskID
#   2=>outcome 3=>Multihash(transcript-root) 4=>usage{1,2,3,4}.
# transcript-root: Merkle over the single TV-008 output chunk (tasks §8.1):
#   leaf = H(0x00 || enc([seq, kind, content])); single leaf IS the root.
#   enc([0,0,b"Hello, MAP!"]) = 83 00 00 4b 48656c6c6f2c204d415021 (array of 3).
transcript_leaf = hashlib.sha256(bytes([0x00]) + enc([0, 0, chunk_content])).digest()
transcript_root = transcript_leaf                          # one leaf => root
assert transcript_root.hex() == hashlib.sha256(
    bytes([0x00]) + bytes.fromhex("8300004b48656c6c6f2c204d415021")).hexdigest(), \
    "TV-009 root disagrees with hand-derived leaf encoding"
complete = {1: task_id, 2: 0,
            3: bytes([0x12, 0x20]) + transcript_root,      # Multihash(root)
            4: {1: 1523, 2: 812, 3: 431, 4: 42000}}
tv009 = sealed(kb, 0x020D, msgid(9), complete)

# TV-010 PAY_QUOTE (0x0400), signed by B. pay §3 PayQuote: 1=>quote-id(16B)
#   2=>pricing-model(uint) 3=>rate-table{unit=>[Asset,Amount]} 5=>[MethodOffer]
#   6=>expiry. Amount = tag-4 [exp,mantissa] (pay §3); Asset = "iso4217:USD"
#   (pay §1 identifier form; bare "USD" is not a valid asset id).
#   (task-id binding requested by TASK-004 §2 has NO PayQuote field — binding
#    is via PAY_AGREE/ASSIGN, pay §3: omitted+reported.)
usd_amount = [ "iso4217:USD", cbor2.CBORTag(4, [-6, 4200000]) ]
pay_quote = {1: bytes([0x10]) * 16, 2: 0,
             3: {0: usd_amount},
             5: [{1: "pay:credit-ledger", 2: b""}],
             6: 1780000600000}
tv010 = sealed(kb, 0x0400, msgid(10), pay_quote)

# TV-010b PAY_QUOTE BOUND to a task (0x0400), signed by B. pay §3.1 key 9:
#   when a PAY_QUOTE answers an OFFER carrying a TaskID (tasks §2), key 9 MUST be
#   present and equal that TaskID. This is TV-010 + the mandatory binding; the
#   TaskID is the TV-006 OFFER's (= TV-002 core[1]). Additive — TV-010 stays the
#   valid *unbound standing quote*; frozen TV-001..010 bytes are undisturbed.
pay_quote_bound = dict(pay_quote); pay_quote_bound[9] = task_id
tv010b = sealed(kb, 0x0400, msgid(11), pay_quote_bound)
assert len(task_id) == 32

# TV-011 DelegationCert (0x0502), signed by RIK = Key A. identity §4 DelegateBody:
#   1 issuer 2 subject 3 not-before 4 not-after ?5 [CapURI] ?6 [ZoneLabel]
#   ?7 max-depth ?8 caveats. RIK-A delegates to DK-A, scope = CAP, depth 1.
delegate_body = {1: aid_a, 2: aid_a_dk, 3: TS, 4: DELEG_EXPIRY, 5: [CAP], 7: 1}
tv011 = sealed(ka, 0x0502, msgid(13), delegate_body)

# TV-012 OFFER (0x0200) signed by DK-A, carrying the 1-link chain [TV-011] in
# ProtHdr key 33. Leaf = DK-A; the chain root is RIK-A (the accountable id).
deleg_offer = {1: task_id, 2: CAP, 3: {}}
tv012 = sealed_delegated(ka_dk, 0x0200, msgid(14), deleg_offer, [tv011["envelope"]])

# TV-013 PAY_AGREE (0x0401), signed by A. pay §3.1 PayAgree: 1 quote-id 2 Multihash
#   (digest of the received PAY_QUOTE **envelope** bytes — here the bound TV-010b)
#   3 method-URI ?4 [Asset, Amount] authorized cap. Binds the bound quote TV-010b.
pay_agree = {1: bytes([0x10]) * 16,
             2: multihash_sha256(tv010b["envelope"]),
             3: "pay:credit-ledger",
             4: usd_amount}
tv013 = sealed(ka, 0x0401, msgid(18), pay_agree)

# TV-014 RESERVE (0x020F), signed by A. tasks §6 Reserve: 1 rsv-id(16) 2 CapURI
#   3 from 4 until ?5 TaskConstraints ?6 PayRef ?7 ext. Reserves CAP in a window.
RSV_ID = bytes([0x20]) * 16
RSV_FROM, RSV_UNTIL = TS, 1780000600000
reserve = {1: RSV_ID, 2: CAP, 3: RSV_FROM, 4: RSV_UNTIL, 5: {5: 1}}   # constraints: zone 1
tv014 = sealed(ka, 0x020F, msgid(19), reserve)

# TV-015 LEASE (0x0210), signed by B. tasks §6 Lease: 1 rsv-id 2 status(0 granted)
#   ?3 [from,until] granted window ?4 PayQuoteRef ?5 ext. Window within TV-014's.
lease = {1: RSV_ID, 2: 0, 3: [1780000100000, 1780000500000]}
tv015 = sealed(kb, 0x0210, msgid(20), lease)

# TV-016 PAY_INVOICE (0x0402), signed by B. pay §4/§4.1 metered (terms 3): the
#   FIRST incremental invoice, so key-3 amount == the watermark's cumulative-
#   billed. Watermark {1 TaskID (=TV-006), 2 through-chunk-seq, 3 cumulative
#   Usage, 4 cumulative-billed}. Model-1 rates (in the agreed PAY_QUOTE, not the
#   invoice): rate[1]=4([-6,3000]) µUSD/token-in, rate[2]=4([-6,5000]) /token-out
#   ⇒ 1523×3000 + 812×5000 = 4_569_000 + 4_060_000 = 8_629_000 µUSD. A verifier
#   recomputes this from the watermark's Usage and the agreed rates.
WM_USAGE = {1: 1523, 2: 812, 3: 431, 4: 42000}
CUM_BILLED = ["iso4217:USD", cbor2.CBORTag(4, [-6, 8629000])]
watermark = {1: task_id, 2: 8, 3: WM_USAGE, 4: CUM_BILLED}
pay_invoice = {1: bytes([0x30]) * 16, 2: bytes([0x10]) * 16, 3: CUM_BILLED,
               4: watermark, 5: b"", 6: TS}
tv016 = sealed(kb, 0x0402, msgid(25), pay_invoice)
# In-script arithmetic self-check (pay §4.1), mirroring NV-001's sig check.
assert 1523 * 3000 + 812 * 5000 == 8629000, "TV-016 billing arithmetic"

# TV-017 escrow hold PAY_INVOICE (0x0402), signed by B. pay §5.2.1/§5.2.3
#   (terms 2): phase key 7 = 1 (hold), amount = the quote key-4 total, NO
#   watermark (presence iff terms 3). Method payload (pay §5.2.4,
#   pay:credit-ledger): the demanded hold expiry-ts as deterministic-CBOR uint.
HOLD_TOTAL = ["iso4217:USD", cbor2.CBORTag(4, [-2, 500])]
HOLD_EXPIRY = 1780000600000
hold_invoice = {1: bytes([0x31]) * 16, 2: bytes([0x10]) * 16, 3: HOLD_TOTAL,
                5: enc(HOLD_EXPIRY), 6: TS, 7: 1}
tv017 = sealed(kb, 0x0402, msgid(27), hold_invoice)

# TV-018 held PAY_RECEIPT (0x0404), signed by B. pay §5.2.1: kind key 5 = 1
#   (held) — the payee's signed statement that a conforming hold exists.
#   Proof-digest key 4 deliberately absent (exercises the optional-key path).
held_receipt = {1: bytes([0x31]) * 16, 2: HOLD_TOTAL, 3: TS, 5: 1}
tv018 = sealed(kb, 0x0404, msgid(28), held_receipt)

# TV-019 PAY_DISPUTE (0x0405), signed by A. pay §6.1: the payer opens a
#   claim-3 (overbilling) dispute over the TV-016 invoice, anchored to the
#   TV-013 PAY_AGREE by its envelope digest (PayAgreeRef, I1) and bound to
#   the TV-006 TaskID. Statement (key 6) deliberately absent — it is never
#   evidence.
DISPUTE_ID = bytes([0x32]) * 16
pay_dispute = {1: DISPUTE_ID, 2: bytes([0x30]) * 16, 3: 3,
               4: {1: bytes([0x10]) * 16,
                   2: multihash_sha256(tv013["envelope"])},
               5: task_id, 7: TS}
tv019 = sealed(ka, 0x0405, msgid(30), pay_dispute)

# TV-020 PAY_DISPUTE_EVIDENCE (0x0406), signed by A. pay §6.1: ONE kind-0
#   (PAY_QUOTE envelope) item whose bytes are the frozen TV-010b envelope
#   VERBATIM — the vector itself exercises the I1 verbatim-carry rule with
#   bytes the suite already pins. CB (key 3) absent: out-of-session (§6.2).
dispute_evidence = {1: DISPUTE_ID, 2: [{1: 0, 2: tv010b["envelope"]}]}
tv020 = sealed(ka, 0x0406, msgid(31), dispute_evidence)

# =====================================================================
# Derived constants (core §6.2 / §7.3, discovery §6, tasks §8.1)
# =====================================================================
TH = hashlib.sha256(b"MAPv1-transcript" + tv001["envelope"] + tv003["envelope"]).digest()
CB = hkdf_sha256(TH, bytes(32), b"map cb")                 # ikm = 32x 0x00 (stand-in)
shard_b = hashlib.sha256(aid_b).digest()[0]                # discovery §6: digest byte
cap_key = multihash_sha256(f"map:idx:1:{CAP}".encode())
sharded_key = multihash_sha256(f"map:idx:1:{CAP}:{shard_b}".encode())

# =====================================================================
# Negative vectors (MUST reject) — ops §6.2 / core §9 classes
# =====================================================================
# NV-001 tampered payload byte -> BAD_SIGNATURE (2000). Envelope offset 60 is
# payload offset 60-47=13 (payload starts after d2 84 5828 <40B protected> a0
# 5835). Flip it and confirm the original signature no longer verifies.
mangled_payload = bytearray(payload); mangled_payload[13] ^= 0xFF
try:
    ka.verify_key.verify(enc(["Signature1", protected, b"", bytes(mangled_payload)]), sig1)
    raise SystemExit("NV-001 did not break the signature")
except BadSignatureError:
    pass
nv001 = enc(cbor2.CBORTag(18, [protected, {}, bytes(mangled_payload), sig1]))

# NV-002 outer tag 18->17 -> MALFORMED (1000): not a COSE_Sign1 envelope.
nv002 = bytearray(tv001["envelope"]); nv002[0] = 0xD1; nv002 = bytes(nv002)
assert nv002[0] == 0xD1

# NV-003 indefinite-length body, re-signed VALID -> MALFORMED (1000):
#   isolates the deterministic-profile check from signature checking.
payload_indef = bytes([0xBF]) + payload[1:] + bytes([0xFF])   # a5(map5) -> bf..ff
assert cbor2.loads(payload_indef) == cbor2.loads(payload)     # same logical body
assert enc(cbor2.loads(payload_indef)) != payload_indef       # but non-canonical
sig_indef = ka.sign(enc(["Signature1", protected, b"", payload_indef])).signature
ka.verify_key.verify(enc(["Signature1", protected, b"", payload_indef]), sig_indef)  # VALID
nv003 = enc(cbor2.CBORTag(18, [protected, {}, payload_indef, sig_indef]))

# NV-004 descriptor re-issued with seq 0 -> STALE vs seq 1 (discovery §5 LWW;
#   a cache decision, not a core §9 error code).
descriptor_stale = dict(descriptor); descriptor_stale[2] = 0
nv004 = sealed(kb, 0x0100, msgid(5), descriptor_stale)["envelope"]

# NV-005 receipt with sig fields 2 and 3 swapped -> BAD_SIGNATURE (2000):
#   role binding (enc(["MAPReceipt", role, core])).
nv005 = enc({1: core_enc, 2: sig_prov, 3: sig_req})
try:
    ka.verify_key.verify(enc(["MAPReceipt", 1, core_enc]), sig_prov)
    raise SystemExit("NV-005 swapped requester sig unexpectedly verified")
except BadSignatureError:
    pass

# NV-006 PAY_QUOTE with a MISMATCHED task binding -> pay §3.1 policy REJECT.
#   key 9 is present but != the OFFER's TaskID. The envelope is well-formed and
#   correctly signed by B (it OPENS); the rejection is the requester's binding
#   check, not a core §9 wire error (cf. NV-004, a discovery §5 cache decision).
bad_task = bytes([task_id[0] ^ 0xFF]) + task_id[1:]
assert bad_task != task_id and len(bad_task) == 32
pay_quote_badbind = dict(pay_quote); pay_quote_badbind[9] = bad_task
nv006 = sealed(kb, 0x0400, msgid(12), pay_quote_badbind)["envelope"]

# NV-007 delegation chain that WIDENS cap scope -> MUST reject (identity §4
#   attenuation; wire 2003). Parent RIK-A->DK scopes to ".../text"; child
#   DK->SK broadens to the parent "inference" prefix. Envelope signed by SK
#   carries [child, parent] (leaf..root). (Renumbered from TASK-010's planned
#   NV-006, already taken by the bound-quote pass.)
nv7_parent_body = {1: aid_a, 2: aid_a_dk, 3: TS, 4: DELEG_EXPIRY, 5: ["map:cap:inference/text"]}
nv7_parent = sealed(ka, 0x0502, msgid(15), nv7_parent_body)
nv7_child_body = {1: aid_a_dk, 2: aid_a_sk, 3: TS, 4: DELEG_EXPIRY, 5: ["map:cap:inference"]}
nv7_child = sealed(ka_dk, 0x0502, msgid(16), nv7_child_body)
nv007 = sealed_delegated(ka_sk, 0x0200, msgid(17), deleg_offer,
                         [nv7_child["envelope"], nv7_parent["envelope"]])["envelope"]

# NV-008 LEASE granted OUTSIDE the requested window -> MUST reject (tasks §6:
#   a granted window must fall within the RESERVE's [from, until]). Same rsv-id
#   as TV-014 but the granted window is entirely after RSV_UNTIL.
lease_bad = {1: RSV_ID, 2: 0, 3: [RSV_UNTIL + 3600000, RSV_UNTIL + 7200000]}
nv008 = sealed(kb, 0x0210, msgid(21), lease_bad)["envelope"]

# NV-009 delegation chain that exceeds max-depth -> MUST reject (identity §4
#   attenuation; wire 2003). A 2-link chain RIK-A->DK->SK where the PARENT
#   (RIK-A->DK) declares max-depth 0 (key 7) — DK may not further delegate —
#   yet DK->SK does. Scopes are IDENTICAL (= CAP) on both links, so scope is
#   NOT widened; only the depth bound is violated, isolating the max-depth
#   check exactly as NV-003 isolates canonicality. Envelope signed by SK
#   carries [child, parent] (leaf..root). (max-depth counting pinned by the
#   2026-07-12 ARCHITECT RULING: a cert with max-depth d admits at most d
#   certs between itself and the leaf; the parent at chain index 1 has 1.)
nv9_parent_body = {1: aid_a, 2: aid_a_dk, 3: TS, 4: DELEG_EXPIRY, 5: [CAP], 7: 0}
nv9_parent = sealed(ka, 0x0502, msgid(22), nv9_parent_body)
nv9_child_body = {1: aid_a_dk, 2: aid_a_sk, 3: TS, 4: DELEG_EXPIRY, 5: [CAP]}
nv9_child = sealed(ka_dk, 0x0502, msgid(23), nv9_child_body)
nv009 = sealed_delegated(ka_sk, 0x0200, msgid(24), deleg_offer,
                         [nv9_child["envelope"], nv9_parent["envelope"]])["envelope"]

# NV-010 metered PAY_INVOICE whose watermark cumulative-billed is off by one
#   -> MUST reject (pay §4.1 arithmetic check; wire 6009). Identical to TV-016
#   except the watermark's cumulative-billed mantissa is 8629001 (the rate×usage
#   product is 8629000): opens + verifies structurally + signed by B, but the
#   billing identity fails. The invoice `amount` (key 3) stays the CORRECT
#   8629000, so exactly ONE mantissa digit differs from TV-016 (plus msg-id and
#   signature), isolating the arithmetic check as NV-001 isolates the signature.
wm_bad = {1: task_id, 2: 8, 3: WM_USAGE,
          4: ["iso4217:USD", cbor2.CBORTag(4, [-6, 8629001])]}
pay_invoice_bad = {1: bytes([0x30]) * 16, 2: bytes([0x10]) * 16, 3: CUM_BILLED,
                   4: wm_bad, 5: b"", 6: TS}
nv010 = sealed(kb, 0x0402, msgid(26), pay_invoice_bad)["envelope"]
assert 1523 * 3000 + 812 * 5000 != 8629001, "NV-010 must be arithmetically wrong"

# NV-011 PAY_RECEIPT with an UNRECOGNIZED kind -> MUST reject (pay §5.2.1
#   fail-closed; wire 6010). Identical content to TV-018 except key 5 = 9
#   (plus msg-id and signature): opens + verifies, but the kind registry only
#   admits 0..3 in this revision — receivers MUST reject unknown kinds.
bad_kind_receipt = {1: bytes([0x31]) * 16, 2: HOLD_TOTAL, 3: TS, 5: 9}
nv011 = sealed(kb, 0x0404, msgid(29), bad_kind_receipt)["envelope"]

# NV-012 PAY_DISPUTE with an UNKNOWN claim code -> MUST reject (pay §6.1
#   fail-closed; codec class). Identical content to TV-019 except key 3 = 9
#   (plus msg-id and signature): opens + verifies, but the claim registry
#   admits only 0..4 in this revision.
pay_dispute_bad = dict(pay_dispute); pay_dispute_bad[3] = 9
nv012 = sealed(ka, 0x0405, msgid(33), pay_dispute_bad)["envelope"]

# =====================================================================
# Emit markdown. Existing sections (through Verification procedure) are kept
# byte-for-byte; new sections are appended, closing with Derived constants.
# =====================================================================
prefix = f"""# MAP Test Vectors (TV-001, TV-002)

Status: Normative examples for draft-map-core-01 and draft-map-trust-01.
All CBOR uses the deterministic encoding profile of draft-map-core-01 §4.2
(RFC 8949 §4.2.1 core deterministic encoding; definite lengths only).
All signatures are Ed25519 (RFC 8032) and verify against the keys below.

## Keys (deterministic; for testing ONLY — never use in production)

Key A (requester) seed = SHA-256("MAP-TEST-VECTOR-KEY-A"):
  seed    = {seed_a.hex()}
  pubkey  = {pub_a.hex()}
  AgentID = {aid_a.hex()}  (multicodec ed25519-pub || key)
  AgentID (multibase base58btc) = {b58(aid_a)}

Key B (provider) seed = SHA-256("MAP-TEST-VECTOR-KEY-B"):
  seed    = {seed_b.hex()}
  pubkey  = {pub_b.hex()}
  AgentID = {aid_b.hex()}
  AgentID (multibase base58btc) = {b58(aid_b)}

## TV-001 — HELLO envelope (COSE_Sign1, CBOR tag 18)

Protected header map (diagnostic):
  {{1: -8, 4: h'{aid_a.hex()}'}}

protected (bstr contents, hex):
  {protected.hex()}

Body (diagnostic):
  {{0: 256, 1: 1, 2: h'000102030405060708090a0b0c0d0e0f', 3: 1780000000000,
   10: {{1: [256], 2: [1], 3: {{}}, 4: 262144}}}}

payload (bstr contents, hex):
  {payload.hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {sig_structure.hex()}

signature (Ed25519 by Key A, hex):
  {sig1.hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(envelope)} bytes):
  {envelope.hex()}

## TV-002 — ExecutionReceipt (dual-signed)

ReceiptCore (diagnostic):
  {{1: h'{core[1].hex()}',
   2: h'{aid_a.hex()}',
   3: h'{aid_b.hex()}',
   4: "map:cap:inference/text-generation",
   5: 1780000000000, 6: 1780000042000, 7: 0,
   8: {{1: 1523, 2: 812, 3: 431, 4: 42000}}, 9: 92}}

core (encoded, hex):
  {core_enc.hex()}

Requester signs enc(["MAPReceipt", 1, core]) — signature (Key A, hex):
  {sig_req.hex()}

Provider signs enc(["MAPReceipt", 2, core]) — signature (Key B, hex):
  {sig_prov.hex()}

Receipt object {{1: core, 2: sig_req, 3: sig_prov}} (hex, {len(receipt)} bytes):
  {receipt.hex()}

## Verification procedure

1. Decode outer CBOR; check tag 18 and 4-element array (TV-001).
2. Re-encode nothing: signatures are computed over the exact transmitted
   byte strings (`protected`, `payload`) — implementations MUST NOT
   re-serialize before verifying.
3. Build Sig_structure / MAPReceipt array with those byte strings, encode
   deterministically, verify Ed25519 signature against the pubkey carried in
   the AgentID (strip 2-byte multicodec prefix).
"""

def tv_block(num, name, mtype, signer, cite, diag, s):
    return f"""
## TV-{num:03d} — {name} ({mtype}), signed by Key {signer}

Field numbers per {cite}. msg-id = h'{msgid(num).hex()}', timestamp {TS}.

Content (diagnostic):
  {diag}

payload (bstr contents, hex):
  {s['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {s['sig_struct'].hex()}

signature (Ed25519 by Key {signer}, hex):
  {s['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(s['envelope'])} bytes):
  {s['envelope'].hex()}
"""

new = ""
new += tv_block(3, "HELLO_ACK", "0x0002", "B", "core §6.1",
                "{1: 256, 2: 1, 3: {}, 4: 262144}", tv003)
new += tv_block(4, "ERROR", "0x0006", "B", "core §9",
                "{1: 1003, 2: false, 3: \"replayed msg-id\", 4: h'000102030405060708090a0b0c0d0e0f'}", tv004)
new += tv_block(5, "DESCRIPTOR", "0x0100", "B", "discovery §2",
                "{1: h'" + aid_b.hex() + "', 2: 1, 3: [256], 4: [1], 5: [\"/map/1.0.0\"], 6: [{1: \"" + CAP + "\"}], 12: [2]}", tv005)
new += tv_block(6, "OFFER", "0x0200", "A", "tasks §2",
                "{1: h'" + task_id.hex() + "', 2: \"" + CAP + "\", 3: {}}", tv006)
new += tv_block(7, "ASSIGN", "0x0202", "A", "tasks §2.1",
                "{1: h'" + task_id.hex() + "', 2: \"" + CAP + "\", 3: {}, 4: {2: [h'" + multihash_sha256(b'MAP-TV-INPUT').hex() + "']}, 5: {1: 64, 2: 1048576}}", tv007)
new += tv_block(8, "CHUNK", "0x0205", "B", "tasks §3",
                "{1: h'" + task_id.hex() + "', 2: 0, 3: 0, 4: h'" + chunk_content.hex() + "'}", tv008)
new += tv_block(9, "COMPLETE", "0x020D", "B", "tasks §4 / §8.1",
                "{1: h'" + task_id.hex() + "', 2: 0, 3: h'" + (bytes([0x12, 0x20]) + transcript_root).hex() + "', 4: {1: 1523, 2: 812, 3: 431, 4: 42000}}", tv009)
new += tv_block(10, "PAY_QUOTE", "0x0400", "B", "pay §3",
                "{1: h'" + (bytes([0x10]) * 16).hex() + "', 2: 0, 3: {0: [\"iso4217:USD\", 4([-6, 4200000])]}, 5: [{1: \"pay:credit-ledger\", 2: h''}], 6: 1780000600000}", tv010)
new += f"""
## TV-010b — PAY_QUOTE bound to a task (0x0400), signed by Key B

Field numbers per pay §3 / §3.1. msg-id = h'{msgid(11).hex()}', timestamp {TS}.
TV-010 plus the mandatory task binding: key 9 (TaskID) MUST be present and equal
the OFFER's TaskID when a PAY_QUOTE answers an OFFER carrying one (pay §3.1).
Key 9 = h'{task_id.hex()}' (= the TV-006 OFFER TaskID). TV-010 remains the valid
*unbound standing quote*; this bound variant is additive.

Content (diagnostic):
  {{1: h'{(bytes([0x10]) * 16).hex()}', 2: 0, 3: {{0: ["iso4217:USD", 4([-6, 4200000])]}}, 5: [{{1: "pay:credit-ledger", 2: h''}}], 6: 1780000600000, 9: h'{task_id.hex()}'}}

payload (bstr contents, hex):
  {tv010b['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv010b['sig_struct'].hex()}

signature (Ed25519 by Key B, hex):
  {tv010b['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv010b['envelope'])} bytes):
  {tv010b['envelope'].hex()}
"""
new += f"""
## TV-011 — DelegationCert (0x0502), signed by Key A (RIK)

Field numbers per identity §4. msg-id = h'{msgid(13).hex()}', timestamp {TS}.
RIK (Key A) delegates to DK (seed SHA-256("MAP-TEST-VECTOR-KEY-A-DK")); cap
scope = one CapURI, max further-delegation depth = 1. A cert IS an envelope
whose kid is the ISSUER (RIK).

DK AgentID  = h'{aid_a_dk.hex()}'
DelegateBody (diagnostic):
  {{1: h'{aid_a.hex()}', 2: h'{aid_a_dk.hex()}', 3: {TS}, 4: {DELEG_EXPIRY}, 5: ["{CAP}"], 7: 1}}

payload (bstr contents, hex):
  {tv011['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv011['sig_struct'].hex()}

signature (Ed25519 by Key A, hex):
  {tv011['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv011['envelope'])} bytes):
  {tv011['envelope'].hex()}

## TV-012 — delegated OFFER (0x0200), signed by DK, chain in ProtHdr key 33

Field numbers per tasks §2. msg-id = h'{msgid(14).hex()}', timestamp {TS}.
An OFFER signed by DK (the leaf), carrying the 1-link chain [TV-011] in the
protected header (identity §6; order leaf..root). A verifier resolves the
accountable root to RIK (Key A). The protected header is
`{{1: -8, 4: DK-AgentID, 33: [TV-011-envelope]}}`.

SK AgentID (used by NV-007) = h'{aid_a_sk.hex()}'
OFFER content (diagnostic):
  {{1: h'{task_id.hex()}', 2: "{CAP}", 3: {{}}}}

payload (bstr contents, hex):
  {tv012['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv012['sig_struct'].hex()}

signature (Ed25519 by DK, hex):
  {tv012['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv012['envelope'])} bytes):
  {tv012['envelope'].hex()}
"""
new += f"""
## TV-013 — PAY_AGREE (0x0401), signed by Key A

Field numbers per pay §3.1. msg-id = h'{msgid(18).hex()}', timestamp {TS}.
Agrees to the bound quote TV-010b: key 2 is the Multihash of TV-010b's received
**envelope** bytes (pay §3.1, invariant I1); key 4 is the authorized cap.

Content (diagnostic):
  {{1: h'{(bytes([0x10]) * 16).hex()}', 2: h'{multihash_sha256(tv010b['envelope']).hex()}', 3: "pay:credit-ledger", 4: ["iso4217:USD", 4([-6, 4200000])]}}

payload (bstr contents, hex):
  {tv013['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv013['sig_struct'].hex()}

signature (Ed25519 by Key A, hex):
  {tv013['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv013['envelope'])} bytes):
  {tv013['envelope'].hex()}

## TV-014 — RESERVE (0x020F), signed by Key A

Field numbers per tasks §6. msg-id = h'{msgid(19).hex()}', timestamp {TS}.
Reserves CAP in the window [{RSV_FROM}, {RSV_UNTIL}] with a zone-1 constraint.

Content (diagnostic):
  {{1: h'{RSV_ID.hex()}', 2: "{CAP}", 3: {RSV_FROM}, 4: {RSV_UNTIL}, 5: {{5: 1}}}}

payload (bstr contents, hex):
  {tv014['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv014['sig_struct'].hex()}

signature (Ed25519 by Key A, hex):
  {tv014['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv014['envelope'])} bytes):
  {tv014['envelope'].hex()}

## TV-015 — LEASE granted (0x0210), signed by Key B

Field numbers per tasks §6. msg-id = h'{msgid(20).hex()}', timestamp {TS}.
Grants TV-014's reservation (status 0) with a window inside the requested one.

Content (diagnostic):
  {{1: h'{RSV_ID.hex()}', 2: 0, 3: [1780000100000, 1780000500000]}}

payload (bstr contents, hex):
  {tv015['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv015['sig_struct'].hex()}

signature (Ed25519 by Key B, hex):
  {tv015['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv015['envelope'])} bytes):
  {tv015['envelope'].hex()}

## TV-016 — metered PAY_INVOICE with UsageWatermark (0x0402), signed by Key B

Field numbers per pay §4 / §4.1. msg-id = h'{msgid(25).hex()}', timestamp {TS}.
The FIRST incremental invoice of a terms-3 (metered) task, so the invoice amount
(key 3) equals the watermark's cumulative-billed (key 4). Model-1 rates in the
agreed PAY_QUOTE (NOT carried in the invoice): rate[1] = 4([-6, 3000]) µUSD per
token-in, rate[2] = 4([-6, 5000]) µUSD per token-out. A verifier recomputes
cumulative-billed from the watermark's Usage: 1523×3000 + 812×5000 =
4569000 + 4060000 = **8629000** µUSD.

Content (diagnostic):
  {{1: h'{(bytes([0x30])*16).hex()}', 2: h'{(bytes([0x10])*16).hex()}', 3: ["iso4217:USD", 4([-6, 8629000])], 4: {{1: h'{task_id.hex()}', 2: 8, 3: {{1: 1523, 2: 812, 3: 431, 4: 42000}}, 4: ["iso4217:USD", 4([-6, 8629000])]}}, 5: h'', 6: {TS}}}

payload (bstr contents, hex):
  {tv016['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv016['sig_struct'].hex()}

signature (Ed25519 by Key B, hex):
  {tv016['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv016['envelope'])} bytes):
  {tv016['envelope'].hex()}

## TV-017 — escrow hold PAY_INVOICE (0x0402, terms 2 phase 1), signed by Key B

Field numbers per pay §4 / §5.2.1. msg-id = h'{msgid(27).hex()}', timestamp {TS}.
The phase-1 (hold) invoice of a terms-2 (escrow) agreement: key 7 = 1 (hold),
amount (key 3) = the agreed quote's key-4 total, NO UsageWatermark (key 4 is
present iff terms 3). The method payload (key 5) is the pay §5.2.4
`pay:credit-ledger` hold payload: the demanded expiry-ts ({HOLD_EXPIRY}) as a
deterministic-CBOR uint.

Content (diagnostic):
  {{1: h'{(bytes([0x31])*16).hex()}', 2: h'{(bytes([0x10])*16).hex()}', 3: ["iso4217:USD", 4([-2, 500])], 5: h'{enc(HOLD_EXPIRY).hex()}', 6: {TS}, 7: 1}}

payload (bstr contents, hex):
  {tv017['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv017['sig_struct'].hex()}

signature (Ed25519 by Key B, hex):
  {tv017['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv017['envelope'])} bytes):
  {tv017['envelope'].hex()}

## TV-018 — held PAY_RECEIPT (0x0404, kind 1), signed by Key B

Field numbers per pay §4 / §5.2.1. msg-id = h'{msgid(28).hex()}', timestamp {TS}.
The receipt for TV-017's settled hold round: kind (key 5) = 1 (held) — the
payee's signed statement that a conforming hold exists on the rail. Kind 1 is
NOT a settlement statement (only kinds 0 and 2 feed
ExecutionReceipt.payment-ref). Key 4 (proof digest) is deliberately absent.

Content (diagnostic):
  {{1: h'{(bytes([0x31])*16).hex()}', 2: ["iso4217:USD", 4([-2, 500])], 3: {TS}, 5: 1}}

payload (bstr contents, hex):
  {tv018['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv018['sig_struct'].hex()}

signature (Ed25519 by Key B, hex):
  {tv018['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv018['envelope'])} bytes):
  {tv018['envelope'].hex()}

## TV-019 — PAY_DISPUTE (0x0405, claim 3 overbilling), signed by Key A

Field numbers per pay §6.1. msg-id = h'{msgid(30).hex()}', timestamp {TS}.
The payer opens a claim-3 (overbilling) dispute over the TV-016 invoice,
anchored to the governing agreement by PayAgreeRef — key 4 = {{quote-id,
multihash of the received TV-013 PAY_AGREE envelope bytes}} (I1) — and bound
to the TV-006 TaskID (key 5). Key 6 (free-text statement) is deliberately
absent: it is never evidence and never influences a verdict.

Content (diagnostic):
  {{1: h'{DISPUTE_ID.hex()}', 2: h'{(bytes([0x30])*16).hex()}', 3: 3, 4: {{1: h'{(bytes([0x10])*16).hex()}', 2: h'{multihash_sha256(tv013['envelope']).hex()}'}}, 5: h'{task_id.hex()}', 7: {TS}}}

payload (bstr contents, hex):
  {tv019['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv019['sig_struct'].hex()}

signature (Ed25519 by Key A, hex):
  {tv019['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv019['envelope'])} bytes):
  {tv019['envelope'].hex()}

## TV-020 — PAY_DISPUTE_EVIDENCE (0x0406), signed by Key A

Field numbers per pay §6.1. msg-id = h'{msgid(31).hex()}', timestamp absent
from content (the envelope carries the timestamp). One EvidenceItem of kind 0
(PAY_QUOTE envelope) whose key-2 bytes are the frozen **TV-010b envelope
VERBATIM** — this vector exercises the I1 verbatim-carry rule with bytes the
suite already pins byte-exactly. Key 3 (CB) absent: the artifact was signed
out-of-session (empty external_aad, pay §6.2); a third party verifies it from
these bytes alone.

Content (diagnostic):
  {{1: h'{DISPUTE_ID.hex()}', 2: [{{1: 0, 2: h'<the {len(tv010b['envelope'])}-byte TV-010b envelope>'}}]}}

payload (bstr contents, hex):
  {tv020['payload'].hex()}

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  {tv020['sig_struct'].hex()}

signature (Ed25519 by Key A, hex):
  {tv020['sig'].hex()}

Complete envelope 18([protected, {{}}, payload, signature]) (hex, {len(tv020['envelope'])} bytes):
  {tv020['envelope'].hex()}
"""

negatives = f"""
## Negative vectors (MUST reject)

Each entry is a byte string an implementation MUST reject; the required error
class is a core §9 code (except NV-004, a discovery §5 cache decision).

### NV-001 — tampered payload byte → BAD_SIGNATURE (2000)
Envelope byte at offset 60 (inside the payload region) XOR 0xFF; the signature
no longer verifies over the received bytes (invariant I1).
envelope (hex, {len(nv001)} bytes):
  {nv001.hex()}

### NV-002 — wrong outer tag → MALFORMED (1000)
TV-001 with the outer CBOR tag rewritten 18 → 17; not a COSE_Sign1 envelope.
envelope (hex, {len(nv002)} bytes):
  {nv002.hex()}

### NV-003 — non-canonical (indefinite-length) body → MALFORMED (1000)
The body map is re-encoded with an indefinite-length head (0xbf…0xff) and
RE-SIGNED, so the Ed25519 signature is VALID and ONLY the deterministic-encoding
profile is violated (isolates the canonicality check from signature checking).
envelope (hex, {len(nv003)} bytes):
  {nv003.hex()}

### NV-004 — stale descriptor seq → STALE (discovery §5 LWW; keep newer)
TV-005 re-issued with seq 0 (≤ the cached seq 1). Not a core §9 error — the
receiver keeps the newer record. No wire byte is malformed.
envelope (hex, {len(nv004)} bytes):
  {nv004.hex()}

### NV-005 — swapped receipt signatures → BAD_SIGNATURE (2000)
TV-002 with signature fields 2 and 3 swapped; each MAPReceipt role tag binds a
specific signer, so both signatures fail to verify.
receipt (hex, {len(nv005)} bytes):
  {nv005.hex()}

### NV-006 — mismatched task binding → pay §3.1 REJECT (policy, not core §9)
A PAY_QUOTE whose key 9 (task binding) is present but ≠ the OFFER's TaskID. The
envelope is well-formed and correctly signed by Key B — it OPENS — but pay §3.1
requires the requester to treat a mismatching binding as a quote for a different
task and reject it. This is the executable counterpart of TV-010b (the matching
bound quote). Key 9 = h'{bad_task.hex()}' (the TaskID h'{task_id.hex()}' with its
first byte flipped). Like NV-004, this is a policy rejection, not a wire error.
envelope (hex, {len(nv006)} bytes):
  {nv006.hex()}

### NV-007 — delegation chain that widens cap scope → BAD_DELEGATION (2003)
A 2-link chain RIK→DK→SK where the child (DK→SK) broadens the cap scope beyond
its parent (identity §4 attenuation is tighten-only). The envelope is signed by
SK (the leaf) and opens; `open_delegated` MUST reject the chain with a widened-
scope error. Parent scope = "map:cap:inference/text"; child scope =
"map:cap:inference" (wider). (Renumbered from TASK-010's planned NV-006, which
the bound-PAY_QUOTE pass already consumed.)
envelope (hex, {len(nv007)} bytes):
  {nv007.hex()}

### NV-008 — LEASE granted outside the requested window → reject (tasks §6)
A LEASE (status 0 = granted) for TV-014's rsv-id whose granted window falls
entirely after the reservation's `until` ({RSV_UNTIL}). tasks §6 requires a
granted window to fall within the requested `[from, until]`; the requester MUST
treat this as a rejection. The envelope is well-formed and signed by Key B — it
opens — but the lease is semantically invalid (cf. NV-004/NV-006, policy rejects).
envelope (hex, {len(nv008)} bytes):
  {nv008.hex()}

### NV-009 — delegation chain exceeding max-depth → BAD_DELEGATION (2003)
A 2-link chain RIK→DK→SK whose parent (RIK→DK) declares max-depth 0 (key 7) —
DK may not further delegate — yet DK→SK does. Both links carry the IDENTICAL
scope ("{CAP}"), so scope is NOT widened; only the identity §4 depth bound is
violated, isolating the max-depth check. The envelope is signed by SK (the
leaf) and opens; `open_delegated` MUST reject the chain with a depth error.
Per the 2026-07-12 ARCHITECT RULING, a cert with max-depth d admits at most d
certificates between itself and the leaf; the parent (chain index 1) has one.
envelope (hex, {len(nv009)} bytes):
  {nv009.hex()}

### NV-010 — metered invoice with off-by-one billing → INVOICE_WATERMARK_VIOLATION (6009)
Identical to TV-016 except the watermark's cumulative-billed mantissa is 8629001,
while the model-1 rate table applied to the watermark's Usage bills exactly
8629000 (1523×3000 + 812×5000). The envelope opens and its signature verifies
(well-formed, signed by Key B), but the pay §4.1 arithmetic check MUST reject it
with 6009. The invoice `amount` (key 3) is left at the correct 8629000, so this
differs from TV-016 in exactly one mantissa digit (plus msg-id and signature),
isolating the arithmetic check the way NV-001 isolates the signature check.
envelope (hex, {len(nv010)} bytes):
  {nv010.hex()}

### NV-011 — receipt with an unrecognized kind → ESCROW_VIOLATION (6010)
Identical content to TV-018 except the kind (key 5) is 9 (plus msg-id and
signature). The envelope opens and its signature verifies (well-formed, signed
by Key B), but pay §5.2.1 registers only kinds 0..3 in this revision and
requires receivers to reject unrecognized kinds fail-closed — the executable
counterpart of TV-018, isolating the kind-registry check.
envelope (hex, {len(nv011)} bytes):
  {nv011.hex()}

### NV-012 — dispute with an unknown claim code → reject (pay §6.1 fail-closed)
Identical content to TV-019 except the claim (key 3) is 9 (plus msg-id and
signature). The envelope opens and its signature verifies (well-formed, signed
by Key A), but pay §6.1 registers only claims 0..4 in this revision and
requires receivers to reject unknown codes fail-closed — the executable
counterpart of TV-019, isolating the claim-registry check exactly as NV-011
isolates the receipt-kind registry.
envelope (hex, {len(nv012)} bytes):
  {nv012.hex()}
"""

derived = f"""
## Derived constants

For cross-implementation spot checks (not themselves envelopes).

Negotiation transcript hash (core §6.2), TH = SHA-256("MAPv1-transcript" ||
TV-001-envelope || TV-003-envelope):
  {TH.hex()}

Channel binding (core §7.3), CB = HKDF-SHA256(salt=TH, ikm=h'{'00'*32}'
(stand-in exporter — no real transport in vectors), info="map cb", L=32):
  {CB.hex()}

Discovery sharding for Key B (discovery §6, first byte of SHA-256(AgentID)):
  shard(B) = {shard_b} (0x{shard_b:02x})
  K(cap)          = {cap_key.hex()}
  K'(cap, shard)  = {sharded_key.hex()}

Transcript root over the single TV-008 output chunk (tasks §8.1), raw 32-byte
Merkle root (the TranscriptBuilder value; wrapped as Multihash in TV-009):
  {transcript_root.hex()}
"""

# =====================================================================
# v2 vectors (accountability-layer rework). TV-021: a dual-signed
# CheckpointArtifact (layer §7.1): requester = Key A (role 1), provider =
# Key B (role 2); both sign enc(["MAPCheckpoint", role, core-bytes]), so the
# cumulative usage, zone and root are all covered by both signatures.
# =====================================================================
def tleaf(seq, kind, content):
    return hashlib.sha256(bytes([0x00]) + enc([seq, kind, content])).digest()

def tparent(l, r):
    return hashlib.sha256(bytes([0x01]) + l + r).digest()

cp_chunks = [b"Hello, ", b"accountable ", b"MAP!"]      # seq 0..2, kind 0
cp_leaves = [tleaf(i, 0, c) for i, c in enumerate(cp_chunks)]
cp_root = tparent(tparent(cp_leaves[0], cp_leaves[1]), cp_leaves[2])  # odd leaf promoted

def checkpoint_artifact(root, sign_req=True, tamper_req=False):
    ccore = {1: task_id, 2: aid_a, 3: aid_b, 4: 2,
             5: bytes([0x12, 0x20]) + root,
             6: {1: 10, 2: 3, 3: 120, 4: 900},
             7: 2, 8: TS + 900}
    ccore_enc = enc(ccore)
    s_prov = kb.sign(enc(["MAPCheckpoint", 2, ccore_enc])).signature
    s_req = ka.sign(enc(["MAPCheckpoint", 1, ccore_enc])).signature
    kb.verify_key.verify(enc(["MAPCheckpoint", 2, ccore_enc]), s_prov)
    ka.verify_key.verify(enc(["MAPCheckpoint", 1, ccore_enc]), s_req)
    if tamper_req:
        s_req = bytes([s_req[0] ^ 0x01]) + s_req[1:]
    art = {1: ccore_enc, 3: s_prov}
    if sign_req:
        art[2] = s_req
    return ccore_enc, enc(art)

tv021_core, tv021 = checkpoint_artifact(cp_root)
_, nv013 = checkpoint_artifact(cp_root, tamper_req=True)
nv014_root = cp_root[:-1] + bytes([cp_root[-1] ^ 0x01])
_, nv014 = checkpoint_artifact(nv014_root)

v2 = f"""
## TV-021 — CheckpointArtifact, dual-signed (layer §7.1)

Transcript prefix (layer §5): three kind-0 leaves, seq 0..2, contents the
UTF-8 strings "Hello, ", "accountable ", "MAP!". Transcript root (raw 32
bytes; CheckpointCore key 5 carries it as Multihash 0x1220 || root):
  {cp_root.hex()}

CheckpointCore: task = TV-002's TaskID, requester = Key A, provider = Key B,
through-seq 2, usage {{1: 10, 2: 3, 3: 120, 4: 900}}, zone 2, timestamp
{TS + 900}. Signing input for role r: enc(["MAPCheckpoint", r, core-bytes]);
role 1 = Key A (requester countersignature), role 2 = Key B (provider).

core bytes (hex, {len(tv021_core)} bytes):
  {tv021_core.hex()}

artifact {{1: core-bytes, 2: sig_req, 3: sig_prov}} (hex, {len(tv021)} bytes):
  {tv021.hex()}

## Negative vectors (v2)

### NV-013 — checkpoint with a corrupted requester countersignature → reject
TV-021 with the first byte of sig_req flipped. The provider signature is
valid; the artifact is not a valid resumption point (layer C-2 / R-1).
artifact (hex, {len(nv013)} bytes):
  {nv013.hex()}

### NV-014 — dual-signed checkpoint whose root does not match the prefix → reject resume
Both signatures are valid, but key 5 carries TV-021's root with its last byte
flipped. Resuming from it against TV-021's transcript prefix MUST fail
(layer §7.4 step 2: the successor recomputes the root and rejects).
artifact (hex, {len(nv014)} bytes):
  {nv014.hex()}
"""

# =====================================================================
# TV-022: a signed A2A Agent Card carrying the MAP extension (binding A2A-1,
# A2A §8.4 JWS with the payload detached; alg EdDSA, kid = Key B's AgentID in
# multibase). JCS here is json.dumps(sort_keys, compact): identical to
# RFC 8785 for this ASCII-only, number-free document (asserted). The card is
# inside the binding's interop profile (A2A-31): no `required: false`, no
# numbers in `params` (A2A-3a), no empty values, so its bytes are also what
# the reference a2a-sdk canonicalizer produces.
# =====================================================================
import base64, json

def b64u(b: bytes) -> str:
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()

def jcs_ascii(o) -> bytes:
    s = json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    assert s.isascii()
    return s.encode()

kid_b = b58(aid_b)
card = {
    "name": "MAP test vector provider",
    "description": "TV-022 Agent Card",
    "version": "1.0.0",
    "supportedInterfaces": [{"url": "https://provider.example.org/a2a",
                             "protocolBinding": "JSONRPC", "protocolVersion": "1.0"}],
    "capabilities": {"streaming": False, "extensions": [{
        "uri": "https://map-protocol.org/ext/accountability/v2",
        "description": "MAP accountability layer: dual-signed receipts, countersigned checkpoints, zone labels, metering evidence.",
        "params": {"agentId": kid_b, "supportedZones": ["2", "3"],
                   "checkpointIntervalParts": "8", "acceptedVoucherRails": ["pay:mpp-tempo"]}}]},
    "defaultInputModes": ["text/plain"],
    "defaultOutputModes": ["text/plain"],
    "skills": [{"id": "generate", "name": "Generate", "description": "Text generation", "tags": ["text"]}],
}
card_payload = jcs_ascii(card)
card_protected = b64u(jcs_ascii({"alg": "EdDSA", "typ": "JOSE", "kid": kid_b}))
card_input = (card_protected + "." + b64u(card_payload)).encode()
card_sig = kb.sign(card_input).signature
kb.verify_key.verify(card_input, card_sig)
signed_card = dict(card)
signed_card["signatures"] = [{"protected": card_protected, "signature": b64u(card_sig)}]
tv022 = jcs_ascii(signed_card)

v2 += f"""
## TV-022 — signed A2A Agent Card with the MAP extension (binding A2A-1, D-4)

Signed by Key B with alg EdDSA, kid = Key B's AgentID (multibase, {kid_b}).
JWS signing input: BASE64URL(protected) || "." || BASE64URL(JCS(card without
"signatures")), the payload detached (A2A §8.4.2). The signed card below is
itself in RFC 8785 canonical form:

{tv022.decode()}

signed card, canonical JSON bytes (hex, {len(tv022)} bytes):
  {tv022.hex()}
"""

# =====================================================================
# TV-023: receipt admissibility at a fixed evaluation time (layer R-2..R-4,
# §8.2). Pins the trust projection's gates across implementations,
# including the unilateral-receipt rule: a receipt signed by only one party
# counts only if the requester signed it and the outcome is negative. Rules
# apply in order R-2 (self-dealing), R-3 (future-dated), R-4 (not
# admissible), then cancelled; several cases pin that order. Verdicts assume
# the default skew bound (120 000 ms) and issuer weight 1.
# =====================================================================
TV023_NOW = 1790000000000
TV023_SKEW = 120000

def tv023_receipt(n, signers, outcome, ended, self_deal=False):
    prov_aid, prov_key = (aid_a, ka) if self_deal else (aid_b, kb)
    rcore = {1: hashlib.sha256(b"tv023-%02d" % n).digest(), 2: aid_a, 3: prov_aid, 4: CAP,
             5: ended - 1000, 6: ended, 7: outcome, 8: {1: 10, 2: 20, 3: 30, 4: 1000}}
    rcore_enc = enc(rcore)
    r = {1: rcore_enc}
    if "req" in signers:
        r[2] = ka.sign(enc(["MAPReceipt", 1, rcore_enc])).signature
    if "prov" in signers:
        r[3] = prov_key.sign(enc(["MAPReceipt", 2, rcore_enc])).signature
    return enc(r)

_past = TV023_NOW - 60000
tv023_cases = [
    ("count",          "dual-signed, outcome 0 (ok)",                         ("req", "prov"), 0, _past, False),
    ("not-admissible", "provider-only, outcome 0",                            ("prov",),       0, _past, False),
    ("not-admissible", "requester-only, outcome 0 (unilateral positive)",     ("req",),        0, _past, False),
    ("count",          "requester-only, outcome 1 (failed)",                  ("req",),        1, _past, False),
    ("count",          "requester-only, outcome 2 (timeout)",                 ("req",),        2, _past, False),
    ("count",          "requester-only, outcome 4 (disputed)",                ("req",),        4, _past, False),
    ("not-admissible", "provider-only, outcome 1 (failed)",                   ("prov",),       1, _past, False),
    ("cancelled",      "dual-signed, outcome 3 (cancelled)",                  ("req", "prov"), 3, _past, False),
    ("not-admissible", "requester-only, outcome 3 (R-4 precedes cancelled)",  ("req",),        3, _past, False),
    ("self-dealing",   "dual-signed, requester == provider (Key A)",          ("req", "prov"), 0, _past, True),
    ("count",          "dual-signed, ended = now + skew (boundary)",          ("req", "prov"), 0, TV023_NOW + TV023_SKEW, False),
    ("future-dated",   "dual-signed, ended = now + skew + 1",                 ("req", "prov"), 0, TV023_NOW + TV023_SKEW + 1, False),
    ("self-dealing",   "self-dealing and future-dated (R-2 precedes R-3)",    ("req", "prov"), 0, TV023_NOW + TV023_SKEW + 1, True),
]
tv023 = [(v, d, tv023_receipt(i + 1, s, o, e, sd)) for i, (v, d, s, o, e, sd) in enumerate(tv023_cases)]

v2 += f"""
## TV-023 — receipt admissibility at a fixed evaluation time (layer R-2..R-4, §8.2)

Thirteen receipts over the TV-002 core layout (requester = Key A, provider =
Key B unless marked self-dealing, capability = TV-002's, started = ended -
1000, metrics {{1: 10, 2: 20, 3: 30, 4: 1000}}, TaskID = SHA-256("tv023-NN")).
Every present signature is valid. Evaluate each at now = {TV023_NOW} ms with
the default skew bound {TV023_SKEW} ms and issuer weight 1. The verdict is
what a trust projection's apply step MUST decide: `count`, or the rule that
skips it (`self-dealing` R-2, `future-dated` R-3, `not-admissible` R-4,
`cancelled` §8.2), with the rules applied in that order.

""" + "".join(f"case {i + 1:02d} {v} -- {d}\n  {r.hex()}\n" for i, (v, d, r) in enumerate(tv023))

out = prefix + new + negatives + derived + v2

# v2 (accountability-layer rework, 2026-09-25): vectors whose message types v2
# retired are annotated, never removed or renumbered. Bytes are unchanged.
RETIRED_V1 = {"TV-006", "TV-007", "TV-008", "TV-009", "TV-010", "TV-010b", "TV-013",
              "TV-014", "TV-015", "TV-016", "TV-017", "TV-018", "TV-019", "TV-020",
              "NV-006", "NV-008", "NV-010", "NV-011", "NV-012",
              # P3: delegation removed from v2 (layer §3.3)
              "TV-011", "TV-012", "NV-007", "NV-009"}
RETIRED_NOTE = """> **Status (2026-09-25, v2 accountability-layer rework).** Vectors marked
> **RETIRED(v1)** belong to message types that v2 retired: the v1 task
> protocol (OFFER/ASSIGN/CHUNK/COMPLETE/RESERVE/LEASE), the v1 settlement
> and dispute protocol (PAY_*), and delegation (DelegationCert and delegated
> envelopes; layer §3.3 drops it). Their bytes are unchanged and still
> regenerate from `gen_vectors.py`, but no v2 implementation is required to
> consume them. Retired vectors are never renumbered or reused. TV-008's
> transcript-root derivation remains live through the *Derived constants*
> section (layer §5). New v2 vectors (CheckpointArtifact, the A2A binding) are
> appended with fresh numbers.

"""
_lines = out.split("\n")
for _i, _line in enumerate(_lines):
    _m = re.match(r"^(#{2,3}) (TV-\d+b?|NV-\d+) ", _line)
    if _m and _m.group(2) in RETIRED_V1:
        _lines[_i] = _line + " — RETIRED(v1)"
out = "\n".join(_lines)
_anchor = "## Keys (deterministic; for testing ONLY"
assert out.count(_anchor) == 1
out = out.replace(_anchor, RETIRED_NOTE + _anchor)

open("map-test-vectors.md", "w", encoding="utf-8").write(out)
print("OK - vectors written and all signatures verified.")
print("TV-021", len(tv021), "| NV-013", len(nv013), "| NV-014", len(nv014), "(v2 checkpoints)")
print("TV-022", len(tv022), "(signed Agent Card)")
print("TV-023", len(tv023), "receipts (admissibility)")
print("TV-001", len(envelope), "| TV-002", len(receipt),
      "| new TVs 3..10 + 10b (bound quote) +", "6 negatives + derived constants")
print("TV-010b", len(tv010b["envelope"]), "| NV-006", len(nv006))
print("TV-011", len(tv011["envelope"]), "| TV-012", len(tv012["envelope"]),
      "| NV-007", len(nv007), "(delegation)")
print("TV-013", len(tv013["envelope"]), "| TV-014", len(tv014["envelope"]),
      "| TV-015", len(tv015["envelope"]), "| NV-008", len(nv008), "(pay-agree + reservations)")
print("NV-009", len(nv009), "(delegation max-depth exceeded)")
print("TV-016", len(tv016["envelope"]), "| NV-010", len(nv010), "(metered watermark billing)")
print("TV-017", len(tv017["envelope"]), "| TV-018", len(tv018["envelope"]), "| NV-011", len(nv011), "(escrow phase/kind)")
print("TV-019", len(tv019["envelope"]), "| TV-020", len(tv020["envelope"]), "| NV-012", len(nv012), "(disputes)")
