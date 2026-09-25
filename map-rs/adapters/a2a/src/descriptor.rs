//! v2 descriptors that embed a signed A2A Agent Card (layer §11.2, D-4/D-5).
//!
//! A descriptor is sealed as a map-core envelope (COSE_Sign1 over
//! deterministic CBOR) with msg-type DESCRIPTOR_V2 and a REQUIRED ttl. Opening
//! one checks everything a querier must check before shortlisting:
//! - the envelope signature, and that the signer is the descriptor subject;
//! - D-5 validity: ttl in (0, 24 h], timestamp not in the future (120 s skew),
//!   not expired;
//! - the embedded card is canonical JSON and its JWS verifies under the
//!   subject's own key (D-4), with the MAP extension naming the subject;
//! - the descriptor's key 7 is exactly the verifying `AgentCardSignature`.

use crate::card::{self, CardError};
use crate::jcs;
use map_core::{msg_type, open, seal, Body, MsgId, PROTOCOL_VERSION_1_0};
use map_discovery::DescriptorV2;
use map_identity::Keypair;
use serde_json::Value;

/// D-5: descriptor validity ceiling.
pub const MAX_VALIDITY_MS: u64 = 24 * 3_600_000;
/// Tolerated clock skew for a descriptor timestamp.
pub const CLOCK_SKEW_MS: u64 = 120_000;

#[derive(Debug)]
pub enum DescriptorError {
    Envelope(map_core::EnvelopeError),
    Discovery(map_discovery::DiscoveryError),
    WrongMsgType(u16),
    SignerNotSubject,
    Ttl(&'static str),
    CardNotCanonical,
    CardSignatureFieldMismatch,
    Card(CardError),
}

impl From<CardError> for DescriptorError {
    fn from(e: CardError) -> Self {
        DescriptorError::Card(e)
    }
}

/// Everything a provider publishes besides its keys.
#[derive(Clone, Debug)]
pub struct DescriptorSpec {
    pub seq: u64,
    pub endpoints: Vec<String>,
    pub capabilities: Vec<String>,
    pub zones: Vec<u8>,
}

/// Sign `unsigned_card`, embed it, and seal the descriptor envelope.
/// Returns the envelope bytes and the signed card (to serve at
/// `/.well-known/agent-card.json`).
pub fn seal_descriptor(
    kp: &Keypair,
    unsigned_card: &Value,
    spec: &DescriptorSpec,
    msg_id: [u8; 16],
    now_ms: u64,
    ttl_ms: u64,
) -> Result<(Vec<u8>, Value), DescriptorError> {
    if ttl_ms == 0 || ttl_ms > MAX_VALIDITY_MS {
        return Err(DescriptorError::Ttl("ttl out of range (0, 24h]"));
    }
    let (signed, entry) = card::sign_card(unsigned_card, kp)?;
    let d = DescriptorV2 {
        agent: kp.agent_id(),
        seq: spec.seq,
        endpoints: spec.endpoints.clone(),
        capabilities: spec.capabilities.clone(),
        zones: spec.zones.clone(),
        card_jcs: jcs::canonicalize(&signed).map_err(CardError::from)?,
        card_signature: jcs::canonicalize(&entry).map_err(CardError::from)?,
        validity_ms: Some(ttl_ms),
    };
    let body = Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: msg_type::DESCRIPTOR_V2,
        msg_id: MsgId(msg_id),
        timestamp_ms: now_ms,
        recipient: None,
        ttl_ms: Some(ttl_ms),
        session_id: None,
        seq: None,
        content: d.to_value(),
    };
    let env = seal(kp, &body, b"").map_err(DescriptorError::Envelope)?;
    Ok((env, signed))
}

/// Open and fully validate a v2 descriptor envelope (see module docs).
pub fn open_descriptor(env: &[u8], now_ms: u64) -> Result<(DescriptorV2, Value), DescriptorError> {
    let opened = open(env, b"").map_err(DescriptorError::Envelope)?;
    if opened.body.msg_type != msg_type::DESCRIPTOR_V2 {
        return Err(DescriptorError::WrongMsgType(opened.body.msg_type));
    }
    let d = DescriptorV2::from_value(&opened.body.content).map_err(DescriptorError::Discovery)?;
    if d.agent != opened.sender {
        return Err(DescriptorError::SignerNotSubject);
    }
    let ttl = opened.body.ttl_ms.ok_or(DescriptorError::Ttl("missing ttl"))?;
    if ttl == 0 || ttl > MAX_VALIDITY_MS {
        return Err(DescriptorError::Ttl("ttl out of range (0, 24h]"));
    }
    if opened.body.timestamp_ms > now_ms.saturating_add(CLOCK_SKEW_MS) {
        return Err(DescriptorError::Ttl("timestamp in the future"));
    }
    if now_ms > opened.body.timestamp_ms.saturating_add(ttl) {
        return Err(DescriptorError::Ttl("expired"));
    }
    let card: Value = serde_json::from_slice(&d.card_jcs).map_err(|_| DescriptorError::CardNotCanonical)?;
    if jcs::canonicalize(&card).map_err(CardError::from)? != d.card_jcs {
        return Err(DescriptorError::CardNotCanonical);
    }
    let entry = card::verify_card(&card, &d.agent)?;
    if jcs::canonicalize(&entry).map_err(CardError::from)? != d.card_signature {
        return Err(DescriptorError::CardSignatureFieldMismatch);
    }
    Ok((d, card))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{build_card, CardParams};

    const NOW: u64 = 1_790_000_000_000;

    fn params() -> CardParams {
        CardParams {
            name: "Provider B".into(),
            description: "MAP test provider".into(),
            version: "1.0.0".into(),
            jsonrpc_url: "http://127.0.0.1:8080/a2a".into(),
            skills: vec![("gen".into(), "Generate".into(), "Text generation".into(), vec!["text".into()])],
            supported_zones: vec![2],
            checkpoint_interval_parts: 8,
            accepted_voucher_rails: vec![],
        }
    }

    fn spec() -> DescriptorSpec {
        DescriptorSpec {
            seq: 1,
            endpoints: vec!["http://127.0.0.1:8080/a2a".into()],
            capabilities: vec!["map:cap:inference/text-generation".into()],
            zones: vec![2],
        }
    }

    #[test]
    fn seal_then_open() {
        let kp = Keypair::from_seed([2; 32]);
        let card = build_card(&kp.agent_id(), &params());
        let (env, signed) = seal_descriptor(&kp, &card, &spec(), [1; 16], NOW, 3_600_000).unwrap();
        let (d, got_card) = open_descriptor(&env, NOW + 1_000).unwrap();
        assert_eq!(d.agent, kp.agent_id());
        assert_eq!(got_card, signed);
    }

    #[test]
    fn card_signed_by_someone_else_is_rejected() {
        // Attacker B' embeds a card signed by a different key: D-4 fails.
        let kp = Keypair::from_seed([2; 32]);
        let other = Keypair::from_seed([9; 32]);
        let (foreign_card, foreign_entry) = card::sign_card(&build_card(&kp.agent_id(), &params()), &other).unwrap();
        let d = DescriptorV2 {
            agent: kp.agent_id(),
            seq: 1,
            endpoints: spec().endpoints,
            capabilities: spec().capabilities,
            zones: vec![2],
            card_jcs: jcs::canonicalize(&foreign_card).unwrap(),
            card_signature: jcs::canonicalize(&foreign_entry).unwrap(),
            validity_ms: Some(3_600_000),
        };
        let body = Body {
            version: PROTOCOL_VERSION_1_0,
            msg_type: msg_type::DESCRIPTOR_V2,
            msg_id: MsgId([1; 16]),
            timestamp_ms: NOW,
            recipient: None,
            ttl_ms: Some(3_600_000),
            session_id: None,
            seq: None,
            content: d.to_value(),
        };
        let env = seal(&kp, &body, b"").unwrap();
        let got = open_descriptor(&env, NOW);
        assert!(
            matches!(got, Err(DescriptorError::Card(CardError::NoMatchingSignature))),
            "got {got:?}"
        );
    }

    #[test]
    fn validity_rules() {
        let kp = Keypair::from_seed([2; 32]);
        let card = build_card(&kp.agent_id(), &params());
        assert!(seal_descriptor(&kp, &card, &spec(), [1; 16], NOW, MAX_VALIDITY_MS + 1).is_err());
        let (env, _) = seal_descriptor(&kp, &card, &spec(), [1; 16], NOW, 3_600_000).unwrap();
        assert!(matches!(open_descriptor(&env, NOW + 3_600_001), Err(DescriptorError::Ttl("expired"))));
        assert!(matches!(open_descriptor(&env, NOW - 200_000), Err(DescriptorError::Ttl(_))));
    }
}
