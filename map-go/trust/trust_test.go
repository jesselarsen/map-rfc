package trust

import (
	"bytes"
	"crypto/ed25519"
	"errors"
	"math"
	"math/rand"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
)

const capURI = "map:cap:inference/text-generation"
const now = uint64(1_790_000_000_000)

func key(b byte) ed25519.PrivateKey { return ed25519.NewKeyFromSeed(bytes.Repeat([]byte{b}, 32)) }

func aid(k ed25519.PrivateKey) identity.AgentID {
	a, _ := identity.FromPub(k.Public().(ed25519.PublicKey))
	return a
}

// mk builds a receipt signed by the listed parties.
func mk(req, prov ed25519.PrivateKey, outcome uint64, ended uint64, sigReq, sigProv bool, task byte) receipt.Receipt {
	var tid core.TaskID
	tid[0] = task
	c := receipt.ReceiptCore{
		Task: tid, Requester: aid(req), Provider: aid(prov), Capability: capURI,
		StartedMs: ended - 1000, EndedMs: ended, Outcome: outcome,
		Metrics: core.Usage{TokensIn: 1, TokensOut: 1, TTFTMs: 1, WallMs: 1},
	}
	cb := c.Encode()
	r := receipt.Receipt{CoreBytes: cb}
	if sigReq {
		r.SigReq = receipt.Sign(req, receipt.RoleRequester, cb)
	}
	if sigProv {
		r.SigProv = receipt.Sign(prov, receipt.RoleProvider, cb)
	}
	return r
}

func verified(t *testing.T, r receipt.Receipt) receipt.Verified {
	t.Helper()
	v, err := r.Verify()
	if err != nil {
		t.Fatal(err)
	}
	return v
}

func TestUnilateralRule(t *testing.T) {
	req, prov := key(1), key(2)
	cases := []struct {
		name            string
		sigReq, sigProv bool
		outcome         uint64
		want            error
	}{
		{"dual ok", true, true, receipt.OutcomeOK, nil},
		{"requester-only ok", true, false, receipt.OutcomeOK, ErrNotAdmissible},
		{"provider-only ok", false, true, receipt.OutcomeOK, ErrNotAdmissible},
		{"requester-only failed", true, false, receipt.OutcomeFailed, nil},
		{"requester-only disputed", true, false, receipt.OutcomeDisputed, nil},
		{"provider-only failed", false, true, receipt.OutcomeFailed, ErrNotAdmissible},
		{"dual cancelled", true, true, receipt.OutcomeCancelled, ErrCancelled},
	}
	for _, c := range cases {
		v := verified(t, mk(req, prov, c.outcome, now-1000, c.sigReq, c.sigProv, 1))
		if got := New(DefaultParams()).Apply(v, now, 1); !errors.Is(got, c.want) {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
	self := verified(t, mk(req, req, receipt.OutcomeOK, now, true, true, 1))
	if err := New(DefaultParams()).Apply(self, now, 1); !errors.Is(err, ErrSelfDealing) {
		t.Errorf("self-dealing: %v", err)
	}
	future := verified(t, mk(req, prov, receipt.OutcomeOK, now+120_001, true, true, 1))
	if err := New(DefaultParams()).Apply(future, now, 1); !errors.Is(err, ErrFutureDated) {
		t.Errorf("future-dated: %v", err)
	}
}

func TestIncrementalEqualsRebuild(t *testing.T) {
	rng := rand.New(rand.NewSource(7))
	keys := []ed25519.PrivateKey{key(1), key(2), key(3), key(4)}
	observer := aid(keys[0])
	w := NewIssuerWeights(observer).With(aid(keys[1]), 0.5)
	store := NewStore()
	inc := New(DefaultParams())
	for i := 0; i < 60; i++ {
		req, prov := keys[rng.Intn(4)], keys[rng.Intn(4)]
		r := mk(req, prov, uint64(rng.Intn(5)), now-uint64(rng.Intn(10))*86_400_000, rng.Intn(3) > 0, rng.Intn(3) > 0, byte(i))
		if r.SigReq == nil && r.SigProv == nil {
			continue
		}
		fresh, err := store.Ingest(r)
		if err != nil {
			t.Fatal(err)
		}
		if fresh {
			v := verified(t, r)
			_ = inc.Apply(v, now, w.Weight(v.Core.Requester))
		}
	}
	rebuilt, err := Rebuild(store, now, DefaultParams(), w.Weight)
	if err != nil {
		t.Fatal(err)
	}
	for _, p := range keys {
		s1, c1, ok1 := inc.Score(aid(p), capURI)
		s2, c2, ok2 := rebuilt.Score(aid(p), capURI)
		if ok1 != ok2 || math.Abs(s1-s2) > 1e-12 || math.Abs(c1-c2) > 1e-12 {
			t.Fatalf("incremental %v/%v/%v != rebuilt %v/%v/%v", s1, c1, ok1, s2, c2, ok2)
		}
	}
}

func TestUnknownIssuersWeighZeroAndCapBoundsOneIssuer(t *testing.T) {
	observer, stranger, prov := key(1), key(9), key(2)
	w := NewIssuerWeights(aid(observer))
	if w.Weight(aid(stranger)) != 0 || w.Weight(aid(observer)) != 1 {
		t.Fatal("T-3 endpoints wrong")
	}
	v := verified(t, mk(stranger, prov, receipt.OutcomeOK, now, true, true, 1))
	if err := New(DefaultParams()).Apply(v, now, w.Weight(aid(stranger))); !errors.Is(err, ErrZeroWeight) {
		t.Fatalf("unknown issuer must weigh 0: %v", err)
	}
	params := DefaultParams()
	params.IssuerCap = 2
	p := New(params)
	for i := 0; i < 50; i++ {
		_ = p.Apply(verified(t, mk(observer, prov, receipt.OutcomeOK, now, true, true, byte(i))), now, 1)
	}
	if _, conf, _ := p.Score(aid(prov), capURI); math.Abs(conf-2) > 1e-9 {
		t.Fatalf("issuer cap not applied: confidence %v", conf)
	}
}

func TestEvidenceSubjectFilter(t *testing.T) {
	req, prov, other := key(1), key(2), key(3)
	ev := []receipt.Verified{
		verified(t, mk(req, prov, receipt.OutcomeOK, now, true, true, 1)),
		verified(t, mk(req, prov, receipt.OutcomeOK, now, false, true, 2)),
		verified(t, mk(req, other, receipt.OutcomeOK, now, true, true, 3)),
	}
	p := New(DefaultParams())
	if n := ApplyVerifiedEvidence(ev, aid(prov), p, now, 1); n != 1 {
		t.Fatalf("applied %d, want 1", n)
	}
	if _, _, ok := p.Score(aid(other), capURI); ok {
		t.Fatal("receipt about another provider was filed under the subject")
	}
}

func TestToleranceIsAnchoredToObserved(t *testing.T) {
	tol := DefaultTolerance()
	if !tol.AcceptsCounters([4]uint64{105, 0, 1, 0}, [4]uint64{100, 0, 0, 0}) {
		t.Fatal("5% and the ±1 floor must be accepted")
	}
	// An inflated claim does not widen its own band: 5% of the observed 100
	// is 5, so 106 is out even though 5% of 106 would cover it.
	if tol.AcceptsCounters([4]uint64{106, 0, 0, 0}, [4]uint64{100, 0, 0, 0}) {
		t.Fatal("band must be anchored to the observed value")
	}
	if !tol.fieldOK(math.MaxUint64, math.MaxUint64) {
		t.Fatal("saturating multiply must not overflow")
	}
}
