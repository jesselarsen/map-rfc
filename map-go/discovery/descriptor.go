// Package discovery implements the MAP descriptor format and DHT-backed
// publish/lookup (draft-map-discovery-01 §2, §5, §6), mirroring the Rust
// map-discovery crate. The wire encoding of Descriptor is kept byte-for-byte
// compatible with the Rust implementation so a Rust node can fetch a Go node's
// descriptor and vice versa.
package discovery

import (
	"encoding/hex"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// NUMShards is the discovery sharding factor (discovery §6).
const NUMShards = 256

// ValidCapURI checks map:cap:[a-z0-9][a-z0-9/._-]{0,126} (core §3).
func ValidCapURI(s string) bool {
	const prefix = "map:cap:"
	if len(s) < len(prefix)+1 || len(s) > len(prefix)+127 {
		return false
	}
	if s[:len(prefix)] != prefix {
		return false
	}
	rest := s[len(prefix):]
	for i, c := range rest {
		ok := (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')
		if i > 0 && (c == '/' || c == '.' || c == '_' || c == '-') {
			ok = true
		}
		if !ok {
			return false
		}
	}
	return true
}

// CapMatches implements prefix-segment capability matching (discovery §4):
// a provider of .../code matches a query for the parent prefix.
func CapMatches(providerCap, queryCap string) bool {
	if providerCap == queryCap {
		return true
	}
	rest := ""
	if len(queryCap) < len(providerCap) && providerCap[:len(queryCap)] == queryCap {
		rest = providerCap[len(queryCap):]
	}
	return rest != "" && rest[0] == '/'
}

// Descriptor is the minimal AgentDescriptor (discovery §2): fields 1-6 plus
// zones (12). Volatile facts (load, TTFT) and trust fields never appear here
// (rules R1/R4).
type Descriptor struct {
	Agent        identity.AgentID
	Seq          uint64
	Versions     []uint16
	Suites       []uint16
	Addrs        []string
	Capabilities []string
	// Zones served; defaults to [2] when absent (per spec).
	Zones []uint8
}

// ToValue encodes the descriptor to its canonical CBOR map (discovery §2),
// in field order 1,2,3,4,5,6,12 — matching the Rust map-discovery ordering so
// the two implementations produce identical bytes.
func (d *Descriptor) ToValue() (cbor.Value, error) {
	for _, c := range d.capabilities() {
		if !ValidCapURI(c) {
			return nil, fmt.Errorf("discovery: invalid cap uri %q", c)
		}
	}
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(d.Agent[:])},
		{K: cbor.Uint(2), V: cbor.Uint(d.Seq)},
		{K: cbor.Uint(3), V: u16Array(d.Versions)},
		{K: cbor.Uint(4), V: u16Array(d.Suites)},
		{K: cbor.Uint(5), V: textArray(d.Addrs)},
	}
	caps := make(cbor.Array, 0, len(d.Capabilities))
	for _, c := range d.Capabilities {
		caps = append(caps, cbor.Map{{K: cbor.Uint(1), V: cbor.Text(c)}})
	}
	m = append(m, cbor.Pair{K: cbor.Uint(6), V: caps})
	if len(d.Zones) > 0 {
		m = append(m, cbor.Pair{K: cbor.Uint(12), V: u8Array(d.Zones)})
	}
	return m, nil
}

func (d *Descriptor) capabilities() []string { return d.Capabilities }

// Encode serializes the descriptor to deterministic CBOR bytes.
func (d *Descriptor) Encode() ([]byte, error) {
	v, err := d.ToValue()
	if err != nil {
		return nil, err
	}
	return cbor.Encode(v), nil
}

// FromValue decodes a descriptor from its CBOR map representation.
func FromValue(v cbor.Value) (*Descriptor, error) {
	m, ok := v.(cbor.Map)
	if !ok {
		return nil, fmt.Errorf("discovery: descriptor is not a map")
	}
	agentRaw, ok := mapGet(m, 1)
	if !ok {
		return nil, fmt.Errorf("discovery: missing field 1")
	}
	ab, ok := agentRaw.(cbor.Bytes)
	if !ok {
		return nil, fmt.Errorf("discovery: field 1 not bytes")
	}
	agent, err := identity.Parse([]byte(ab))
	if err != nil {
		return nil, fmt.Errorf("discovery: field 1 bad agent id: %w", err)
	}
	seq, err := getUint(m, 2)
	if err != nil {
		return nil, err
	}
	versions, err := getU16s(m, 3)
	if err != nil {
		return nil, err
	}
	suites, err := getU16s(m, 4)
	if err != nil {
		return nil, err
	}
	addrs, err := getTexts(m, 5)
	if err != nil {
		return nil, err
	}
	caps, err := getCaps(m, 6)
	if err != nil {
		return nil, err
	}
	zones := []uint8{2}
	if z, ok := mapGet(m, 12); ok {
		arr, ok := z.(cbor.Array)
		if !ok {
			return nil, fmt.Errorf("discovery: field 12 not array")
		}
		zones = zones[:0]
		for _, e := range arr {
			u, ok := e.(cbor.Uint)
			if !ok || u > 3 {
				return nil, fmt.Errorf("discovery: field 12 bad zone")
			}
			zones = append(zones, uint8(u))
		}
	}
	return &Descriptor{
		Agent:        agent,
		Seq:          seq,
		Versions:     versions,
		Suites:       suites,
		Addrs:        addrs,
		Capabilities: caps,
		Zones:        zones,
	}, nil
}

// Decode parses a descriptor from CBOR bytes.
func Decode(b []byte) (*Descriptor, error) {
	v, err := cbor.Decode(b)
	if err != nil {
		return nil, fmt.Errorf("discovery: cbor: %w", err)
	}
	return FromValue(v)
}

// CID returns the descriptor's content CID (discovery §5): the SHA-256
// multihash of its encoded bytes.
func (d *Descriptor) CID() ([]byte, error) {
	enc, err := d.Encode()
	if err != nil {
		return nil, err
	}
	return MultihashSHA256(enc), nil
}

// CIDHex returns the hex form of the CID (handy for logging / manual exchange).
func (d *Descriptor) CIDHex() (string, error) {
	c, err := d.CID()
	if err != nil {
		return "", err
	}
	return hex.EncodeToString(c), nil
}

func u16Array(v []uint16) cbor.Array {
	a := make(cbor.Array, 0, len(v))
	for _, x := range v {
		a = append(a, cbor.Uint(x))
	}
	return a
}

func u8Array(v []uint8) cbor.Array {
	a := make(cbor.Array, 0, len(v))
	for _, x := range v {
		a = append(a, cbor.Uint(x))
	}
	return a
}

func textArray(v []string) cbor.Array {
	a := make(cbor.Array, 0, len(v))
	for _, x := range v {
		a = append(a, cbor.Text(x))
	}
	return a
}

func mapGet(m cbor.Map, k uint64) (cbor.Value, bool) {
	for _, p := range m {
		if u, ok := p.K.(cbor.Uint); ok && uint64(u) == k {
			return p.V, true
		}
	}
	return nil, false
}

func getUint(m cbor.Map, k uint64) (uint64, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return 0, fmt.Errorf("discovery: missing field %d", k)
	}
	u, ok := v.(cbor.Uint)
	if !ok {
		return 0, fmt.Errorf("discovery: field %d not uint", k)
	}
	return uint64(u), nil
}

func getU16s(m cbor.Map, k uint64) ([]uint16, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return nil, fmt.Errorf("discovery: missing field %d", k)
	}
	arr, ok := v.(cbor.Array)
	if !ok {
		return nil, fmt.Errorf("discovery: field %d not array", k)
	}
	out := make([]uint16, 0, len(arr))
	for _, e := range arr {
		u, ok := e.(cbor.Uint)
		if !ok || u > 0xFFFF {
			return nil, fmt.Errorf("discovery: field %d bad u16", k)
		}
		out = append(out, uint16(u))
	}
	return out, nil
}

func getTexts(m cbor.Map, k uint64) ([]string, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return nil, fmt.Errorf("discovery: missing field %d", k)
	}
	arr, ok := v.(cbor.Array)
	if !ok {
		return nil, fmt.Errorf("discovery: field %d not array", k)
	}
	out := make([]string, 0, len(arr))
	for _, e := range arr {
		t, ok := e.(cbor.Text)
		if !ok {
			return nil, fmt.Errorf("discovery: field %d not text", k)
		}
		out = append(out, string(t))
	}
	return out, nil
}

func getCaps(m cbor.Map, k uint64) ([]string, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return nil, fmt.Errorf("discovery: missing field %d", k)
	}
	arr, ok := v.(cbor.Array)
	if !ok {
		return nil, fmt.Errorf("discovery: field %d not array", k)
	}
	out := make([]string, 0, len(arr))
	for _, e := range arr {
		cm, ok := e.(cbor.Map)
		if !ok {
			return nil, fmt.Errorf("discovery: field %d entry not map", k)
		}
		cv, ok := mapGet(cm, 1)
		if !ok {
			return nil, fmt.Errorf("discovery: field %d entry missing cap", k)
		}
		ct, ok := cv.(cbor.Text)
		if !ok {
			return nil, fmt.Errorf("discovery: field %d entry not text", k)
		}
		cs := string(ct)
		if !ValidCapURI(cs) {
			return nil, fmt.Errorf("discovery: field %d invalid cap uri %q", k, cs)
		}
		out = append(out, cs)
	}
	return out, nil
}
