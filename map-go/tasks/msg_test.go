package tasks

import (
	"bytes"
	"encoding/hex"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

func hex32(b [32]byte) string { return hex.EncodeToString(b[:]) }

func mustDecode(s string) [32]byte {
	b, err := hex.DecodeString(s)
	if err != nil || len(b) != 32 {
		panic("bad hex")
	}
	var out [32]byte
	copy(out[:], b)
	return out
}

// roundtripAllMessages mirrors the Rust map-tasks msg.rs roundtrip coverage.
// Equality is checked by re-encoding (structs carry interface/slice fields that
// are not directly comparable), which is exactly the wire-byte property we care
// about for interop.
func TestMessageRoundtrip(t *testing.T) {
	var task TaskID
	for i := range task {
		task[i] = 7
	}
	eq := func(orig cbor.Value, dec func(cbor.Value) (cbor.Value, error)) bool {
		got, err := dec(orig)
		if err != nil {
			return false
		}
		return bytes.Equal(cbor.Encode(got), cbor.Encode(orig))
	}

	offer := (Offer{Task: task, Capability: "map:cap:x", Constraints: TaskConstraints{TTFTMs: 500, hasTTFT: true}, ExpiryMs: 1000, hasExpiry: true}).ToValue()
	if !eq(offer, func(v cbor.Value) (cbor.Value, error) { var x Offer; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("offer roundtrip")
	}

	quote := (Quote{Task: task, TTFTMs: 400, ExpiryMs: 2000}).ToValue()
	if !eq(quote, func(v cbor.Value) (cbor.Value, error) { var x Quote; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("quote roundtrip")
	}

	assign := (Assign{
		Task: task, Capability: "map:cap:x", Constraints: TaskConstraints{TTFTMs: 500, hasTTFT: true},
		Input: NewInputInline([]byte("hi")), Flow: FlowInit{MsgCredits: 64, ByteCredits: 1 << 20},
	}).ToValue()
	if !eq(assign, func(v cbor.Value) (cbor.Value, error) { var x Assign; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("assign roundtrip")
	}

	if _, err := (Accept{}).decoded(Accept{}.ToValue()); err != nil {
		t.Fatalf("accept roundtrip: %v", err)
	}

	reject := (Reject{Code: 3000}).ToValue()
	if !eq(reject, func(v cbor.Value) (cbor.Value, error) { var x Reject; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("reject roundtrip")
	}

	chunk := (ChunkMsg{Task: task, Seq: 3, Kind: ChunkOutputText, Content: []byte("out"), Fin: true}).ToValue()
	if !eq(chunk, func(v cbor.Value) (cbor.Value, error) { var x ChunkMsg; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("chunk roundtrip")
	}
	chunk0 := (ChunkMsg{Task: task, Seq: 0, Kind: ChunkUsage, Content: nil, Fin: false}).ToValue()
	if !eq(chunk0, func(v cbor.Value) (cbor.Value, error) { var x ChunkMsg; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("chunk0 roundtrip")
	}

	complete := (Complete{Task: task, Outcome: 0, TranscriptRoot: mustDecode(crossImplRoots.single), Usage: Usage{TokensIn: 1, TokensOut: 2, TTFTMs: 3, WallMs: 4}}).ToValue()
	if !eq(complete, func(v cbor.Value) (cbor.Value, error) { var x Complete; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("complete roundtrip")
	}

	cancel := (Cancel{Task: task, Code: 5001}).ToValue()
	if !eq(cancel, func(v cbor.Value) (cbor.Value, error) { var x Cancel; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("cancel roundtrip")
	}
	fc := (FlowCredit{Scope: task, MsgCredits: 64, ByteCredits: 1024}).ToValue()
	if !eq(fc, func(v cbor.Value) (cbor.Value, error) { var x FlowCredit; e := x.FromValue(v); return x.ToValue(), e }) {
		t.Fatal("flowcredit roundtrip")
	}
}

func TestInputSpecRoundtripAndRejects(t *testing.T) {
	inline := NewInputInline([]byte("prompt")).ToValue()
	if _, err := (InputSpec{}).decoded(inline); err != nil {
		t.Fatalf("inline roundtrip: %v", err)
	}
	mh := func(b byte) []byte { return append(append([]byte{0x12, 0x20}, b), make([]byte, 31)...) }
	blocks := InputSpec{Blocks: [][]byte{mh(0xaa), mh(0xbb)}, hasBlocks: true}.ToValue()
	if _, err := (InputSpec{}).decoded(blocks); err != nil {
		t.Fatalf("blocks roundtrip: %v", err)
	}
	ctx := InputSpec{Context: []cbor.Value{cbor.Map{{K: cbor.Uint(1), V: cbor.Text("ref")}}}, hasContext: true}.ToValue()
	if _, err := (InputSpec{}).decoded(ctx); err != nil {
		t.Fatalf("context roundtrip: %v", err)
	}
	// Empty / malformed rejected.
	if _, err := (InputSpec{}).decoded(cbor.Map{}); err == nil {
		t.Fatal("empty InputSpec should be rejected")
	}
	if _, err := (InputSpec{}).decoded(cbor.Map{{K: cbor.Uint(2), V: cbor.Array{}}}); err == nil {
		t.Fatal("empty blocks array should be rejected")
	}
}

func TestCompleteRejectsNonMultihashRoot(t *testing.T) {
	var task TaskID
	for i := range task {
		task[i] = 0
	}
	c := Complete{Task: task, Outcome: 0, TranscriptRoot: [32]byte{}, Usage: Usage{}}
	v := c.ToValue()
	// Corrupt key 3 to a bare 32-byte string.
	if m, ok := v.(cbor.Map); ok {
		for i, p := range m {
			if u, ok := p.K.(cbor.Uint); ok && u == 3 {
				m[i].V = cbor.Bytes(make([]byte, 32))
			}
		}
	}
	if _, err := (Complete{}).decoded(v); err == nil {
		t.Fatal("Complete with non-multihash root should be rejected")
	}
}

func TestFlowCreditEnforced(t *testing.T) {
	f := NewFlowAccount()
	if err := f.TryConsume(1); err == nil {
		t.Fatal("empty account should reject")
	}
	f.Grant(2, 10)
	if err := f.TryConsume(6); err != nil {
		t.Fatalf("first consume should succeed: %v", err)
	}
	if err := f.TryConsume(6); err == nil {
		t.Fatal("byte credit should be exhausted")
	}
	if err := f.TryConsume(4); err != nil {
		t.Fatalf("4 bytes remain, should succeed: %v", err)
	}
	if err := f.TryConsume(0); err == nil {
		t.Fatal("no message credit left should reject")
	}
}

// decoded helpers for roundtrip tests.
func (o Offer) decoded(v cbor.Value) (Offer, error) {
	var x Offer
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (q Quote) decoded(v cbor.Value) (Quote, error) {
	var x Quote
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (a Assign) decoded(v cbor.Value) (Assign, error) {
	var x Assign
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (a Accept) decoded(v cbor.Value) (Accept, error) {
	if err := a.FromValue(v); err != nil {
		return a, err
	}
	return a, nil
}

func (r Reject) decoded(v cbor.Value) (Reject, error) {
	var x Reject
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (c ChunkMsg) decoded(v cbor.Value) (ChunkMsg, error) {
	var x ChunkMsg
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (c Complete) decoded(v cbor.Value) (Complete, error) {
	var x Complete
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (c Cancel) decoded(v cbor.Value) (Cancel, error) {
	var x Cancel
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (f FlowCredit) decoded(v cbor.Value) (FlowCredit, error) {
	var x FlowCredit
	if err := f.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}

func (i InputSpec) decoded(v cbor.Value) (InputSpec, error) {
	var x InputSpec
	if err := x.FromValue(v); err != nil {
		return x, err
	}
	return x, nil
}
