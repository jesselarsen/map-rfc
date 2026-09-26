// Package meter is MAP metering bound to evidence (spec/layer.md §9),
// mirroring Rust map-meter: exact decimal amounts, asset identifiers, the
// exact price of a cumulative usage figure under a rate table, and the
// voucher mapping (spec/mapping-payment-rails.md). Money moves on an
// external rail; MAP never mints vouchers. No rounding rule exists anywhere:
// every price is an exact `uint counter × decimal-fraction rate` product.
package meter

import (
	"crypto/sha256"
	"errors"
	"fmt"
	"math"
	"math/bits"
	"strings"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/core"
)

// Errors.
var (
	ErrBadAmount    = errors.New("meter: invalid amount encoding")
	ErrBadAsset     = errors.New("meter: invalid asset identifier")
	ErrMissingRate  = errors.New("meter: rate table lacks a required unit")
	ErrExponent     = errors.New("meter: exponent mismatch")
	ErrAsset        = errors.New("meter: asset mismatch")
	ErrOverflow     = errors.New("meter: overflow")
	ErrUnsupported  = errors.New("meter: pricing model has no usage-based rule")
	ErrInexactUnits = errors.New("meter: amount is not exact in the rail's atomic units")
)

// Amount is a CBOR decimal fraction (tag 4): value = Mantissa × 10^Exponent.
// Layer M-4: amounts are non-negative, so the mantissa is unsigned and a
// negative mantissa on the wire is rejected.
type Amount struct {
	Exponent int32
	Mantissa uint64
}

// ToValue encodes 4([exponent, mantissa]).
func (a Amount) ToValue() cbor.Value {
	return cbor.Tag{Num: 4, Content: cbor.Array{cbor.Int(a.Exponent), cbor.Uint(a.Mantissa)}}
}

// AmountFromValue decodes an Amount, rejecting negative mantissas (M-4).
func AmountFromValue(v cbor.Value) (Amount, error) {
	t, ok := v.(cbor.Tag)
	if !ok || t.Num != 4 {
		return Amount{}, ErrBadAmount
	}
	arr, ok := t.Content.(cbor.Array)
	if !ok || len(arr) != 2 {
		return Amount{}, ErrBadAmount
	}
	var e int64
	switch x := arr[0].(type) {
	case cbor.Uint:
		if uint64(x) > math.MaxInt32 {
			return Amount{}, ErrBadAmount
		}
		e = int64(x)
	case cbor.Int:
		e = int64(x)
	default:
		return Amount{}, ErrBadAmount
	}
	if e < math.MinInt32 || e > math.MaxInt32 {
		return Amount{}, ErrBadAmount
	}
	m, ok := arr[1].(cbor.Uint) // a negative integer decodes as Int: rejected
	if !ok {
		return Amount{}, ErrBadAmount
	}
	return Amount{Exponent: int32(e), Mantissa: uint64(m)}, nil
}

// String renders the exact decimal value.
func (a Amount) String() string {
	digits := fmt.Sprint(a.Mantissa)
	if a.Exponent >= 0 {
		return digits + strings.Repeat("0", int(a.Exponent))
	}
	frac := int(-a.Exponent)
	if len(digits) > frac {
		return digits[:len(digits)-frac] + "." + digits[len(digits)-frac:]
	}
	return "0." + strings.Repeat("0", frac-len(digits)) + digits
}

// Asset is an asset identifier (layer §9.1): iso4217:USD, asset:sat,
// erc20:<chain>:<addr>, credit:<mesh>.
type Asset string

// NewAsset validates the identifier's prefix.
func NewAsset(s string) (Asset, error) {
	for _, p := range []string{"iso4217:", "asset:", "erc20:", "credit:"} {
		if strings.HasPrefix(s, p) && len(s) > len(p) {
			return Asset(s), nil
		}
	}
	return "", fmt.Errorf("%w: %q", ErrBadAsset, s)
}

// ERC20 is a convenience constructor for erc20:<chain>:<addr>.
func ERC20(chain, address string) Asset { return Asset("erc20:" + chain + ":" + address) }

// AssetAmount pairs an asset with an amount.
type AssetAmount struct {
	Asset  Asset
	Amount Amount
}

// Rate-table unit codes (layer §9.2); each prices a single counted unit.
const (
	UnitFlat       = 0
	UnitTokenIn    = 1
	UnitTokenOut   = 2
	UnitMs         = 3
	UnitInvocation = 4
)

// Rate is one rate-table entry.
type Rate struct {
	Unit  uint64
	Price AssetAmount
}

func rate(table []Rate, unit uint64) (AssetAmount, error) {
	for _, r := range table {
		if r.Unit == unit {
			return r.Price, nil
		}
	}
	return AssetAmount{}, fmt.Errorf("%w: unit %d", ErrMissingRate, unit)
}

func mulAdd(acc, counter, mantissa uint64) (uint64, error) {
	hi, lo := bits.Mul64(counter, mantissa)
	if hi != 0 {
		return 0, ErrOverflow
	}
	sum, carry := bits.Add64(acc, lo, 0)
	if carry != 0 {
		return 0, ErrOverflow
	}
	return sum, nil
}

// Billed is the exact price of cumulative usage under model and table
// (layer §9, M-3): model 1 per-token in×rate[1] + out×rate[2]; model 2
// per-ms wall_ms×rate[3]; model 3 per-invocation 1×rate[4]. Model 0 (flat)
// and unknown models are errors.
func Billed(model uint64, table []Rate, u core.Usage) (AssetAmount, error) {
	switch model {
	case 1:
		ri, err := rate(table, UnitTokenIn)
		if err != nil {
			return AssetAmount{}, err
		}
		ro, err := rate(table, UnitTokenOut)
		if err != nil {
			return AssetAmount{}, err
		}
		if ri.Asset != ro.Asset {
			return AssetAmount{}, ErrAsset
		}
		if ri.Amount.Exponent != ro.Amount.Exponent {
			return AssetAmount{}, ErrExponent
		}
		total, err := mulAdd(0, u.TokensIn, ri.Amount.Mantissa)
		if err != nil {
			return AssetAmount{}, err
		}
		if total, err = mulAdd(total, u.TokensOut, ro.Amount.Mantissa); err != nil {
			return AssetAmount{}, err
		}
		return AssetAmount{ri.Asset, Amount{ri.Amount.Exponent, total}}, nil
	case 2:
		r, err := rate(table, UnitMs)
		if err != nil {
			return AssetAmount{}, err
		}
		total, err := mulAdd(0, u.WallMs, r.Amount.Mantissa)
		if err != nil {
			return AssetAmount{}, err
		}
		return AssetAmount{r.Asset, Amount{r.Amount.Exponent, total}}, nil
	case 3:
		r, err := rate(table, UnitInvocation)
		if err != nil {
			return AssetAmount{}, err
		}
		return r, nil
	default:
		return AssetAmount{}, fmt.Errorf("%w: model %d", ErrUnsupported, model)
	}
}

func multihashSHA256(data []byte) []byte {
	d := sha256.Sum256(data)
	return append([]byte{0x12, 0x20}, d[:]...)
}

// DigestRailBytes is RAIL-1: a rail's own canonical signed-authorization
// bytes, digested exactly as-is.
func DigestRailBytes(railBytes []byte) []byte { return multihashSHA256(railBytes) }

// DigestMPPTempo is RAIL-5: the MPP Tempo fallback digest over
// [channelId, [exponent, mantissa], reference, method].
func DigestMPPTempo(channelID []byte, cumulative Amount, reference, method string) []byte {
	return multihashSHA256(cbor.Encode(cbor.Array{
		cbor.Bytes(channelID),
		cbor.Array{cbor.Int(cumulative.Exponent), cbor.Uint(cumulative.Mantissa)},
		cbor.Text(reference),
		cbor.Text(method),
	}))
}

// ToAtomicUnits is RAIL-2/RAIL-3: exact by construction; the exponent must
// equal -decimals, otherwise converting would need rounding, which is never
// done.
func ToAtomicUnits(a Amount, decimals uint32) (uint64, error) {
	if int64(a.Exponent) != -int64(decimals) {
		return 0, fmt.Errorf("%w: exponent %d, decimals %d", ErrInexactUnits, a.Exponent, decimals)
	}
	return a.Mantissa, nil
}

// VoucherCeiling is layer M-6: the most a requester should authorize in its
// next cumulative voucher, the price of the usage in its latest dual-signed
// checkpoint plus a deliberate, bounded credit extension.
func VoucherCeiling(model uint64, table []Rate, acknowledged core.Usage, credit AssetAmount) (AssetAmount, error) {
	owed, err := Billed(model, table, acknowledged)
	if err != nil {
		return AssetAmount{}, err
	}
	if owed.Asset != credit.Asset {
		return AssetAmount{}, ErrAsset
	}
	if owed.Amount.Exponent != credit.Amount.Exponent {
		return AssetAmount{}, ErrExponent
	}
	sum, carry := bits.Add64(owed.Amount.Mantissa, credit.Amount.Mantissa, 0)
	if carry != 0 {
		return AssetAmount{}, ErrOverflow
	}
	return AssetAmount{owed.Asset, Amount{owed.Amount.Exponent, sum}}, nil
}

// WithinCeiling checks a voucher the requester is about to sign against M-6.
func WithinCeiling(voucherCumulative, ceiling AssetAmount) (bool, error) {
	if voucherCumulative.Asset != ceiling.Asset {
		return false, ErrAsset
	}
	if voucherCumulative.Amount.Exponent != ceiling.Amount.Exponent {
		return false, ErrExponent
	}
	return voucherCumulative.Amount.Mantissa <= ceiling.Amount.Mantissa, nil
}
