# MAP Test Vectors (TV-001, TV-002)

Status: Normative examples for draft-map-core-00 and draft-map-trust-00.
All CBOR uses the deterministic encoding profile of draft-map-core-00 §4.2
(RFC 8949 §4.2.1 core deterministic encoding; definite lengths only).
All signatures are Ed25519 (RFC 8032) and verify against the keys below.

## Keys (deterministic; for testing ONLY — never use in production)

Key A (requester) seed = SHA-256("MAP-TEST-VECTOR-KEY-A"):
  seed    = 6dbbc0077f020e6085d0db9d5662db32158e2eda8521a348ebc3643b1ff877e5
  pubkey  = 65c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1
  AgentID = ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1  (multicodec ed25519-pub || key)
  AgentID (multibase base58btc) = z6MkmJaZFLtdW8yn2t9b8An5D3bbYvziY7XabCVQ6xSCxah6

Key B (provider) seed = SHA-256("MAP-TEST-VECTOR-KEY-B"):
  seed    = ce1fba32e140ef080bc9523218892aa845b6d37a887f5518b7355cbdc21e6ff2
  pubkey  = c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d
  AgentID = ed01c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d
  AgentID (multibase base58btc) = z6Mkszo9kizvbjPPLe5KnKHedt9pCieVg3NCPoFdzD7446EG

## TV-001 — HELLO envelope (COSE_Sign1, CBOR tag 18)

Protected header map (diagnostic):
  {1: -8, 4: h'ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1'}

protected (bstr contents, hex):
  a20127045822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1

Body (diagnostic):
  {0: 256, 1: 1, 2: h'000102030405060708090a0b0c0d0e0f', 3: 1780000000000,
   10: {1: [256], 2: [1], 3: {}, 4: 262144}}

payload (bstr contents, hex):
  a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000

Sig_structure = ["Signature1", protected, h'', payload], encoded (hex):
  846a5369676e6174757265315828a20127045822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1405835a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000

signature (Ed25519 by Key A, hex):
  10307036a1d861d52cd830f729108aeda08654d455a5de07fe98402c9504099f84a77527dd031731ab52b7fd902e7ee125a2ff7d4740767a7e5b158b30840d0f

Complete envelope 18([protected, {}, payload, signature]) (hex, 166 bytes):
  d2845828a20127045822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1a05835a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000584010307036a1d861d52cd830f729108aeda08654d455a5de07fe98402c9504099f84a77527dd031731ab52b7fd902e7ee125a2ff7d4740767a7e5b158b30840d0f

## TV-002 — ExecutionReceipt (dual-signed)

ReceiptCore (diagnostic):
  {1: h'd06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25',
   2: h'ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1',
   3: h'ed01c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d',
   4: "map:cap:inference/text-generation",
   5: 1780000000000, 6: 1780000042000, 7: 0,
   8: {1: 1523, 2: 812, 3: 431, 4: 42000}, 9: 92}

core (encoded, hex):
  a9015820d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25025822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1035822ed01c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d0478216d61703a6361703a696e666572656e63652f746578742d67656e65726174696f6e051b0000019e70448800061b0000019e70452c10070008a4011905f30219032c031901af0419a41009185c

Requester signs enc(["MAPReceipt", 1, core]) — signature (Key A, hex):
  86e77545e95f12b67c227f0ec2327a12b29d74f2e10750b34b4c62ed16bcbe767892de0aa639cb41d4f19fa2c4699b5b395f2b96985477e48a6617fb277fd602

Provider signs enc(["MAPReceipt", 2, core]) — signature (Key B, hex):
  debf7676fe7b664ac12f093ebf2519632d86ab4e1a87435aaa4a4877580c26aa6af442abd8bc3b535d06b2253d3150e3fd20015d5f159dc0f56033475e676d04

Receipt object {1: core, 2: sig_req, 3: sig_prov} (hex, 327 bytes):
  a30158bda9015820d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25025822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1035822ed01c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d0478216d61703a6361703a696e666572656e63652f746578742d67656e65726174696f6e051b0000019e70448800061b0000019e70452c10070008a4011905f30219032c031901af0419a41009185c02584086e77545e95f12b67c227f0ec2327a12b29d74f2e10750b34b4c62ed16bcbe767892de0aa639cb41d4f19fa2c4699b5b395f2b96985477e48a6617fb277fd602035840debf7676fe7b664ac12f093ebf2519632d86ab4e1a87435aaa4a4877580c26aa6af442abd8bc3b535d06b2253d3150e3fd20015d5f159dc0f56033475e676d04

## Verification procedure

1. Decode outer CBOR; check tag 18 and 4-element array (TV-001).
2. Re-encode nothing: signatures are computed over the exact transmitted
   byte strings (`protected`, `payload`) — implementations MUST NOT
   re-serialize before verifying.
3. Build Sig_structure / MAPReceipt array with those byte strings, encode
   deterministically, verify Ed25519 signature against the pubkey carried in
   the AgentID (strip 2-byte multicodec prefix).
