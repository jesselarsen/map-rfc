// Package trust is the observer-local trust projection over dual-signed
// execution receipts (spec/layer.md §8), mirroring Rust map-trust.
//
// Scores are derived data: Rebuild over the receipt store reproduces any
// incrementally maintained projection (T-1), and the admissibility rules
// R-2..R-4 live inside Projection.Apply so both paths enforce them
// identically (T-2). R-1 is receipt.Receipt.Verify.
package trust

import (
	"bytes"
	"errors"
	"math"
	"sort"

	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
)

// Params is projection policy (layer §8). Every field is local policy.
type Params struct {
	// DailyDecay is the recency decay per day, from the receipt's `ended`.
	DailyDecay float64
	// Prior is the symmetric Beta prior applied at read time.
	Prior float64
	// SkewMs is the R-3 bound: `ended` later than now + SkewMs does not count.
	SkewMs uint64
	// IssuerCap, when > 0, caps one issuer's total (weighted, decayed)
	// evidence per (provider, capability) cell at read time (T-3).
	IssuerCap float64
}

// DefaultParams matches Rust ProjParams::default().
func DefaultParams() Params {
	return Params{DailyDecay: 0.98, Prior: 1.0, SkewMs: 120_000}
}

// Skip reasons: why Apply did not count a receipt.
var (
	ErrSelfDealing   = errors.New("self-dealing")   // R-2
	ErrFutureDated   = errors.New("future-dated")   // R-3
	ErrNotAdmissible = errors.New("not-admissible") // R-4
	ErrZeroWeight    = errors.New("zero-weight")    // T-3
	ErrCancelled     = errors.New("cancelled")      // §8.2
)

type cellKey struct {
	provider   identity.AgentID
	capability string
}

type ab struct{ a, b float64 }

// Projection is the per-(provider, capability) Beta evidence, kept per issuer.
type Projection struct {
	params Params
	cells  map[cellKey]map[identity.AgentID]*ab
}

// New returns an empty projection.
func New(p Params) *Projection {
	return &Projection{params: p, cells: map[cellKey]map[identity.AgentID]*ab{}}
}

// Apply folds one verified receipt in, enforcing R-2, R-3, R-4, the
// cancelled rule and T-3, in that order. issuerWeight is the observer's
// weight for the receipt's requester (see IssuerWeights).
func (p *Projection) Apply(v receipt.Verified, nowMs uint64, issuerWeight float64) error {
	c := v.Core
	if c.Requester == c.Provider {
		return ErrSelfDealing
	}
	limit := nowMs + p.params.SkewMs
	if limit < nowMs {
		limit = math.MaxUint64
	}
	if c.EndedMs > limit {
		return ErrFutureDated
	}
	negative := c.Outcome == receipt.OutcomeFailed || c.Outcome == receipt.OutcomeTimeout || c.Outcome == receipt.OutcomeDisputed
	requesterOnly := v.RequesterSigned && !v.ProviderSigned
	if !(v.Dual || (requesterOnly && negative)) {
		return ErrNotAdmissible
	}
	if c.Outcome == receipt.OutcomeCancelled {
		return ErrCancelled
	}
	if math.IsNaN(issuerWeight) || math.IsInf(issuerWeight, 0) || issuerWeight <= 0 {
		return ErrZeroWeight
	}
	var ageMs uint64
	if nowMs > c.EndedMs {
		ageMs = nowMs - c.EndedMs
	}
	days := float64(ageMs) / 86_400_000.0
	w := issuerWeight * math.Pow(p.params.DailyDecay, days)
	key := cellKey{c.Provider, c.Capability}
	cell := p.cells[key]
	if cell == nil {
		cell = map[identity.AgentID]*ab{}
		p.cells[key] = cell
	}
	e := cell[c.Requester]
	if e == nil {
		e = &ab{}
		cell[c.Requester] = e
	}
	switch c.Outcome {
	case receipt.OutcomeOK:
		e.a += w
	case receipt.OutcomeFailed, receipt.OutcomeTimeout:
		e.b += w
	case receipt.OutcomeDisputed:
		e.b += 2 * w
	}
	return nil
}

// Score returns (score, confidence) for a cell (layer §8.2): score =
// α/(α+β) with the prior folded in at read time, confidence = α+β. ok is
// false if no admissible evidence exists for the cell.
func (p *Projection) Score(provider identity.AgentID, capability string) (score, confidence float64, ok bool) {
	cell := p.cells[cellKey{provider, capability}]
	if cell == nil {
		return 0, 0, false
	}
	issuers := make([]identity.AgentID, 0, len(cell))
	for k := range cell {
		issuers = append(issuers, k)
	}
	// Deterministic summation order (matches Rust: AgentID byte order).
	sort.Slice(issuers, func(i, j int) bool { return bytes.Compare(issuers[i][:], issuers[j][:]) < 0 })
	var a, b float64
	for _, k := range issuers {
		e := cell[k]
		total := e.a + e.b
		scale := 1.0
		if p.params.IssuerCap > 0 && total > p.params.IssuerCap && total > 0 {
			scale = p.params.IssuerCap / total
		}
		a += e.a * scale
		b += e.b * scale
	}
	pr := p.params.Prior
	return (a + pr) / (a + b + 2*pr), a + b, true
}

// Rebuild recomputes a projection from evidence alone (T-1).
func Rebuild(store *Store, nowMs uint64, params Params, weight func(identity.AgentID) float64) (*Projection, error) {
	p := New(params)
	for _, r := range store.log {
		v, err := r.Verify()
		if err != nil {
			return nil, err
		}
		_ = p.Apply(v, nowMs, weight(v.Core.Requester)) // skipped receipts are not evidence
	}
	return p, nil
}

// IssuerWeights implements T-3's endpoints: the observer's own receipts
// weigh 1, explicitly configured issuers their configured value, and every
// other issuer 0 (never the neutral 0.5).
type IssuerWeights struct {
	observer identity.AgentID
	known    map[identity.AgentID]float64
}

// NewIssuerWeights returns weights for observer.
func NewIssuerWeights(observer identity.AgentID) *IssuerWeights {
	return &IssuerWeights{observer: observer, known: map[identity.AgentID]float64{}}
}

// With weighs issuer at w, clamped to [0, 1] (non-finite = 0).
func (w *IssuerWeights) With(issuer identity.AgentID, weight float64) *IssuerWeights {
	if math.IsNaN(weight) || math.IsInf(weight, 0) {
		weight = 0
	}
	w.known[issuer] = math.Max(0, math.Min(1, weight))
	return w
}

// Weight returns the weight for issuer.
func (w *IssuerWeights) Weight(issuer identity.AgentID) float64 {
	if issuer == w.observer {
		return 1
	}
	return w.known[issuer]
}

// Store is an append-only, idempotent receipt store.
type Store struct {
	log  []receipt.Receipt
	seen map[[32]byte]bool
}

// NewStore returns an empty store.
func NewStore() *Store { return &Store{seen: map[[32]byte]bool{}} }

// Ingest verifies r, then appends it iff unseen. It returns false for a
// duplicate.
func (s *Store) Ingest(r receipt.Receipt) (bool, error) {
	if _, err := r.Verify(); err != nil {
		return false, err
	}
	ca := r.ContentAddress()
	if s.seen[ca] {
		return false, nil
	}
	s.seen[ca] = true
	s.log = append(s.log, r)
	return true, nil
}

// Len is the number of stored receipts.
func (s *Store) Len() int { return len(s.log) }

// ApplyVerifiedEvidence applies evidence about subject from an untrusted
// holder: receipts that do not name subject as provider are ignored, the
// rest pass through Apply's gates. It returns the count applied.
func ApplyVerifiedEvidence(evidence []receipt.Verified, subject identity.AgentID, p *Projection, nowMs uint64, trustInIssuer float64) int {
	n := 0
	for _, v := range evidence {
		if v.Core.Provider == subject && p.Apply(v, nowMs, trustInIssuer) == nil {
			n++
		}
	}
	return n
}
