package tasks

import (
	"crypto/sha256"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// ChunkKind enumerates task chunk kinds (tasks §3). Kinds 0/1/4 are
// transcript-bearing.
type ChunkKind uint8

const (
	ChunkOutputText ChunkKind = 0
	ChunkOutputByte ChunkKind = 1
	ChunkLog        ChunkKind = 2
	ChunkUsage      ChunkKind = 3
	ChunkToolIO     ChunkKind = 4
)

// InTranscript reports whether the kind contributes to the transcript root.
func (k ChunkKind) InTranscript() bool {
	switch k {
	case ChunkOutputText, ChunkOutputByte, ChunkToolIO:
		return true
	default:
		return false
	}
}

// Chunk is the transcript-level view (seq, kind, content); the wire ChunkMsg
// carries additionally TaskID and fin.
type Chunk struct {
	Seq     uint64
	Kind    ChunkKind
	Content []byte
}

// transcriptLeaf = SHA256(0x00 || enc([seq, kind, content])) (tasks §8.1).
func transcriptLeaf(c Chunk) [32]byte {
	enc := cbor.Encode(cbor.Array{
		cbor.Uint(c.Seq),
		cbor.Uint(uint64(c.Kind)),
		cbor.Bytes(c.Content),
	})
	h := sha256.New()
	h.Write([]byte{0x00})
	h.Write(enc)
	var out [32]byte
	copy(out[:], h.Sum(nil))
	return out
}

// parent = SHA256(0x01 || left || right).
func transcriptParent(l, r [32]byte) [32]byte {
	h := sha256.New()
	h.Write([]byte{0x01})
	h.Write(l[:])
	h.Write(r[:])
	var out [32]byte
	copy(out[:], h.Sum(nil))
	return out
}

// emptyTranscriptRoot = SHA256(0x00 || enc([])).
func emptyTranscriptRoot() [32]byte {
	enc := cbor.Encode(cbor.Array{})
	h := sha256.New()
	h.Write([]byte{0x00})
	h.Write(enc)
	var out [32]byte
	copy(out[:], h.Sum(nil))
	return out
}

// TranscriptBuilder is a streaming-friendly Merkle commitment (tasks §8.1).
// Non-transcript kinds are accepted and ignored so callers can feed every chunk.
type TranscriptBuilder struct {
	leaves [][32]byte
}

// NewTranscript returns an empty builder.
func NewTranscript() *TranscriptBuilder { return &TranscriptBuilder{} }

// Push accumulates a transcript-bearing chunk leaf; non-transcript kinds are
// silently skipped.
func (t *TranscriptBuilder) Push(c Chunk) {
	if c.Kind.InTranscript() {
		t.leaves = append(t.leaves, transcriptLeaf(c))
	}
}

// LeafCount returns the number of accumulated transcript leaves.
func (t *TranscriptBuilder) LeafCount() int { return len(t.leaves) }

// Root returns the commitment: empty root for no leaves; pairwise reduce with
// odd nodes promoted unchanged; a single leaf equals its leaf hash.
func (t *TranscriptBuilder) Root() [32]byte {
	if len(t.leaves) == 0 {
		return emptyTranscriptRoot()
	}
	level := make([][32]byte, len(t.leaves))
	copy(level, t.leaves)
	for len(level) > 1 {
		var next [][32]byte
		for i := 0; i+1 < len(level); i += 2 {
			next = append(next, transcriptParent(level[i], level[i+1]))
		}
		if len(level)%2 == 1 {
			next = append(next, level[len(level)-1]) // promoted unchanged
		}
		level = next
	}
	return level[0]
}
