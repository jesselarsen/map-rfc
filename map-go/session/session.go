package session

import (
	"crypto/ed25519"
	"crypto/rand"
	"fmt"
	"time"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/hello"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// mapU1Bytes builds the single-entry content map {1: <bytes>} used by
// PING/PONG (the echoed msg-id / opaque payload).
func mapU1Bytes(b []byte) cbor.Value {
	return cbor.Map{{K: cbor.Uint(1), V: cbor.Bytes(b)}}
}

// Message types (core §5).
const (
	MsgHello    = 0x0001
	MsgHelloAck = 0x0002
	MsgPing     = 0x0003
	MsgPong     = 0x0004
	MsgClose    = 0x0005
	MsgError    = 0x0006
)

type state int

const (
	stateIdle state = iota
	stateEstablished
	stateClosed
)

// Clock supplies the local time in Unix-epoch milliseconds.
type Clock interface{ NowMs() uint64 }

// MsgIDSource supplies fresh 16-byte message ids (and the session id).
type MsgIDSource interface{ NextMsgID() []byte }

// SystemClock is the production clock.
type SystemClock struct{}

// NowMs returns the wall clock in ms.
func (SystemClock) NowMs() uint64 { return uint64(time.Now().UnixMilli()) }

// RandMsgIDs is the production id source (crypto/rand).
type RandMsgIDs struct{}

// NextMsgID returns 16 random bytes.
func (RandMsgIDs) NextMsgID() []byte {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		panic(err) // crypto/rand failure is unrecoverable
	}
	return b
}

// SessionConfig is the per-peer session configuration.
type SessionConfig struct {
	Keypair      ed25519.PrivateKey
	ExpectedPeer identity.AgentID
	Hello        Hello
}

// SessionError carries the reason a session operation failed, including the
// core §9 code where one applies (so a harness can assert on it).
type SessionError struct {
	Kind      string
	Code      uint16
	Retryable bool
	Text      string
}

func (e *SessionError) Error() string {
	if e.Text != "" {
		return fmt.Sprintf("session: %s [%d] %s", e.Kind, e.Code, e.Text)
	}
	return fmt.Sprintf("session: %s [%d]", e.Kind, e.Code)
}

// Incoming is a delivered in-session message. Raw is the exact received frame
// (invariant I1) — payment references and cross-session checks bind it.
type Incoming struct {
	MsgType uint64
	Content cbor.Value
	MsgID   []byte
	Seq     uint64
	Raw     []byte
}

// Session is an established (or in-handshake) MAP session over a Transport.
type Session struct {
	cfg             SessionConfig
	transport       Transport
	clock           Clock
	ids             MsgIDSource
	state           state
	sessionID       []byte
	negotiated      Negotiated
	cb              [32]byte
	accountablePeer identity.AgentID
	outSeq          uint64
	inSeq           SeqTracker
	inReplay        *ReplayCache
}

// AccountablePeer is the peer's accountable identity: the delegation-chain
// root if it delegated during the handshake, else its signing key.
func (s *Session) AccountablePeer() identity.AgentID { return s.accountablePeer }

// Negotiated returns the negotiation result.
func (s *Session) Negotiated() Negotiated { return s.negotiated }

// ChannelBinding returns the session's channel binding (harness use: craft
// frames that must verify under this session's CB).
func (s *Session) ChannelBinding() [32]byte { return s.cb }

// SessionID returns this session's 16-byte id (harness use).
func (s *Session) SessionID() []byte { return s.sessionID }

// SendRaw pushes a pre-sealed frame straight to the transport, bypassing
// sealing and the send counters. Conformance-harness use only (replay,
// relay, crafted-frame scenarios).
func (s *Session) SendRaw(frame []byte) error { return s.transport.Send(frame) }

// zeros16 is the handshake-ERROR session-id placeholder.
var zeros16 = make([]byte, 16)

// handshakeErrorMsgID is the fixed msg-id for pre-established ERRORs.
var handshakeErrorMsgID = func() []byte {
	b := make([]byte, 16)
	for i := range b {
		b[i] = 0xEE
	}
	return b
}()

func (s *Session) offeredVersion() uint16 {
	best := uint16(0)
	for _, v := range s.cfg.Hello.Versions {
		if v > best {
			best = v
		}
	}
	return best
}

// emitHandshakeError seals a best-effort out-of-session ERROR (empty aad,
// fixed msg-id, zero session-id).
func (s *Session) emitHandshakeError(code uint16) {
	body := envelope.Body{
		Version:   uint64(s.offeredVersion()),
		MsgType:   MsgError,
		MsgID:     handshakeErrorMsgID,
		Timestamp: s.clock.NowMs(),
		SessionID: zeros16,
		Content:   ErrorMsg{Code: code, Retryable: false}.ToValue(),
	}
	if env, err := envelope.SealBody(s.cfg.Keypair, body, nil); err == nil {
		_ = s.transport.Send(env)
	}
}

// Initiate drives the initiator side of the handshake to completion.
func Initiate(cfg SessionConfig, transport Transport, clock Clock, ids MsgIDSource) (*Session, error) {
	sessionID := ids.NextMsgID()
	version := uint16(0)
	for _, v := range cfg.Hello.Versions {
		if v > version {
			version = v
		}
	}
	if version == 0 {
		return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
	}
	helloBody := envelope.Body{
		Version:   uint64(version),
		MsgType:   MsgHello,
		MsgID:     ids.NextMsgID(),
		Timestamp: clock.NowMs(),
		SessionID: sessionID,
		Content:   cfg.Hello.ToValue(),
	}
	helloEnv, err := envelope.SealBody(cfg.Keypair, helloBody, nil)
	if err != nil {
		return nil, err
	}
	if err := transport.Send(helloEnv); err != nil {
		return nil, &SessionError{Kind: "transport", Code: 0}
	}

	ackEnv, err := transport.Recv()
	if err != nil {
		return nil, &SessionError{Kind: "transport", Code: 0}
	}
	opened, err := envelope.Open(ackEnv, nil)
	if err != nil {
		return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
	}
	if opened.Body.MsgType == MsgError {
		em, _ := ParseErrorMsg(opened.Body.Content)
		return nil, &SessionError{Kind: "peer_error", Code: em.Code, Retryable: em.Retryable, Text: em.Text}
	}
	if opened.Body.MsgType != MsgHelloAck {
		return nil, &SessionError{Kind: "state", Code: CodeStateViolation}
	}
	if opened.KID != cfg.ExpectedPeer {
		return nil, &SessionError{Kind: "peer_mismatch", Code: CodeBadSignature}
	}
	accountable := opened.KID
	// The responder MUST echo our session-id (reference-impl requirement).
	if !bytesEqual(opened.Body.SessionID, sessionID) {
		return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
	}
	inReplay := NewReplayCache()
	if inReplay.Check(opened.KID, opened.Body.MsgID, opened.Body.Timestamp, clock.NowMs()) != Fresh {
		return nil, &SessionError{Kind: "replay", Code: CodeReplay}
	}
	ack, err := ParseHelloAck(opened.Body.Content)
	if err != nil {
		return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
	}
	// Re-run negotiate against the ACK's one-element view: the responder's
	// choice MUST be within our own offer (downgrade defense).
	negotiated, err := Negotiate(cfg.Hello, ack.AsHello())
	if err != nil {
		return nil, &SessionError{Kind: "handshake", Code: CodeUnsupportedVersion}
	}
	th := hello.TranscriptHash(helloEnv, ackEnv)
	cb, err := hello.ChannelBinding(th, transport.Exporter())
	if err != nil {
		return nil, err
	}
	return established(cfg, transport, clock, ids, sessionID, negotiated, cb, accountable, inReplay), nil
}

// Respond drives the responder side of the handshake to completion.
func Respond(cfg SessionConfig, transport Transport, clock Clock, ids MsgIDSource) (*Session, error) {
	helloEnv, err := transport.Recv()
	if err != nil {
		return nil, &SessionError{Kind: "transport", Code: 0}
	}
	opened, err := envelope.Open(helloEnv, nil)
	if err != nil {
		return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
	}
	pre := &Session{cfg: cfg, transport: transport, clock: clock, ids: ids}
	if opened.Body.MsgType != MsgHello {
		pre.emitHandshakeError(CodeStateViolation)
		return nil, &SessionError{Kind: "state", Code: CodeStateViolation}
	}
	if opened.KID != cfg.ExpectedPeer {
		pre.emitHandshakeError(CodeBadSignature)
		return nil, &SessionError{Kind: "peer_mismatch", Code: CodeBadSignature}
	}
	accountable := opened.KID
	inReplay := NewReplayCache()
	if inReplay.Check(opened.KID, opened.Body.MsgID, opened.Body.Timestamp, clock.NowMs()) != Fresh {
		pre.emitHandshakeError(CodeReplay)
		return nil, &SessionError{Kind: "replay", Code: CodeReplay}
	}
	if opened.Body.SessionID == nil {
		pre.emitHandshakeError(CodeMalformed)
		return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
	}
	sessionID := opened.Body.SessionID
	initiatorHello, err := ParseHello(opened.Body.Content)
	if err != nil {
		pre.emitHandshakeError(CodeMalformed)
		return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
	}
	negotiated, err := Negotiate(initiatorHello, cfg.Hello)
	if err != nil {
		pre.emitHandshakeError(CodeUnsupportedVersion)
		return nil, &SessionError{Kind: "handshake", Code: CodeUnsupportedVersion}
	}
	ackContent := negotiated.Ack(cfg.Hello.MaxMessageSize, cfg.Hello.MaxTasks).ToValue()
	ackBody := envelope.Body{
		Version:   uint64(negotiated.Version),
		MsgType:   MsgHelloAck,
		MsgID:     ids.NextMsgID(),
		Timestamp: clock.NowMs(),
		SessionID: sessionID,
		Content:   ackContent,
	}
	ackEnv, err := envelope.SealBody(cfg.Keypair, ackBody, nil)
	if err != nil {
		return nil, err
	}
	if err := transport.Send(ackEnv); err != nil {
		return nil, &SessionError{Kind: "transport", Code: 0}
	}
	th := hello.TranscriptHash(helloEnv, ackEnv)
	cb, err := hello.ChannelBinding(th, transport.Exporter())
	if err != nil {
		return nil, err
	}
	return established(cfg, transport, clock, ids, sessionID, negotiated, cb, accountable, inReplay), nil
}

func established(cfg SessionConfig, transport Transport, clock Clock, ids MsgIDSource, sessionID []byte, negotiated Negotiated, cb [32]byte, accountable identity.AgentID, inReplay *ReplayCache) *Session {
	return &Session{
		cfg:             cfg,
		transport:       transport,
		clock:           clock,
		ids:             ids,
		state:           stateEstablished,
		sessionID:       sessionID,
		negotiated:      negotiated,
		cb:              cb,
		accountablePeer: accountable,
		inReplay:        inReplay,
	}
}

// emit seals and sends one in-session envelope (CB-bound, seq-numbered) and
// returns the sealed frame (the TASK-017 I1 seam).
func (s *Session) emit(msgType uint64, content cbor.Value) ([]byte, error) {
	seq := s.outSeq
	recipient := s.cfg.ExpectedPeer
	body := envelope.Body{
		Version:   uint64(s.negotiated.Version),
		MsgType:   msgType,
		MsgID:     s.ids.NextMsgID(),
		Timestamp: s.clock.NowMs(),
		Recipient: &recipient,
		SessionID: s.sessionID,
		Seq:       &seq,
		Content:   content,
	}
	env, err := envelope.SealBody(s.cfg.Keypair, body, s.cb[:])
	if err != nil {
		return nil, err
	}
	if err := s.transport.Send(env); err != nil {
		s.state = stateClosed
		return nil, &SessionError{Kind: "transport", Code: 0}
	}
	s.outSeq++
	return env, nil
}

func (s *Session) sendError(code uint16, retryable bool) {
	_, _ = s.emit(MsgError, ErrorMsg{Code: code, Retryable: retryable}.ToValue())
}

// Send transmits an in-session message and returns the sealed frame.
func (s *Session) Send(msgType uint64, content cbor.Value) ([]byte, error) {
	if s.state != stateEstablished {
		return nil, &SessionError{Kind: "state", Code: CodeStateViolation}
	}
	return s.emit(msgType, content)
}

// Ping sends a PING carrying an opaque 8-byte payload echoed by PONG.
func (s *Session) Ping() error {
	if s.state != stateEstablished {
		return &SessionError{Kind: "state", Code: CodeStateViolation}
	}
	_, err := s.emit(MsgPing, mapU1Bytes(s.ids.NextMsgID()[:8]))
	return err
}

// Close sends CLOSE with an ErrorMsg-shaped reason and transitions to Closed.
func (s *Session) Close(code uint16, reason string) error {
	if s.state != stateEstablished {
		return &SessionError{Kind: "state", Code: CodeStateViolation}
	}
	_, err := s.emit(MsgClose, ErrorMsg{Code: code, Retryable: false, Text: reason}.ToValue())
	s.state = stateClosed
	return err
}

// Recv returns the next real message. PING is auto-answered with PONG and the
// loop continues; CLOSE/ERROR surface as errors. Receive order is normative
// (core §4.3, cheapest-first): size -> verify -> sender -> session-id ->
// replay -> seq.
func (s *Session) Recv() (*Incoming, error) {
	for {
		if s.state != stateEstablished {
			return nil, &SessionError{Kind: "state", Code: CodeStateViolation}
		}
		frame, err := s.transport.Recv()
		if err == ErrTooLarge {
			s.sendError(CodeTooLarge, false)
			return nil, &SessionError{Kind: "too_large", Code: CodeTooLarge}
		}
		if err != nil {
			return nil, &SessionError{Kind: "transport", Code: 0}
		}
		// Size-first: covers transports (e.g. in-memory) that do not enforce
		// the ceiling during framing.
		if uint64(len(frame)) > uint64(s.negotiated.MaxMessageSize) {
			s.sendError(CodeTooLarge, false)
			return nil, &SessionError{Kind: "too_large", Code: CodeTooLarge}
		}
		opened, err := envelope.Open(frame, s.cb[:])
		if err != nil {
			if ee, ok := err.(*envelope.Error); ok && ee.Code == envelope.CodeBadSignature {
				s.sendError(CodeBadSignature, false)
				return nil, &SessionError{Kind: "sig", Code: CodeBadSignature}
			}
			s.sendError(CodeMalformed, false)
			return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
		}
		if opened.KID != s.cfg.ExpectedPeer {
			s.sendError(CodeBadSignature, false)
			return nil, &SessionError{Kind: "peer_mismatch", Code: CodeBadSignature}
		}
		// Session-id binding (core §6.1): a frame must carry THIS session's id.
		if !bytesEqual(opened.Body.SessionID, s.sessionID) {
			s.sendError(CodeStateViolation, false)
			s.state = stateClosed
			return nil, &SessionError{Kind: "sid", Code: CodeStateViolation}
		}
		if s.inReplay.Check(opened.KID, opened.Body.MsgID, opened.Body.Timestamp, s.clock.NowMs()) != Fresh {
			s.sendError(CodeReplay, true)
			return nil, &SessionError{Kind: "replay", Code: CodeReplay}
		}
		if opened.Body.Seq == nil {
			s.sendError(CodeMalformed, false)
			return nil, &SessionError{Kind: "malformed", Code: CodeMalformed}
		}
		seq := *opened.Body.Seq
		if !s.inSeq.Accept(seq) {
			s.sendError(CodeReplay, true)
			return nil, &SessionError{Kind: "sequence", Code: CodeReplay}
		}
		switch opened.Body.MsgType {
		case MsgPing:
			// Auto-PONG carrying the PING's msg-id (content key 1).
			_, _ = s.emit(MsgPong, mapU1Bytes(opened.Body.MsgID))
			continue
		case MsgClose:
			s.state = stateClosed
			em, _ := ParseErrorMsg(opened.Body.Content)
			return nil, &SessionError{Kind: "peer_closed", Code: em.Code, Text: em.Text}
		case MsgError:
			em, _ := ParseErrorMsg(opened.Body.Content)
			return nil, &SessionError{Kind: "peer_error", Code: em.Code, Retryable: em.Retryable, Text: em.Text}
		default:
			return &Incoming{
				MsgType: opened.Body.MsgType,
				Content: opened.Body.Content,
				MsgID:   opened.Body.MsgID,
				Seq:     seq,
				Raw:     frame,
			}, nil
		}
	}
}

func bytesEqual(a, b []byte) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
