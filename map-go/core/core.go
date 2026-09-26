// Package core holds the small value types that several v2 layers share
// (spec/layer.md): the 32-byte TaskID carried by receipts and checkpoints,
// and the cumulative Usage counters carried by checkpoints and priced by
// metering. It mirrors map-core's task_id.rs and usage.rs.
package core

import (
	"crypto/rand"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// TaskID is a 32-byte task identifier (ReceiptCore key 1, CheckpointCore
// key 1). How a binding derives it is binding-specific.
type TaskID [32]byte

// RandomTaskID draws a TaskID from the OS RNG.
func RandomTaskID() TaskID {
	var t TaskID
	if _, err := rand.Read(t[:]); err != nil {
		panic(err)
	}
	return t
}

// TaskIDFromBytes copies b into a TaskID; ok is false unless len(b) == 32.
func TaskIDFromBytes(b []byte) (TaskID, bool) {
	var t TaskID
	if len(b) != 32 {
		return t, false
	}
	copy(t[:], b)
	return t, true
}

// Usage is cumulative usage since task start (layer §6.1). Wire form
// {1 tokens-in, 2 tokens-out, 3 ttft-ms, 4 wall-ms}, all uint, all REQUIRED.
type Usage struct {
	TokensIn  uint64
	TokensOut uint64
	TTFTMs    uint64
	WallMs    uint64
}

// ToValue encodes the usage map.
func (u Usage) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(u.TokensIn)},
		{K: cbor.Uint(2), V: cbor.Uint(u.TokensOut)},
		{K: cbor.Uint(3), V: cbor.Uint(u.TTFTMs)},
		{K: cbor.Uint(4), V: cbor.Uint(u.WallMs)},
	}
}

// UsageFromValue decodes a usage map; all four keys are required uints.
func UsageFromValue(v cbor.Value) (Usage, error) {
	m, ok := v.(cbor.Map)
	if !ok {
		return Usage{}, fmt.Errorf("usage: not a map")
	}
	var f [4]uint64
	for i := range f {
		x, ok := m.Get(uint64(i + 1))
		u, isUint := x.(cbor.Uint)
		if !ok || !isUint {
			return Usage{}, fmt.Errorf("usage: key %d missing or not a uint", i+1)
		}
		f[i] = uint64(u)
	}
	return Usage{TokensIn: f[0], TokensOut: f[1], TTFTMs: f[2], WallMs: f[3]}, nil
}

// Dominates reports component-wise u >= prev (layer C-4 / M-2).
func (u Usage) Dominates(prev Usage) bool {
	return u.TokensIn >= prev.TokensIn && u.TokensOut >= prev.TokensOut &&
		u.TTFTMs >= prev.TTFTMs && u.WallMs >= prev.WallMs
}

// Counters returns the four counters in wire-key order.
func (u Usage) Counters() [4]uint64 {
	return [4]uint64{u.TokensIn, u.TokensOut, u.TTFTMs, u.WallMs}
}
