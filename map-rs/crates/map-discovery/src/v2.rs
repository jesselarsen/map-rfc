//! v2 AgentDescriptor (spec/layer.md §11.2): the stable-plane record that
//! embeds the provider's A2A Agent Card, so one claim is signed twice (COSE
//! over the whole descriptor, JWS over the card). This crate treats the card
//! as opaque bytes; the card checks (D-4: the card's JWS key is the subject
//! AgentID) live in the A2A binding (map-a2a), keeping JSON out of discovery.
//!
//! DescriptorCore keys used here: 1 AgentID, 2 seq, 3 endpoints, 4 [CapEntry],
//! 5 accepted zones, 6 card (RFC 8785 JCS bytes), 7 card signature (JCS bytes
//! of the A2A `AgentCardSignature` object), ?12 validity (ms). Keys 8–11
//! (model, hardware, pricing, security hints) and 13 (extensions) are
//! optional in the layer spec and not produced by this implementation.

use crate::DiscoveryError;
use map_core::cbor::Value;
use map_identity::AgentId;

/// A capability URI MUST start with this prefix (layer §11.3).
pub const CAP_PREFIX: &str = "map:cap:";

#[derive(Clone, Debug, PartialEq)]
pub struct DescriptorV2 {
    pub agent: AgentId,
    /// Strictly increasing per subject; the LWW conflict key.
    pub seq: u64,
    /// Service endpoints (A2A card URL(s) and/or multiaddrs).
    pub endpoints: Vec<String>,
    pub capabilities: Vec<String>,
    /// Zone labels this provider accepts tasks under (layer §10.2).
    pub zones: Vec<u8>,
    /// The A2A Agent Card as RFC 8785 canonical JSON bytes.
    pub card_jcs: Vec<u8>,
    /// The card's `AgentCardSignature` object as canonical JSON bytes.
    pub card_signature: Vec<u8>,
    pub validity_ms: Option<u64>,
}

fn bad(k: u64) -> DiscoveryError {
    DiscoveryError::BadField(k)
}

impl DescriptorV2 {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.agent.as_bytes().to_vec())),
            (Value::Uint(2), Value::Uint(self.seq)),
            (Value::Uint(3), Value::Array(self.endpoints.iter().map(|e| Value::Text(e.clone())).collect())),
            (
                Value::Uint(4),
                Value::Array(
                    self.capabilities
                        .iter()
                        .map(|c| Value::Map(vec![(Value::Uint(1), Value::Text(c.clone()))]))
                        .collect(),
                ),
            ),
            (Value::Uint(5), Value::Array(self.zones.iter().map(|z| Value::Uint(*z as u64)).collect())),
            (Value::Uint(6), Value::Bytes(self.card_jcs.clone())),
            (Value::Uint(7), Value::Bytes(self.card_signature.clone())),
        ];
        if let Some(v) = self.validity_ms {
            m.push((Value::Uint(12), Value::Uint(v)));
        }
        Value::Map(m)
    }

    /// Decode and validate shape: non-empty endpoints, capabilities (each a
    /// `map:cap:` URI) and zones (each <= 3).
    pub fn from_value(v: &Value) -> Result<DescriptorV2, DiscoveryError> {
        let get = |k: u64| v.map_get(k).ok_or(DiscoveryError::MissingField(k));
        let agent = AgentId::from_bytes(get(1)?.as_bytes().ok_or(bad(1))?).map_err(|_| bad(1))?;
        let seq = get(2)?.as_uint().ok_or(bad(2))?;
        let endpoints = get(3)?
            .as_array()
            .ok_or(bad(3))?
            .iter()
            .map(|e| e.as_text().map(str::to_string).ok_or(bad(3)))
            .collect::<Result<Vec<_>, _>>()?;
        let capabilities = get(4)?
            .as_array()
            .ok_or(bad(4))?
            .iter()
            .map(|c| {
                c.map_get(1)
                    .and_then(|x| x.as_text())
                    .filter(|s| s.starts_with(CAP_PREFIX) && s.len() > CAP_PREFIX.len())
                    .map(str::to_string)
                    .ok_or(bad(4))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let zones = get(5)?
            .as_array()
            .ok_or(bad(5))?
            .iter()
            .map(|z| z.as_uint().filter(|z| *z <= 3).map(|z| z as u8).ok_or(bad(5)))
            .collect::<Result<Vec<_>, _>>()?;
        if endpoints.is_empty() {
            return Err(bad(3));
        }
        if capabilities.is_empty() {
            return Err(bad(4));
        }
        if zones.is_empty() {
            return Err(bad(5));
        }
        let card_jcs = get(6)?.as_bytes().ok_or(bad(6))?.to_vec();
        let card_signature = get(7)?.as_bytes().ok_or(bad(7))?.to_vec();
        let validity_ms = match v.map_get(12) {
            None => None,
            Some(x) => Some(x.as_uint().ok_or(bad(12))?),
        };
        Ok(DescriptorV2 { agent, seq, endpoints, capabilities, zones, card_jcs, card_signature, validity_ms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;

    fn sample() -> DescriptorV2 {
        DescriptorV2 {
            agent: Keypair::from_seed([2; 32]).agent_id(),
            seq: 1,
            endpoints: vec!["https://provider.example.org/a2a".into()],
            capabilities: vec!["map:cap:inference/text-generation".into()],
            zones: vec![2, 3],
            card_jcs: br#"{"name":"x"}"#.to_vec(),
            card_signature: br#"{"protected":"p","signature":"s"}"#.to_vec(),
            validity_ms: Some(3_600_000),
        }
    }

    #[test]
    fn roundtrip() {
        let d = sample();
        assert_eq!(DescriptorV2::from_value(&d.to_value()).unwrap(), d);
    }

    #[test]
    fn shape_rules() {
        let mut d = sample();
        d.capabilities = vec!["inference".into()];
        assert!(DescriptorV2::from_value(&d.to_value()).is_err());
        let mut d = sample();
        d.zones = vec![4];
        assert!(DescriptorV2::from_value(&d.to_value()).is_err());
        let mut d = sample();
        d.endpoints.clear();
        assert!(DescriptorV2::from_value(&d.to_value()).is_err());
    }
}
