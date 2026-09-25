//! TASK-006 §16 tests. Unit checks (publish/ingest, ttl, seq-LWW, republish,
//! index, rebuild) are single-threaded; the flow checks run two sessions over
//! `InMemoryDuplex` in scoped threads with fixed clocks.

use crate::*;
use map_core::cbor::Value;
use map_core::hello::Hello;
use map_core::{seal, Body, MsgId, PROTOCOL_VERSION_1_0};
use map_discovery::{Descriptor, DescriptorCache, Ingest};
use map_identity::{AgentId, Keypair};
use map_session::{
    CountingMsgIds, FixedClock, InMemoryDuplex, Session, SessionConfig,
};
use std::thread;

const TS: u64 = 1_780_000_000_000;
const TTL: u64 = 6 * 3600 * 1000;
const CAP: &str = "map:cap:inference/text-generation";

fn kp(seed: u8) -> Keypair {
    Keypair::from_seed([seed; 32])
}

fn descriptor(agent: AgentId, seq: u64, caps: &[&str]) -> Descriptor {
    Descriptor {
        agent,
        seq,
        versions: vec![0x0100],
        suites: vec![1],
        addrs: vec!["/ip4/192.0.2.1/udp/4433/quic-v1".to_string()],
        capabilities: caps.iter().map(|c| c.to_string()).collect(),
        zones: vec![2],
    }
}

fn hello() -> Hello {
    Hello {
        versions: vec![PROTOCOL_VERSION_1_0],
        suites: vec![1],
        extensions: vec![],
        max_message_size: 262_144,
        max_tasks: None,
        zones: vec![],
    }
}

fn cfg(seed: u8, peer: AgentId) -> SessionConfig {
    SessionConfig::new(kp(seed), peer, hello())
}

// ---- unit -----------------------------------------------------------------

#[test]
fn publish_and_ingest_roundtrip() {
    let b = kp(2);
    let desc = descriptor(b.agent_id(), 1, &[CAP]);
    let env = publish_descriptor(&b, &desc, [9u8; 16], TS, TTL).unwrap();
    let mut cache = DescriptorCache::new();
    let (got, ingest) = ingest_descriptor_envelope(&env, &mut cache, TS).unwrap();
    assert_eq!(got, desc);
    assert_eq!(ingest, Ingest::Accepted);
    assert_eq!(cache.get(&b.agent_id()).unwrap().0, 1);
}

#[test]
fn publish_rejects_subject_not_signer() {
    let b = kp(2);
    let desc_for_c = descriptor(kp(3).agent_id(), 1, &[CAP]); // subject C
    assert!(matches!(
        publish_descriptor(&b, &desc_for_c, [9u8; 16], TS, TTL),
        Err(MeshError::InvalidDescriptor(_))
    ));
}

#[test]
fn ingest_rejects_kid_ne_subject() {
    // Hand-seal a descriptor for C but signed by B (kid B ≠ subject C).
    let b = kp(2);
    let desc_for_c = descriptor(kp(3).agent_id(), 1, &[CAP]);
    let body = Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: map_core::msg_type::DESCRIPTOR,
        msg_id: MsgId([9u8; 16]),
        timestamp_ms: TS,
        recipient: None,
        ttl_ms: Some(TTL),
        session_id: None,
        seq: None,
        content: desc_for_c.to_value().unwrap(),
    };
    let forged = seal(&b, &body, b"").unwrap();
    let mut cache = DescriptorCache::new();
    assert!(matches!(
        ingest_descriptor_envelope(&forged, &mut cache, TS),
        Err(MeshError::InvalidDescriptor(_))
    ));
    assert!(cache.get(&kp(3).agent_id()).is_none());
}

#[test]
fn ingest_rejects_expired_ttl() {
    let b = kp(2);
    let desc = descriptor(b.agent_id(), 1, &[CAP]);
    let env = publish_descriptor(&b, &desc, [9u8; 16], TS, TTL).unwrap();
    let mut cache = DescriptorCache::new();
    // now beyond timestamp + ttl.
    assert!(matches!(
        ingest_descriptor_envelope(&env, &mut cache, TS + TTL + 1),
        Err(MeshError::InvalidDescriptor(_))
    ));
}

#[test]
fn ingest_seq_lww_stale() {
    let b = kp(2);
    let mut cache = DescriptorCache::new();
    let env5 = publish_descriptor(&b, &descriptor(b.agent_id(), 5, &[CAP]), [5u8; 16], TS, TTL).unwrap();
    let env3 = publish_descriptor(&b, &descriptor(b.agent_id(), 3, &[CAP]), [3u8; 16], TS, TTL).unwrap();
    assert_eq!(ingest_descriptor_envelope(&env5, &mut cache, TS).unwrap().1, Ingest::Accepted);
    // Older seq after a newer one is seen → Stale, cache keeps seq 5.
    assert_eq!(ingest_descriptor_envelope(&env3, &mut cache, TS).unwrap().1, Ingest::Stale);
    assert_eq!(cache.get(&b.agent_id()).unwrap().0, 5);
}

#[test]
fn needs_republish_boundary() {
    assert!(!needs_republish(TS, TS + 5 * 3600 * 1000 + 59 * 60 * 1000)); // 5h59m
    assert!(needs_republish(TS, TS + 6 * 3600 * 1000 + 60 * 1000)); // 6h01m
}

#[test]
fn index_provide_and_query() {
    let mut idx = MeshIndex::new();
    let b = kp(2).agent_id();
    let sh = map_discovery::shard(&b);
    idx.provide(CAP, sh, b.clone());
    idx.provide(CAP, sh, b.clone()); // idempotent (set)
    assert_eq!(idx.providers(CAP, sh), vec![b.clone()]);
    assert!(idx.providers(CAP, sh.wrapping_add(1)).is_empty());
}

#[test]
fn rebuild_equals_incremental_any_order() {
    // Three agents, one two-seq republish; build incrementally in a scrambled
    // order, then rebuild sorted — the two indexes must be identical.
    let envs: Vec<(Descriptor, Vec<u8>)> = [
        (kp(2), 1u64, vec![CAP]),
        (kp(3), 1, vec!["map:cap:inference/embedding"]),
        (kp(2), 2, vec![CAP, "map:cap:tool/sql"]), // republish, higher seq
        (kp(4), 7, vec![CAP]),
    ]
    .iter()
    .map(|(k, seq, caps)| {
        let d = descriptor(k.agent_id(), *seq, caps);
        let e = publish_descriptor(k, &d, [*seq as u8; 16], TS, TTL).unwrap();
        (d, e)
    })
    .collect();

    // Incremental in a deliberately non-sorted order (seq 2 before seq 1).
    let mut incremental = MeshIndex::new();
    for i in [2, 0, 3, 1] {
        incremental.ingest(&envs[i].0, &envs[i].1);
    }
    let bytes: Vec<Vec<u8>> = envs.iter().map(|(_, e)| e.clone()).collect();
    let rebuilt = MeshIndex::rebuild(&bytes).unwrap();
    assert_eq!(incremental, rebuilt);
}

// ---- flows (threaded) -----------------------------------------------------

/// Run initiator `fa` and responder `fb` concurrently over a fresh duplex.
fn run_pair<FB, RB>(fa: impl FnOnce(&mut ClientSession), fb: FB) -> RB
where
    FB: FnOnce(&mut ServerSession) -> RB + Send,
    RB: Send,
{
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);
    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || {
            let mut b = Session::respond(cfg(2, a_for_b), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            fb(&mut b)
        });
        let mut a = Session::initiate(cfg(1, b_id.clone()), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        fa(&mut a);
        hb.join().unwrap()
    })
}

type ClientSession = Session<InMemoryDuplex, FixedClock, CountingMsgIds>;
type ServerSession = Session<InMemoryDuplex, FixedClock, CountingMsgIds>;

#[test]
fn cap_query_end_to_end() {
    let b = kp(2);
    let desc = descriptor(b.agent_id(), 1, &[CAP]);
    let env = publish_descriptor(&b, &desc, [9u8; 16], TS, TTL).unwrap();
    let mut index = MeshIndex::new();
    index.ingest(&desc, &env);

    let mut cache = DescriptorCache::new();
    run_pair(
        |a| {
            let (good, bad) = query_caps(a, CAP, None, None, &mut cache).unwrap();
            assert_eq!(good.len(), 1);
            assert_eq!(good[0].agent, kp(2).agent_id());
            assert!(bad.is_empty());
        },
        |b_sess| {
            serve_one(b_sess, &index, &StatusSnapshot::default(), &mut RateLimiter::default()).unwrap();
        },
    );
    // A's cache now holds B's verified descriptor (fetched from an untrusted index).
    assert_eq!(cache.get(&kp(2).agent_id()).unwrap().0, 1);
}

#[test]
fn cap_response_mixed_good_and_bad() {
    let b = kp(2);
    let good_env = publish_descriptor(&b, &descriptor(b.agent_id(), 1, &[CAP]), [9u8; 16], TS, TTL).unwrap();
    let mut cache = DescriptorCache::new();
    run_pair(
        |a| {
            let (good, bad) = query_caps(a, CAP, None, None, &mut cache).unwrap();
            assert_eq!(good.len(), 1); // the valid one ingested
            assert_eq!(bad.len(), 1); // the garbage one reported, not fatal
        },
        |b_sess| {
            b_sess.recv().unwrap(); // the CAP_QUERY
            let resp = Value::Map(vec![(
                Value::Uint(1),
                Value::Array(vec![Value::Bytes(good_env.clone()), Value::Bytes(vec![0xff; 12])]),
            )]);
            b_sess.send(map_core::msg_type::CAP_RESPONSE, resp).unwrap();
        },
    );
}

#[test]
fn forged_index_entry_cannot_be_cached() {
    // A CAP_RESPONSE whose "descriptor" is signed by B but claims subject C.
    let b = kp(2);
    let desc_for_c = descriptor(kp(3).agent_id(), 1, &[CAP]);
    let body = Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: map_core::msg_type::DESCRIPTOR,
        msg_id: MsgId([9u8; 16]),
        timestamp_ms: TS,
        recipient: None,
        ttl_ms: Some(TTL),
        session_id: None,
        seq: None,
        content: desc_for_c.to_value().unwrap(),
    };
    let forged = seal(&b, &body, b"").unwrap();
    let mut cache = DescriptorCache::new();
    run_pair(
        |a| {
            let (good, bad) = query_caps(a, CAP, None, None, &mut cache).unwrap();
            assert!(good.is_empty());
            assert_eq!(bad.len(), 1);
        },
        |b_sess| {
            b_sess.recv().unwrap();
            let resp = Value::Map(vec![(Value::Uint(1), Value::Array(vec![Value::Bytes(forged.clone())]))]);
            b_sess.send(map_core::msg_type::CAP_RESPONSE, resp).unwrap();
        },
    );
    assert!(cache.get(&kp(3).agent_id()).is_none(), "forged entry must not cache");
}

#[test]
fn status_query_report_echo_and_no_caching() {
    let snap = StatusSnapshot { queue_length: Some(3), est_ttft_ms: Some(420), accepting: Some(1) };
    let echo = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let cache = DescriptorCache::new(); // never mutated — STATUS has no cache path
    let report = run_pair(
        |a| {
            let r = query_status(a, &[CAP], Some(echo)).unwrap();
            assert_eq!(r.echo, Some(echo));
            assert_eq!(r.accepting, Some(1));
            assert_eq!(r.queue_length, Some(3));
            assert_eq!(r.timestamp_ms, TS);
            // Volatile status has no cache path (decision 3).
            assert!(cache.get(&kp(2).agent_id()).is_none());
        },
        |b_sess| {
            serve_one(b_sess, &MeshIndex::new(), &snap, &mut RateLimiter::default()).unwrap();
        },
    );
    let _ = report;
}
