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
import cbor2, hashlib, hmac, base58
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

# =====================================================================
# Emit markdown. Existing sections (through Verification procedure) are kept
# byte-for-byte; new sections are appended, closing with Derived constants.
# =====================================================================
prefix = f"""# MAP Test Vectors (TV-001, TV-002)

Status: Normative examples for draft-map-core-00 and draft-map-trust-00.
All CBOR uses the deterministic encoding profile of draft-map-core-00 §4.2
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

out = prefix + new + negatives + derived
open("map-test-vectors.md", "w", encoding="utf-8").write(out)
print("OK - vectors written and all signatures verified.")
print("TV-001", len(envelope), "| TV-002", len(receipt),
      "| new TVs 3..10 + 10b (bound quote) +", "6 negatives + derived constants")
print("TV-010b", len(tv010b["envelope"]), "| NV-006", len(nv006))
print("TV-011", len(tv011["envelope"]), "| TV-012", len(tv012["envelope"]),
      "| NV-007", len(nv007), "(delegation)")
