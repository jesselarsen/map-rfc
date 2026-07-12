package cbor

import (
	"bytes"
	"encoding/hex"
	"errors"
	"testing"
)

func h(s string) []byte {
	b, err := hex.DecodeString(s)
	if err != nil {
		panic(err)
	}
	return b
}

// Encoding spot checks against RFC 8949 §4.2.1 shortest-form rules and the
// vector file's byte patterns.
func TestEncodeCanonical(t *testing.T) {
	cases := []struct {
		v    Value
		want string
	}{
		{Uint(0), "00"},
		{Uint(23), "17"},
		{Uint(24), "1818"},
		{Uint(255), "18ff"},
		{Uint(256), "190100"}, // Version 1.0
		{Uint(65535), "19ffff"},
		{Uint(65536), "1a00010000"},
		{Uint(262144), "1a00040000"},                // TV-001 max-message-size
		{Uint(1780000000000), "1b0000019e70448800"}, // 64-bit timestamp
		{Int(-8), "27"},                             // COSE alg EdDSA
		{Int(-6), "25"},                             // tag-4 exponent
		{Bool(false), "f4"},
		{Bool(true), "f5"},
		{Bytes{}, "40"},
		{Text("map cb"), "666d6170206362"},
		{Array{}, "80"},
		{Map{}, "a0"},
		{Tag{Num: 4, Content: Array{Int(-6), Uint(4200000)}}, "c482251a00401640"}, // pay §3 Amount
	}
	for _, c := range cases {
		if got := Encode(c.v); !bytes.Equal(got, h(c.want)) {
			t.Errorf("Encode(%v) = %x, want %s", c.v, got, c.want)
		}
	}
}

// RFC 8949 §4.2.1: map keys sort bytewise on their ENCODED form — mixed
// uint/text keys exercise this (TASK-015 §9): enc(1)=0x01 < enc(256)=0x19…
// < enc("z")=0x617a.
func TestMapKeySortMixed(t *testing.T) {
	m := Map{
		{K: Text("z"), V: Uint(1)},
		{K: Uint(256), V: Uint(2)},
		{K: Uint(1), V: Uint(3)},
	}
	want := h("a3" + "0103" + "19010002" + "617a01")
	if got := Encode(m); !bytes.Equal(got, want) {
		t.Errorf("mixed-key map = %x, want %x", got, want)
	}
	// And the strict decoder accepts it back, round-tripping identically.
	v, err := Decode(want)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(Encode(v), want) {
		t.Error("round trip not stable")
	}
}

func TestDecodeStrictRejects(t *testing.T) {
	cases := []struct{ name, in string }{
		{"non-minimal uint (ai=24, arg<24)", "1817"},
		{"non-minimal uint (ai=25, arg<256)", "1900ff"},
		{"non-minimal uint (ai=26)", "1a0000ffff"},
		{"non-minimal uint (ai=27)", "1b00000000ffffffff"},
		{"non-minimal bytes length", "5818" + "00010203040506070809000102030405060708090001020304"},
		{"indefinite-length map", "bf0101ff"},
		{"indefinite-length array", "9f01ff"},
		{"indefinite-length bytes", "5f4101ff"},
		{"reserved ai 28", "1c"},
		{"break outside container", "ff"},
		{"float16", "f93c00"},
		{"float64", "fb3ff0000000000000"},
		{"null", "f6"},
		{"undefined", "f7"},
		{"simple 16", "f0"},
		{"unsorted map keys", "a202010101"},
		{"duplicate map keys", "a201010102"},
		{"trailing bytes", "0000"},
		{"truncated", "1b00"},
		{"invalid utf-8 text", "61ff"},
	}
	for _, c := range cases {
		in := h(c.in)
		if _, err := Decode(in); !errors.Is(err, ErrNonCanonical) {
			t.Errorf("%s: Decode(%s) err = %v, want ErrNonCanonical", c.name, c.in, err)
		}
	}
}

func TestDecodeAcceptsCanonical(t *testing.T) {
	// The TV-001 payload (frozen) is canonical and must round-trip.
	payload := h("a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000")
	v, err := Decode(payload)
	if err != nil {
		t.Fatalf("TV-001 payload rejected: %v", err)
	}
	if !bytes.Equal(Encode(v), payload) {
		t.Error("TV-001 payload round trip not byte-stable")
	}
	// Sorted 2-key map is fine.
	if _, err := Decode(h("a201010202")); err != nil {
		t.Errorf("sorted map rejected: %v", err)
	}
}
