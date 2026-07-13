//! The session state machine: HELLO handshake, channel binding, and in-session
//! send/recv with sequence, replay, and size enforcement (core §6/§7/§12).
//!
//! Boundary that IS the anti-relay property: handshake envelopes are sealed
//! with **empty** `external_aad`; every in-session envelope is sealed with the
//! **channel binding** derived from the negotiation transcript. An envelope
//! from one session therefore fails verification in any other (different CB).

use crate::clock::{Clock, MsgIdSource};
use crate::transport::{Transport, TransportError};
use map_core::cbor::Value;
use map_core::errors::{codes, ErrorMsg};
use map_core::hello::{channel_binding, negotiate, transcript_hash, Hello, HelloAck, Negotiated, NegotiateError};
use map_core::replay::{ReplayCache, ReplayCheck, SeqTracker};
use map_core::{msg_type, open, open_delegated, seal, seal_delegated, Body, EnvelopeError, MsgId};
use map_identity::{AgentId, Keypair};

/// Lifecycle per core §12. Initiator: `Idle → HelloSent → Established → Closed`;
/// responder answers HELLO directly: `Idle → Established → Closed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    HelloSent,
    Established,
    Closed,
}

/// Every rejection carries the wire error code it maps to (core §9).
#[derive(Debug)]
pub enum SessionError {
    /// Transport closed / peer gone.
    Transport,
    /// Local envelope build or a decode failure that isn't peer-attributable.
    Envelope(EnvelopeError),
    /// No mutual version/suite during the handshake (→ 1001).
    Handshake(NegotiateError),
    /// A received frame was malformed CBOR / wrong shape (→ 1000).
    Malformed,
    /// Signature did not verify — wrong signer, wrong session, or tamper (→ 2000).
    BadSignature,
    /// Sender AgentID ≠ the transport-authenticated peer (→ 2000).
    PeerMismatch,
    /// Raw frame exceeded the negotiated max-message-size (→ 1004).
    TooLarge,
    /// Replayed msg-id or timestamp outside the window (→ 1003).
    Replay,
    /// Sequence number not the expected next value (→ 1003).
    Sequence,
    /// A method was called in a state that forbids it (→ 1006).
    State,
    /// The peer's handshake delegation chain (ProtHdr key 33) failed to
    /// validate (→ 2003).
    BadDelegation,
    /// Peer sent CLOSE.
    PeerClosed { code: u16, reason: String },
    /// Peer sent ERROR.
    PeerError { code: u16, retryable: bool, text: Option<String> },
}

impl SessionError {
    /// The core §9 wire code this error corresponds to, if any.
    pub fn wire_code(&self) -> Option<u16> {
        Some(match self {
            SessionError::Malformed => codes::MALFORMED,
            SessionError::Handshake(_) => codes::UNSUPPORTED_VERSION,
            SessionError::Replay | SessionError::Sequence => codes::REPLAY,
            SessionError::TooLarge => codes::TOO_LARGE,
            SessionError::State => codes::STATE_VIOLATION,
            SessionError::BadSignature | SessionError::PeerMismatch => codes::BAD_SIGNATURE,
            SessionError::BadDelegation => codes::BAD_DELEGATION,
            SessionError::PeerClosed { code, .. } | SessionError::PeerError { code, .. } => *code,
            SessionError::Transport | SessionError::Envelope(_) => return None,
        })
    }
}

impl From<EnvelopeError> for SessionError {
    fn from(e: EnvelopeError) -> Self {
        SessionError::Envelope(e)
    }
}

/// Static configuration for one endpoint.
pub struct SessionConfig {
    pub keypair: Keypair,
    /// The peer identity authenticated by the transport (core §7.3 / TH4).
    /// When the peer delegates, this is its LEAF (the actual signing key), not
    /// the accountable root — see [`Session::accountable_peer`].
    pub expected_peer: AgentId,
    /// Local HELLO offer (initiator) / answer basis (responder).
    pub hello: Hello,
    /// Optional delegation chain (ordered leaf..root, identity §4) proving this
    /// endpoint's key is authorized by a root identity. When non-empty, our
    /// handshake envelope carries it in ProtHdr key 33; the peer validates it
    /// and records our ROOT as accountable. Empty = no delegation.
    pub delegation_chain: Vec<Vec<u8>>,
}

impl SessionConfig {
    /// A config with no delegation chain (the common case).
    pub fn new(keypair: Keypair, expected_peer: AgentId, hello: Hello) -> Self {
        SessionConfig {
            keypair,
            expected_peer,
            hello,
            delegation_chain: Vec::new(),
        }
    }
}

/// A delivered in-session message.
#[derive(Debug)]
pub struct Incoming {
    pub msg_type: u16,
    pub content: Value,
    pub msg_id: MsgId,
    pub seq: u64,
    /// The exact received envelope frame, byte-for-byte (invariant I1).
    /// Payment references digest THESE bytes, never a re-serialization of
    /// `content` (pay §3.1; TASK-017 / ERRATA 2026-07-12).
    pub raw: Vec<u8>,
}

/// An established (or in-handshake) session over a transport.
pub struct Session<T: Transport, C: Clock, M: MsgIdSource> {
    cfg: SessionConfig,
    transport: T,
    clock: C,
    ids: M,
    state: SessionState,
    session_id: [u8; 16],
    negotiated: Negotiated,
    th: [u8; 32],
    cb: [u8; 32],
    /// The peer's accountable identity: the delegation-chain ROOT if the peer
    /// delegated during the handshake, else its leaf (== `expected_peer`).
    accountable_peer: AgentId,
    out_seq: u64,
    in_seq: SeqTracker,
    in_replay: ReplayCache,
    pings_unanswered: u32,
    msgs_sent: u64,
    msgs_received: u64,
}

impl<T: Transport, C: Clock, M: MsgIdSource> Session<T, C, M> {
    // ---- handshake --------------------------------------------------------

    /// Drive the initiator side of the handshake to completion (blocks).
    pub fn initiate(cfg: SessionConfig, transport: T, clock: C, mut ids: M) -> Result<Self, SessionError> {
        let session_id = ids.next_msg_id();
        let version = *cfg
            .hello
            .versions
            .iter()
            .max()
            .ok_or(SessionError::Malformed)?;
        let hello_env = {
            let body = handshake_body(msg_type::HELLO, version, ids.next_msg_id(), clock.now_ms(), session_id, cfg.hello.to_value());
            seal_handshake(&cfg, &body)?
        };
        transport.send(&hello_env).map_err(|_| SessionError::Transport)?;

        let ack_env = transport.recv().map_err(|_| SessionError::Transport)?;
        let opened = open(&ack_env, b"")?;

        // An ERROR here means the responder rejected our HELLO (e.g. 1001).
        if opened.body.msg_type == msg_type::ERROR {
            let (code, retryable, text) = parse_error(&opened.body.content);
            return Err(SessionError::PeerError { code, retryable, text });
        }
        if opened.body.msg_type != msg_type::HELLO_ACK {
            return Err(SessionError::State);
        }
        if opened.sender != cfg.expected_peer {
            // Best-effort notify; we have no CB yet so this is an empty-aad ERROR.
            return Err(SessionError::PeerMismatch);
        }
        // Record the responder's accountable identity (root if it delegated).
        let accountable = peer_accountable(&ack_env, &opened.sender, &cfg.keypair.agent_id(), clock.now_ms())?;
        // The responder MUST echo our session-id (this impl's requirement; see
        // report R3 — core §6 does not yet mandate it).
        if opened.body.session_id != Some(session_id) {
            return Err(SessionError::Malformed);
        }
        // Record the peer's first msg-id so an in-session frame can't replay it.
        let mut in_replay = ReplayCache::new();
        match in_replay.check(&opened.sender, opened.body.msg_id, opened.body.timestamp_ms, clock.now_ms()) {
            ReplayCheck::Fresh => {}
            _ => return Err(SessionError::Replay),
        }

        // Parse the singleton HELLO_ACK and re-run negotiate against its
        // one-element Hello view (FR-3): validates the choice is within our offer.
        let ack = HelloAck::from_value(&opened.body.content).map_err(|_| SessionError::Malformed)?;
        let negotiated = negotiate(&cfg.hello, &ack.as_hello()).map_err(SessionError::Handshake)?;

        let th = transcript_hash(&hello_env, &ack_env);
        let cb = channel_binding(&th, transport.exporter());

        Ok(Session::established(cfg, transport, clock, ids, session_id, negotiated, th, cb, accountable, in_replay))
    }

    /// Drive the responder side of the handshake to completion (blocks).
    pub fn respond(cfg: SessionConfig, transport: T, clock: C, mut ids: M) -> Result<Self, SessionError> {
        let hello_env = transport.recv().map_err(|_| SessionError::Transport)?;
        let opened = open(&hello_env, b"")?;

        let mut session = PreEstablished {
            cfg: &cfg,
            transport: &transport,
            clock: &clock,
        };

        if opened.body.msg_type != msg_type::HELLO {
            session.emit_handshake_error(codes::STATE_VIOLATION);
            return Err(SessionError::State);
        }
        if opened.sender != cfg.expected_peer {
            session.emit_handshake_error(codes::BAD_SIGNATURE);
            return Err(SessionError::PeerMismatch);
        }
        // If the initiator delegated (ProtHdr key 33), validate the chain and
        // record its ROOT as accountable; the transport-auth check above already
        // bound the LEAF. A bad chain is a 2003 rejection.
        let accountable = match peer_accountable(&hello_env, &opened.sender, &cfg.keypair.agent_id(), clock.now_ms()) {
            Ok(id) => id,
            Err(_) => {
                session.emit_handshake_error(codes::BAD_DELEGATION);
                return Err(SessionError::BadDelegation);
            }
        };
        let mut in_replay = ReplayCache::new();
        match in_replay.check(&opened.sender, opened.body.msg_id, opened.body.timestamp_ms, clock.now_ms()) {
            ReplayCheck::Fresh => {}
            _ => {
                session.emit_handshake_error(codes::REPLAY);
                return Err(SessionError::Replay);
            }
        }
        let session_id = opened.body.session_id.ok_or(SessionError::Malformed)?;
        // A Hello with max-message-size below the 65536 floor is malformed here.
        let initiator_hello = Hello::from_value(&opened.body.content).map_err(|_| SessionError::Malformed)?;

        let negotiated = match negotiate(&initiator_hello, &cfg.hello) {
            Ok(n) => n,
            Err(e) => {
                session.emit_handshake_error(codes::UNSUPPORTED_VERSION);
                return Err(SessionError::Handshake(e));
            }
        };

        // HELLO_ACK: singletons per core §6.1 (key 4 = OUR max, key 5 = OUR max-tasks).
        let ack_content = negotiated.to_ack(cfg.hello.max_message_size, cfg.hello.max_tasks).to_value();
        let ack_env = {
            let body = handshake_body(msg_type::HELLO_ACK, negotiated.version, ids.next_msg_id(), clock.now_ms(), session_id, ack_content);
            seal_handshake(&cfg, &body)?
        };
        transport.send(&ack_env).map_err(|_| SessionError::Transport)?;

        let th = transcript_hash(&hello_env, &ack_env);
        let cb = channel_binding(&th, transport.exporter());

        Ok(Session::established(cfg, transport, clock, ids, session_id, negotiated, th, cb, accountable, in_replay))
    }

    #[allow(clippy::too_many_arguments)]
    fn established(
        cfg: SessionConfig,
        transport: T,
        clock: C,
        ids: M,
        session_id: [u8; 16],
        negotiated: Negotiated,
        th: [u8; 32],
        cb: [u8; 32],
        accountable_peer: AgentId,
        in_replay: ReplayCache,
    ) -> Self {
        Session {
            cfg,
            transport,
            clock,
            ids,
            state: SessionState::Established,
            session_id,
            negotiated,
            th,
            cb,
            accountable_peer,
            out_seq: 0,
            in_seq: SeqTracker::new(),
            in_replay,
            pings_unanswered: 0,
            msgs_sent: 0,
            msgs_received: 0,
        }
    }

    // ---- in-session I/O ---------------------------------------------------

    /// Send an in-session message. Precondition: Established. Returns the
    /// sealed envelope bytes that were written to the transport, so the
    /// caller can bind a payment reference to the exact sent frame (I1,
    /// pay §3.1) without re-sealing.
    pub fn send(&mut self, msg_type: u16, content: Value) -> Result<Vec<u8>, SessionError> {
        if self.state != SessionState::Established {
            return Err(SessionError::State);
        }
        let env = self.emit(msg_type, content)?;
        self.msgs_sent += 1;
        Ok(env)
    }

    /// Push a pre-sealed frame straight to the transport, bypassing sealing
    /// and the send counters. Conformance-harness use only (replay, relay, and
    /// crafted-frame scenarios); not part of normal operation.
    pub fn send_raw(&self, frame: &[u8]) -> Result<(), SessionError> {
        self.transport.send(frame).map_err(|_| SessionError::Transport)
    }

    /// Receive the next real message. PING is auto-answered with PONG and the
    /// loop continues; CLOSE/ERROR surface as `Err`. The FR-7 check order is
    /// normative — size (raw frame) → verify → sender → replay → seq — with the
    /// parenthetical "before signature work" overriding the sentence order.
    pub fn recv(&mut self) -> Result<Incoming, SessionError> {
        loop {
            if self.state != SessionState::Established {
                return Err(SessionError::State);
            }
            let frame = match self.transport.recv() {
                Ok(f) => f,
                // A framed transport (e.g. TCP) rejects an oversize prefix
                // before the body; map that to the same 1004 path as the
                // post-read size check below.
                Err(TransportError::TooLarge) => {
                    self.send_error(codes::TOO_LARGE, false);
                    return Err(SessionError::TooLarge);
                }
                Err(_) => return Err(SessionError::Transport),
            };

            if frame.len() as u64 > self.negotiated.max_message_size as u64 {
                // Unverifiable (never decoded): notify without echoing content.
                self.send_error(codes::TOO_LARGE, false);
                return Err(SessionError::TooLarge);
            }
            let opened = match open(&frame, &self.cb) {
                Ok(o) => o,
                Err(EnvelopeError::Identity(_)) => {
                    self.send_error(codes::BAD_SIGNATURE, false);
                    return Err(SessionError::BadSignature);
                }
                Err(_) => {
                    self.send_error(codes::MALFORMED, false);
                    return Err(SessionError::Malformed);
                }
            };
            if opened.sender != self.cfg.expected_peer {
                self.send_error(codes::BAD_SIGNATURE, false);
                return Err(SessionError::PeerMismatch);
            }
            // Every in-session envelope MUST carry THIS session's SessionID
            // (core §6.1); a mismatch is a state violation and closes the
            // session. (TASK-020 FR-2: the reference previously skipped this
            // check post-handshake — Go enforced it first, TASK-019.)
            if opened.body.session_id != Some(self.session_id) {
                self.send_error(codes::STATE_VIOLATION, false);
                self.state = SessionState::Closed;
                return Err(SessionError::State);
            }
            match self.in_replay.check(&opened.sender, opened.body.msg_id, opened.body.timestamp_ms, self.clock.now_ms()) {
                ReplayCheck::Fresh => {}
                _ => {
                    self.send_error(codes::REPLAY, true);
                    return Err(SessionError::Replay);
                }
            }
            let seq = match opened.body.seq {
                Some(s) => s,
                None => {
                    self.send_error(codes::MALFORMED, false);
                    return Err(SessionError::Malformed);
                }
            };
            if !self.in_seq.accept(seq) {
                self.send_error(codes::REPLAY, true);
                return Err(SessionError::Sequence);
            }

            match opened.body.msg_type {
                t if t == msg_type::PING => {
                    // Auto-PONG carrying the PING's msg-id (content key 1).
                    let pong = Value::Map(vec![(Value::Uint(1), Value::Bytes(opened.body.msg_id.0.to_vec()))]);
                    self.emit(msg_type::PONG, pong)?;
                    continue;
                }
                t if t == msg_type::PONG => {
                    self.pings_unanswered = 0;
                    self.msgs_received += 1;
                    return Ok(incoming(opened.body, seq, frame));
                }
                t if t == msg_type::CLOSE => {
                    self.state = SessionState::Closed;
                    let (code, retryable, text) = parse_error(&opened.body.content);
                    let _ = retryable;
                    return Err(SessionError::PeerClosed { code, reason: text.unwrap_or_default() });
                }
                t if t == msg_type::ERROR => {
                    let (code, retryable, text) = parse_error(&opened.body.content);
                    return Err(SessionError::PeerError { code, retryable, text });
                }
                _ => {
                    self.msgs_received += 1;
                    return Ok(incoming(opened.body, seq, frame));
                }
            }
        }
    }

    /// Send a PING (empty content); increments the unanswered-ping counter.
    pub fn ping(&mut self) -> Result<(), SessionError> {
        if self.state != SessionState::Established {
            return Err(SessionError::State);
        }
        self.emit(msg_type::PING, Value::Map(vec![]))?;
        self.pings_unanswered += 1;
        Ok(())
    }

    /// Send CLOSE with an ErrorMsg-shaped reason and transition to Closed.
    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), SessionError> {
        if self.state != SessionState::Established {
            return Err(SessionError::State);
        }
        let mut em = ErrorMsg::new(code, false);
        em.message = Some(reason.to_string());
        self.emit(msg_type::CLOSE, em.to_value())?;
        self.state = SessionState::Closed;
        Ok(())
    }

    // ---- accessors --------------------------------------------------------

    pub fn state(&self) -> SessionState {
        self.state
    }
    pub fn negotiated(&self) -> &Negotiated {
        &self.negotiated
    }
    pub fn channel_binding(&self) -> &[u8; 32] {
        &self.cb
    }
    pub fn transcript_hash(&self) -> &[u8; 32] {
        &self.th
    }
    pub fn session_id(&self) -> &[u8; 16] {
        &self.session_id
    }
    /// The peer's accountable identity (delegation root if it delegated, else
    /// its leaf). Receipts and trust accrue here (identity §4 / trust §5).
    pub fn accountable_peer(&self) -> &AgentId {
        &self.accountable_peer
    }
    /// The session's injected clock reading — lets layered protocols timestamp
    /// without a wall-clock read of their own (used by map-mesh serve/query).
    pub fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }
    pub fn pings_unanswered(&self) -> u32 {
        self.pings_unanswered
    }
    pub fn msgs_sent(&self) -> u64 {
        self.msgs_sent
    }
    pub fn msgs_received(&self) -> u64 {
        self.msgs_received
    }

    // ---- internals --------------------------------------------------------

    /// Seal and send one in-session envelope (CB-bound, seq-numbered).
    /// Returns the sealed frame (I1: the exact bytes on the wire).
    fn emit(&mut self, msg_type: u16, content: Value) -> Result<Vec<u8>, SessionError> {
        let msg_id = self.ids.next_msg_id();
        let ts = self.clock.now_ms();
        let body = Body {
            version: self.negotiated.version,
            msg_type,
            msg_id: MsgId(msg_id),
            timestamp_ms: ts,
            recipient: Some(self.cfg.expected_peer.clone()),
            ttl_ms: None,
            session_id: Some(self.session_id),
            seq: Some(self.out_seq),
            content,
        };
        let env = seal(&self.cfg.keypair, &body, &self.cb)?;
        self.transport.send(&env).map_err(|_| {
            self.state = SessionState::Closed;
            SessionError::Transport
        })?;
        self.out_seq += 1;
        Ok(env)
    }

    /// Best-effort ERROR notification (core §14: no received content echoed).
    fn send_error(&mut self, code: u16, retryable: bool) {
        let _ = self.emit(msg_type::ERROR, ErrorMsg::new(code, retryable).to_value());
    }
}

/// Minimal handshake-time helper for emitting an empty-aad ERROR before a
/// session exists (borrows the config/transport/clock without owning them).
struct PreEstablished<'a, T: Transport, C: Clock> {
    cfg: &'a SessionConfig,
    transport: &'a T,
    clock: &'a C,
}

impl<T: Transport, C: Clock> PreEstablished<'_, T, C> {
    fn emit_handshake_error(&mut self, code: u16) {
        // Handshake ERRORs are out-of-session: empty aad, no seq, fixed msg-id.
        let body = handshake_body(
            msg_type::ERROR,
            *self.cfg.hello.versions.iter().max().unwrap_or(&map_core::PROTOCOL_VERSION_1_0),
            [0xEE; 16],
            self.clock.now_ms(),
            [0u8; 16],
            ErrorMsg::new(code, false).to_value(),
        );
        if let Ok(env) = seal(&self.cfg.keypair, &body, b"") {
            let _ = self.transport.send(&env);
        }
    }
}

/// Seal a handshake envelope, attaching the config's delegation chain (ProtHdr
/// key 33) when present. Empty chain → a plain `seal` (byte-identical to before).
fn seal_handshake(cfg: &SessionConfig, body: &Body) -> Result<Vec<u8>, EnvelopeError> {
    if cfg.delegation_chain.is_empty() {
        seal(&cfg.keypair, body, b"")
    } else {
        seal_delegated(&cfg.keypair, body, b"", &cfg.delegation_chain)
    }
}

/// Resolve the peer's accountable identity from a handshake envelope: the
/// delegation-chain ROOT if it carries key 33, else the `leaf` (its signer).
/// `our_id` is passed as the audience for any audience-caveat'd chain.
fn peer_accountable(
    env: &[u8],
    leaf: &AgentId,
    our_id: &AgentId,
    now_ms: u64,
) -> Result<AgentId, SessionError> {
    match open_delegated(env, b"", Some(our_id), now_ms) {
        Ok(od) => Ok(od.scope.root),
        Err(EnvelopeError::MissingHeader(_)) => Ok(leaf.clone()),
        Err(EnvelopeError::Delegation(_)) => Err(SessionError::BadDelegation),
        Err(e) => Err(SessionError::Envelope(e)),
    }
}

/// Out-of-session envelope body: keys {0,1,2,3,6,10}, no recipient, no seq (§7).
fn handshake_body(msg_type: u16, version: u16, msg_id: [u8; 16], ts: u64, session_id: [u8; 16], content: Value) -> Body {
    Body {
        version,
        msg_type,
        msg_id: MsgId(msg_id),
        timestamp_ms: ts,
        recipient: None,
        ttl_ms: None,
        session_id: Some(session_id),
        seq: None,
        content,
    }
}

fn incoming(body: Body, seq: u64, raw: Vec<u8>) -> Incoming {
    Incoming {
        msg_type: body.msg_type,
        content: body.content,
        msg_id: body.msg_id,
        seq,
        raw,
    }
}

/// Read an ErrorMsg-shaped content map into (code, retryable, message).
fn parse_error(content: &Value) -> (u16, bool, Option<String>) {
    match ErrorMsg::from_value(content) {
        Ok(em) => (em.code, em.retryable, em.message),
        Err(_) => (codes::MALFORMED, false, None),
    }
}
