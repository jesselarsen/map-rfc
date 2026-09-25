//! Normative vector constants, copied EXACTLY from the single source of truth
//! `spec/test-vectors/map-test-vectors.md`. Do not edit these to make a
//! test pass — if a producer path disagrees, the bug is in map-core/map-trust
//! (TASK-003 §11/§17). TASK-004 extends this file in lockstep with new TVs.

// --- Keys (deterministic; TEST ONLY) ---------------------------------------

/// seed_A = SHA-256("MAP-TEST-VECTOR-KEY-A").
pub const SEED_A_HEX: &str = "6dbbc0077f020e6085d0db9d5662db32158e2eda8521a348ebc3643b1ff877e5";
pub const PUBKEY_A_HEX: &str = "65c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1";
pub const AGENT_A_MB: &str = "z6MkmJaZFLtdW8yn2t9b8An5D3bbYvziY7XabCVQ6xSCxah6";

/// seed_B = SHA-256("MAP-TEST-VECTOR-KEY-B").
pub const SEED_B_HEX: &str = "ce1fba32e140ef080bc9523218892aa845b6d37a887f5518b7355cbdc21e6ff2";
pub const PUBKEY_B_HEX: &str = "c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d";
pub const AGENT_B_MB: &str = "z6Mkszo9kizvbjPPLe5KnKHedt9pCieVg3NCPoFdzD7446EG";

// --- TV-001 — HELLO envelope (COSE_Sign1, tag 18) --------------------------

/// payload (body) bstr contents.
pub const TV001_PAYLOAD_HEX: &str =
    "a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000";
/// Ed25519 signature by Key A over the Sig_structure.
pub const TV001_SIG_HEX: &str =
    "10307036a1d861d52cd830f729108aeda08654d455a5de07fe98402c9504099f84a77527dd031731ab52b7fd902e7ee125a2ff7d4740767a7e5b158b30840d0f";
/// Complete envelope 18([protected, {}, payload, signature]) — 166 bytes.
pub const TV001_HEX: &str =
    "d2845828a20127045822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1a05835a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000584010307036a1d861d52cd830f729108aeda08654d455a5de07fe98402c9504099f84a77527dd031731ab52b7fd902e7ee125a2ff7d4740767a7e5b158b30840d0f";

// --- TV-002 — ExecutionReceipt (dual-signed) -------------------------------

/// task-id = BLAKE2b-256("example-task"). Hardcoded — NO blake2 dependency.
pub const TV002_TASK_ID_HEX: &str =
    "d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25";
/// ReceiptCore encoded (the signed bytes).
pub const TV002_CORE_HEX: &str =
    "a9015820d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25025822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1035822ed01c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d0478216d61703a6361703a696e666572656e63652f746578742d67656e65726174696f6e051b0000019e70448800061b0000019e70452c10070008a4011905f30219032c031901af0419a41009185c";
/// Requester signature (Key A) over enc(["MAPReceipt", 1, core]).
pub const TV002_SIG_REQ_HEX: &str =
    "86e77545e95f12b67c227f0ec2327a12b29d74f2e10750b34b4c62ed16bcbe767892de0aa639cb41d4f19fa2c4699b5b395f2b96985477e48a6617fb277fd602";
/// Provider signature (Key B) over enc(["MAPReceipt", 2, core]).
pub const TV002_SIG_PROV_HEX: &str =
    "debf7676fe7b664ac12f093ebf2519632d86ab4e1a87435aaa4a4877580c26aa6af442abd8bc3b535d06b2253d3150e3fd20015d5f159dc0f56033475e676d04";
/// Receipt object {1: core, 2: sig_req, 3: sig_prov} — 327 bytes.
pub const TV002_HEX: &str =
    "a30158bda9015820d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25025822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1035822ed01c93c1b6a4ec188ddaf89164b788a66d375dca5169d05ba1b26b676204a00459d0478216d61703a6361703a696e666572656e63652f746578742d67656e65726174696f6e051b0000019e70448800061b0000019e70452c10070008a4011905f30219032c031901af0419a41009185c02584086e77545e95f12b67c227f0ec2327a12b29d74f2e10750b34b4c62ed16bcbe767892de0aa639cb41d4f19fa2c4699b5b395f2b96985477e48a6617fb277fd602035840debf7676fe7b664ac12f093ebf2519632d86ab4e1a87435aaa4a4877580c26aa6af442abd8bc3b535d06b2253d3150e3fd20015d5f159dc0f56033475e676d04";
