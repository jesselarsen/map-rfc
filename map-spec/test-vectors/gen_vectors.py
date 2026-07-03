#!/usr/bin/env python3
"""Generate reproducible MAP test vectors: COSE_Sign1 envelope (HELLO) and dual-signed ExecutionReceipt."""
import cbor2, hashlib, base58
from nacl.signing import SigningKey

def agent_id(pub: bytes) -> bytes:
    # multicodec ed25519-pub (0xED, varint -> 0xED 0x01) || raw pubkey
    return bytes([0xED, 0x01]) + pub

def b58(b: bytes) -> str:
    return "z" + base58.b58encode(b).decode()  # multibase base58btc

def enc(o) -> bytes:
    return cbor2.dumps(o, canonical=True)

# --- Keys (deterministic seeds for reproducibility) ---
seed_a = hashlib.sha256(b"MAP-TEST-VECTOR-KEY-A").digest()
seed_b = hashlib.sha256(b"MAP-TEST-VECTOR-KEY-B").digest()
ka, kb = SigningKey(seed_a), SigningKey(seed_b)
pub_a, pub_b = ka.verify_key.encode(), kb.verify_key.encode()
aid_a, aid_b = agent_id(pub_a), agent_id(pub_b)

# =====================================================================
# TV1: HELLO envelope (COSE_Sign1, tag 18)
# =====================================================================
protected_map = {1: -8, 4: aid_a}                    # alg: EdDSA, kid: sender AgentID
protected = enc(protected_map)
body = {
    0: 0x0100,                                        # protocol-version 1.0
    1: 0x0001,                                        # msg-type HELLO
    2: bytes.fromhex("4d41502d54563031-"[:0] or "aa") ,  # placeholder replaced below
}
body = {
    0: 0x0100,
    1: 0x0001,
    2: bytes.fromhex("000102030405060708090a0b0c0d0e0f"),  # msg-id
    3: 1780000000000,                                 # timestamp (fixed, ms)
    10: {                                             # content: Hello
        1: [0x0100],                                  # supported protocol versions
        2: [1],                                       # supported crypto suites (1 = MAP-SUITE-1)
        3: {},                                        # offered extensions
        4: 262144,                                    # max-message-size
    },
}
payload = enc(body)
sig_structure = enc(["Signature1", protected, b"", payload])
sig1 = ka.sign(sig_structure).signature
envelope = enc(cbor2.CBORTag(18, [protected, {}, payload, sig1]))
# verify round-trip
ka.verify_key.verify(sig_structure, sig1)

# =====================================================================
# TV2: ExecutionReceipt, dual-signed (requester A, provider B)
# =====================================================================
core = {
    1: bytes.fromhex(hashlib.blake2b(b"example-task", digest_size=32).hexdigest()),  # task-id/hash
    2: aid_a,                                         # requester
    3: aid_b,                                         # provider
    4: "map:cap:inference/text-generation",           # capability URI
    5: 1780000000000, 6: 1780000042000,               # start / end ts
    7: 0,                                             # outcome: OK
    8: {1: 1523, 2: 812, 3: 431, 4: 42000},           # tokens-in, tokens-out, ttft-ms, wall-ms
    9: 92,                                            # rating 0-100 (requester-asserted)
}
core_enc = enc(core)
sig_req = ka.sign(enc(["MAPReceipt", 1, core_enc])).signature   # role 1 = requester
sig_prov = kb.sign(enc(["MAPReceipt", 2, core_enc])).signature  # role 2 = provider
receipt = enc({1: core_enc, 2: sig_req, 3: sig_prov})
ka.verify_key.verify(enc(["MAPReceipt", 1, core_enc]), sig_req)
kb.verify_key.verify(enc(["MAPReceipt", 2, core_enc]), sig_prov)

out = f"""# MAP Test Vectors (TV-001, TV-002)

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
open("map-test-vectors.md", "w").write(out)
print("OK — vectors written and all signatures verified.")
print("envelope bytes:", len(envelope), "| receipt bytes:", len(receipt))
