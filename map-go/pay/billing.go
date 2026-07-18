package pay

import (
	"math/big"

	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

// BillingError enumerates why an exact billed amount could not be produced
// (mirrors map-pay billing::BillingError). It is internal to Billed; the
// watermark verifier collapses every arithmetic failure to code 6009.
type BillingError int

const (
	// BillMissingRate: the rate table lacks a unit the model requires.
	BillMissingRate BillingError = iota
	// BillExponentMismatch: combined rates disagree on exponent (§3).
	BillExponentMismatch
	// BillAssetMismatch: combined rates are in different assets (§3).
	BillAssetMismatch
	// BillOverflow: a product or sum exceeded int64 mantissa range.
	BillOverflow
	// BillUnsupportedModel: the pricing model has no billing rule here.
	BillUnsupportedModel
)

func (e BillingError) Error() string { return "pay: billing error" }

func rateOf(table []RateEntry, unit uint64) (AssetAmount, bool) {
	for _, r := range table {
		if r.Unit == unit {
			return r.Amount, true
		}
	}
	return AssetAmount{}, false
}

// term is the exact counter × mantissa product (big.Int intermediate; the
// caller narrows the final sum to int64).
func term(counter uint64, mantissa int64) *big.Int {
	return new(big.Int).Mul(new(big.Int).SetUint64(counter), big.NewInt(mantissa))
}

func makeAmount(exponent int32, asset string, total *big.Int) (AssetAmount, error) {
	if !total.IsInt64() {
		return AssetAmount{}, BillOverflow
	}
	return AssetAmount{Asset: asset, Amount: Amount{Exponent: exponent, Mantissa: total.Int64()}}, nil
}

// Billed is the exact amount owed for usage under model and rateTable
// (pay §4.1): model 1 per-token in×rate[1]+out×rate[2]; model 2 per-ms
// wall×rate[3]; model 3 per-invocation 1×rate[4]. Model 0 (flat) and unknown
// are errors — the flat total is not computed from usage. Arithmetic is exact;
// overflow is an error, never a wrap.
func Billed(model uint64, rateTable []RateEntry, usage tasks.Usage) (AssetAmount, error) {
	switch model {
	case 1:
		ri, ok := rateOf(rateTable, UnitTokenIn)
		if !ok {
			return AssetAmount{}, BillMissingRate
		}
		ro, ok := rateOf(rateTable, UnitTokenOut)
		if !ok {
			return AssetAmount{}, BillMissingRate
		}
		if ri.Asset != ro.Asset {
			return AssetAmount{}, BillAssetMismatch
		}
		if ri.Amount.Exponent != ro.Amount.Exponent {
			return AssetAmount{}, BillExponentMismatch
		}
		total := new(big.Int).Add(term(usage.TokensIn, ri.Amount.Mantissa), term(usage.TokensOut, ro.Amount.Mantissa))
		return makeAmount(ri.Amount.Exponent, ri.Asset, total)
	case 2:
		r, ok := rateOf(rateTable, UnitMS)
		if !ok {
			return AssetAmount{}, BillMissingRate
		}
		return makeAmount(r.Amount.Exponent, r.Asset, term(usage.WallMs, r.Amount.Mantissa))
	case 3:
		r, ok := rateOf(rateTable, UnitInvocation)
		if !ok {
			return AssetAmount{}, BillMissingRate
		}
		return makeAmount(r.Amount.Exponent, r.Asset, big.NewInt(r.Amount.Mantissa)) // one invocation
	default:
		return AssetAmount{}, BillUnsupportedModel
	}
}

// AgreedTerms is the billing basis a requester verifies each metered invoice
// against (from the PAY_QUOTE it agreed to, pay §3).
type AgreedTerms struct {
	Task          [32]byte
	PricingModel  uint64
	RateTable     []RateEntry
	Terms         uint64
	AuthorizedCap *AssetAmount
	QuotedTotal   *AssetAmount // pay §5.2: required under terms 1/2
}

// le reports a<=b requiring the same asset AND exponent; a mismatch is not
// comparable and returns (false,false) — the caller treats that as a violation
// (§3: comparisons are mantissa-only, never converted).
func le(a, b AssetAmount) (result, comparable bool) {
	if a.Asset != b.Asset || a.Amount.Exponent != b.Amount.Exponent {
		return false, false
	}
	return a.Amount.Mantissa <= b.Amount.Mantissa, true
}

// VerifyWatermark verifies a metered invoice's UsageWatermark against the
// agreement and the last ACCEPTED watermark (pay §4.1 checks 1,2,3,5,6 — in
// order, first failure wins). Check 4 (meter consistency vs received kind-3
// chunks) needs runtime chunk state and is applied by the caller. Returns the
// specific core §9 wire code on rejection, or 0 on pass.
func VerifyWatermark(agreed AgreedTerms, lastAccepted *UsageWatermark, inv PayInvoice) uint16 {
	// 1. presence: key 4 present iff terms 3.
	terms3 := agreed.Terms == 3
	if inv.Watermark == nil {
		if terms3 {
			return CodeWatermarkViolation
		}
		return 0 // non-metered invoice: nothing to verify here
	}
	if !terms3 {
		return CodeWatermarkViolation
	}
	w := *inv.Watermark
	// 2. task binding.
	if w.Task != agreed.Task {
		return CodeTaskBindingMismatch
	}
	// 3. monotonicity vs the last accepted watermark.
	if lastAccepted != nil {
		prev := *lastAccepted
		usageMonotone := w.Usage.TokensIn >= prev.Usage.TokensIn &&
			w.Usage.TokensOut >= prev.Usage.TokensOut &&
			w.Usage.TTFTMs >= prev.Usage.TTFTMs &&
			w.Usage.WallMs >= prev.Usage.WallMs
		billedMonotone, _ := le(prev.CumulativeBille, w.CumulativeBille)
		if w.ThroughSeq < prev.ThroughSeq || !usageMonotone || !billedMonotone {
			return CodeWatermarkViolation
		}
	}
	// 5. arithmetic: cumulative-billed == rate × usage, and the invoice amount
	//    is the delta since the last accepted watermark.
	computed, err := Billed(agreed.PricingModel, agreed.RateTable, w.Usage)
	if err != nil {
		return CodeWatermarkViolation
	}
	if !computed.Equal(w.CumulativeBille) {
		return CodeWatermarkViolation
	}
	var prevBilled int64
	if lastAccepted != nil {
		prevBilled = lastAccepted.CumulativeBille.Amount.Mantissa
	}
	expectedDelta := w.CumulativeBille.Amount.Mantissa - prevBilled
	amountOK := expectedDelta >= 0 &&
		inv.Amount.Asset == w.CumulativeBille.Asset &&
		inv.Amount.Amount.Exponent == w.CumulativeBille.Amount.Exponent &&
		inv.Amount.Amount.Mantissa == expectedDelta
	if !amountOK {
		return CodeWatermarkViolation
	}
	// 6. cap: cumulative-billed does not exceed the authorized cap.
	if agreed.AuthorizedCap != nil {
		ok, comparable := le(w.CumulativeBille, *agreed.AuthorizedCap)
		if !comparable || !ok {
			return CodeCapExceeded
		}
	}
	return 0
}
