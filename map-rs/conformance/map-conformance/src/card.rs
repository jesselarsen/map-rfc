//! TV-022: a signed A2A Agent Card carrying the MAP extension (binding A2A-1,
//! layer D-4). Generated independently in Python (pynacl, json.dumps); Rust
//! must produce the same canonical bytes and verify the Python-signed card.

use crate::positive::key_b;
use crate::vectors3::TV022_CARD_HEX;
use crate::{assert_bytes, from_hex};
use map_a2a::card::{build_card, sign_card, verify_card, CardParams};
use map_a2a::jcs;

fn params() -> CardParams {
    CardParams {
        name: "MAP test vector provider".into(),
        description: "TV-022 Agent Card".into(),
        version: "1.0.0".into(),
        jsonrpc_url: "https://provider.example.org/a2a".into(),
        skills: vec![("generate".into(), "Generate".into(), "Text generation".into(), vec!["text".into()])],
        supported_zones: vec![2, 3],
        checkpoint_interval_parts: 8,
        accepted_voucher_rails: vec!["pay:mpp-tempo".into()],
    }
}

pub fn case_tv022_produce() -> Result<(), String> {
    let kp = key_b();
    let (signed, _) = sign_card(&build_card(&kp.agent_id(), &params()), &kp).map_err(|e| format!("sign: {e:?}"))?;
    assert_bytes(TV022_CARD_HEX, &jcs::canonicalize(&signed).map_err(|e| format!("jcs: {e:?}"))?)
}

pub fn case_tv022_consume() -> Result<(), String> {
    let bytes = from_hex(TV022_CARD_HEX)?;
    let card: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("json: {e}"))?;
    verify_card(&card, &key_b().agent_id()).map_err(|e| format!("verify: {e:?}"))?;
    // The published bytes are themselves canonical.
    if jcs::canonicalize(&card).map_err(|e| format!("jcs: {e:?}"))? != bytes {
        return Err("TV-022 is not in RFC 8785 canonical form".into());
    }
    Ok(())
}
