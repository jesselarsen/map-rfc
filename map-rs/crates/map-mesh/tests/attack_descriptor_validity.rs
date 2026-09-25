//! Regression tests for Role 7 finding A8 (descriptor validity). Layer D-5:
//! validity is checked on ingest. That includes the 24 h ceiling and a bounded
//! clock skew; otherwise a descriptor with a huge ttl or a future timestamp
//! never expires.

use map_core::{msg_type, seal, Body, MsgId, PROTOCOL_VERSION_1_0};
use map_discovery::{Descriptor, DescriptorCache};
use map_identity::Keypair;
use map_mesh::{ingest_descriptor_envelope, MAX_DESCRIPTOR_VALIDITY_MS};

const NOW: u64 = 1_790_000_000_000;
const HOUR: u64 = 3_600_000;

/// A descriptor envelope sealed directly, bypassing `publish_descriptor`'s
/// own range check, as an attacker would.
fn envelope(kp: &Keypair, timestamp_ms: u64, ttl_ms: u64) -> Vec<u8> {
    let desc = Descriptor {
        agent: kp.agent_id(),
        seq: 1,
        versions: vec![PROTOCOL_VERSION_1_0],
        suites: vec![1],
        addrs: vec!["/ip4/127.0.0.1/udp/4001/quic-v1".into()],
        capabilities: vec!["map:cap:inference/text-generation".into()],
        zones: vec![2],
    };
    let body = Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: msg_type::DESCRIPTOR,
        msg_id: MsgId([7; 16]),
        timestamp_ms,
        recipient: None,
        ttl_ms: Some(ttl_ms),
        session_id: None,
        seq: None,
        content: desc.to_value().unwrap(),
    };
    seal(kp, &body, b"").unwrap()
}

#[test]
fn fresh_descriptor_is_accepted() {
    let kp = Keypair::from_seed([1; 32]);
    let env = envelope(&kp, NOW - HOUR, 6 * HOUR);
    assert!(ingest_descriptor_envelope(&env, &mut DescriptorCache::new(), NOW).is_ok());
}

#[test]
fn ttl_beyond_24h_is_rejected() {
    let kp = Keypair::from_seed([1; 32]);
    let env = envelope(&kp, NOW, MAX_DESCRIPTOR_VALIDITY_MS + 1);
    assert!(ingest_descriptor_envelope(&env, &mut DescriptorCache::new(), NOW).is_err());
}

#[test]
fn future_dated_descriptor_is_rejected() {
    let kp = Keypair::from_seed([1; 32]);
    // Valid ttl, but timestamped a day ahead: without the skew bound it would
    // stay "fresh" for two days.
    let env = envelope(&kp, NOW + 24 * HOUR, 6 * HOUR);
    assert!(ingest_descriptor_envelope(&env, &mut DescriptorCache::new(), NOW).is_err());
}

#[test]
fn expired_descriptor_is_rejected() {
    let kp = Keypair::from_seed([1; 32]);
    let env = envelope(&kp, NOW - 7 * HOUR, 6 * HOUR);
    assert!(ingest_descriptor_envelope(&env, &mut DescriptorCache::new(), NOW).is_err());
}
