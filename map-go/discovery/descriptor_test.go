package discovery

import (
	"bytes"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/identity"
)

func mkDesc(t *testing.T) *Descriptor {
	t.Helper()
	var aid identity.AgentID
	aid[0], aid[1] = 0xED, 0x01
	for i := 2; i < 34; i++ {
		aid[i] = byte(i)
	}
	return &Descriptor{
		Agent:        aid,
		Seq:          7,
		Versions:     []uint16{0x0100},
		Suites:       []uint16{1},
		Addrs:        []string{"/ip4/127.0.0.1/udp/4001/quic-v1"},
		Capabilities: []string{"map:cap:demo/excuse"},
		Zones:        []uint8{2},
	}
}

func TestDescriptorRoundTrip(t *testing.T) {
	d := mkDesc(t)
	enc, err := d.Encode()
	if err != nil {
		t.Fatalf("encode: %v", err)
	}
	// Field order check: 1 (agent) must precede 2 (seq) etc. We re-decode and
	// compare structurally (the canonical ordering guarantees byte stability).
	dec, err := Decode(enc)
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	if dec.Agent != d.Agent || dec.Seq != d.Seq {
		t.Fatalf("core mismatch: %+v vs %+v", dec, d)
	}
	if len(dec.Versions) != 1 || dec.Versions[0] != 0x0100 {
		t.Fatalf("versions mismatch: %v", dec.Versions)
	}
	if len(dec.Capabilities) != 1 || dec.Capabilities[0] != "map:cap:demo/excuse" {
		t.Fatalf("caps mismatch: %v", dec.Capabilities)
	}
	if len(dec.Zones) != 1 || dec.Zones[0] != 2 {
		t.Fatalf("zones mismatch: %v", dec.Zones)
	}
	// Re-encode must be deterministic (no map-key reordering).
	enc2, _ := dec.Encode()
	if !bytes.Equal(enc, enc2) {
		t.Fatalf("non-deterministic encoding")
	}
}

func TestValidCapURI(t *testing.T) {
	ok := []string{"map:cap:demo/excuse", "map:cap:code", "map:cap:a"}
	bad := []string{"map:cap:", "MAP:cap:x", "map:cap:/x", "notcap:x", "map:cap:" + string(make([]byte, 0))}
	for _, s := range ok {
		if !ValidCapURI(s) {
			t.Fatalf("expected valid: %q", s)
		}
	}
	for _, s := range bad {
		if ValidCapURI(s) {
			t.Fatalf("expected invalid: %q", s)
		}
	}
}

func TestCapMatches(t *testing.T) {
	if !CapMatches("map:cap:code/rust", "map:cap:code") {
		t.Fatalf("prefix match failed")
	}
	if !CapMatches("map:cap:code", "map:cap:code") {
		t.Fatalf("exact equality must match")
	}
	if CapMatches("map:cap:codex", "map:cap:code") {
		t.Fatalf("non-prefix sibling should not match")
	}
}

func TestShardDeterministic(t *testing.T) {
	d := mkDesc(t)
	s1 := Shard(d.Agent)
	s2 := Shard(d.Agent)
	if s1 != s2 {
		t.Fatalf("shard not deterministic")
	}
}

func TestCIDHex(t *testing.T) {
	d := mkDesc(t)
	c, err := d.CID()
	if err != nil {
		t.Fatalf("cid: %v", err)
	}
	// 0x12 0x20 prefix for SHA-256 multihash.
	if len(c) != 34 || c[0] != 0x12 || c[1] != 0x20 {
		t.Fatalf("bad cid prefix: %x", c[:2])
	}
	h, err := d.CIDHex()
	if err != nil || len(h) != 68 {
		t.Fatalf("bad cid hex: %q", h)
	}
}
