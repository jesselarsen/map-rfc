//! Positive conformance cases: key derivation (FR-1) and byte-exact
//! produce/consume of TV-001 and TV-002 (FR-2..FR-5).

use crate::vectors::*;
use crate::{assert_bytes, from_hex};
use map_core::{msg_type, open, seal, Body, Hello, MsgId, PROTOCOL_VERSION_1_0};
use map_identity::Keypair;
use map_tasks::TaskId;
use map_trust::{
    sign_receipt, Metrics, Outcome, Receipt, ReceiptCore, ROLE_PROVIDER, ROLE_REQUESTER,
};

const TIMESTAMP_MS: u64 = 1_780_000_000_000;

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).into()
}

pub(crate) fn key_a() -> Keypair {
    Keypair::from_seed(sha256(b"MAP-TEST-VECTOR-KEY-A"))
}

pub(crate) fn key_b() -> Keypair {
    Keypair::from_seed(sha256(b"MAP-TEST-VECTOR-KEY-B"))
}

/// FR-1: derived keys match the published seed/pubkey/multibase exactly.
pub fn case_fr1_keys() -> Result<(), String> {
    for (label, ikm, seed_hex, pubkey_hex, multibase) in [
        ("A", "MAP-TEST-VECTOR-KEY-A", SEED_A_HEX, PUBKEY_A_HEX, AGENT_A_MB),
        ("B", "MAP-TEST-VECTOR-KEY-B", SEED_B_HEX, PUBKEY_B_HEX, AGENT_B_MB),
    ] {
        let seed = sha256(ikm.as_bytes());
        if hex::encode(seed) != seed_hex {
            return Err(format!("key {label}: seed mismatch"));
        }
        let id = Keypair::from_seed(seed).agent_id();
        if hex::encode(&id.as_bytes()[2..]) != pubkey_hex {
            return Err(format!("key {label}: pubkey mismatch"));
        }
        if id.to_multibase() != multibase {
            return Err(format!(
                "key {label}: multibase mismatch (got {})",
                id.to_multibase()
            ));
        }
    }
    Ok(())
}

/// The TV-001 HELLO body. msg-id is the literal bytes 00..0f (§9) and the empty
/// extensions map (key 3) is present, not omitted.
fn tv001_body() -> Body {
    let hello = Hello {
        versions: vec![0x0100],
        suites: vec![1],
        extensions: vec![],
        max_message_size: 262_144,
        max_tasks: None,
        zones: vec![],
    };
    Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: msg_type::HELLO,
        msg_id: MsgId([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]),
        timestamp_ms: TIMESTAMP_MS,
        recipient: None,
        ttl_ms: None,
        session_id: None,
        seq: None,
        content: hello.to_value(),
    }
}

/// FR-2: sealing the HELLO body with Key A and empty aad reproduces the vector.
pub fn case_tv001_produce() -> Result<(), String> {
    let env = seal(&key_a(), &tv001_body(), b"").map_err(|e| format!("seal: {e}"))?;
    assert_bytes(TV001_HEX, &env)
}

/// FR-3: opening the vector envelope succeeds and yields the expected fields
/// (over the received payload bytes — invariant I1).
pub fn case_tv001_consume() -> Result<(), String> {
    let env = from_hex(TV001_HEX)?;
    let opened = open(&env, b"").map_err(|e| format!("open: {e}"))?;
    if opened.sender != key_a().agent_id() {
        return Err("sender AgentID mismatch".into());
    }
    if opened.body.msg_type != msg_type::HELLO {
        return Err(format!("msg_type {:#06x} != HELLO", opened.body.msg_type));
    }
    if opened.body.timestamp_ms != TIMESTAMP_MS {
        return Err("timestamp mismatch".into());
    }
    assert_bytes(TV001_PAYLOAD_HEX, &opened.payload)
}

/// The TV-002 ReceiptCore.
fn tv002_core() -> Result<ReceiptCore, String> {
    let task_bytes = from_hex(TV002_TASK_ID_HEX)?;
    let task = TaskId(
        <[u8; 32]>::try_from(task_bytes.as_slice()).map_err(|_| "task-id not 32 bytes")?,
    );
    Ok(ReceiptCore {
        task,
        requester: key_a().agent_id(),
        provider: key_b().agent_id(),
        capability: "map:cap:inference/text-generation".to_string(),
        started_ms: TIMESTAMP_MS,
        ended_ms: 1_780_000_042_000,
        outcome: Outcome::Ok,
        metrics: Metrics {
            tokens_in: 1523,
            tokens_out: 812,
            ttft_ms: 431,
            wall_ms: 42_000,
        },
        rating: Some(92),
        payment_ref: None,
        transcript_root: None,
    })
}

/// FR-4: the encoded core, both signatures, and the assembled receipt are all
/// byte-identical to the vector.
pub fn case_tv002_produce() -> Result<(), String> {
    let core_bytes = tv002_core()?
        .encode()
        .map_err(|e| format!("core encode: {e}"))?;
    assert_bytes(TV002_CORE_HEX, &core_bytes)?;

    let sig_req = sign_receipt(&key_a(), ROLE_REQUESTER, &core_bytes);
    let sig_prov = sign_receipt(&key_b(), ROLE_PROVIDER, &core_bytes);
    assert_bytes(TV002_SIG_REQ_HEX, &sig_req)?;
    assert_bytes(TV002_SIG_PROV_HEX, &sig_prov)?;

    let receipt = Receipt {
        core_bytes,
        sig_requester: Some(sig_req),
        sig_provider: Some(sig_prov),
    };
    let enc = receipt.encode().map_err(|e| format!("receipt encode: {e}"))?;
    assert_bytes(TV002_HEX, &enc)
}

/// FR-5: decoding + verifying the vector receipt yields the same core fields.
pub fn case_tv002_consume() -> Result<(), String> {
    let bytes = from_hex(TV002_HEX)?;
    let receipt = Receipt::decode_bytes(&bytes).map_err(|e| format!("decode: {e}"))?;
    let verified = receipt.verify().map_err(|e| format!("verify: {e}"))?;
    if !verified.dual {
        return Err("receipt should be dual-signed".into());
    }
    if verified.core != tv002_core()? {
        return Err("decoded core fields differ from the vector".into());
    }
    Ok(())
}
