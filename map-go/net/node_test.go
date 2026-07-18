package net

import (
	"context"
	"crypto/rand"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/session"
)

func randSeed(t *testing.T) []byte {
	t.Helper()
	seed := make([]byte, 32)
	if _, err := rand.Read(seed); err != nil {
		t.Fatalf("seed: %v", err)
	}
	return seed
}

func hello() session.Hello {
	max := uint32(1 << 20)
	return session.Hello{
		Versions:       []uint16{0x0100},
		Suites:         []uint16{1},
		MaxMessageSize: max,
	}
}

// TestMapNodeHandshake verifies two MapNodes establish a session over
// libp2p/QUIC and that the channel binding (FR-4) and agent-id binding (FR-3)
// hold, by exchanging a PING/PONG and an in-session message.
func TestMapNodeHandshake(t *testing.T) {
	ctx := context.Background()
	seedB := randSeed(t)
	seedA := randSeed(t)

	nb, err := New(seedB, hello(), "/ip4/127.0.0.1/udp/0/quic-v1")
	if err != nil {
		t.Fatalf("new B: %v", err)
	}
	defer nb.Close()
	na, err := New(seedA, hello(), "")
	if err != nil {
		t.Fatalf("new A: %v", err)
	}
	defer na.Close()

	ma, err := nb.ListenAddr()
	if err != nil {
		t.Fatalf("listen B: %v", err)
	}

	// Accept in the background.
	type acc struct {
		s   *session.Session
		err error
	}
	got := make(chan acc, 1)
	go func() {
		s := nb.Accept()
		got <- acc{s: s}
	}()

	dialAddr := ma.String() + "/p2p/" + nb.PeerID()
	t.Logf("dialAddr=%s", dialAddr)
	sa, err := na.Dial(ctx, dialAddr, nb.LocalAgent())
	if err != nil {
		t.Fatalf("dial A: %v", err)
	}
	r := <-got
	if r.err != nil {
		t.Fatalf("accept B: %v", r.err)
	}
	sb := r.s
	if sb == nil {
		t.Fatalf("accept returned nil session")
	}

	// In-session message round-trip: A sends a custom msg, B receives.
	const testMsg = uint64(0x7E57)
	want := []byte("hello-over-libp2p")
	if _, err := sa.Send(testMsg, cbor.Bytes(want)); err != nil {
		t.Fatalf("send A: %v", err)
	}
	inc, err := sb.Recv()
	if err != nil {
		t.Fatalf("recv B: %v", err)
	}
	if inc.MsgType != testMsg {
		t.Fatalf("msg type: got %x want %x", inc.MsgType, testMsg)
	}
	gotBytes, ok := inc.Content.(cbor.Bytes)
	if !ok {
		t.Fatalf("content is not cbor.Bytes: %T", inc.Content)
	}
	if string(gotBytes) != string(want) {
		t.Fatalf("payload: got %q want %q", gotBytes, want)
	}

	// PING/PONG auto-handled by Recv loop; fire a ping and confirm session
	// stays alive by sending another message back.
	if err := sb.Ping(); err != nil {
		t.Fatalf("ping B: %v", err)
	}
	if _, err := sb.Send(testMsg, cbor.Bytes([]byte("pong-ok"))); err != nil {
		t.Fatalf("send B: %v", err)
	}
	inc2, err := sa.Recv()
	if err != nil {
		t.Fatalf("recv A: %v", err)
	}
	b2, ok := inc2.Content.(cbor.Bytes)
	if !ok {
		t.Fatalf("content2 not cbor.Bytes: %T", inc2.Content)
	}
	if string(b2) != "pong-ok" {
		t.Fatalf("roundtrip2: got %q", b2)
	}

	// Channel binding must be non-zero (proves the exporter fired).
	if sa.ChannelBinding() == [32]byte{} {
		t.Fatalf("A channel binding is zero")
	}
	if sb.ChannelBinding() == [32]byte{} {
		t.Fatalf("B channel binding is zero")
	}
}
