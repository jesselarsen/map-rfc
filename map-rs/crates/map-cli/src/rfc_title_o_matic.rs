//! `rfc-title-o-matic` — the Rust standalone demo provider for the MAP interop
//! matrix (TASK-020). It listens on libp2p/QUIC, accepts MAP sessions, and
//! answers tasks for the demo capability `map:cap:demo/rfc-title` with a single
//! deterministic RFC-style title chunk. It is a real task-answerer (not a stub)
//! and is wire-compatible with the Go `map-excuse` provider over the same task
//! protocol (tasks §1–§13, cross-impl-verified transcript codec).
//!
//! Usage:
//!   rfc-title-o-matic --seed <hex> --listen "/ip4/0.0.0.0/udp/0/quic-v1" \
//!                     --cap map:cap:demo/rfc-title

use map_core::hello::Hello;
use map_discovery::{valid_cap_uri, Descriptor};
use map_identity::Keypair;
use map_mesh::publish::publish_descriptor;
use map_net::{MapNode, NetError};
use map_session::{SessionError, SystemClock, RandomMsgIds};
use map_net::Libp2pTransport;
use map_taskrun::{Executor, ProviderTask, ProviderState};
use map_tasks::ChunkKind;
use std::process::exit;

const CAP: &str = "map:cap:demo/rfc-title";
const LISTEN: &str = "/ip4/0.0.0.0/udp/0/quic-v1";
const MAX_MESSAGE_SIZE: u32 = 262_144;
const PROTOCOL_VERSION: u16 = 0x0100;

/// The RFC-title capability executor: deterministic output from the prompt.
struct TitleExecutor;

impl Executor for TitleExecutor {
    fn execute(&self, input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)> {
        let prompt = String::from_utf8_lossy(input).into_owned();
        let title = format!(
            "RFC {}-o-matic: On the urgent need for \"{}\" in autonomous agent networks",
            hash_tag(&prompt),
            prompt
        );
        vec![(ChunkKind::OutputText, title.into_bytes())]
    }
}

/// A small deterministic tag from the prompt so two runs with the same input
/// produce the same title (the transcript root is then reproducible).
fn hash_tag(s: &str) -> u16 {
    let mut h: u16 = 0;
    for b in s.as_bytes() {
        h = h.wrapping_mul(31).wrapping_add(*b as u16);
    }
    h % 9000 + 1000 // a plausible RFC number range
}

fn die(msg: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("FAIL {msg}: {e}");
    exit(1);
}

fn parse_seed(s: &str) -> [u8; 32] {
    match hex::decode(s) {
        Ok(v) if v.len() == 32 => {
            let mut a = [0u8; 32];
            a.copy_from_slice(&v);
            a
        }
        _ => die("serve", "bad --seed (need 32-byte hex)"),
    }
}

type LpSession = map_session::Session<Libp2pTransport, SystemClock, RandomMsgIds>;

fn main() {
    let mut seed_hex = String::new();
    let mut listen = LISTEN.to_string();
    let mut cap = CAP.to_string();
    let mut i = 1;
    let args: Vec<String> = std::env::args().collect();
    while i < args.len() {
        match args[i].as_str() {
            "--seed" => {
                if let Some(v) = args.get(i + 1) {
                    seed_hex = v.clone();
                }
            }
            "--listen" => {
                if let Some(v) = args.get(i + 1) {
                    listen = v.clone();
                }
            }
            "--cap" => {
                if let Some(v) = args.get(i + 1) {
                    cap = v.clone();
                }
            }
            other => die("serve", format!("unknown flag {other}")),
        }
        i += 2;
    }

    if seed_hex.is_empty() {
        die("serve", "--seed required");
    }
    if !valid_cap_uri(&cap) {
        die("serve", format!("invalid cap uri {cap}"));
    }

    let seed = parse_seed(&seed_hex);
    let hello = Hello {
        versions: vec![PROTOCOL_VERSION],
        suites: vec![1],
        extensions: vec![],
        max_message_size: MAX_MESSAGE_SIZE,
        max_tasks: None,
        zones: vec![],
    };

    let node = match MapNode::new(seed, hello) {
        Ok(n) => n,
        Err(e) => die("serve", e),
    };

    let addr: libp2p::Multiaddr = match listen.parse() {
        Ok(a) => a,
        Err(e) => die("serve", format!("addr: {e}")),
    };
    let (bound, peer) = match node.listen_with_peer(addr) {
        Ok(v) => v,
        Err(e) => die("serve", e),
    };
    let listen_addr = format!("{bound}/p2p/{peer}");
    println!("LISTENING {listen_addr}");

    // Publish a descriptor so peers can discover this provider (discovery §2/§6).
    // Best-effort: the demo works without a DHT bootstrap, so a failure here is
    // only a warning (mirrors the Go map-excuse "skipped" branch).
    let agent = node.local_agent().clone();
    let desc = Descriptor {
        agent: agent.clone(),
        seq: 1,
        versions: vec![PROTOCOL_VERSION],
        suites: vec![1],
        addrs: vec![listen_addr.clone()],
        capabilities: vec![cap.clone()],
        zones: vec![2],
    };
    match publish_descriptor(
        &Keypair::from_seed(seed),
        &desc,
        [0u8; 16],
        now_ms(),
        6 * 3600 * 1000,
    ) {
        Ok(env) => match node.publish_descriptor(&desc, &env) {
            Ok(()) => println!("PUBLISHED descriptor for {cap}"),
            Err(e) => eprintln!("descriptor publish skipped: {e}"),
        },
        Err(e) => eprintln!("descriptor envelope skipped: {e}"),
    }

    // Accept loop: each inbound session runs one provider task.
    loop {
        let sess = match node.accept() {
            Ok(s) => s,
            Err(NetError::Closed) => return,
            Err(e) => {
                eprintln!("accept error: {e}");
                return;
            }
        };
        std::thread::spawn(move || run_one(sess));
    }
}

fn run_one(mut s: LpSession) {
    let mut provider = ProviderTask::new(TitleExecutor);
    while !matches!(
        provider.state(),
        ProviderState::Completed | ProviderState::Cancelled | ProviderState::Failed
    ) {
        let inc = match s.recv() {
            Ok(m) => m,
            Err(e) => {
                // A peer close before completion just ends this session.
                eprintln!("task recv error: {e:?}");
                return;
            }
        };
        for (mt, val) in provider.on_incoming(&inc, now_ms()) {
            if let Err(e) = s.send(mt, val) {
                eprintln!("task send error: {e:?}");
                return;
            }
        }
    }
    // Await the requester's CLOSE before tearing down so COMPLETE is flushed
    // and the requester has verified (avoids a shutdown race).
    loop {
        match s.recv() {
            Ok(_) => continue,
            Err(SessionError::PeerClosed { .. }) => break,
            Err(e) => {
                eprintln!("await close error: {e:?}");
                break;
            }
        }
    }
    let _ = s.close(0, "done");
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
