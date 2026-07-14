// Package session implements the MAP session layer (draft-map-core-01 §6/§7):
// HELLO/HELLO_ACK negotiation, transcript hash + channel binding, anti-replay
// and sequence tracking, length-prefixed framing, and the established-session
// send/receive pipeline. It is the wire twin of the Rust map-session crate.
package session

import (
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// MinMaxMessageSize is the floor a HELLO may advertise (core §6.3).
const MinMaxMessageSize = 65536

// DefaultMaxMessageSize is the recommended cap (core §6.3).
const DefaultMaxMessageSize = 262144

// Ext is one negotiated extension: (key, max-supported-version).
type Ext struct {
	Key, Version uint64
}

// Hello is the initiator's offer (core §6.1). Fields 1/2 are LISTS.
type Hello struct {
	Versions       []uint16 // 1
	Suites         []uint16 // 2
	Extensions     []Ext    // 3 (map on the wire; always emitted, even empty)
	MaxMessageSize uint32   // 4 (>= MinMaxMessageSize)
	MaxTasks       *uint64  // 5 (optional)
	Zones          []uint8  // 7 (optional; omitted when empty; values <= 3)
}

func extMap(exts []Ext) cbor.Value {
	m := make(cbor.Map, len(exts))
	for i, e := range exts {
		m[i] = cbor.Pair{K: cbor.Uint(e.Key), V: cbor.Uint(e.Version)}
	}
	return m
}

func parseExtMap(v cbor.Value) ([]Ext, error) {
	m, ok := v.(cbor.Map)
	if !ok {
		return nil, fmt.Errorf("extensions must be a map")
	}
	out := make([]Ext, 0, len(m))
	for _, p := range m {
		k, ok1 := p.K.(cbor.Uint)
		val, ok2 := p.V.(cbor.Uint)
		if !ok1 || !ok2 {
			return nil, fmt.Errorf("extension entries must be uint=>uint")
		}
		out = append(out, Ext{Key: uint64(k), Version: uint64(val)})
	}
	return out, nil
}

// ToValue encodes the HELLO content (lists for keys 1/2).
func (h Hello) ToValue() cbor.Value {
	versions := make(cbor.Array, len(h.Versions))
	for i, v := range h.Versions {
		versions[i] = cbor.Uint(v)
	}
	suites := make(cbor.Array, len(h.Suites))
	for i, s := range h.Suites {
		suites[i] = cbor.Uint(s)
	}
	m := cbor.Map{
		{K: cbor.Uint(1), V: versions},
		{K: cbor.Uint(2), V: suites},
		{K: cbor.Uint(3), V: extMap(h.Extensions)},
		{K: cbor.Uint(4), V: cbor.Uint(uint64(h.MaxMessageSize))},
	}
	if h.MaxTasks != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: cbor.Uint(*h.MaxTasks)})
	}
	if len(h.Zones) > 0 {
		zones := make(cbor.Array, len(h.Zones))
		for i, z := range h.Zones {
			zones[i] = cbor.Uint(z)
		}
		m = append(m, cbor.Pair{K: cbor.Uint(7), V: zones})
	}
	return m
}

func uint16List(v cbor.Value, key uint64) ([]uint16, error) {
	arr, ok := v.(cbor.Array)
	if !ok {
		return nil, fmt.Errorf("key %d must be an array", key)
	}
	out := make([]uint16, len(arr))
	for i, e := range arr {
		u, ok := e.(cbor.Uint)
		if !ok || uint64(u) > 0xffff {
			return nil, fmt.Errorf("key %d entries must be uint16", key)
		}
		out[i] = uint16(u)
	}
	return out, nil
}

// ParseHello decodes HELLO content, rejecting a max-message-size below the floor.
func ParseHello(v cbor.Value) (Hello, error) {
	var h Hello
	get := func(k uint64) cbor.Value {
		if m, ok := v.(cbor.Map); ok {
			for _, p := range m {
				if uk, ok := p.K.(cbor.Uint); ok && uint64(uk) == k {
					return p.V
				}
			}
		}
		return nil
	}
	v1, v2, v3, v4 := get(1), get(2), get(3), get(4)
	if v1 == nil || v2 == nil || v3 == nil || v4 == nil {
		return h, fmt.Errorf("HELLO missing a required key (1,2,3,4)")
	}
	var err error
	if h.Versions, err = uint16List(v1, 1); err != nil {
		return h, err
	}
	if h.Suites, err = uint16List(v2, 2); err != nil {
		return h, err
	}
	if h.Extensions, err = parseExtMap(v3); err != nil {
		return h, err
	}
	mms, ok := v4.(cbor.Uint)
	if !ok || uint64(mms) > 0xffffffff {
		return h, fmt.Errorf("HELLO max-message-size must be uint32")
	}
	if uint64(mms) < MinMaxMessageSize {
		return h, fmt.Errorf("HELLO max-message-size %d below floor %d", mms, MinMaxMessageSize)
	}
	h.MaxMessageSize = uint32(mms)
	if v5 := get(5); v5 != nil {
		u, ok := v5.(cbor.Uint)
		if !ok {
			return h, fmt.Errorf("HELLO max-tasks must be uint")
		}
		mt := uint64(u)
		h.MaxTasks = &mt
	}
	if v7 := get(7); v7 != nil {
		arr, ok := v7.(cbor.Array)
		if !ok {
			return h, fmt.Errorf("HELLO zones must be an array")
		}
		for _, e := range arr {
			z, ok := e.(cbor.Uint)
			if !ok || uint64(z) > 3 {
				return h, fmt.Errorf("HELLO zone must be uint <= 3")
			}
			h.Zones = append(h.Zones, uint8(z))
		}
	}
	return h, nil
}

// HelloAck is the responder's answer (core §6.1). Fields 1/2 are SINGLETONS.
type HelloAck struct {
	Version        uint16  // 1
	Suite          uint16  // 2
	Extensions     []Ext   // 3 (always emitted)
	MaxMessageSize uint32  // 4 (the RESPONDER'S OWN max, not the negotiated min)
	MaxTasks       *uint64 // 5 (optional)
}

// ToValue encodes the HELLO_ACK content (singletons for keys 1/2).
func (a HelloAck) ToValue() cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(a.Version)},
		{K: cbor.Uint(2), V: cbor.Uint(a.Suite)},
		{K: cbor.Uint(3), V: extMap(a.Extensions)},
		{K: cbor.Uint(4), V: cbor.Uint(uint64(a.MaxMessageSize))},
	}
	if a.MaxTasks != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: cbor.Uint(*a.MaxTasks)})
	}
	return m
}

// ParseHelloAck decodes HELLO_ACK content.
func ParseHelloAck(v cbor.Value) (HelloAck, error) {
	var a HelloAck
	get := func(k uint64) cbor.Value {
		if m, ok := v.(cbor.Map); ok {
			for _, p := range m {
				if uk, ok := p.K.(cbor.Uint); ok && uint64(uk) == k {
					return p.V
				}
			}
		}
		return nil
	}
	v1, v2, v3, v4 := get(1), get(2), get(3), get(4)
	if v1 == nil || v2 == nil || v3 == nil || v4 == nil {
		return a, fmt.Errorf("HELLO_ACK missing a required key (1,2,3,4)")
	}
	ver, ok := v1.(cbor.Uint)
	if !ok || uint64(ver) > 0xffff {
		return a, fmt.Errorf("HELLO_ACK version must be uint16")
	}
	suite, ok := v2.(cbor.Uint)
	if !ok || uint64(suite) > 0xffff {
		return a, fmt.Errorf("HELLO_ACK suite must be uint16")
	}
	a.Version, a.Suite = uint16(ver), uint16(suite)
	var err error
	if a.Extensions, err = parseExtMap(v3); err != nil {
		return a, err
	}
	mms, ok := v4.(cbor.Uint)
	if !ok || uint64(mms) > 0xffffffff {
		return a, fmt.Errorf("HELLO_ACK max-message-size must be uint32")
	}
	a.MaxMessageSize = uint32(mms)
	if v5 := get(5); v5 != nil {
		u, ok := v5.(cbor.Uint)
		if !ok {
			return a, fmt.Errorf("HELLO_ACK max-tasks must be uint")
		}
		mt := uint64(u)
		a.MaxTasks = &mt
	}
	return a, nil
}

// AsHello views the ACK as a one-element Hello so the initiator can re-run
// Negotiate to validate the responder's choice sits within its own offer.
func (a HelloAck) AsHello() Hello {
	return Hello{
		Versions:       []uint16{a.Version},
		Suites:         []uint16{a.Suite},
		Extensions:     a.Extensions,
		MaxMessageSize: a.MaxMessageSize,
		MaxTasks:       a.MaxTasks,
	}
}

// Negotiated is the outcome of Negotiate.
type Negotiated struct {
	Version        uint16
	Suite          uint16
	Extensions     []Ext
	MaxMessageSize uint32 // min of both peers
}

// Ack builds the HELLO_ACK for this result. `responderMax`/`responderMaxTasks`
// are the RESPONDER'S OWN limits (core §6.1 — key 4 is not the negotiated min).
func (n Negotiated) Ack(responderMax uint32, responderMaxTasks *uint64) HelloAck {
	return HelloAck{
		Version:        n.Version,
		Suite:          n.Suite,
		Extensions:     n.Extensions,
		MaxMessageSize: responderMax,
		MaxTasks:       responderMaxTasks,
	}
}

// Negotiate selects the highest mutual version, the FIRST mutual suite in the
// INITIATOR's preference order, the extension intersection at min(offered,
// local) version, and max-message-size = min of both (core §6.1).
func Negotiate(initiator, local Hello) (Negotiated, error) {
	var n Negotiated
	version, ok := highestMutual(initiator.Versions, local.Versions)
	if !ok {
		return n, fmt.Errorf("no common protocol version")
	}
	suite, ok := firstMutual(initiator.Suites, local.Suites)
	if !ok {
		return n, fmt.Errorf("no common cryptographic suite")
	}
	n.Version, n.Suite = version, suite
	for _, ie := range initiator.Extensions {
		for _, le := range local.Extensions {
			if ie.Key == le.Key {
				v := ie.Version
				if le.Version < v {
					v = le.Version
				}
				n.Extensions = append(n.Extensions, Ext{Key: ie.Key, Version: v})
				break
			}
		}
	}
	n.MaxMessageSize = initiator.MaxMessageSize
	if local.MaxMessageSize < n.MaxMessageSize {
		n.MaxMessageSize = local.MaxMessageSize
	}
	return n, nil
}

func highestMutual(offered, local []uint16) (uint16, bool) {
	best, found := uint16(0), false
	for _, v := range offered {
		if contains16(local, v) && v >= best {
			best, found = v, true
		}
	}
	return best, found
}

// firstMutual returns the first entry of `offered` also in `local` — the
// initiator's preference order governs (core §6.1).
func firstMutual(offered, local []uint16) (uint16, bool) {
	for _, v := range offered {
		if contains16(local, v) {
			return v, true
		}
	}
	return 0, false
}

func contains16(xs []uint16, x uint16) bool {
	for _, v := range xs {
		if v == x {
			return true
		}
	}
	return false
}
