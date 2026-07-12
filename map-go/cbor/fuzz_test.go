package cbor

import (
	"bytes"
	"encoding/hex"
	"testing"
)

// FuzzDecode: the strict decoder must never panic, and anything it
// accepts must re-encode to the identical bytes (accept ⇒ canonical),
// per TASK-015 §16. Seed corpus from the frozen vectors.
func FuzzDecode(f *testing.F) {
	seeds := []string{
		// TV-001 complete envelope.
		"d2845828a20127045822ed0165c883140cbc33dcf60c2fee834eb54fc90dcc2a9cb724472f0ff287a9bface1a05835a50019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000584010307036a1d861d52cd830f729108aeda08654d455a5de07fe98402c9504099f84a77527dd031731ab52b7fd902e7ee125a2ff7d4740767a7e5b158b30840d0f",
		// TV-010 payload (tag 4, nested maps/arrays, empty bstr).
		"a5001901000119040002500a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a031b0000019e704488000aa5015010101010101010101010101010101010020003a100826b69736f343231373a555344c482251a004016400581a201717061793a6372656469742d6c65646765720240061b0000019e704dafc0",
		// NV-003 non-canonical payload (must be rejected, never panic).
		"bf0019010001010250000102030405060708090a0b0c0d0e0f031b0000019e704488000aa4018119010002810103a0041a00040000ff",
		"a202010101", // unsorted
		"f93c00",     // float
		"",
	}
	for _, s := range seeds {
		b, err := hex.DecodeString(s)
		if err != nil {
			f.Fatal(err)
		}
		f.Add(b)
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		v, err := Decode(data)
		if err != nil {
			return
		}
		if !bytes.Equal(Encode(v), data) {
			t.Fatalf("accepted non-canonical input: %x", data)
		}
	})
}
