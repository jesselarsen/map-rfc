package a2a

import (
	"math"
	"strings"
	"testing"
)

// bs is a single backslash, spelled so that no tool rewrites JSON escapes
// in this source file.
const bs = "\x5c"

func esc(h string) string { return bs + "u" + h }

// RFC 8785 §3.2.4 example (non-canonical numbers, escapes, member order)
// and its published canonical form.
func TestRFC8785Section324Example(t *testing.T) {
	input := `{"numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001], "string": "` +
		esc("20ac") + `$` + esc("000F") + esc("000a") + `A'` + esc("0042") + esc("0022") + esc("005c") +
		bs + bs + bs + `"` + bs + `/", "literals": [null, true, false]}`
	v, err := ParseJSON([]byte(input))
	if err != nil {
		t.Fatal(err)
	}
	got, err := Canonicalize(v)
	if err != nil {
		t.Fatal(err)
	}
	want := `{"literals":[null,true,false],"numbers":[333333333.3333333,1e+30,4.5,0.002,1e-27],"string":"€$` +
		esc("000f") + bs + `nA'B` + bs + `"` + bs + bs + bs + bs + bs + `"/"}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
}

// RFC 8785 §3.2.3: members sorted by UTF-16 code units (U+1F600 as a
// surrogate pair sorts before U+FB33).
func TestRFC8785UTF16MemberOrdering(t *testing.T) {
	// Code points spelled numerically: U+FB33 is an NFC composition
	// exclusion, so a literal would be decomposed by any normalizing editor.
	euro, oUml, emoji, dalet := string(rune(0x20ac)), string(rune(0xf6)), string(rune(0x1f600)), string(rune(0xfb33))
	input := `{"` + euro + `":"Euro Sign","` + bs + `r":"Carriage Return","` + dalet + `":"Hebrew","1":"One","` +
		emoji + `":"Emoji","` + esc("0080") + `":"Control","` + oUml + `":"Latin"}`
	v, err := ParseJSON([]byte(input))
	if err != nil {
		t.Fatal(err)
	}
	b, _ := Canonicalize(v)
	got := string(b)
	last := 0
	for _, k := range []string{bs + "r", "1", string(rune(0x80)), oUml, euro, emoji, dalet} {
		pos := strings.Index(got, `"`+k+`":`)
		if pos < last {
			t.Fatalf("key %q out of order in %s", k, got)
		}
		last = pos
	}
}

func TestES6Numbers(t *testing.T) {
	cases := []struct {
		x    float64
		want string
	}{
		{0, "0"}, {math.Copysign(0, -1), "0"}, {1, "1"}, {-1.5, "-1.5"}, {100, "100"},
		{1e21, "1e+21"}, {1e20, "100000000000000000000"},
		{123456789012345680000.0, "123456789012345680000"},
		{1e-6, "0.000001"}, {1e-7, "1e-7"}, {0.1, "0.1"}, {5e-324, "5e-324"},
		{1.7976931348623157e308, "1.7976931348623157e+308"},
		{9007199254740992.0, "9007199254740992"},
		{295147905179352830000.0, "295147905179352830000"},
	}
	for _, c := range cases {
		if got, err := ES6Number(c.x); err != nil || got != c.want {
			t.Errorf("%g: got %q, want %q", c.x, got, c.want)
		}
	}
	if _, err := ES6Number(math.NaN()); err == nil {
		t.Error("NaN accepted")
	}
	if _, err := ES6Number(math.Inf(1)); err == nil {
		t.Error("Inf accepted")
	}
}
