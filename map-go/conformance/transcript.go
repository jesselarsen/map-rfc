// Package conformance reproduces and checks the frozen MAP test-vector
// set (map-spec/test-vectors/map-test-vectors.md) byte-for-byte: every
// positive vector is regenerated from seeds and consumed; every negative
// vector is rejected with the required error class.
package conformance

import (
	"crypto/sha256"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// Transcript commitment, draft-map-tasks-00 §8.1:
//
//	leaf(c)      = H(0x00 || enc([chunk-seq, kind, content]))
//	parent(l, r) = H(0x01 || l || r)
//	odd node       promoted unchanged to the next level
//	empty root   = H(0x00 || enc([]))

// Chunk is one output/tool chunk for transcript purposes.
type Chunk struct {
	Seq     uint64
	Kind    uint64
	Content []byte
}

func leaf(c Chunk) [32]byte {
	enc := cbor.Encode(cbor.Array{cbor.Uint(c.Seq), cbor.Uint(c.Kind), cbor.Bytes(c.Content)})
	return sha256.Sum256(append([]byte{0x00}, enc...))
}

func parent(l, r [32]byte) [32]byte {
	buf := make([]byte, 0, 65)
	buf = append(buf, 0x01)
	buf = append(buf, l[:]...)
	buf = append(buf, r[:]...)
	return sha256.Sum256(buf)
}

// TranscriptRoot computes the Merkle root over chunks in chunk-seq order.
func TranscriptRoot(chunks []Chunk) [32]byte {
	if len(chunks) == 0 {
		return sha256.Sum256(append([]byte{0x00}, cbor.Encode(cbor.Array{})...))
	}
	level := make([][32]byte, len(chunks))
	for i, c := range chunks {
		level[i] = leaf(c)
	}
	for len(level) > 1 {
		next := make([][32]byte, 0, (len(level)+1)/2)
		for i := 0; i < len(level); i += 2 {
			if i+1 < len(level) {
				next = append(next, parent(level[i], level[i+1]))
			} else {
				next = append(next, level[i]) // odd node promoted unchanged
			}
		}
		level = next
	}
	return level[0]
}
