// Package pay is the MAP payments runtime for map-go (impl #2): the pay-family
// wire codecs (pay §3/§4/§5.2.1), the pay:credit-ledger settlement method
// (pay §5.1), exact metered billing arithmetic (pay §4.1), and the pump-style
// provider/requester runtimes for terms 0 (postpaid), 1 (prepaid), and 3
// (metered). It is a byte-for-byte port of map-rs/crates/map-pay +
// map-rs/crates/map-taskrun/src/{payments,metered,escrow}.rs, arbitrated by
// the frozen conformance vectors (TV-010b/013/016/017/018).
//
// Independence marker: this package is PURE STDLIB (it imports only the
// stdlib-only internal packages cbor/identity/tasks), so the conformance
// surface never links libp2p. Terms-2 escrow and pay §6 disputes are NOT
// ported here (future work); the credit-ledger holds only the postpaid Pay.
package pay

import (
	"crypto/sha256"
	"fmt"
	"math"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

// Message type code points (core §5; mirror of map-core msg_type).
const (
	MsgPayQuote   uint64 = 0x0400
	MsgPayAgree   uint64 = 0x0401
	MsgPayInvoice uint64 = 0x0402
	MsgPayProof   uint64 = 0x0403
	MsgPayReceipt uint64 = 0x0404
)

// Rate-table unit codes (pay §3).
const (
	UnitFlat       uint64 = 0
	UnitTokenIn    uint64 = 1
	UnitTokenOut   uint64 = 2
	UnitMS         uint64 = 3
	UnitInvocation uint64 = 4
)

// Terms codes (PayQuote key 7, pay §3).
const (
	TermsPostpaid uint64 = 0
	TermsPrepaid  uint64 = 1
	TermsEscrow   uint64 = 2
	TermsMetered  uint64 = 3
)

// Escrow invoice phases (PayInvoice key 7, pay §5.2.1 — terms 2 only).
const (
	PhaseHold    uint64 = 1
	PhaseRelease uint64 = 2
)

// Receipt kinds (PayReceipt key 5, pay §5.2.1).
const (
	KindSettled  uint64 = 0
	KindHeld     uint64 = 1
	KindReleased uint64 = 2
	KindRefunded uint64 = 3
)

// CreditLedgerMethod is the pay:credit-ledger method URI (pay §5.1).
const CreditLedgerMethod = "pay:credit-ledger"

// Error codes (core §9, 6xxx payment family) surfaced by the runtimes.
const (
	CodeMalformed             uint16 = 1000
	CodeQuoteExpired          uint16 = 6000
	CodePaymentRequired       uint16 = 6001
	CodeSettlementFailed      uint16 = 6002
	CodeMethodUnsupported     uint16 = 6004
	CodeCapExceeded           uint16 = 6005
	CodeAgreementDigestMismat uint16 = 6006
	CodeTaskBindingMismatch   uint16 = 6007
	CodeInvalidPaymentProof   uint16 = 6008
	CodeWatermarkViolation    uint16 = 6009
	CodeEscrowViolation       uint16 = 6010
)

// PayError is a fail-closed codec error carrying the offending field key, so
// tests can assert the exact rejection (e.g. BadField(5) for NV-011).
type PayError struct {
	Kind string // "missing" | "bad" | "amount" | "asset"
	Key  uint64
}

func (e PayError) Error() string {
	switch e.Kind {
	case "amount":
		return "pay: invalid amount encoding"
	case "asset":
		return "pay: invalid asset identifier"
	default:
		return fmt.Sprintf("pay: %s field %d", e.Kind, e.Key)
	}
}

func missingField(k uint64) PayError { return PayError{Kind: "missing", Key: k} }
func badField(k uint64) PayError     { return PayError{Kind: "bad", Key: k} }

var errBadAmount = PayError{Kind: "amount"}
var errBadAsset = PayError{Kind: "asset"}

// PayRefDigest is the pay §3.1 payment-reference digest: multihash-sha256
// (0x12 0x20 || sha256) over a message's RECEIVED ENVELOPE bytes (I1). Same
// construction both impls.
func PayRefDigest(envelopeBytes []byte) []byte {
	sum := sha256.Sum256(envelopeBytes)
	out := make([]byte, 0, 34)
	out = append(out, 0x12, 0x20)
	return append(out, sum[:]...)
}

// --- signed-int + field helpers -------------------------------------------

// asI64 reads a signed integer from a Uint or Int value (the encoder emits
// major-0 for non-negative Int, so both forms appear on the wire).
func asI64(v cbor.Value) (int64, bool) {
	switch x := v.(type) {
	case cbor.Uint:
		if uint64(x) > math.MaxInt64 {
			return 0, false
		}
		return int64(x), true
	case cbor.Int:
		return int64(x), true
	}
	return 0, false
}

func mapGet(m cbor.Map, k uint64) (cbor.Value, bool) {
	for _, p := range m {
		if u, ok := p.K.(cbor.Uint); ok && uint64(u) == k {
			return p.V, true
		}
	}
	return nil, false
}

func reqMap(v cbor.Value) (cbor.Map, error) {
	m, ok := v.(cbor.Map)
	if !ok {
		return nil, errBadAmount
	}
	return m, nil
}

func reqUint(m cbor.Map, k uint64) (uint64, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return 0, missingField(k)
	}
	u, ok := v.(cbor.Uint)
	if !ok {
		return 0, badField(k)
	}
	return uint64(u), nil
}

func reqText(m cbor.Map, k uint64) (string, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return "", missingField(k)
	}
	t, ok := v.(cbor.Text)
	if !ok {
		return "", badField(k)
	}
	return string(t), nil
}

func reqBytes(m cbor.Map, k uint64) ([]byte, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return nil, missingField(k)
	}
	b, ok := v.(cbor.Bytes)
	if !ok {
		return nil, badField(k)
	}
	return append([]byte(nil), b...), nil
}

func reqID16(m cbor.Map, k uint64) ([16]byte, error) {
	var id [16]byte
	b, err := reqBytes(m, k)
	if err != nil {
		return id, err
	}
	if len(b) != 16 {
		return id, badField(k)
	}
	copy(id[:], b)
	return id, nil
}

// --- Amount / AssetAmount (pay §1/§3) --------------------------------------

// Amount is a CBOR decimal fraction (tag 4): value = mantissa × 10^exponent.
type Amount struct {
	Exponent int32
	Mantissa int64
}

// ToValue encodes the tag-4 [exponent, mantissa]. Int encodes identically to
// Uint for non-negative values, matching the Rust Value::int helper and the
// frozen vectors.
func (a Amount) ToValue() cbor.Value {
	return cbor.Tag{Num: 4, Content: cbor.Array{cbor.Int(int64(a.Exponent)), cbor.Int(a.Mantissa)}}
}

// AmountFromValue decodes a tag-4 decimal fraction.
func AmountFromValue(v cbor.Value) (Amount, error) {
	t, ok := v.(cbor.Tag)
	if !ok || t.Num != 4 {
		return Amount{}, errBadAmount
	}
	arr, ok := t.Content.(cbor.Array)
	if !ok || len(arr) != 2 {
		return Amount{}, errBadAmount
	}
	e, ok := asI64(arr[0])
	if !ok || e < math.MinInt32 || e > math.MaxInt32 {
		return Amount{}, errBadAmount
	}
	m, ok := asI64(arr[1])
	if !ok {
		return Amount{}, errBadAmount
	}
	return Amount{Exponent: int32(e), Mantissa: m}, nil
}

// ValidAsset checks the pay §1 asset prefix set.
func ValidAsset(s string) bool {
	for _, p := range []string{"iso4217:", "asset:", "erc20:", "credit:"} {
		if len(s) > len(p) && s[:len(p)] == p {
			return true
		}
	}
	return false
}

// AssetAmount is the [Asset, Amount] pairing used throughout pay.
type AssetAmount struct {
	Asset  string
	Amount Amount
}

// ToValue encodes [asset_text, amount].
func (aa AssetAmount) ToValue() cbor.Value {
	return cbor.Array{cbor.Text(aa.Asset), aa.Amount.ToValue()}
}

// AssetAmountFromValue decodes and validates an [Asset, Amount] array.
func AssetAmountFromValue(v cbor.Value) (AssetAmount, error) {
	arr, ok := v.(cbor.Array)
	if !ok || len(arr) != 2 {
		return AssetAmount{}, errBadAmount
	}
	as, ok := arr[0].(cbor.Text)
	if !ok || !ValidAsset(string(as)) {
		return AssetAmount{}, errBadAsset
	}
	am, err := AmountFromValue(arr[1])
	if err != nil {
		return AssetAmount{}, err
	}
	return AssetAmount{Asset: string(as), Amount: am}, nil
}

func aaAt(m cbor.Map, k uint64) (AssetAmount, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return AssetAmount{}, missingField(k)
	}
	return AssetAmountFromValue(v)
}

// Equal reports byte-equal asset AND exponent AND mantissa.
func (aa AssetAmount) Equal(b AssetAmount) bool {
	return aa.Asset == b.Asset && aa.Amount.Exponent == b.Amount.Exponent && aa.Amount.Mantissa == b.Amount.Mantissa
}

// --- UsageWatermark (PayInvoice key 4, pay §4.1) ---------------------------

// UsageWatermark is carried by metered (terms 3) invoices only; all four keys
// REQUIRED. Reuses tasks.Usage (identical keys 1-4).
type UsageWatermark struct {
	Task            [32]byte
	ThroughSeq      uint64
	Usage           tasks.Usage
	CumulativeBille AssetAmount
}

// ToValue encodes the watermark map.
func (w UsageWatermark) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(w.Task[:])},
		{K: cbor.Uint(2), V: cbor.Uint(w.ThroughSeq)},
		{K: cbor.Uint(3), V: w.Usage.ToValue()},
		{K: cbor.Uint(4), V: w.CumulativeBille.ToValue()},
	}
}

// UsageWatermarkFromValue decodes; every key is required.
func UsageWatermarkFromValue(v cbor.Value) (UsageWatermark, error) {
	var w UsageWatermark
	m, ok := v.(cbor.Map)
	if !ok {
		return w, badField(4)
	}
	tb, err := reqBytes(m, 1)
	if err != nil {
		return w, err
	}
	if len(tb) != 32 {
		return w, badField(1)
	}
	copy(w.Task[:], tb)
	if w.ThroughSeq, err = reqUint(m, 2); err != nil {
		return w, err
	}
	uv, ok := mapGet(m, 3)
	if !ok {
		return w, missingField(3)
	}
	if err = w.Usage.FromValue(uv); err != nil {
		return w, err
	}
	if w.CumulativeBille, err = aaAt(m, 4); err != nil {
		return w, err
	}
	return w, nil
}

// --- MethodOffer (pay §3) --------------------------------------------------

// MethodOffer is one offered settlement method: URI + opaque params.
type MethodOffer struct {
	URI    string
	Params []byte
}

// ToValue encodes {1: uri, 2: params}.
func (mo MethodOffer) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Text(mo.URI)},
		{K: cbor.Uint(2), V: cbor.Bytes(mo.Params)},
	}
}

// MethodOfferFromValue decodes a method offer.
func MethodOfferFromValue(v cbor.Value) (MethodOffer, error) {
	m, ok := v.(cbor.Map)
	if !ok {
		return MethodOffer{}, badField(5)
	}
	uri, err := reqText(m, 1)
	if err != nil {
		return MethodOffer{}, err
	}
	params, err := reqBytes(m, 2)
	if err != nil {
		return MethodOffer{}, err
	}
	return MethodOffer{URI: uri, Params: params}, nil
}
