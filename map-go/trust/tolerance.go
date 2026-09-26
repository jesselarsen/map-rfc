package trust

import (
	"math"
	"math/bits"

	"github.com/jesselarsen/map-rfc/map-go/receipt"
)

// Tolerance is the local countersignature policy (layer §6.5): the second
// signer checks proposed usage and timestamps against its OWN observation.
// The window is anchored to the observed value, never max(claimed,
// observed), so an inflated claim cannot widen its own acceptance band.
type Tolerance struct {
	UsagePct uint64 // per usage counter, percent of observed (minimum ±1)
	TimeMs   uint64 // on started/ended
}

// DefaultTolerance is ±5% / ±2000 ms.
func DefaultTolerance() Tolerance { return Tolerance{UsagePct: 5, TimeMs: 2000} }

func absDiff(a, b uint64) uint64 {
	if a > b {
		return a - b
	}
	return b - a
}

func (t Tolerance) fieldOK(claimed, observed uint64) bool {
	hi, prod := bits.Mul64(observed, t.UsagePct)
	if hi != 0 {
		prod = math.MaxUint64 // saturate, as Rust's saturating_mul
	}
	tol := prod / 100
	if tol < 1 {
		tol = 1
	}
	return absDiff(claimed, observed) <= tol
}

// AcceptsCounters checks the four usage counters (the checkpoint check).
func (t Tolerance) AcceptsCounters(claimed, observed [4]uint64) bool {
	for i := range claimed {
		if !t.fieldOK(claimed[i], observed[i]) {
			return false
		}
	}
	return true
}

// Accepts checks claimed against observed on all four usage fields and both
// timestamps.
func (t Tolerance) Accepts(claimed, observed receipt.ReceiptCore) bool {
	return t.AcceptsCounters(claimed.Metrics.Counters(), observed.Metrics.Counters()) &&
		absDiff(claimed.StartedMs, observed.StartedMs) <= t.TimeMs &&
		absDiff(claimed.EndedMs, observed.EndedMs) <= t.TimeMs
}
