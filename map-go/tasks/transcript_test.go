package tasks

import (
	"testing"
)

// crossImplRoots are transcript roots computed by an INDEPENDENT reference
// (the spec rule in draft-map-tasks-01 §8.1, reproduced as a separate Python
// oracle) and by the Rust reference impl. Both implementations MUST commit to
// exactly these roots; this guards the interop-critical transcript commitment
// against CBOR-determinism drift. If a future cbor change perturbs encoding,
// this test fails loudly instead of silently breaking Rust<->Go interop.
var crossImplRoots = struct {
	empty  string
	single string
	two    string
	three  string
	mixed  string
	amb1   string
}{
	empty:  "085edad400785fca7e7e90b1fac4beb776fc2beee5aa24352d5f39b5d57efcad",
	single: "7b591b5f22992cd5915f187464047599831ee17bc8fe76e9025bbe454915c35f",
	two:    "8929922b0e868184825284ed2bdde3c5e402488584873d2f8bad110926f2d87d",
	three:  "c29f5fb0b280e05f330b03369eefd0dc5fdde944b11c2688d773869106b10d66",
	mixed:  "9a9c956bb6ced6eb9f2f46c60331ba1e0b53f770cbec9a39c99786d617c7a515",
	amb1:   "da14441bde0f5670b66efc01491ff404f25c96489c23d101abe1e4d690ebb1d3",
}

func hexroot(t *testing.T, chunks ...Chunk) string {
	t.Helper()
	b := NewTranscript()
	for _, c := range chunks {
		b.Push(c)
	}
	return hex32(b.Root())
}

func TestTranscriptCrossImplVectors(t *testing.T) {
	if got := hexroot(t); got != crossImplRoots.empty {
		t.Fatalf("empty root: got %s want %s", got, crossImplRoots.empty)
	}
	if got := hexroot(t, Chunk{Seq: 0, Kind: ChunkOutputText, Content: []byte("hello")}); got != crossImplRoots.single {
		t.Fatalf("single root: got %s want %s", got, crossImplRoots.single)
	}
	if got := hexroot(t,
		Chunk{Seq: 0, Kind: ChunkOutputText, Content: []byte("hello")},
		Chunk{Seq: 1, Kind: ChunkOutputText, Content: []byte("world")},
	); got != crossImplRoots.two {
		t.Fatalf("two root: got %s want %s", got, crossImplRoots.two)
	}
	if got := hexroot(t,
		Chunk{Seq: 0, Kind: ChunkOutputText, Content: []byte("a")},
		Chunk{Seq: 1, Kind: ChunkOutputText, Content: []byte("b")},
		Chunk{Seq: 2, Kind: ChunkOutputText, Content: []byte("c")},
	); got != crossImplRoots.three {
		t.Fatalf("three root: got %s want %s", got, crossImplRoots.three)
	}
	if got := hexroot(t,
		Chunk{Seq: 0, Kind: ChunkOutputText, Content: []byte("hello ")},
		Chunk{Seq: 1, Kind: ChunkLog, Content: []byte("ignored")},
		Chunk{Seq: 2, Kind: ChunkOutputText, Content: []byte("world")},
	); got != crossImplRoots.mixed {
		t.Fatalf("mixed root: got %s want %s", got, crossImplRoots.mixed)
	}
	if got := hexroot(t, Chunk{Seq: 0x01, Kind: ChunkOutputByte, Content: []byte{0x02, 0x03}}); got != crossImplRoots.amb1 {
		t.Fatalf("amb1 root: got %s want %s", got, crossImplRoots.amb1)
	}
}

func TestTranscriptOrderAndContentSensitive(t *testing.T) {
	a := NewTranscript()
	a.Push(Chunk{Seq: 0, Kind: ChunkOutputText, Content: []byte("hello ")})
	a.Push(Chunk{Seq: 1, Kind: ChunkLog, Content: []byte("ignored")})
	a.Push(Chunk{Seq: 2, Kind: ChunkOutputText, Content: []byte("world")})
	b := NewTranscript()
	b.Push(Chunk{Seq: 0, Kind: ChunkOutputText, Content: []byte("hello ")})
	b.Push(Chunk{Seq: 2, Kind: ChunkOutputText, Content: []byte("worlD")})
	if a.Root() == b.Root() {
		t.Fatal("transcript root should differ on content change")
	}
	c1 := NewTranscript()
	c1.Push(Chunk{Seq: 0x01, Kind: ChunkOutputByte, Content: []byte{0x02, 0x03}})
	c2 := NewTranscript()
	c2.Push(Chunk{Seq: 0x0102, Kind: ChunkOutputByte, Content: []byte{0x03}})
	if c1.Root() == c2.Root() {
		t.Fatal("ambiguity guard: (seq=1,content=[2,3]) must differ from (seq=258,content=[3])")
	}
}

func TestTranscriptMerkleShapes(t *testing.T) {
	empty := NewTranscript().Root()
	if empty != mustDecode(crossImplRoots.empty) {
		t.Fatal("empty root mismatch")
	}
	l0 := Chunk{Seq: 0, Kind: ChunkOutputText, Content: []byte("a")}
	single := NewTranscript()
	single.Push(l0)
	if single.Root() != transcriptLeaf(l0) {
		t.Fatal("single-leaf root must equal its leaf hash")
	}
	l1 := Chunk{Seq: 1, Kind: ChunkOutputText, Content: []byte("b")}
	l2 := Chunk{Seq: 2, Kind: ChunkOutputText, Content: []byte("c")}
	three := NewTranscript()
	three.Push(l0)
	three.Push(l1)
	three.Push(l2)
	if three.Root() != transcriptParent(transcriptParent(transcriptLeaf(l0), transcriptLeaf(l1)), transcriptLeaf(l2)) {
		t.Fatal("three-leaf root shape mismatch")
	}
}
