//! Two-node loopback integration tests (TASK-012 FR-6). Gated behind the
//! `net-tests` feature AND `#[ignore]` so a network-less CI stays green:
//!
//!   cargo +stable-x86_64-pc-windows-gnu test -p map-net --features net-tests -- --ignored
//!
//! (On this windows-gnu box, put `C:\msys64\mingw64\bin` on PATH first.)
#![cfg(feature = "net-tests")]

use map_core::cbor::Value;
use map_core::hello::Hello;
use map_core::PROTOCOL_VERSION_1_0;
use map_discovery::{multihash_sha256, shard, Descriptor};
use map_identity::Keypair;
use map_mesh::{ingest_descriptor_envelope, publish_descriptor};
use map_net::MapNode;
use map_session::SessionState;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CAP: &str = "map:cap:inference/text-generation";
const PRIVATE: u16 = 0xF000;
const TTL: u64 = 6 * 3600 * 1000;

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

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn descriptor(agent: map_identity::AgentId) -> Descriptor {
    Descriptor {
        agent,
        seq: 1,
        versions: vec![0x0100],
        suites: vec![1],
        addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        capabilities: vec![CAP.to_string()],
        zones: vec![2],
    }
}

/// FR-6: two real nodes handshake over TCP, echo an in-session message, and
/// resolve a capability + descriptor through the Kademlia DHT.
#[test]
#[ignore]
fn two_node_handshake_echo_and_kad() {
    let a = MapNode::new([1u8; 32], hello()).expect("node A");
    let b = MapNode::new([2u8; 32], hello()).expect("node B");
    let b_id = b.local_agent().clone();

    let b_addr = b.listen("/ip4/127.0.0.1/tcp/0".parse().unwrap()).expect("B listens");

    // A dials B; both handshake (B's side completes inside its swarm task).
    let mut a_sess = a.dial(b_addr, b_id.clone()).expect("A dials B");
    assert_eq!(a_sess.state(), SessionState::Established);
    // No delegation → the accountable peer is B's leaf identity.
    assert_eq!(a_sess.accountable_peer(), &b_id);

    let mut b_sess = b.accept().expect("B accepts the session");

    // Echo one message A → B → A over the real transport.
    a_sess.send(PRIVATE, Value::Text("ping over libp2p".into())).unwrap();
    let got = b_sess.recv().expect("B receives ping");
    assert_eq!(got.content, Value::Text("ping over libp2p".into()));
    b_sess.send(got.msg_type, got.content).unwrap();
    let echo = a_sess.recv().expect("A receives echo");
    assert_eq!(echo.content, Value::Text("ping over libp2p".into()));

    // Kad: B publishes its signed descriptor; A resolves it (untrusted → verify).
    let desc = descriptor(b_id.clone());
    let env = publish_descriptor(&Keypair::from_seed([2u8; 32]), &desc, [9u8; 16], now_ms(), TTL)
        .expect("build descriptor");
    b.publish_descriptor(&desc, &env).expect("B publishes to kad");

    // Provider + record propagation across the 2-node net — retry briefly.
    let mut providers = Vec::new();
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(250));
        providers = a.find_providers(CAP, shard(&b_id)).unwrap();
        if providers.contains(&b_id) {
            break;
        }
    }
    assert!(providers.contains(&b_id), "A should find B as a provider of CAP");

    let cid = multihash_sha256(&env);
    let mut fetched = None;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(250));
        fetched = a.fetch_descriptor(&cid).unwrap();
        if fetched.is_some() {
            break;
        }
    }
    let fetched = fetched.expect("A fetches B's descriptor record");
    assert_eq!(fetched, env, "kad record must be the exact signed envelope");
    // Re-verify (kad is untrusted transport): the fetched bytes must open as B's.
    let mut cache = map_discovery::DescriptorCache::new();
    let (verified, _) =
        ingest_descriptor_envelope(&fetched, &mut cache, now_ms()).expect("descriptor verifies");
    assert_eq!(verified.agent, b_id);
}

/// FR-3: dialing B's address while expecting a DIFFERENT AgentID is refused —
/// libp2p's Noise handshake authenticates B ≠ the expected peer.
#[test]
#[ignore]
fn dial_wrong_peer_is_refused() {
    let a = MapNode::new([1u8; 32], hello()).unwrap();
    let b = MapNode::new([2u8; 32], hello()).unwrap();
    let wrong = Keypair::from_seed([3u8; 32]).agent_id(); // C, not B

    let b_addr = b.listen("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let res = a.dial(b_addr, wrong);
    assert!(res.is_err(), "dial expecting the wrong AgentID must fail");
}
