//! `map-cli interop` — the Rust peer for the live Rust↔Go interop matrix
//! (TASK-020). Runs ONE scenario in a `--role` of `dial` or `listen` and
//! prints exactly one final line: `OK <scenario>` or `FAIL <scenario>:
//! <detail>`, exiting 0 or 1. A listener additionally prints `LISTENING
//! <addr>` once bound so the driver can hand the port to the dialer.
//!
//! The scenario contract is shared verbatim with the Go peer
//! (`map-go/cmd/map-interop`); see `interop/MATRIX.md`.

use map_core::cbor::Value;
use map_core::delegation::{issue, DelegateBody};
use map_core::envelope::{seal, Body, MsgId, PROTOCOL_VERSION_1_0};
use map_core::errors::codes;
use map_core::hello::Hello;
use map_identity::Keypair;
#[cfg(feature = "libp2p-interop")]
use map_net::{MapNode, NetError};
use map_session::{RandomMsgIds, Session, SessionConfig, SessionError, SystemClock, TcpTransport};
#[cfg(feature = "libp2p-interop")]
use map_taskrun::{Executor, ProviderTask, RequesterTask};
#[cfg(feature = "libp2p-interop")]
use map_tasks::{ChunkKind, TaskId};
#[cfg(feature = "libp2p-interop")]
use sha2::{Digest, Sha256};
use std::net::{TcpListener, TcpStream};
use std::process::exit;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

const TEST_MSG_TYPE: u16 = 0xF000;
const MAX: u32 = 262_144;
const CAP: &str = "map:cap:inference/text-generation";

struct Flags {
    role: String,
    addr: String,
    seed: [u8; 32],
    peer_seed: [u8; 32],
    issuer_seed: Option<[u8; 32]>,
    // Read only by the gated libp2p pay scenarios; dead in a minimal build.
    #[cfg_attr(not(feature = "libp2p-interop"), allow(dead_code))]
    operator_seed: Option<[u8; 32]>,
    scenario: String,
    transport: String,
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
    let result = if f.transport == "libp2p" {
        #[cfg(feature = "libp2p-interop")]
        {
            if f.role == "listen" {
                libp2p_task::run_libp2p_listen(&f)
            } else {
                libp2p_task::run_libp2p_dial(&f)
            }
        }
        #[cfg(not(feature = "libp2p-interop"))]
        {
            Err("built without the `libp2p-interop` feature: --transport libp2p unavailable".to_string())
        }
    } else if f.role == "listen" {
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
    let (mut seed, mut peer_seed, mut issuer_seed) = (None, None, None);
    let mut operator_seed = None;
    let mut transport = "tcp".to_string();
    let mut i = 2; // args[0]=bin, args[1]="interop"
    while i < args.len() {
        let val = args.get(i + 1).cloned();
        match args[i].as_str() {
            "--role" => role = val,
            "--addr" => addr = val,
            "--seed" => seed = val.and_then(|v| parse_seed(&v)),
            "--peer-seed" => peer_seed = val.and_then(|v| parse_seed(&v)),
            "--issuer-seed" => issuer_seed = val.and_then(|v| parse_seed(&v)),
            "--operator-seed" => operator_seed = val.and_then(|v| parse_seed(&v)),
            "--scenario" => scenario = val,
            "--transport" => transport = val.unwrap_or_else(|| "tcp".to_string()),
            other => return Err(format!("unknown flag {other}")),
        }
        i += 2;
    }
    Ok(Flags {
        role: role.ok_or("missing --role")?,
        addr: addr.ok_or("missing --addr")?,
        seed: seed.ok_or("missing/invalid --seed")?,
        peer_seed: peer_seed.ok_or("missing/invalid --peer-seed")?,
        issuer_seed,
        operator_seed,
        scenario: scenario.ok_or("missing --scenario")?,
        transport,
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
    if f.scenario == "delegated" {
        if let Some(is) = &f.issuer_seed {
            cfg.delegation_chain = vec![delegation_cert(&kp(is), &kp(&f.seed))];
        }
    }
    cfg
}

fn listen_cfg(f: &Flags) -> SessionConfig {
    SessionConfig::new(kp(&f.seed), kp(&f.peer_seed).agent_id(), std_hello())
}

fn delegation_cert(issuer: &Keypair, subject: &Keypair) -> Vec<u8> {
    let body = DelegateBody {
        issuer: issuer.agent_id(),
        subject: subject.agent_id(),
        not_before_ms: 0,
        not_after_ms: 1u64 << 62,
        caps: Some(vec![CAP.to_string()]),
        zones: None,
        max_depth: Some(1),
        caveats: vec![],
    };
    issue(issuer, &body).expect("cert issues")
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
        SessionError::BadDelegation => codes::BAD_DELEGATION,
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
        "echo" | "delegated" => {
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
        "delegated" => {
            let root = f.issuer_seed.as_ref().map(|is| kp(is).agent_id());
            if Some(s.accountable_peer().clone()) != root {
                return Err("accountable peer != issuer root".into());
            }
            echo_loop(&mut s)
        }
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

// --- libp2p task scenarios ---------------------------------------------------
//
// These run the live task protocol (tasks §1–§13) over a real libp2p/QUIC
// session (core §10 / map-net). The wire format is identical to the Go peer
// (`map-go/cmd/map-interop --transport libp2p`) — the transcript codec is
// cross-impl-verified in `map-go/tasks/transcript_test.go` (TV-008) — so the
// same three scenarios run Rust↔Go in both orientations. The executor output
// is deterministic and matches the Go `scenarioExecutor` so rust↔rust and
// mixed orientations verify the same roots.
//
// Gated behind `libp2p-interop` (default-ON) so a minimal `map-cli` build
// (`--no-default-features`) drops the entire libp2p transport stack.
#[cfg(feature = "libp2p-interop")]
mod libp2p_task {
    use super::*;

const LIBP2P_CAP: &str = "map:cap:demo/excuse";
const LIBP2P_LISTEN: &str = "/ip4/127.0.0.1/udp/0/quic-v1";

/// Deterministic executor for the task-* libp2p scenarios.
struct Libp2pExecutor {
    scenario: String,
}

impl Executor for Libp2pExecutor {
    fn execute(&self, input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)> {
        let prompt = String::from_utf8_lossy(input).into_owned();
        match self.scenario.as_str() {
            "task-stream" => vec![(
                ChunkKind::OutputText,
                format!("Excuse: {prompt}").into_bytes(),
            )],
            "task-multichunk" => (0..20)
                .map(|i| {
                    (
                        ChunkKind::OutputText,
                        format!("chunk-{i}: {prompt}").into_bytes(),
                    )
                })
                .collect(),
            "task-transcript" => vec![
                (ChunkKind::OutputText, format!("alpha {prompt}").into_bytes()),
                (ChunkKind::OutputBytes, vec![0x01, 0x02, 0x03]),
                (ChunkKind::OutputText, format!("omega {prompt}").into_bytes()),
            ],
            _ => vec![(ChunkKind::OutputText, prompt.into_bytes())],
        }
    }
}

/// Derive a deterministic 32-byte TaskId from the scenario name.
fn task_id(scenario: &str) -> TaskId {
    let h = Sha256::digest(scenario.as_bytes());
    let mut id = [0u8; 32];
    id.copy_from_slice(&h);
    TaskId(id)
}

type LpSession = Session<map_net::Libp2pTransport, SystemClock, RandomMsgIds>;

    pub fn run_libp2p_listen(f: &Flags) -> Result<(), String> {
    if !is_libp2p_scenario(&f.scenario) {
        return Err(format!(
            "libp2p transport only supports task-*/pay-* scenarios, got {}",
            f.scenario
        ));
    }
    let node = MapNode::new(f.seed, std_hello()).map_err(|e| format!("node: {e}"))?;
    let (bound, peer) = node
        .listen_with_peer(LIBP2P_LISTEN.parse().map_err(|e| format!("addr: {e}"))?)
        .map_err(|e| format!("listen: {e}"))?;
    println!("LISTENING {bound}/p2p/{peer}");

    let mut s: LpSession = node.accept().map_err(|e| format!("accept: {e}"))?;
    let mut provider = ProviderTask::new(Libp2pExecutor {
        scenario: f.scenario.clone(),
    });
    // Pump the provider FSM to terminal (COMPLETE sent).
    while !matches!(
        provider.state(),
        map_taskrun::ProviderState::Completed
            | map_taskrun::ProviderState::Cancelled
            | map_taskrun::ProviderState::Failed
    ) {
        let inc = s.recv().map_err(|e| format!("recv: {e:?}"))?;
        for (mt, val) in provider.on_incoming(&inc, now_ms()) {
            s.send(mt, val).map_err(|e| format!("send: {e:?}"))?;
        }
    }
    // pay-* scenarios: run the payee side over the same stream, after the task.
    if is_pay_scenario(&f.scenario) {
        provider_pay_exchange(&mut s, f)?;
    }
    // Await the requester's CLOSE before tearing down, so the last frame is flushed
    // and the requester has verified (avoids a shutdown race).
    loop {
        match s.recv() {
            Ok(_) => continue,
            Err(SessionError::PeerClosed { .. }) => break,
            Err(e) => return Err(format!("await close: {e:?}")),
        }
    }
    // The peer already closed; an explicit close is best-effort and may report
    // State if the session is already torn down — ignore it.
    let _ = s.close(0, "done");
    Ok(())
}

    pub fn run_libp2p_dial(f: &Flags) -> Result<(), String> {
    if !is_libp2p_scenario(&f.scenario) {
        return Err(format!(
            "libp2p transport only supports task-*/pay-* scenarios, got {}",
            f.scenario
        ));
    }
    let expected = kp(&f.peer_seed).agent_id();
    let node = MapNode::new(f.seed, std_hello()).map_err(|e| format!("node: {e}"))?;
    let mut s: LpSession = node
        .dial(f.addr.parse().map_err(|e| format!("addr: {e}"))?, expected)
        .map_err(|e| match e {
            NetError::Session(se) => format!("handshake: {se:?}"),
            other => format!("dial: {other}"),
        })?;

    let mut r = RequesterTask::new(task_id(&f.scenario), LIBP2P_CAP, Default::default(), b"traffic".to_vec());
    for (mt, val) in r.start_offer(now_ms()) {
        s.send(mt, val).map_err(|e| format!("send: {e:?}"))?;
    }
    while !(r.state().is_terminal() || r.verified().is_some()) {
        let msgs = r.poll_deadline(now_ms());
        if !msgs.is_empty() {
            for (mt, val) in msgs {
                s.send(mt, val).map_err(|e| format!("send: {e:?}"))?;
            }
        }
        let inc = s.recv().map_err(|e| format!("recv: {e:?}"))?;
        for (mt, val) in r.on_incoming(&inc, now_ms()) {
            s.send(mt, val).map_err(|e| format!("send: {e:?}"))?;
        }
    }
    let root = match r.verified() {
        Some((_, root)) => *root,
        None => return Err("task did not verify".into()),
    };
    // pay-* scenarios: run the payer side (hosting the ledger operator) after
    // the transcript verifies, over the same stream.
    if is_pay_scenario(&f.scenario) {
        requester_pay_exchange(&mut s, f)?;
    }
    s.close(0, "done").map_err(|e| format!("close: {e:?}"))?;
    println!("OK transcript-root: {}", hex::encode(root));
    Ok(())
}

// --- pay scenarios (over libp2p, after the task transcript verifies) --------

use map_core::msg_type;
use map_pay::codec::{AssetAmount, MethodOffer, PayQuote, Usage};
use map_pay::credit_ledger::LedgerOperator;
use map_pay::{Amount, Asset};
use map_taskrun::{
    MeteredProviderPayment, MeteredRequesterPayment, ProviderPayState, ProviderPayment,
    RequesterPayment,
};

const PAY_EXPIRY: u64 = 1u64 << 62;
const PAY_NOW: u64 = 1;

fn is_pay_scenario(s: &str) -> bool {
    matches!(s, "pay-postpaid" | "pay-metered")
}
fn is_libp2p_scenario(s: &str) -> bool {
    matches!(
        s,
        "task-stream" | "task-multichunk" | "task-transcript" | "pay-postpaid" | "pay-metered"
    )
}

/// Extract the wire code from an ERROR outgoing for a diagnostic.
fn pay_err(where_: &str, out: &[(u16, Value)]) -> String {
    if let Some((mt, val)) = out.first() {
        if *mt == msg_type::ERROR {
            if let Some(Value::Uint(c)) = val.map_get(1) {
                return format!("pay {where_}: ERROR {c}");
            }
        }
    }
    format!("pay {where_}: unexpected output")
}

fn usd(exp: i32, mantissa: i64) -> AssetAmount {
    AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(exp, mantissa) }
}

fn metered_quote(task: [u8; 32]) -> PayQuote {
    PayQuote {
        quote_id: [0x10; 16],
        pricing_model: 1,
        rate_table: vec![(1, usd(-6, 3000)), (2, usd(-6, 5000))],
        total: Some(usd(-6, 10_000_000)),
        methods: vec![MethodOffer { uri: "pay:credit-ledger".into(), params: vec![] }],
        expiry_ms: PAY_EXPIRY,
        terms: Some(3),
        ext: None,
        task_binding: Some(task),
    }
}

fn provider_pay_exchange(s: &mut LpSession, f: &Flags) -> Result<(), String> {
    let op_seed = f.operator_seed.ok_or("pay scenario needs --operator-seed")?;
    let prov = kp(&f.seed).agent_id();
    let req = kp(&f.peer_seed).agent_id();
    let op_aid = kp(&op_seed).agent_id();
    let task = task_id(&f.scenario).0;

    if f.scenario == "pay-metered" {
        let quote = metered_quote(task);
        let mut mp = MeteredProviderPayment::new(prov, req, op_aid, task, quote.clone());
        let sealed = s.send(msg_type::PAY_QUOTE, quote.to_value()).map_err(|e| format!("send quote: {e:?}"))?;
        mp.bind_quote_envelope(&sealed);
        let inc = s.recv().map_err(|e| format!("recv agree: {e:?}"))?;
        let a = mp.on_agree(&inc);
        if !a.is_empty() {
            return Err(pay_err("metered on_agree", &a));
        }
        let usages = [
            Usage { tokens_in: 100, tokens_out: 40, ttft_ms: 0, wall_ms: 1000 },
            Usage { tokens_in: 200, tokens_out: 80, ttft_ms: 0, wall_ms: 2000 },
        ];
        let seqs = [4u64, 9];
        let ids = [[0x21u8; 16], [0x22u8; 16]];
        for i in 0..2 {
            for (mt, val) in mp.bill(usages[i], seqs[i], ids[i], PAY_NOW) {
                s.send(mt, val).map_err(|e| format!("send bill: {e:?}"))?;
            }
            let inc = s.recv().map_err(|e| format!("recv proof: {e:?}"))?;
            let rout = mp.on_proof(&inc, PAY_NOW);
            if rout.first().map(|o| o.0) != Some(msg_type::PAY_RECEIPT) {
                return Err(pay_err("metered on_proof", &rout));
            }
            s.send(rout[0].0, rout[0].1.clone()).map_err(|e| format!("send receipt: {e:?}"))?;
        }
        if mp.receipts().len() != 2 {
            return Err(format!("metered: want 2 settlements, got {}", mp.receipts().len()));
        }
        return Ok(());
    }

    // pay-postpaid.
    let mut pp = ProviderPayment::new(prov, req, op_aid);
    let out = pp.issue_quote([0x10; 16], task, usd(-2, 500), PAY_EXPIRY);
    let sealed = s.send(out[0].0, out[0].1.clone()).map_err(|e| format!("send quote: {e:?}"))?;
    pp.bind_quote_envelope(&sealed);
    let inc = s.recv().map_err(|e| format!("recv agree: {e:?}"))?;
    let a = pp.on_agree(&inc);
    if !a.is_empty() {
        return Err(pay_err("on_agree", &a));
    }
    let iout = pp.invoice([0x20; 16], PAY_NOW);
    s.send(iout[0].0, iout[0].1.clone()).map_err(|e| format!("send invoice: {e:?}"))?;
    let inc = s.recv().map_err(|e| format!("recv proof: {e:?}"))?;
    let rout = pp.on_proof(&inc, PAY_NOW);
    if rout.first().map(|o| o.0) != Some(msg_type::PAY_RECEIPT) {
        return Err(pay_err("on_proof", &rout));
    }
    s.send(rout[0].0, rout[0].1.clone()).map_err(|e| format!("send receipt: {e:?}"))?;
    if pp.state() != ProviderPayState::Settled {
        return Err("postpaid: provider not settled".into());
    }
    Ok(())
}

fn requester_pay_exchange(s: &mut LpSession, f: &Flags) -> Result<(), String> {
    let op_seed = f.operator_seed.ok_or("pay scenario needs --operator-seed")?;
    let me = kp(&f.seed).agent_id();
    let prov = kp(&f.peer_seed).agent_id();
    let task = task_id(&f.scenario).0;

    if f.scenario == "pay-metered" {
        let mut op = LedgerOperator::new(kp(&op_seed), -6);
        op.credit(&me, 2_000_000);
        let mut mr = MeteredRequesterPayment::new(task, me, prov);
        let inc = s.recv().map_err(|e| format!("recv quote: {e:?}"))?;
        let aout = mr.on_quote(&inc, PAY_NOW);
        if aout.first().map(|o| o.0) != Some(msg_type::PAY_AGREE) {
            return Err(pay_err("metered on_quote", &aout));
        }
        s.send(aout[0].0, aout[0].1.clone()).map_err(|e| format!("send agree: {e:?}"))?;
        for _ in 0..2 {
            let cinc = s.recv().map_err(|e| format!("recv chunk: {e:?}"))?;
            mr.on_chunk(&cinc);
            let iinc = s.recv().map_err(|e| format!("recv invoice: {e:?}"))?;
            let pout = mr.on_invoice(&iinc, &mut op, PAY_NOW);
            if pout.first().map(|o| o.0) != Some(msg_type::PAY_PROOF) {
                return Err(pay_err("metered on_invoice", &pout));
            }
            s.send(pout[0].0, pout[0].1.clone()).map_err(|e| format!("send proof: {e:?}"))?;
            let rinc = s.recv().map_err(|e| format!("recv receipt: {e:?}"))?;
            mr.on_receipt(&rinc);
        }
        if mr.receipts().len() != 2 {
            return Err(format!("metered: want 2 receipts, got {}", mr.receipts().len()));
        }
        return Ok(());
    }

    // pay-postpaid.
    let mut op = LedgerOperator::new(kp(&op_seed), -2);
    op.credit(&me, 1000);
    let mut rp = RequesterPayment::new(Some(task), me, prov);
    let inc = s.recv().map_err(|e| format!("recv quote: {e:?}"))?;
    let aout = rp.on_quote(&inc, PAY_NOW);
    if aout.first().map(|o| o.0) != Some(msg_type::PAY_AGREE) {
        return Err(pay_err("on_quote", &aout));
    }
    s.send(aout[0].0, aout[0].1.clone()).map_err(|e| format!("send agree: {e:?}"))?;
    let iinc = s.recv().map_err(|e| format!("recv invoice: {e:?}"))?;
    let pout = rp.on_invoice(&iinc, &mut op, PAY_NOW);
    if pout.first().map(|o| o.0) != Some(msg_type::PAY_PROOF) {
        return Err(pay_err("on_invoice", &pout));
    }
    s.send(pout[0].0, pout[0].1.clone()).map_err(|e| format!("send proof: {e:?}"))?;
    let rinc = s.recv().map_err(|e| format!("recv receipt: {e:?}"))?;
    rp.on_receipt(&rinc);
    if rp.receipt_bytes().is_none() {
        return Err("postpaid: no receipt received".into());
    }
    Ok(())
}

// close mod libp2p_task (gated)
}
