package session

import (
	"encoding/binary"
	"errors"
	"io"
	"net"

	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// lenPrefixBytes is the 4-byte big-endian frame length prefix (core §10).
const lenPrefixBytes = 4

// ErrTransportClosed is returned once an endpoint can no longer send/recv.
var ErrTransportClosed = errors.New("session: transport closed")

// Transport is a message-framed byte channel: one complete envelope per
// Send/Recv, plus the keying material bound into the channel binding (§7.3).
type Transport interface {
	Send(frame []byte) error
	Recv() ([]byte, error)
	Exporter() []byte
}

// Exporter builds the identity-pair exporter used by the test-harness
// transports and the core §10 libp2p binding: initiator-AgentID ||
// responder-AgentID (68 bytes). NOT a substitute for a TLS/Noise exporter on
// a deployable transport.
func Exporter(initiator, responder identity.AgentID) []byte {
	out := make([]byte, 0, 68)
	out = append(out, initiator[:]...)
	out = append(out, responder[:]...)
	return out
}

// writeFrame writes a 4-byte BE length prefix then the frame body.
func writeFrame(w io.Writer, frame []byte) error {
	var hdr [lenPrefixBytes]byte
	binary.BigEndian.PutUint32(hdr[:], uint32(len(frame)))
	if _, err := w.Write(hdr[:]); err != nil {
		return err
	}
	_, err := w.Write(frame)
	return err
}

// readFrame reads one length-prefixed frame, rejecting a prefix greater than
// max BEFORE reading the body (size-first; core §6.3 — a hostile prefix can
// never drive a large allocation).
func readFrame(r io.Reader, max uint32) ([]byte, error) {
	var hdr [lenPrefixBytes]byte
	if _, err := io.ReadFull(r, hdr[:]); err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(hdr[:])
	if n > max {
		return nil, ErrTooLarge
	}
	buf := make([]byte, n)
	if _, err := io.ReadFull(r, buf); err != nil {
		return nil, err
	}
	return buf, nil
}

// ErrTooLarge signals a frame prefix above the negotiated ceiling.
var ErrTooLarge = errors.New("session: frame exceeds max-message-size")

// TCPTransport frames MAP envelopes over a net.Conn. TEST-HARNESS ONLY: raw
// TCP does not authenticate the static keys, so its exporter (the identity
// pair) is honest only by the harness pinning both peer identities — never a
// deployment transport (those are core §10's TLS/QUIC/libp2p bindings).
type TCPTransport struct {
	conn     net.Conn
	exporter []byte
	max      uint32
}

// NewTCPTransport wraps a connected net.Conn. `initiator`/`responder` are the
// two authenticated AgentIDs (stream-initiator first); `max` is the read
// ceiling (use the local HELLO's max-message-size).
func NewTCPTransport(conn net.Conn, initiator, responder identity.AgentID, max uint32) *TCPTransport {
	return &TCPTransport{conn: conn, exporter: Exporter(initiator, responder), max: max}
}

// Send writes one length-prefixed frame.
func (t *TCPTransport) Send(frame []byte) error {
	if err := writeFrame(t.conn, frame); err != nil {
		return ErrTransportClosed
	}
	return nil
}

// Recv reads one length-prefixed frame (size-first).
func (t *TCPTransport) Recv() ([]byte, error) {
	f, err := readFrame(t.conn, t.max)
	if err == ErrTooLarge {
		return nil, err
	}
	if err != nil {
		return nil, ErrTransportClosed
	}
	return f, nil
}

// Exporter returns the identity-pair keying material.
func (t *TCPTransport) Exporter() []byte { return t.exporter }

// InMemoryDuplex is a deterministic in-process pipe for unit tests.
type InMemoryDuplex struct {
	tx       chan []byte
	rx       chan []byte
	exporter []byte
}

// NewDuplexPair returns a connected (initiator, responder) pair sharing the
// identity-pair exporter.
func NewDuplexPair(initiator, responder identity.AgentID) (*InMemoryDuplex, *InMemoryDuplex) {
	ir := make(chan []byte, 16) // initiator -> responder
	ri := make(chan []byte, 16) // responder -> initiator
	exp := Exporter(initiator, responder)
	return &InMemoryDuplex{tx: ir, rx: ri, exporter: exp},
		&InMemoryDuplex{tx: ri, rx: ir, exporter: exp}
}

// Send delivers one frame to the peer.
func (d *InMemoryDuplex) Send(frame []byte) error {
	cp := append([]byte(nil), frame...)
	select {
	case d.tx <- cp:
		return nil
	default:
		// Buffer full — block-free fallback keeps unit tests from deadlocking
		// on a wedged peer; a real test drains promptly.
		d.tx <- cp
		return nil
	}
}

// Recv blocks for the next frame from the peer.
func (d *InMemoryDuplex) Recv() ([]byte, error) {
	f, ok := <-d.rx
	if !ok {
		return nil, ErrTransportClosed
	}
	return f, nil
}

// Exporter returns the identity-pair keying material.
func (d *InMemoryDuplex) Exporter() []byte { return d.exporter }
