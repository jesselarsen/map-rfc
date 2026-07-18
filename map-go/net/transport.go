// Package net implements the MAP libp2p/QUIC transport binding (TASK-012
// equivalent for Go), mirroring the Rust map-net crate. It provides MapNode —
// a blocking API over a background libp2p host running the /map/1.0.0 protocol
// over QUIC, with a Kademlia DHT for descriptor publication and lookup.
//
// Scope (per the Rust map-net precedent): LAN/loopback grade. NAT traversal
// (AutoNAT/relay/DCUtR), discovery privacy, and anti-eclipse defenses are out
// of scope.
package net

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"

	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/session"
)

// mapProtocol is the MAP application-layer protocol carried over libp2p
// substreams (core §10). Matches the Rust MAP_PROTOCOL.
const mapProtocol = "/map/1.0.0"

// lenPrefixBytes is the 4-byte big-endian frame length prefix (core §10).
const lenPrefixBytes = 4

// ErrTooLarge signals a frame prefix above the negotiated ceiling.
var ErrTooLarge = fmt.Errorf("net: frame exceeds max-message-size")

// Libp2pTransport bridges a libp2p substream (an io.ReadWriteCloser) onto the
// synchronous session.Transport interface, framing one MAP envelope per frame
// with a 4-byte big-endian length prefix (identical wire framing to the Rust
// Libp2pTransport and the Go TCPTransport). The exporter carries the
// authenticated peer identities (initiator || responder), matching Rust's
// identity-pair exporter (FR-4): libp2p's Noise handshake authenticates the
// PeerID, and PeerID ≡ AgentID (FR-3) makes the peer identity trustworthy.
type Libp2pTransport struct {
	stream   io.ReadWriteCloser
	exporter []byte
	max      uint32
}

// NewLibp2pTransport wraps a stream. initiator/responder are the two
// authenticated AgentIDs (stream-initiator first); max is the read ceiling (the
// local HELLO's max-message-size).
func NewLibp2pTransport(stream io.ReadWriteCloser, initiator, responder identity.AgentID, max uint32) *Libp2pTransport {
	return &Libp2pTransport{
		stream:   stream,
		exporter: session.Exporter(initiator, responder),
		max:      max,
	}
}

// Send writes one length-prefixed frame.
func (t *Libp2pTransport) Send(frame []byte) error {
	var hdr [lenPrefixBytes]byte
	binary.BigEndian.PutUint32(hdr[:], uint32(len(frame)))
	if _, err := t.stream.Write(hdr[:]); err != nil {
		return err
	}
	if _, err := t.stream.Write(frame); err != nil {
		return err
	}
	return nil
}

// Recv reads one length-prefixed frame, rejecting an over-large prefix before
// allocating the body (size-first; core §6.3).
func (t *Libp2pTransport) Recv() ([]byte, error) {
	var hdr [lenPrefixBytes]byte
	if _, err := io.ReadFull(t.stream, hdr[:]); err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(hdr[:])
	if n > t.max {
		return nil, ErrTooLarge
	}
	buf := make([]byte, n)
	if _, err := io.ReadFull(t.stream, buf); err != nil {
		return nil, err
	}
	return buf, nil
}

// Exporter returns the identity-pair keying material (FR-4).
func (t *Libp2pTransport) Exporter() []byte { return t.exporter }

// ensure net satisfies the session.Transport interface at compile time.
var _ session.Transport = (*Libp2pTransport)(nil)

// placeholder to keep net import used if a build needs it during iteration.
var _ = net.Listen
