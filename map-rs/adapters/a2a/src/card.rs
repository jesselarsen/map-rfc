//! A2A Agent Cards (A2A v1.0 §4.4, §8.4) with the MAP extension block
//! (binding A2A-1..A2A-3) and JWS signatures.
//!
//! Signing follows A2A §8.4.2 with the payload detached: the signature is
//! over `BASE64URL(protected) || "." || BASE64URL(JCS(card without
//! `signatures`))`, and a verifier rebuilds that payload from the card it
//! received. MAP uses `alg: "EdDSA"` (RFC 8037) with `kid` = the provider's
//! AgentID in multibase form, so the card's signing key IS the AgentID key
//! (layer rule D-4).
//!
//! Default-value removal (A2A §8.4.1 rule 1) needs the proto schema. This
//! module applies it for the repeated fields MAP-built cards may leave empty
//! (empty arrays of non-REQUIRED repeated fields are dropped); cards built
//! here never emit other default values. A card from a different generator
//! that relies on further default elision may not verify here. That is a
//! known interop limit, stated rather than hidden (binding A2A-15).

use crate::{b64, jcs};
use map_identity::{verify, AgentId, Keypair};
use serde_json::{json, Map, Value};

/// The MAP extension URI (binding A2A-1). PLACEHOLDER: not registered.
pub const MAP_EXTENSION_URI: &str = "https://map-protocol.org/ext/accountability/v2";

/// A2A repeated fields that are REQUIRED and therefore kept even when empty
/// (A2A proto: AgentCard.supportedInterfaces/defaultInputModes/
/// defaultOutputModes/skills, AgentSkill.tags).
const REQUIRED_REPEATED: &[&str] = &["supportedInterfaces", "defaultInputModes", "defaultOutputModes", "skills", "tags"];

#[derive(Debug, PartialEq)]
pub enum CardError {
    NotAnObject,
    NoSignatures,
    /// No signature entry whose `kid` is the expected AgentID with alg EdDSA.
    NoMatchingSignature,
    BadEncoding(&'static str),
    BadSignature,
    Jcs(jcs::JcsError),
    /// The card's MAP extension block is missing or names another agent.
    ExtensionMismatch,
}

impl From<jcs::JcsError> for CardError {
    fn from(e: jcs::JcsError) -> Self {
        CardError::Jcs(e)
    }
}

/// Inputs for a MAP provider's Agent Card.
#[derive(Clone, Debug)]
pub struct CardParams {
    pub name: String,
    pub description: String,
    pub version: String,
    /// JSON-RPC interface URL (A2A §5: `supportedInterfaces[].url`).
    pub jsonrpc_url: String,
    /// (id, name, description, tags)
    pub skills: Vec<(String, String, String, Vec<String>)>,
    pub supported_zones: Vec<u8>,
    pub checkpoint_interval_parts: u64,
    pub accepted_voucher_rails: Vec<String>,
}

/// Build an unsigned Agent Card with the MAP extension declared (A2A-1..3).
pub fn build_card(agent: &AgentId, p: &CardParams) -> Value {
    let skills: Vec<Value> = p
        .skills
        .iter()
        .map(|(id, name, desc, tags)| json!({"id": id, "name": name, "description": desc, "tags": tags}))
        .collect();
    json!({
        "name": p.name,
        "description": p.description,
        "version": p.version,
        "supportedInterfaces": [
            {"url": p.jsonrpc_url, "protocolBinding": "JSONRPC", "protocolVersion": "1.0"}
        ],
        "capabilities": {
            "streaming": false,
            "extensions": [{
                "uri": MAP_EXTENSION_URI,
                "description": "MAP accountability layer: dual-signed receipts, countersigned checkpoints, zone labels, metering evidence.",
                // `required: false` is an implicit-presence default and so is
                // omitted (A2A-2); integers in `params` are decimal strings
                // (A2A-3a) so protobuf-based SDKs canonicalize them alike.
                "params": {
                    "agentId": agent.to_multibase(),
                    "supportedZones": p.supported_zones.iter().map(u8::to_string).collect::<Vec<_>>(),
                    "checkpointIntervalParts": p.checkpoint_interval_parts.to_string(),
                    "acceptedVoucherRails": p.accepted_voucher_rails
                }
            }]
        },
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": skills
    })
}

/// Recursively drop empty arrays of non-REQUIRED repeated fields.
fn drop_empty_repeated(v: &mut Value) {
    match v {
        Value::Object(m) => {
            let empties: Vec<String> = m
                .iter()
                .filter(|(k, x)| matches!(x, Value::Array(a) if a.is_empty()) && !REQUIRED_REPEATED.contains(&k.as_str()))
                .map(|(k, _)| k.clone())
                .collect();
            for k in empties {
                m.remove(&k);
            }
            for x in m.values_mut() {
                drop_empty_repeated(x);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(drop_empty_repeated),
        _ => {}
    }
}

/// Drop implicit-presence scalars (proto3 fields with neither `optional` nor
/// REQUIRED) that hold their default, from every object in `list`.
fn drop_implicit_defaults(list: Option<&mut Value>, fields: &[(&str, Value)]) {
    let Some(Value::Array(items)) = list else { return };
    for item in items.iter_mut().filter_map(Value::as_object_mut) {
        for (name, default) in fields {
            if item.get(*name) == Some(default) {
                item.remove(*name);
            }
        }
    }
}

/// The detached JWS payload: JCS of the card without `signatures`, after
/// default-value removal (A2A §8.4.1 rule 1). The implicit-presence scalars
/// in the card schema are `AgentExtension.{uri, description, required}` and
/// `AgentInterface.tenant`; every other card scalar is REQUIRED or
/// `optional` and is kept as sent.
pub fn signing_payload(card: &Value) -> Result<Vec<u8>, CardError> {
    let mut c = card.clone();
    let m = c.as_object_mut().ok_or(CardError::NotAnObject)?;
    m.remove("signatures");
    let empty = || Value::String(String::new());
    drop_implicit_defaults(
        m.get_mut("capabilities").and_then(|x| x.get_mut("extensions")),
        &[("uri", empty()), ("description", empty()), ("required", Value::Bool(false))],
    );
    drop_implicit_defaults(m.get_mut("supportedInterfaces"), &[("tenant", empty())]);
    drop_empty_repeated(&mut c);
    Ok(jcs::canonicalize(&c)?)
}

fn protected_header(agent: &AgentId) -> Result<String, CardError> {
    let h = json!({"alg": "EdDSA", "typ": "JOSE", "kid": agent.to_multibase()});
    Ok(b64::encode(&jcs::canonicalize(&h)?))
}

/// Sign `card` with `kp`, replacing any existing `signatures`. Returns the
/// signed card and the `AgentCardSignature` entry.
pub fn sign_card(card: &Value, kp: &Keypair) -> Result<(Value, Value), CardError> {
    let payload = signing_payload(card)?;
    let protected = protected_header(&kp.agent_id())?;
    let input = format!("{protected}.{}", b64::encode(&payload));
    let sig = kp.sign(input.as_bytes());
    let entry = json!({"protected": protected, "signature": b64::encode(&sig)});
    let mut signed = card.clone();
    signed
        .as_object_mut()
        .ok_or(CardError::NotAnObject)?
        .insert("signatures".into(), Value::Array(vec![entry.clone()]));
    Ok((signed, entry))
}

/// Verify that `card` carries a valid EdDSA signature by `expected` and that
/// its MAP extension names the same AgentID. Returns the verifying entry.
pub fn verify_card(card: &Value, expected: &AgentId) -> Result<Value, CardError> {
    let sigs = card
        .get("signatures")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or(CardError::NoSignatures)?;
    let payload_b64 = b64::encode(&signing_payload(card)?);
    let want_kid = expected.to_multibase();
    for entry in sigs {
        let (Some(protected), Some(signature)) =
            (entry.get("protected").and_then(Value::as_str), entry.get("signature").and_then(Value::as_str))
        else {
            continue;
        };
        let header_bytes = b64::decode(protected).map_err(|_| CardError::BadEncoding("protected"))?;
        let header: Value = serde_json::from_slice(&header_bytes).map_err(|_| CardError::BadEncoding("protected json"))?;
        if header.get("alg").and_then(Value::as_str) != Some("EdDSA")
            || header.get("kid").and_then(Value::as_str) != Some(want_kid.as_str())
        {
            continue;
        }
        let sig = b64::decode(signature).map_err(|_| CardError::BadEncoding("signature"))?;
        let input = format!("{protected}.{payload_b64}");
        verify(expected, input.as_bytes(), &sig).map_err(|_| CardError::BadSignature)?;
        if map_extension_agent(card) != Some(want_kid.as_str()) {
            return Err(CardError::ExtensionMismatch);
        }
        return Ok(entry.clone());
    }
    Err(CardError::NoMatchingSignature)
}

/// The MAP extension's `params`, if the card declares the extension.
pub fn map_extension_params(card: &Value) -> Option<&Map<String, Value>> {
    card.get("capabilities")?
        .get("extensions")?
        .as_array()?
        .iter()
        .find(|e| e.get("uri").and_then(Value::as_str) == Some(MAP_EXTENSION_URI))?
        .get("params")?
        .as_object()
}

fn map_extension_agent(card: &Value) -> Option<&str> {
    map_extension_params(card)?.get("agentId")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> CardParams {
        CardParams {
            name: "Test provider".into(),
            description: "A MAP-capable test agent".into(),
            version: "1.0.0".into(),
            jsonrpc_url: "http://127.0.0.1:8080/a2a".into(),
            skills: vec![("summarize".into(), "Summarize".into(), "Summarizes text".into(), vec!["text".into()])],
            supported_zones: vec![2, 3],
            checkpoint_interval_parts: 8,
            accepted_voucher_rails: vec!["pay:mpp-tempo".into()],
        }
    }

    #[test]
    fn sign_and_verify() {
        let kp = Keypair::from_seed([2; 32]);
        let card = build_card(&kp.agent_id(), &params());
        let (signed, _entry) = sign_card(&card, &kp).unwrap();
        verify_card(&signed, &kp.agent_id()).unwrap();
        // Member order and whitespace do not matter (JCS).
        let reparsed: Value = serde_json::from_str(&serde_json::to_string_pretty(&signed).unwrap()).unwrap();
        verify_card(&reparsed, &kp.agent_id()).unwrap();
    }

    #[test]
    fn tamper_wrong_key_and_extension_mismatch() {
        let kp = Keypair::from_seed([2; 32]);
        let other = Keypair::from_seed([3; 32]);
        let (mut signed, _) = sign_card(&build_card(&kp.agent_id(), &params()), &kp).unwrap();
        // Signed by kp but checked against another identity: no matching kid.
        assert_eq!(verify_card(&signed, &other.agent_id()), Err(CardError::NoMatchingSignature));
        // Any change to the card breaks the signature.
        signed["description"] = json!("changed");
        assert_eq!(verify_card(&signed, &kp.agent_id()), Err(CardError::BadSignature));
        // A card that names another AgentID in its MAP extension, even if
        // validly signed by kp, fails D-4's cross-check.
        let card = build_card(&other.agent_id(), &params());
        let (signed, _) = sign_card(&card, &kp).unwrap();
        assert_eq!(verify_card(&signed, &kp.agent_id()), Err(CardError::ExtensionMismatch));
    }

    #[test]
    fn a2a_spec_default_removal_example() {
        // A2A §8.4.1 example: empty non-required `extensions` is dropped,
        // explicitly set optional booleans stay, REQUIRED empty `skills` stays.
        let card = json!({
            "name": "Example Agent",
            "description": "",
            "capabilities": {"streaming": false, "pushNotifications": false, "extensions": []},
            "skills": []
        });
        let got = String::from_utf8(signing_payload(&card).unwrap()).unwrap();
        assert_eq!(
            got,
            r#"{"capabilities":{"pushNotifications":false,"streaming":false},"description":"","name":"Example Agent","skills":[]}"#
        );
    }

    #[test]
    fn implicit_presence_defaults_are_dropped_explicit_values_kept() {
        // `required: false` and an empty `tenant` are proto3 defaults on
        // implicit-presence fields: omitted. `required: true` is kept.
        let card = json!({
            "name": "n",
            "supportedInterfaces": [{"url": "u", "tenant": ""}],
            "capabilities": {"extensions": [
                {"uri": "a", "required": false},
                {"uri": "b", "required": true, "description": ""}
            ]}
        });
        let got = String::from_utf8(signing_payload(&card).unwrap()).unwrap();
        assert_eq!(
            got,
            r#"{"capabilities":{"extensions":[{"uri":"a"},{"required":true,"uri":"b"}]},"name":"n","supportedInterfaces":[{"url":"u"}]}"#
        );
    }

    #[test]
    fn built_cards_stay_inside_the_interop_profile() {
        // A2A-31: nothing a protobuf-based SDK would canonicalize differently.
        fn walk(v: &Value, in_params: bool) {
            match v {
                Value::String(s) => assert!(!s.is_empty() && s.is_ascii(), "empty or non-ASCII string {s:?}"),
                Value::Array(a) => {
                    assert!(!a.is_empty(), "empty array");
                    a.iter().for_each(|x| walk(x, in_params));
                }
                Value::Object(m) => {
                    assert!(!m.is_empty(), "empty object");
                    assert_ne!(m.get("required"), Some(&Value::Bool(false)));
                    m.iter().for_each(|(k, x)| walk(x, in_params || k == "params"));
                }
                Value::Number(_) => assert!(!in_params, "number inside a Struct"),
                _ => {}
            }
        }
        let kp = Keypair::from_seed([2; 32]);
        let (signed, _) = sign_card(&build_card(&kp.agent_id(), &params()), &kp).unwrap();
        walk(&signed, false);
    }
}
