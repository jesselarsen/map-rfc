package session

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/binary"
	"net"
	"sync"
	"testing"
	"time"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// --- test fixtures ---------------------------------------------------------

const testTS = 1_780_000_000_000

func keyFromLabel(label string) ed25519.PrivateKey {
	seed := sha256.Sum256([]byte(label))
	return ed25519.NewKeyFromSeed(seed[:])
}

func aidOf(k ed25519.PrivateKey) identity.AgentID {
	a, err := identity.FromPub(k.Public().(ed25519.PublicKey))
	if err != nil {
		panic(err)
	}
	return a
}

// fixedClock and countingIDs keep handshakes deterministic in unit tests.
type fixedClock struct{ ms uint64 }

func (c fixedClock) NowMs() uint64 { return c.ms }

type countingIDs struct {
	n    byte
	base byte
}

func (c *countingIDs) NextMsgID() []byte {
	c.n++
	b := make([]byte, 16)
	b[0] = c.base
	b[1] = c.n
	return b
}

func stdHello() Hello {
	return Hello{
		Versions:       []uint16{0x0100},
		Suites:         []uint16{1},
		MaxMessageSize: 262144,
	}
}

// pair establishes a connected initiator/responder over InMemoryDuplex with
// the given configs, returning both sessions (or the errors).
func establishDuplex(t *testing.T, initCfg, respCfg SessionConfig) (*Session, *Session) {
	t.Helper()
	a := aidOf(initCfg.Keypair)
	b := aidOf(respCfg.Keypair)
	di, dr := NewDuplexPair(a, b)
	var resp *Session
	var respErr error
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		resp, respErr = Respond(respCfg, dr, fixedClock{testTS}, &countingIDs{base: 0xB0})
	}()
	init, initErr := Initiate(initCfg, di, fixedClock{testTS}, &countingIDs{base: 0xA0})
	wg.Wait()
	if initErr != nil {
		t.Fatalf("initiate: %v", initErr)
	}
	if respErr != nil {
		t.Fatalf("respond: %v", respErr)
	}
	return init, resp
}

func cfgFor(kp ed25519.PrivateKey, peer identity.AgentID, h Hello) SessionConfig {
	return SessionConfig{Keypair: kp, ExpectedPeer: peer, Hello: h}
}

// --- FR-5 happy path -------------------------------------------------------

func TestLoopbackInMemory(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	init, resp := establishDuplex(t,
		cfgFor(ka, aidOf(kb), stdHello()),
		cfgFor(kb, aidOf(ka), stdHello()))

	// Echo: initiator sends, responder receives and echoes, initiator verifies.
	payload := cbor.Map{{K: cbor.Uint(1), V: cbor.Text("ping-map")}}
	if _, err := init.Send(0xF000, payload); err != nil {
		t.Fatalf("init send: %v", err)
	}
	got, err := resp.Recv()
	if err != nil {
		t.Fatalf("resp recv: %v", err)
	}
	if got.MsgType != 0xF000 {
		t.Errorf("msg-type = %#x", got.MsgType)
	}
	if _, err := resp.Send(0xF000, got.Content); err != nil {
		t.Fatalf("resp echo: %v", err)
	}
	back, err := init.Recv()
	if err != nil {
		t.Fatalf("init recv echo: %v", err)
	}
	m, ok := back.Content.(cbor.Map)
	if !ok || len(m) != 1 || m[0].V.(cbor.Text) != "ping-map" {
		t.Errorf("echo content mismatch: %v", back.Content)
	}

	// PING -> auto-PONG: the responder auto-answers and Recv keeps looping,
	// so a follow-up real message is what surfaces.
	if err := init.Ping(); err != nil {
		t.Fatalf("ping: %v", err)
	}
	// responder processes the PING (auto-PONG) then the next real message.
	if _, err := init.Send(0xF000, payload); err != nil {
		t.Fatalf("init send after ping: %v", err)
	}
	msg, err := resp.Recv() // PING auto-answered internally; returns the real msg
	if err != nil {
		t.Fatalf("resp recv after ping: %v", err)
	}
	if msg.MsgType != 0xF000 {
		t.Errorf("expected real message after auto-PONG, got %#x", msg.MsgType)
	}
	// drain the auto-PONG on the initiator side.
	if _, err := init.Recv(); err != nil {
		if se, ok := err.(*SessionError); !ok || se.Kind != "peer_closed" {
			// PONG is delivered as an Incoming; no error expected here.
			t.Fatalf("init recv pong: %v", err)
		}
	}
}

func TestLoopbackTCP(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	aid, bid := aidOf(ka), aidOf(kb)

	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()

	var resp *Session
	var respErr error
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		conn, err := ln.Accept()
		if err != nil {
			respErr = err
			return
		}
		tr := NewTCPTransport(conn, aid, bid, 262144)
		resp, respErr = Respond(cfgFor(kb, aid, stdHello()), tr, SystemClock{}, RandMsgIDs{})
	}()

	conn, err := net.Dial("tcp", ln.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	tr := NewTCPTransport(conn, aid, bid, 262144)
	init, err := Initiate(cfgFor(ka, bid, stdHello()), tr, SystemClock{}, RandMsgIDs{})
	if err != nil {
		t.Fatalf("initiate over tcp: %v", err)
	}
	wg.Wait()
	if respErr != nil {
		t.Fatalf("respond over tcp: %v", respErr)
	}

	if _, err := init.Send(0xF000, cbor.Text("over-the-wire")); err != nil {
		t.Fatalf("send: %v", err)
	}
	got, err := resp.Recv()
	if err != nil {
		t.Fatalf("recv: %v", err)
	}
	if got.Content.(cbor.Text) != "over-the-wire" {
		t.Errorf("tcp content mismatch")
	}
	if err := init.Close(0, "done"); err != nil {
		t.Fatalf("close: %v", err)
	}
	_, err = resp.Recv()
	se, ok := err.(*SessionError)
	if !ok || se.Kind != "peer_closed" {
		t.Errorf("expected peer_closed, got %v", err)
	}
}

// --- FR-5 negatives (white-box: craft frames with the shared CB) -----------

// craftFrame seals an in-session envelope from `from` to its peer using the
// session's channel binding, with a chosen seq/sid — the tool for adversarial
// receive-path tests.
func craftFrame(t *testing.T, from *Session, msgType uint64, seq uint64, sid []byte, content cbor.Value) []byte {
	t.Helper()
	recipient := from.cfg.ExpectedPeer
	body := envelope.Body{
		Version:   uint64(from.negotiated.Version),
		MsgType:   msgType,
		MsgID:     from.ids.NextMsgID(),
		Timestamp: from.clock.NowMs(),
		Recipient: &recipient,
		SessionID: sid,
		Seq:       &seq,
		Content:   content,
	}
	env, err := envelope.SealBody(from.cfg.Keypair, body, from.cb[:])
	if err != nil {
		t.Fatal(err)
	}
	return env
}

func expectRecvCode(t *testing.T, s *Session, wantKind string, wantCode uint16) {
	t.Helper()
	_, err := s.Recv()
	se, ok := err.(*SessionError)
	if !ok {
		t.Fatalf("expected SessionError, got %v", err)
	}
	if se.Kind != wantKind || se.Code != wantCode {
		t.Errorf("got kind=%q code=%d, want kind=%q code=%d", se.Kind, se.Code, wantKind, wantCode)
	}
}

func TestReplayRejected(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	init, resp := establishDuplex(t, cfgFor(ka, aidOf(kb), stdHello()), cfgFor(kb, aidOf(ka), stdHello()))
	frame := craftFrame(t, init, 0xF000, 0, init.sessionID, cbor.Uint(1))
	if err := init.transport.Send(frame); err != nil {
		t.Fatal(err)
	}
	if _, err := resp.Recv(); err != nil {
		t.Fatalf("first delivery must succeed: %v", err)
	}
	// Re-send the identical frame -> replayed msg-id -> 1003, session survives.
	if err := init.transport.Send(frame); err != nil {
		t.Fatal(err)
	}
	expectRecvCode(t, resp, "replay", CodeReplay)
	if resp.state != stateEstablished {
		t.Errorf("replay must not close the session")
	}
}

func TestSeqGapRejected(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	init, resp := establishDuplex(t, cfgFor(ka, aidOf(kb), stdHello()), cfgFor(kb, aidOf(ka), stdHello()))
	// First expected seq is 0; deliver seq 1 -> gap -> 1003.
	frame := craftFrame(t, init, 0xF000, 1, init.sessionID, cbor.Uint(1))
	if err := init.transport.Send(frame); err != nil {
		t.Fatal(err)
	}
	expectRecvCode(t, resp, "sequence", CodeReplay)
}

func TestTamperRejected(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	init, resp := establishDuplex(t, cfgFor(ka, aidOf(kb), stdHello()), cfgFor(kb, aidOf(ka), stdHello()))
	frame := craftFrame(t, init, 0xF000, 0, init.sessionID, cbor.Uint(1))
	frame[len(frame)-1] ^= 0xFF // flip a signature byte
	if err := init.transport.Send(frame); err != nil {
		t.Fatal(err)
	}
	expectRecvCode(t, resp, "sig", CodeBadSignature)
}

func TestWrongSessionIDRejected(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	init, resp := establishDuplex(t, cfgFor(ka, aidOf(kb), stdHello()), cfgFor(kb, aidOf(ka), stdHello()))
	wrong := make([]byte, 16)
	copy(wrong, init.sessionID)
	wrong[0] ^= 0xFF
	frame := craftFrame(t, init, 0xF000, 0, wrong, cbor.Uint(1))
	if err := init.transport.Send(frame); err != nil {
		t.Fatal(err)
	}
	expectRecvCode(t, resp, "sid", CodeStateViolation)
	if resp.state != stateClosed {
		t.Errorf("wrong session-id must close the session")
	}
}

func TestOversizedFrameRejected(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	init, resp := establishDuplex(t,
		cfgFor(ka, aidOf(kb), Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 65536}),
		cfgFor(kb, aidOf(ka), Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 65536}))
	junk := make([]byte, 70000) // > negotiated max
	if err := init.transport.Send(junk); err != nil {
		t.Fatal(err)
	}
	expectRecvCode(t, resp, "too_large", CodeTooLarge)
}

func TestVersionDisjointHandshake(t *testing.T) {
	ka, kb := keyFromLabel("MAP-TEST-VECTOR-KEY-A"), keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	initCfg := cfgFor(ka, aidOf(kb), Hello{Versions: []uint16{0x0200}, Suites: []uint16{1}, MaxMessageSize: 262144})
	respCfg := cfgFor(kb, aidOf(ka), Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 262144})
	di, dr := NewDuplexPair(aidOf(ka), aidOf(kb))
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		_, _ = Respond(respCfg, dr, fixedClock{testTS}, &countingIDs{base: 0xB0})
	}()
	_, err := Initiate(initCfg, di, fixedClock{testTS}, &countingIDs{base: 0xA0})
	wg.Wait()
	se, ok := err.(*SessionError)
	if !ok || se.Code != CodeUnsupportedVersion {
		t.Fatalf("expected peer ERROR 1001, got %v", err)
	}
}

// --- FR-5 delegated handshake ----------------------------------------------

func delegationCert(issuer, subject ed25519.PrivateKey, capURI string) []byte {
	body := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(aidOf(issuer).Bytes())},
		{K: cbor.Uint(2), V: cbor.Bytes(aidOf(subject).Bytes())},
		{K: cbor.Uint(3), V: cbor.Uint(testTS)},
		{K: cbor.Uint(4), V: cbor.Uint(testTS + 600000)},
		{K: cbor.Uint(5), V: cbor.Array{cbor.Text(capURI)}},
		{K: cbor.Uint(7), V: cbor.Uint(1)},
	}
	env, err := envelope.Seal(issuer, 0x0502, make([]byte, 16), testTS, body, nil)
	if err != nil {
		panic(err)
	}
	return env
}

// A8 regression: expiry is amortized O(1). 200k fresh ids spread over a
// window-sized span must stay fast and keep only the in-window ids.
func TestReplayCacheExpiryIsLinear(t *testing.T) {
	c := NewReplayCache()
	var sender identity.AgentID
	start := time.Now()
	const n = 200_000
	for i := 0; i < n; i++ {
		now := uint64(1_790_000_000_000 + i*10) // 10 ms apart: 2 000 s total
		id := make([]byte, 16)
		binary.BigEndian.PutUint64(id, uint64(i))
		if c.Check(sender, id, now, now) != Fresh {
			t.Fatalf("id %d not fresh", i)
		}
	}
	if el := time.Since(start); el > 5*time.Second {
		t.Fatalf("200k checks took %v; expiry is not amortized O(1)", el)
	}
	if want := DefaultWindowMs/10 + 1; c.Len() > int(want) {
		t.Fatalf("cache holds %d ids, want at most %d in-window", c.Len(), want)
	}
	// An id seen inside the window is still a replay.
	last := uint64(1_790_000_000_000 + (n-1)*10)
	id := make([]byte, 16)
	binary.BigEndian.PutUint64(id, uint64(n-1))
	if c.Check(sender, id, last, last) != Replayed {
		t.Fatal("in-window replay not detected")
	}
}
