//! `map-cli interop` — the Rust peer for the live Rust↔Go interop matrix
//! (TASK-020). Runs ONE scenario in a `--role` of `dial` or `listen` and
//! prints exactly one final line: `OK <scenario>` or `FAIL <scenario>:
//! <detail>`, exiting 0 or 1. A listener additionally prints `LISTENING
//! <addr>` once bound so the driver can hand the port to the dialer.
//!
//! The scenario contract is shared verbatim with the Go peer
//! (`map-go/cmd/map-interop`); see `interop/MATRIX.md`.

use map_core::cbor::Value;
use map_core::envelope::{seal, Body, MsgId, PROTOCOL_VERSION_1_0};
use map_core::errors::codes;
use map_core::hello::Hello;
use map_identity::Keypair;
use map_session::{RandomMsgIds, Session, SessionConfig, SessionError, SystemClock, TcpTransport};
use std::net::{TcpListener, TcpStream};
use std::process::exit;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

const TEST_MSG_TYPE: u16 = 0xF000;
const MAX: u32 = 262_144;

struct Flags {
    role: String,
    addr: String,
    seed: [u8; 32],
    peer_seed: [u8; 32],
    scenario: String,
}

/// Entry point (never returns; prints OK/FAIL and exits).
pub fn run(args: &[String]) -> ! {
    let f = match parse(args) {
        Ok(f) => f,
        Err(e) => {
            println!("FAIL ?: {e}");
            exit(2);
        }
    };
    let result = if f.role == "listen" {
        run_listen(&f)
    } else {
        run_dial(&f)
    };
    match result {
        Ok(()) => {
            println!("OK {}", f.scenario);
            exit(0);
        }
        Err(e) => {
            println!("FAIL {}: {e}", f.scenario);
            exit(1);
        }
    }
}

fn parse(args: &[String]) -> Result<Flags, String> {
    let (mut role, mut addr, mut scenario) = (None, None, None);
    let (mut seed, mut peer_seed) = (None, None);
    let mut i = 2; // args[0]=bin, args[1]="interop"
    while i < args.len() {
        let val = args.get(i + 1).cloned();
        match args[i].as_str() {
            "--role" => role = val,
            "--addr" => addr = val,
            "--seed" => seed = val.and_then(|v| parse_seed(&v)),
            "--peer-seed" => peer_seed = val.and_then(|v| parse_seed(&v)),
            "--scenario" => scenario = val,
            // The v1 libp2p task/pay cells are retired (P2); only TCP remains.
            "--transport" if val.as_deref() == Some("tcp") => {}
            other => return Err(format!("unknown flag {other}")),
        }
        i += 2;
    }
    Ok(Flags {
        role: role.ok_or("missing --role")?,
        addr: addr.ok_or("missing --addr")?,
        seed: seed.ok_or("missing/invalid --seed")?,
        peer_seed: peer_seed.ok_or("missing/invalid --peer-seed")?,
        scenario: scenario.ok_or("missing --scenario")?,
    })
}

fn parse_seed(s: &str) -> Option<[u8; 32]> {
    let v = hex::decode(s).ok()?;
    if v.len() != 32 {
        return None;
    }
    let mut a = [0u8; 32];
    a.copy_from_slice(&v);
    Some(a)
}

fn kp(seed: &[u8; 32]) -> Keypair {
    Keypair::from_seed(*seed)
}

fn std_hello() -> Hello {
    Hello {
        versions: vec![PROTOCOL_VERSION_1_0],
        suites: vec![1],
        extensions: vec![],
        max_message_size: MAX,
        max_tasks: None,
        zones: vec![],
    }
}

fn dial_cfg(f: &Flags) -> SessionConfig {
    let mut cfg = SessionConfig::new(kp(&f.seed), kp(&f.peer_seed).agent_id(), std_hello());
    if f.scenario == "versiondisjoint" {
        cfg.hello.versions = vec![0x0200];
    }
    cfg
}

fn listen_cfg(f: &Flags) -> SessionConfig {
    SessionConfig::new(kp(&f.seed), kp(&f.peer_seed).agent_id(), std_hello())
}

fn echo_content() -> Value {
    Value::Map(vec![(Value::Uint(1), Value::Text("echo".into()))])
}

fn echo_ok(v: &Value) -> bool {
    matches!(v.map_get(1), Some(Value::Text(t)) if t == "echo")
}

// --- transport setup -------------------------------------------------------

fn dial_transport(f: &Flags) -> Result<TcpTransport, String> {
    let stream = TcpStream::connect(&f.addr).map_err(|e| format!("connect: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    // Exporter order: dialer(initiator) || listener(responder).
    Ok(TcpTransport::new(stream, &kp(&f.seed).agent_id(), &kp(&f.peer_seed).agent_id(), MAX))
}

fn accept_transport(ln: &TcpListener, f: &Flags) -> Result<TcpTransport, String> {
    let (stream, _) = ln.accept().map_err(|e| format!("accept: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    // Exporter order: dialer(initiator=peer) || listener(responder=me).
    Ok(TcpTransport::new(stream, &kp(&f.peer_seed).agent_id(), &kp(&f.seed).agent_id(), MAX))
}

/// Seal an in-session envelope with a chosen seq/session-id under the session's
/// channel binding (harness use).
fn craft_frame(s: &Session<TcpTransport, SystemClock, RandomMsgIds>, f: &Flags, seq: u64, sid: [u8; 16], content: Value) -> Vec<u8> {
    let body = Body {
        version: s.negotiated().version,
        msg_type: TEST_MSG_TYPE,
        msg_id: MsgId([0xC0 ^ seq as u8; 16]),
        timestamp_ms: now_ms(),
        recipient: Some(kp(&f.peer_seed).agent_id()),
        ttl_ms: None,
        session_id: Some(sid),
        seq: Some(seq),
        content,
    };
    seal(&kp(&f.seed), &body, s.channel_binding()).expect("craft seals")
}

// --- code helpers ----------------------------------------------------------

fn err_code(e: &SessionError) -> Option<u16> {
    Some(match e {
        SessionError::Replay | SessionError::Sequence => codes::REPLAY,
        SessionError::BadSignature | SessionError::PeerMismatch => codes::BAD_SIGNATURE,
        SessionError::State => codes::STATE_VIOLATION,
        SessionError::TooLarge => codes::TOO_LARGE,
        SessionError::Malformed => codes::MALFORMED,
        SessionError::Handshake(_) => codes::UNSUPPORTED_VERSION,
        SessionError::PeerError { code, .. } => *code,
        SessionError::PeerClosed { code, .. } => *code,
        SessionError::Transport | SessionError::Envelope(_) => return None,
    })
}

fn expect_peer_code(e: SessionError, code: u16) -> Result<(), String> {
    match e {
        SessionError::PeerError { code: c, .. } if c == code => Ok(()),
        other => Err(format!("expected peer_error {code}, got {other:?}")),
    }
}

fn expect_local_code(e: SessionError, code: u16) -> Result<(), String> {
    match err_code(&e) {
        Some(c) if c == code => Ok(()),
        _ => Err(format!("expected local error {code}, got {e:?}")),
    }
}

fn recv_err(s: &mut Session<TcpTransport, SystemClock, RandomMsgIds>) -> SessionError {
    match s.recv() {
        Ok(_) => SessionError::Transport, // stand-in: a value means "no error"
        Err(e) => e,
    }
}

// --- dialer ----------------------------------------------------------------

fn run_dial(f: &Flags) -> Result<(), String> {
    if f.scenario == "versiondisjoint" {
        let tr = dial_transport(f)?;
        return match Session::initiate(dial_cfg(f), tr, SystemClock, RandomMsgIds) {
            Ok(_) => Err("handshake unexpectedly succeeded".into()),
            Err(e) => expect_peer_code(e, codes::UNSUPPORTED_VERSION),
        };
    }
    if f.scenario == "relay" {
        return run_dial_relay(f);
    }

    let tr = dial_transport(f)?;
    let mut s = Session::initiate(dial_cfg(f), tr, SystemClock, RandomMsgIds).map_err(|e| format!("handshake: {e:?}"))?;
    match f.scenario.as_str() {
        "echo" => {
            s.send(TEST_MSG_TYPE, echo_content()).map_err(|e| format!("send: {e:?}"))?;
            let got = s.recv().map_err(|e| format!("recv echo: {e:?}"))?;
            if !echo_ok(&got.content) {
                return Err("echo mismatch".into());
            }
            s.close(0, "done").map_err(|e| format!("close: {e:?}"))
        }
        "replay" => {
            let raw = s.send(TEST_MSG_TYPE, echo_content()).map_err(|e| format!("send: {e:?}"))?;
            s.send_raw(&raw).map_err(|e| format!("resend: {e:?}"))?;
            expect_peer_code(recv_err(&mut s), codes::REPLAY)
        }
        "seqgap" => {
            let frame = craft_frame(&s, f, 1, *s.session_id(), echo_content());
            s.send_raw(&frame).map_err(|e| format!("send: {e:?}"))?;
            expect_peer_code(recv_err(&mut s), codes::REPLAY)
        }
        "tamper" => {
            let raw = s.send(TEST_MSG_TYPE, echo_content()).map_err(|e| format!("send: {e:?}"))?;
            let mut bad = raw.clone();
            let n = bad.len();
            bad[n - 1] ^= 0xFF;
            s.send_raw(&bad).map_err(|e| format!("resend: {e:?}"))?;
            expect_peer_code(recv_err(&mut s), codes::BAD_SIGNATURE)
        }
        "wrongsid" => {
            let mut wrong = *s.session_id();
            wrong[0] ^= 0xFF;
            let frame = craft_frame(&s, f, 0, wrong, echo_content());
            s.send_raw(&frame).map_err(|e| format!("send: {e:?}"))?;
            expect_peer_code(recv_err(&mut s), codes::STATE_VIOLATION)
        }
        other => Err(format!("unknown scenario {other}")),
    }
}

fn run_dial_relay(f: &Flags) -> Result<(), String> {
    let tra = dial_transport(f)?;
    let mut sa = Session::initiate(dial_cfg(f), tra, SystemClock, RandomMsgIds).map_err(|e| format!("handshake A: {e:?}"))?;
    let raw_a = sa.send(TEST_MSG_TYPE, echo_content()).map_err(|e| format!("send A: {e:?}"))?;
    let _ = sa.close(0, "done");

    let trb = dial_transport(f)?;
    let mut sb = Session::initiate(dial_cfg(f), trb, SystemClock, RandomMsgIds).map_err(|e| format!("handshake B: {e:?}"))?;
    sb.send_raw(&raw_a).map_err(|e| format!("relay send: {e:?}"))?;
    expect_peer_code(recv_err(&mut sb), codes::BAD_SIGNATURE)
}

// --- listener --------------------------------------------------------------

fn run_listen(f: &Flags) -> Result<(), String> {
    let ln = TcpListener::bind(&f.addr).map_err(|e| format!("bind: {e}"))?;
    println!("LISTENING {}", ln.local_addr().map_err(|e| format!("{e}"))?);

    if f.scenario == "relay" {
        return run_listen_relay(f, &ln);
    }

    let tr = accept_transport(&ln, f)?;
    let mut s = match Session::respond(listen_cfg(f), tr, SystemClock, RandomMsgIds) {
        Ok(s) => s,
        Err(e) => {
            // versiondisjoint: respond fails with the handshake error — expected.
            if f.scenario == "versiondisjoint" {
                return expect_local_code(e, codes::UNSUPPORTED_VERSION);
            }
            return Err(format!("respond: {e:?}"));
        }
    };
    match f.scenario.as_str() {
        "versiondisjoint" => Err("handshake unexpectedly succeeded".into()),
        "echo" => echo_loop(&mut s),
        "replay" => {
            s.recv().map_err(|e| format!("first msg must be ok: {e:?}"))?;
            expect_local_code(recv_err(&mut s), codes::REPLAY)
        }
        "seqgap" => expect_local_code(recv_err(&mut s), codes::REPLAY),
        "tamper" => {
            s.recv().map_err(|e| format!("good frame must be ok: {e:?}"))?;
            expect_local_code(recv_err(&mut s), codes::BAD_SIGNATURE)
        }
        "wrongsid" => expect_local_code(recv_err(&mut s), codes::STATE_VIOLATION),
        other => Err(format!("unknown scenario {other}")),
    }
}

fn run_listen_relay(f: &Flags, ln: &TcpListener) -> Result<(), String> {
    let tra = accept_transport(ln, f)?;
    let mut sa = Session::respond(listen_cfg(f), tra, SystemClock, RandomMsgIds).map_err(|e| format!("respond A: {e:?}"))?;
    sa.recv().map_err(|e| format!("session A first msg: {e:?}"))?;
    let _ = sa.recv(); // drain peer CLOSE

    let trb = accept_transport(ln, f)?;
    let mut sb = Session::respond(listen_cfg(f), trb, SystemClock, RandomMsgIds).map_err(|e| format!("respond B: {e:?}"))?;
    expect_local_code(recv_err(&mut sb), codes::BAD_SIGNATURE)
}

fn echo_loop(s: &mut Session<TcpTransport, SystemClock, RandomMsgIds>) -> Result<(), String> {
    loop {
        match s.recv() {
            Ok(m) => {
                s.send(TEST_MSG_TYPE, m.content).map_err(|e| format!("echo: {e:?}"))?;
            }
            Err(SessionError::PeerClosed { .. }) => return Ok(()),
            Err(e) => return Err(format!("recv: {e:?}")),
        }
    }
}
