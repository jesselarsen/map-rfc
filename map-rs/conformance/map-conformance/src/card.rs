//! TV-022: a signed A2A Agent Card carrying the MAP extension (binding A2A-1,
//! layer D-4). Generated independently in Python (pynacl, json.dumps); Rust
//! must produce the same canonical bytes and verify the Python-signed card.

use crate::positive::key_b;
use crate::vectors3::{TV022_CARD_HEX, TV024_ENV_HEX};
use crate::{assert_bytes, from_hex};
use map_a2a::card::{build_card, sign_card, verify_card, CardParams};
use map_a2a::descriptor::{open_descriptor, seal_descriptor, DescriptorSpec};
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

/// TV-024 parameters, as the generator states them.
const TS: u64 = 1_780_000_000_000;
const TV024_TTL_MS: u64 = 3_600_000;

fn tv024_spec() -> DescriptorSpec {
    DescriptorSpec {
        seq: 1,
        endpoints: vec!["https://provider.example.org/a2a".into()],
        capabilities: vec!["map:cap:inference/text-generation".into()],
        zones: vec![2, 3],
    }
}

/// TV-024 produce: sealing TV-022's card into a v2 descriptor reproduces the
/// Python-generated envelope byte-for-byte.
pub fn case_tv024_produce() -> Result<(), String> {
    let kp = key_b();
    let unsigned = build_card(&kp.agent_id(), &params());
    let (env, _) = seal_descriptor(&kp, &unsigned, &tv024_spec(), [0x24; 16], TS, TV024_TTL_MS)
        .map_err(|e| format!("seal: {e:?}"))?;
    assert_bytes(TV024_ENV_HEX, &env)
}

/// TV-024 consume: the frozen envelope opens and validates within its
/// window, and is refused once expired.
pub fn case_tv024_consume() -> Result<(), String> {
    let env = from_hex(TV024_ENV_HEX)?;
    let (d, card) = open_descriptor(&env, TS + 1_000).map_err(|e| format!("open: {e:?}"))?;
    if d.agent != key_b().agent_id() || d.seq != 1 || d.zones != vec![2, 3] {
        return Err("TV-024 descriptor fields differ".into());
    }
    if jcs::canonicalize(&card).map_err(|e| format!("jcs: {e:?}"))? != from_hex(TV022_CARD_HEX)? {
        return Err("TV-024 embeds a card other than TV-022".into());
    }
    if open_descriptor(&env, TS + TV024_TTL_MS + 1).is_ok() {
        return Err("TV-024 accepted after expiry".into());
    }
    Ok(())
}
