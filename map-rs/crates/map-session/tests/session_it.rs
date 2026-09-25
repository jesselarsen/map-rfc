//! Integration tests for `map-session` (TASK-005 §16).
//!
//! Two styles:
//! * threaded, over the real `InMemoryDuplex` — happy-path handshake, echo,
//!   close, concurrency, property roundtrip;
//! * single-threaded, over a test-controlled `ChannelTransport` — the negative
//!   recv cases need crafted frames, so we pre-load a HELLO (making `respond()`
//!   return without blocking) and then inject arbitrary in-session frames.

use map_core::cbor::Value;
use map_core::hello::Hello;
use map_core::{msg_type, seal, Body, MsgId, PROTOCOL_VERSION_1_0};
use map_identity::{AgentId, Keypair};
use map_session::{
    CountingMsgIds, FixedClock, InMemoryDuplex, Session, SessionConfig, SessionError,
    SessionState, Transport, TransportError,
};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

const TS: u64 = 1_780_000_000_000;
const PRIVATE: u16 = 0xF000;

fn kp(seed: u8) -> Keypair {
    Keypair::from_seed([seed; 32])
}

fn hello(max: u32, versions: Vec<u16>) -> Hello {
    Hello {
        versions,
        suites: vec![1],
        extensions: vec![],
        max_message_size: max,
        max_tasks: None,
        zones: vec![],
    }
}

fn default_hello() -> Hello {
    hello(262_144, vec![PROTOCOL_VERSION_1_0])
}

// ---------------------------------------------------------------------------
// Threaded happy-path tests over InMemoryDuplex.
// ---------------------------------------------------------------------------

fn cfg(seed: u8, peer: AgentId, h: Hello) -> SessionConfig {
    SessionConfig::new(kp(seed), peer, h)
}

#[test]
fn handshake_establishes_with_identical_th_cb_negotiated() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || {
            let b = Session::respond(cfg(2, a_for_b, default_hello()), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            (*b.transcript_hash(), *b.channel_binding(), b.negotiated().version, b.negotiated().suite, b.negotiated().max_message_size, b.state())
        });
        let a = Session::initiate(cfg(1, b_id.clone(), default_hello()), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        let (bth, bcb, bver, bsuite, bmax, bstate) = hb.join().unwrap();

        assert_eq!(a.state(), SessionState::Established);
        assert_eq!(bstate, SessionState::Established);
        assert_eq!(a.transcript_hash(), &bth);
        assert_eq!(a.channel_binding(), &bcb);
        assert_eq!((a.negotiated().version, a.negotiated().suite, a.negotiated().max_message_size), (bver, bsuite, bmax));
        assert_eq!(bver, PROTOCOL_VERSION_1_0);
    });
}

#[test]
fn private_message_delivered_with_seq_zero() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || {
            let mut b = Session::respond(cfg(2, a_for_b, default_hello()), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            b.recv().unwrap()
        });
        let mut a = Session::initiate(cfg(1, b_id.clone(), default_hello()), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        a.send(PRIVATE, Value::Map(vec![(Value::Uint(1), Value::Text("hi".into()))])).unwrap();
        let inc = hb.join().unwrap();
        assert_eq!(inc.msg_type, PRIVATE);
        assert_eq!(inc.seq, 0);
        assert_eq!(inc.content, Value::Map(vec![(Value::Uint(1), Value::Text("hi".into()))]));
    });
}

#[test]
fn ping_is_auto_answered_with_pong() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    thread::scope(|s| {
        let a_for_b = a_id.clone();
        // B pumps recv: the PING is auto-ponged, then B blocks for a real msg.
        let hb = s.spawn(move || {
            let mut b = Session::respond(cfg(2, a_for_b, default_hello()), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            b.recv().unwrap() // returns the private message (after auto-PONG)
        });
        let mut a = Session::initiate(cfg(1, b_id.clone(), default_hello()), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        a.ping().unwrap();
        assert_eq!(a.pings_unanswered(), 1);
        let pong = a.recv().unwrap();
        assert_eq!(pong.msg_type, msg_type::PONG);
        assert_eq!(a.pings_unanswered(), 0);
        a.send(PRIVATE, Value::Map(vec![])).unwrap();
        let inc = hb.join().unwrap();
        assert_eq!(inc.msg_type, PRIVATE);
    });
}

#[test]
fn close_reason_round_trips() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || {
            let mut b = Session::respond(cfg(2, a_for_b, default_hello()), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            let err = b.recv().unwrap_err();
            (err, b.state())
        });
        let mut a = Session::initiate(cfg(1, b_id.clone(), default_hello()), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        a.close(1006, "shutting down").unwrap();
        assert_eq!(a.state(), SessionState::Closed);
        let (err, bstate) = hb.join().unwrap();
        assert_eq!(bstate, SessionState::Closed);
        match err {
            SessionError::PeerClosed { code, reason } => {
                assert_eq!(code, 1006);
                assert_eq!(reason, "shutting down");
            }
            other => panic!("expected PeerClosed, got {other:?}"),
        }
    });
}

#[test]
fn peer_mismatch_yields_error_2000_both_sides() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let wrong = kp(9).agent_id(); // B expects a third party, not A
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    thread::scope(|s| {
        let hb = s.spawn(move || Session::respond(cfg(2, wrong, default_hello()), tb, FixedClock { ms: TS }, CountingMsgIds::new()).map(|_| ()));
        let a_res = Session::initiate(cfg(1, b_id.clone(), default_hello()), ta, FixedClock { ms: TS }, CountingMsgIds::new());
        let b_res = hb.join().unwrap();

        assert!(matches!(b_res, Err(SessionError::PeerMismatch)));
        match a_res {
            Err(SessionError::PeerError { code, .. }) => assert_eq!(code, 2000),
            other => panic!("initiator expected PeerError 2000, got {:?}", other.map(|_| ())),
        }
    });
}

#[test]
fn no_mutual_version_yields_error_1001() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || Session::respond(cfg(2, a_for_b, hello(262_144, vec![0x0100])), tb, FixedClock { ms: TS }, CountingMsgIds::new()).map(|_| ()));
        // A offers only v2 (0x0200); B only v1 → no overlap.
        let a_res = Session::initiate(cfg(1, b_id.clone(), hello(262_144, vec![0x0200])), ta, FixedClock { ms: TS }, CountingMsgIds::new());
        let b_res = hb.join().unwrap();

        assert!(matches!(b_res, Err(SessionError::Handshake(_))));
        match a_res {
            Err(SessionError::PeerError { code, .. }) => assert_eq!(code, 1001),
            other => panic!("initiator expected PeerError 1001, got {:?}", other.map(|_| ())),
        }
    });
}

#[test]
fn concurrent_hundred_each_way_preserves_seq() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || {
            let mut b = Session::respond(cfg(2, a_for_b, default_hello()), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            for i in 0..100u64 {
                let inc = b.recv().unwrap();
                assert_eq!(inc.seq, i);
            }
            for i in 0..100u64 {
                b.send(PRIVATE, Value::Uint(i)).unwrap();
            }
        });
        let mut a = Session::initiate(cfg(1, b_id.clone(), default_hello()), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        for i in 0..100u64 {
            a.send(PRIVATE, Value::Uint(i)).unwrap();
        }
        for i in 0..100u64 {
            let inc = a.recv().unwrap();
            assert_eq!(inc.seq, i);
        }
        hb.join().unwrap();
        assert_eq!(a.msgs_sent(), 100);
        assert_eq!(a.msgs_received(), 100);
    });
}

#[test]
fn random_content_round_trips() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    fn lcg(state: &mut u64) -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *state
    }
    fn content(state: &mut u64) -> Value {
        let n = (lcg(state) % 5) as u64;
        Value::Map((0..n).map(|i| (Value::Uint(i), Value::Uint(lcg(state)))).collect())
    }

    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || {
            let mut b = Session::respond(cfg(2, a_for_b, default_hello()), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            let mut got = Vec::new();
            for _ in 0..25 {
                got.push(b.recv().unwrap().content);
            }
            got
        });
        let mut a = Session::initiate(cfg(1, b_id.clone(), default_hello()), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        let mut state = 0x1234_5678u64;
        let mut sent = Vec::new();
        for _ in 0..25 {
            let c = content(&mut state);
            sent.push(c.clone());
            a.send(PRIVATE, c).unwrap();
        }
        let got = hb.join().unwrap();
        assert_eq!(sent, got);
    });
}

// ---------------------------------------------------------------------------
// Crafted-frame tests over a test-controlled transport.
// ---------------------------------------------------------------------------

struct ChannelTransport {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
    exporter: Vec<u8>,
}
impl Transport for ChannelTransport {
    fn send(&self, frame: &[u8]) -> Result<(), TransportError> {
        self.tx.send(frame.to_vec()).map_err(|_| TransportError::Closed)
    }
    fn recv(&self) -> Result<Vec<u8>, TransportError> {
        self.rx.recv().map_err(|_| TransportError::Closed)
    }
    fn exporter(&self) -> &[u8] {
        &self.exporter
    }
}

fn exporter(a: &AgentId, b: &AgentId) -> Vec<u8> {
    let mut e = Vec::with_capacity(68);
    e.extend_from_slice(a.as_bytes());
    e.extend_from_slice(b.as_bytes());
    e
}

/// A HELLO from A (arrays, empty aad), the wire form the responder expects.
fn craft_hello(a: &Keypair, h: &Hello, session_id: [u8; 16], msg_id: [u8; 16]) -> Vec<u8> {
    let body = Body {
        version: *h.versions.iter().max().unwrap(),
        msg_type: msg_type::HELLO,
        msg_id: MsgId(msg_id),
        timestamp_ms: TS,
        recipient: None,
        ttl_ms: None,
        session_id: Some(session_id),
        seq: None,
        content: h.to_value(),
    };
    seal(a, &body, b"").unwrap()
}

/// An in-session frame from A, sealed under `cb` with a chosen seq/msg-id.
#[allow(clippy::too_many_arguments)]
fn craft_frame(a: &Keypair, peer_b: &AgentId, session_id: [u8; 16], msg_id: [u8; 16], seq: u64, mtype: u16, content: Value, cb: &[u8]) -> Vec<u8> {
    let body = Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: mtype,
        msg_id: MsgId(msg_id),
        timestamp_ms: TS,
        recipient: Some(peer_b.clone()),
        ttl_ms: None,
        session_id: Some(session_id),
        seq: Some(seq),
        content,
    };
    seal(a, &body, cb).unwrap()
}

type Responder = Session<ChannelTransport, FixedClock, CountingMsgIds>;

/// Establish a responder session by pre-loading a crafted HELLO. Returns the
/// session, the inbound injector, the outbound drain, and the session-id.
fn establish_responder(hello_a: &Hello, session_id: [u8; 16]) -> (Responder, Sender<Vec<u8>>, Receiver<Vec<u8>>, [u8; 16]) {
    let a = kp(1);
    let b_id = kp(2).agent_id();
    let (in_tx, in_rx) = channel();
    let (out_tx, out_rx) = channel();
    // Pre-load the HELLO so respond() returns without blocking.
    in_tx.send(craft_hello(&a, hello_a, session_id, [1u8; 16])).unwrap();
    let ct = ChannelTransport { tx: out_tx, rx: in_rx, exporter: exporter(&a.agent_id(), &b_id) };
    let b = Session::respond(cfg(2, a.agent_id(), default_hello()), ct, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
    let _ack = out_rx.recv().unwrap(); // drain the HELLO_ACK
    (b, in_tx, out_rx, session_id)
}

#[test]
fn oversize_frame_is_rejected_1004() {
    // Negotiate down to the 65536 floor, then deliver a >65536-byte frame.
    let (mut b, in_tx, _out, _sid) = establish_responder(&hello(65_536, vec![PROTOCOL_VERSION_1_0]), [5u8; 16]);
    in_tx.send(vec![0u8; 70_000]).unwrap();
    assert!(matches!(b.recv(), Err(SessionError::TooLarge)));
}

#[test]
fn duplicate_msg_id_is_replay_1003() {
    let (mut b, in_tx, _out, sid) = establish_responder(&default_hello(), [5u8; 16]);
    let a = kp(1);
    let cb = *b.channel_binding();
    let b_id = kp(2).agent_id();
    in_tx.send(craft_frame(&a, &b_id, sid, [7u8; 16], 0, PRIVATE, Value::Uint(1), &cb)).unwrap();
    assert!(b.recv().is_ok());
    // Same msg-id again → replay, before the seq check.
    in_tx.send(craft_frame(&a, &b_id, sid, [7u8; 16], 1, PRIVATE, Value::Uint(2), &cb)).unwrap();
    assert!(matches!(b.recv(), Err(SessionError::Replay)));
}

#[test]
fn non_monotonic_seq_is_rejected_1003() {
    let (mut b, in_tx, _out, sid) = establish_responder(&default_hello(), [5u8; 16]);
    let a = kp(1);
    let cb = *b.channel_binding();
    let b_id = kp(2).agent_id();
    in_tx.send(craft_frame(&a, &b_id, sid, [7u8; 16], 0, PRIVATE, Value::Uint(1), &cb)).unwrap();
    assert!(b.recv().is_ok());
    // New msg-id (passes replay) but seq 2 when 1 is expected → Sequence.
    in_tx.send(craft_frame(&a, &b_id, sid, [8u8; 16], 2, PRIVATE, Value::Uint(2), &cb)).unwrap();
    assert!(matches!(b.recv(), Err(SessionError::Sequence)));
}

#[test]
fn wrong_session_id_is_state_violation_1006() {
    // core §6.1 / TASK-020 FR-2: an in-session frame whose body key 6 is not
    // THIS session's SessionID is a state violation, and closes the session.
    let (mut b, in_tx, _out, sid) = establish_responder(&default_hello(), [5u8; 16]);
    let a = kp(1);
    let cb = *b.channel_binding();
    let b_id = kp(2).agent_id();
    let mut wrong = sid;
    wrong[0] ^= 0xFF;
    // Correctly signed and CB-bound, but carrying a foreign session-id.
    in_tx.send(craft_frame(&a, &b_id, wrong, [7u8; 16], 0, PRIVATE, Value::Uint(1), &cb)).unwrap();
    assert!(matches!(b.recv(), Err(SessionError::State)));
    // The session is closed; a subsequent recv is a State error too.
    assert!(matches!(b.recv(), Err(SessionError::State)));
}

#[test]
fn bad_signature_is_rejected_2000() {
    let (mut b, in_tx, _out, sid) = establish_responder(&default_hello(), [5u8; 16]);
    let a = kp(1);
    let b_id = kp(2).agent_id();
    // Sealed under the WRONG channel binding → verification fails under B's CB.
    let wrong_cb = [0u8; 32];
    in_tx.send(craft_frame(&a, &b_id, sid, [7u8; 16], 0, PRIVATE, Value::Uint(1), &wrong_cb)).unwrap();
    assert!(matches!(b.recv(), Err(SessionError::BadSignature)));
}

#[test]
fn anti_relay_frame_from_another_session_fails() {
    // Two sessions differing only by session-id → different HELLO bytes →
    // different TH → different CB. A frame valid for one fails in the other.
    let (b1, _in1, _o1, sid1) = establish_responder(&default_hello(), [5u8; 16]);
    let (mut b2, in2, _o2, _sid2) = establish_responder(&default_hello(), [6u8; 16]);
    let cb1 = *b1.channel_binding();
    let cb2 = *b2.channel_binding();
    assert_ne!(cb1, cb2, "distinct sessions must have distinct channel bindings");

    let a = kp(1);
    let b_id = kp(2).agent_id();
    let frame_for_b1 = craft_frame(&a, &b_id, sid1, [7u8; 16], 0, PRIVATE, Value::Uint(1), &cb1);
    in2.send(frame_for_b1).unwrap();
    assert!(matches!(b2.recv(), Err(SessionError::BadSignature)));
}

#[test]
fn downgrade_tamper_changes_channel_binding() {
    // Stripping a version from the HELLO changes its bytes, hence TH and CB.
    // (In a live MITM the HELLO signature catches the tamper first; this is the
    // belt-and-suspenders §6.2 property.)
    let (b_full, _i1, _o1, sid) = establish_responder(&hello(262_144, vec![0x0100, 0x0101]), [5u8; 16]);
    let (mut b_stripped, in_s, _o2, _sid) = establish_responder(&hello(262_144, vec![0x0100]), [5u8; 16]);
    assert_ne!(b_full.channel_binding(), b_stripped.channel_binding());

    let a = kp(1);
    let b_id = kp(2).agent_id();
    let frame = craft_frame(&a, &b_id, sid, [7u8; 16], 0, PRIVATE, Value::Uint(1), b_full.channel_binding());
    in_s.send(frame).unwrap();
    assert!(matches!(b_stripped.recv(), Err(SessionError::BadSignature)));
}

#[test]
fn methods_after_close_return_state() {
    let (mut b, _in, _out, _sid) = establish_responder(&default_hello(), [5u8; 16]);
    b.close(1006, "bye").unwrap();
    assert_eq!(b.state(), SessionState::Closed);
    assert!(matches!(b.send(PRIVATE, Value::Uint(1)), Err(SessionError::State)));
    assert!(matches!(b.ping(), Err(SessionError::State)));
    assert!(matches!(b.recv(), Err(SessionError::State)));
    assert!(matches!(b.close(1006, "again"), Err(SessionError::State)));
}

#[test]
fn malformed_hello_below_size_floor_is_rejected() {
    // A HELLO whose max-message-size is below the 65536 floor is malformed.
    let a = kp(1);
    let b_id = kp(2).agent_id();
    let (in_tx, in_rx) = channel();
    let (out_tx, out_rx) = channel();
    in_tx.send(craft_hello(&a, &hello(1000, vec![PROTOCOL_VERSION_1_0]), [5u8; 16], [1u8; 16])).unwrap();
    let ct = ChannelTransport { tx: out_tx, rx: in_rx, exporter: exporter(&a.agent_id(), &b_id) };
    let res = Session::respond(cfg(2, a.agent_id(), default_hello()), ct, FixedClock { ms: TS }, CountingMsgIds::new());
    assert!(matches!(res.map(|_| ()), Err(SessionError::Malformed)));
    let _ = out_rx; // no ACK expected
}

#[test]
fn negotiated_max_is_min_of_offers_and_session_id_is_echoed() {
    let a_id = kp(1).agent_id();
    let b_id = kp(2).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);
    thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hb = s.spawn(move || {
            let b = Session::respond(cfg(2, a_for_b, hello(131_072, vec![PROTOCOL_VERSION_1_0])), tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            (b.negotiated().max_message_size, *b.session_id())
        });
        let a = Session::initiate(cfg(1, b_id.clone(), hello(262_144, vec![PROTOCOL_VERSION_1_0])), ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        let (bmax, bsid) = hb.join().unwrap();
        assert_eq!(a.negotiated().max_message_size, 131_072);
        assert_eq!(bmax, 131_072);
        assert_eq!(a.session_id(), &bsid); // responder echoed the initiator's id
    });
}

#[test]
fn peer_error_frame_surfaces_as_peer_error() {
    let (mut b, in_tx, _out, sid) = establish_responder(&default_hello(), [5u8; 16]);
    let a = kp(1);
    let cb = *b.channel_binding();
    let b_id = kp(2).agent_id();
    let content = map_core::errors::ErrorMsg::new(3000, false).to_value();
    in_tx.send(craft_frame(&a, &b_id, sid, [7u8; 16], 0, msg_type::ERROR, content, &cb)).unwrap();
    match b.recv() {
        Err(SessionError::PeerError { code, .. }) => assert_eq!(code, 3000),
        other => panic!("expected PeerError, got {other:?}"),
    }
}

#[test]
fn error_wire_codes_map_to_core_9() {
    assert_eq!(SessionError::Malformed.wire_code(), Some(1000));
    assert_eq!(SessionError::Replay.wire_code(), Some(1003));
    assert_eq!(SessionError::Sequence.wire_code(), Some(1003));
    assert_eq!(SessionError::TooLarge.wire_code(), Some(1004));
    assert_eq!(SessionError::State.wire_code(), Some(1006));
    assert_eq!(SessionError::BadSignature.wire_code(), Some(2000));
    assert_eq!(SessionError::PeerMismatch.wire_code(), Some(2000));
}

#[test]
fn handshake_msg_id_cannot_be_replayed_in_session_1003() {
    // The HELLO's msg-id enters the per-session replay cache at handshake, so a
    // later in-session frame reusing it is rejected 1003 (advisor item #1 / §8).
    // `establish_responder`'s crafted HELLO uses msg-id [1;16].
    let (mut b, in_tx, _out, sid) = establish_responder(&default_hello(), [5u8; 16]);
    let a = kp(1);
    let cb = *b.channel_binding();
    let b_id = kp(2).agent_id();
    in_tx.send(craft_frame(&a, &b_id, sid, [1u8; 16], 0, PRIVATE, Value::Uint(1), &cb)).unwrap();
    assert!(matches!(b.recv(), Err(SessionError::Replay)));
}
