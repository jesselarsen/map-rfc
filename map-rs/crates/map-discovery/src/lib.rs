//! MAP discovery bounded context (draft-map-discovery-01).
//!
//! Data semantics (discovery §5, ops §2): the agent is the sole source of
//! truth for its descriptor; everything here — provider-record keys, caches
//! — is derived and rebuildable. Conflict resolution is `seq` LWW with the
//! subject as the single writer. Volatile facts (load, TTFT) and trust
//! fields never appear in these structures (rules R1/R4).

use map_core::cbor::Value;
use map_identity::AgentId;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;

pub mod v2;
pub use v2::DescriptorV2;

#[derive(Debug, PartialEq)]
pub enum DiscoveryError {
    BadCapUri(String),
    MissingField(u64),
    BadField(u64),
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscoveryError::BadCapUri(s) => write!(f, "invalid capability URI: {s}"),
            DiscoveryError::MissingField(k) => write!(f, "descriptor missing field {k}"),
            DiscoveryError::BadField(k) => write!(f, "descriptor field {k} invalid"),
        }
    }
}
impl std::error::Error for DiscoveryError {}

/// `map:cap:[a-z0-9][a-z0-9/._-]{0,126}` (core §3). Hand-rolled to keep the
/// scaffold dependency-free of regex engines.
pub fn valid_cap_uri(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("map:cap:") else {
        return false;
    };
    let b = rest.as_bytes();
    if b.is_empty() || b.len() > 127 {
        return false;
    }
    b.iter().enumerate().all(|(i, c)| {
        c.is_ascii_lowercase()
            || c.is_ascii_digit()
            || (i > 0 && matches!(c, b'/' | b'.' | b'_' | b'-'))
    })
}

/// Prefix-segment capability matching (discovery §4): a provider of
/// `.../code` matches a query for the parent.
pub fn cap_matches(provider_cap: &str, query_cap: &str) -> bool {
    provider_cap == query_cap
        || provider_cap
            .strip_prefix(query_cap)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// multihash for sha2-256: 0x12 0x20 || digest.
pub fn multihash_sha256(data: &[u8]) -> Vec<u8> {
    let d = Sha256::digest(data);
    let mut out = Vec::with_capacity(34);
    out.extend_from_slice(&[0x12, 0x20]);
    out.extend_from_slice(&d);
    out
}

pub const NUM_SHARDS: u16 = 256;

/// discovery §6 (as corrected): the shard is the first byte of the *digest*
/// — the first byte of the multihash is the constant codec prefix 0x12 and
/// would collapse all agents into one shard.
pub fn shard(agent: &AgentId) -> u8 {
    Sha256::digest(agent.as_bytes())[0]
}

/// Layer D-8 (experimental, finding A4): an epoch-salted shard. Rotating
/// `epoch_salt` reassigns every agent's shard at once, so an identity ground
/// for a favourable shard under one salt is worthless under the next. Not
/// wired into publication or lookup; evaluation E3 measures it.
pub fn shard_epoch(agent: &AgentId, epoch_salt: &[u8]) -> u8 {
    let mut h = Sha256::new();
    h.update(epoch_salt);
    h.update(agent.as_bytes());
    h.finalize()[0]
}

/// K(cap) — the unsharded capability index key (discovery §5).
pub fn capability_key(cap: &str) -> Vec<u8> {
    multihash_sha256(format!("map:idx:1:{cap}").as_bytes())
}

/// K'(cap, s) — the shard an agent publishes to / a querier samples.
pub fn sharded_capability_key(cap: &str, shard: u8) -> Vec<u8> {
    multihash_sha256(format!("map:idx:1:{cap}:{shard}").as_bytes())
}

/// Minimal AgentDescriptor (discovery §2): required fields 1–6 plus zones.
/// Hardware buckets, models, pricing hints, security profile, and memory
/// topics are TODO(scaffold) — they extend the same map, keys 7–14.
#[derive(Clone, Debug, PartialEq)]
pub struct Descriptor {
    pub agent: AgentId,
    /// Strictly increasing per subject; the LWW conflict key.
    pub seq: u64,
    pub versions: Vec<u16>,
    pub suites: Vec<u16>,
    pub addrs: Vec<String>,
    pub capabilities: Vec<String>,
    /// Zones served (default `[2]` per spec when absent).
    pub zones: Vec<u8>,
}

impl Descriptor {
    pub fn to_value(&self) -> Result<Value, DiscoveryError> {
        for c in &self.capabilities {
            if !valid_cap_uri(c) {
                return Err(DiscoveryError::BadCapUri(c.clone()));
            }
        }
        let mut m: Vec<(Value, Value)> = vec![
            (Value::Uint(1), Value::Bytes(self.agent.as_bytes().to_vec())),
            (Value::Uint(2), Value::Uint(self.seq)),
            (
                Value::Uint(3),
                Value::Array(self.versions.iter().map(|v| Value::Uint(*v as u64)).collect()),
            ),
            (
                Value::Uint(4),
                Value::Array(self.suites.iter().map(|v| Value::Uint(*v as u64)).collect()),
            ),
            (
                Value::Uint(5),
                Value::Array(self.addrs.iter().map(|a| Value::Text(a.clone())).collect()),
            ),
            (
                Value::Uint(6),
                Value::Array(
                    self.capabilities
                        .iter()
                        .map(|c| Value::Map(vec![(Value::Uint(1), Value::Text(c.clone()))]))
                        .collect(),
                ),
            ),
        ];
        if !self.zones.is_empty() {
            m.push((
                Value::Uint(12),
                Value::Array(self.zones.iter().map(|z| Value::Uint(*z as u64)).collect()),
            ));
        }
        Ok(Value::Map(m))
    }

    pub fn from_value(v: &Value) -> Result<Descriptor, DiscoveryError> {
        let agent = AgentId::from_bytes(
            v.map_get(1)
                .ok_or(DiscoveryError::MissingField(1))?
                .as_bytes()
                .ok_or(DiscoveryError::BadField(1))?,
        )
        .map_err(|_| DiscoveryError::BadField(1))?;
        let seq = v
            .map_get(2)
            .ok_or(DiscoveryError::MissingField(2))?
            .as_uint()
            .ok_or(DiscoveryError::BadField(2))?;
        let u16s = |k: u64| -> Result<Vec<u16>, DiscoveryError> {
            v.map_get(k)
                .ok_or(DiscoveryError::MissingField(k))?
                .as_array()
                .ok_or(DiscoveryError::BadField(k))?
                .iter()
                .map(|x| {
                    x.as_uint()
                        .and_then(|n| u16::try_from(n).ok())
                        .ok_or(DiscoveryError::BadField(k))
                })
                .collect()
        };
        let versions = u16s(3)?;
        let suites = u16s(4)?;
        let addrs = v
            .map_get(5)
            .ok_or(DiscoveryError::MissingField(5))?
            .as_array()
            .ok_or(DiscoveryError::BadField(5))?
            .iter()
            .map(|x| {
                x.as_text()
                    .map(|s| s.to_string())
                    .ok_or(DiscoveryError::BadField(5))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let capabilities = v
            .map_get(6)
            .ok_or(DiscoveryError::MissingField(6))?
            .as_array()
            .ok_or(DiscoveryError::BadField(6))?
            .iter()
            .map(|entry| {
                let cap = entry
                    .map_get(1)
                    .and_then(|x| x.as_text())
                    .ok_or(DiscoveryError::BadField(6))?
                    .to_string();
                if !valid_cap_uri(&cap) {
                    return Err(DiscoveryError::BadCapUri(cap));
                }
                Ok(cap)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let zones = match v.map_get(12) {
            None => vec![2],
            Some(x) => x
                .as_array()
                .ok_or(DiscoveryError::BadField(12))?
                .iter()
                .map(|z| {
                    z.as_uint()
                        .and_then(|n| u8::try_from(n).ok())
                        .filter(|n| *n <= 3)
                        .ok_or(DiscoveryError::BadField(12))
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(Descriptor {
            agent,
            seq,
            versions,
            suites,
            addrs,
            capabilities,
            zones,
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum Ingest {
    Accepted,
    /// `seq` did not exceed the cached value — keep the newer record.
    Stale,
}

/// Derived cache of descriptor pointers keyed (subject, seq) — single
/// writer per key, LWW by seq, rebuildable by re-fetch (discovery §5).
#[derive(Default)]
pub struct DescriptorCache {
    latest: HashMap<AgentId, (u64, Vec<u8>)>,
}

impl DescriptorCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest(&mut self, agent: &AgentId, seq: u64, descriptor_cid: Vec<u8>) -> Ingest {
        match self.latest.get(agent) {
            Some((cached, _)) if *cached >= seq => Ingest::Stale,
            _ => {
                self.latest.insert(agent.clone(), (seq, descriptor_cid));
                Ingest::Accepted
            }
        }
    }

    pub fn get(&self, agent: &AgentId) -> Option<&(u64, Vec<u8>)> {
        self.latest.get(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;

    #[test]
    fn cap_uri_and_matching() {
        assert!(valid_cap_uri("map:cap:inference/text-generation"));
        assert!(valid_cap_uri("map:cap:tool/compiler/rust"));
        assert!(!valid_cap_uri("map:cap:/leading-slash"));
        assert!(!valid_cap_uri("map:cap:UPPER"));
        assert!(!valid_cap_uri("cap:inference"));

        assert!(cap_matches(
            "map:cap:inference/text-generation/code",
            "map:cap:inference/text-generation"
        ));
        // No partial-segment matches.
        assert!(!cap_matches(
            "map:cap:inference/text-generational",
            "map:cap:inference/text-generation"
        ));
    }

    #[test]
    fn shard_uses_digest_not_multihash_prefix() {
        // The bug this test guards against: shards taken from the multihash
        // prefix are the constant 0x12. Distinct agents must spread.
        let shards: std::collections::HashSet<u8> = (0u8..20)
            .map(|i| shard(&Keypair::from_seed([i; 32]).agent_id()))
            .collect();
        assert!(shards.len() > 1, "all agents collapsed into one shard");
        // And sharded keys for different shards differ.
        assert_ne!(
            sharded_capability_key("map:cap:relay", 0),
            sharded_capability_key("map:cap:relay", 1)
        );
    }

    #[test]
    fn descriptor_roundtrip_and_lww_cache() {
        let agent = Keypair::from_seed([9u8; 32]).agent_id();
        let d = Descriptor {
            agent: agent.clone(),
            seq: 7,
            versions: vec![0x0100],
            suites: vec![1],
            addrs: vec!["/ip4/192.0.2.1/udp/4433/quic-v1".into()],
            capabilities: vec!["map:cap:inference/text-generation".into()],
            zones: vec![1, 2],
        };
        let v = d.to_value().unwrap();
        assert_eq!(Descriptor::from_value(&v).unwrap(), d);

        let mut cache = DescriptorCache::new();
        assert_eq!(cache.ingest(&agent, 7, vec![1]), Ingest::Accepted);
        assert_eq!(cache.ingest(&agent, 7, vec![2]), Ingest::Stale);
        assert_eq!(cache.ingest(&agent, 6, vec![3]), Ingest::Stale);
        assert_eq!(cache.ingest(&agent, 8, vec![4]), Ingest::Accepted);
        assert_eq!(cache.get(&agent).unwrap().0, 8);
    }
}
