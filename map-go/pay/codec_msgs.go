package pay

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// RateEntry is one (unit-code, price) row of a PayQuote rate table.
type RateEntry struct {
	Unit   uint64
	Amount AssetAmount
}

// PayQuote is msg-type 0x0400 (pay §3).
type PayQuote struct {
	QuoteID      [16]byte
	PricingModel uint64
	RateTable    []RateEntry
	Total        *AssetAmount // key 4 (REQUIRED under terms 1/2)
	Methods      []MethodOffer
	ExpiryMs     uint64
	Terms        *uint64    // key 7
	Ext          cbor.Value // key 8 (opaque passthrough)
	TaskBinding  *[32]byte  // key 9
}

// ToValue encodes the quote (only present optional keys emitted).
func (q PayQuote) ToValue() cbor.Value {
	var rate cbor.Map
	for _, r := range q.RateTable {
		rate = append(rate, cbor.Pair{K: cbor.Uint(r.Unit), V: r.Amount.ToValue()})
	}
	var methods cbor.Array
	for _, mo := range q.Methods {
		methods = append(methods, mo.ToValue())
	}
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(q.QuoteID[:])},
		{K: cbor.Uint(2), V: cbor.Uint(q.PricingModel)},
		{K: cbor.Uint(3), V: rate},
	}
	if q.Total != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: q.Total.ToValue()})
	}
	m = append(m, cbor.Pair{K: cbor.Uint(5), V: methods})
	m = append(m, cbor.Pair{K: cbor.Uint(6), V: cbor.Uint(q.ExpiryMs)})
	if q.Terms != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(7), V: cbor.Uint(*q.Terms)})
	}
	if q.Ext != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(8), V: q.Ext})
	}
	if q.TaskBinding != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(9), V: cbor.Bytes(q.TaskBinding[:])})
	}
	return m
}

// PayQuoteFromValue decodes a quote.
func PayQuoteFromValue(v cbor.Value) (PayQuote, error) {
	var q PayQuote
	m, ok := v.(cbor.Map)
	if !ok {
		return q, badField(0)
	}
	var err error
	if q.QuoteID, err = reqID16(m, 1); err != nil {
		return q, err
	}
	if q.PricingModel, err = reqUint(m, 2); err != nil {
		return q, err
	}
	rv, ok := mapGet(m, 3)
	if !ok {
		return q, missingField(3)
	}
	rm, ok := rv.(cbor.Map)
	if !ok {
		return q, badField(3)
	}
	for _, p := range rm {
		unit, ok := p.K.(cbor.Uint)
		if !ok {
			return q, badField(3)
		}
		aa, err := AssetAmountFromValue(p.V)
		if err != nil {
			return q, badField(3)
		}
		q.RateTable = append(q.RateTable, RateEntry{Unit: uint64(unit), Amount: aa})
	}
	if tv, ok := mapGet(m, 4); ok {
		aa, err := AssetAmountFromValue(tv)
		if err != nil {
			return q, err
		}
		q.Total = &aa
	}
	mv, ok := mapGet(m, 5)
	if !ok {
		return q, missingField(5)
	}
	ma, ok := mv.(cbor.Array)
	if !ok {
		return q, badField(5)
	}
	for _, e := range ma {
		mo, err := MethodOfferFromValue(e)
		if err != nil {
			return q, err
		}
		q.Methods = append(q.Methods, mo)
	}
	if q.ExpiryMs, err = reqUint(m, 6); err != nil {
		return q, err
	}
	if tv, ok := mapGet(m, 7); ok {
		t, ok := tv.(cbor.Uint)
		if !ok {
			return q, badField(7)
		}
		tt := uint64(t)
		q.Terms = &tt
	}
	if ev, ok := mapGet(m, 8); ok {
		q.Ext = ev
	}
	if bv, ok := mapGet(m, 9); ok {
		b, ok := bv.(cbor.Bytes)
		if !ok || len(b) != 32 {
			return q, badField(9)
		}
		var tb [32]byte
		copy(tb[:], b)
		q.TaskBinding = &tb
	}
	return q, nil
}

// PayAgree is msg-type 0x0401 (pay §3).
type PayAgree struct {
	QuoteID       [16]byte
	QuoteDigest   []byte
	Method        string
	AuthorizedCap *AssetAmount
}

// ToValue encodes the agreement.
func (a PayAgree) ToValue() cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(a.QuoteID[:])},
		{K: cbor.Uint(2), V: cbor.Bytes(a.QuoteDigest)},
		{K: cbor.Uint(3), V: cbor.Text(a.Method)},
	}
	if a.AuthorizedCap != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: a.AuthorizedCap.ToValue()})
	}
	return m
}

// PayAgreeFromValue decodes an agreement.
func PayAgreeFromValue(v cbor.Value) (PayAgree, error) {
	var a PayAgree
	m, ok := v.(cbor.Map)
	if !ok {
		return a, badField(0)
	}
	var err error
	if a.QuoteID, err = reqID16(m, 1); err != nil {
		return a, err
	}
	if a.QuoteDigest, err = reqBytes(m, 2); err != nil {
		return a, err
	}
	if a.Method, err = reqText(m, 3); err != nil {
		return a, err
	}
	if cv, ok := mapGet(m, 4); ok {
		aa, err := AssetAmountFromValue(cv)
		if err != nil {
			return a, err
		}
		a.AuthorizedCap = &aa
	}
	return a, nil
}

// PayInvoice is msg-type 0x0402 (pay §4/§5.2.1).
type PayInvoice struct {
	InvoiceID     [16]byte
	QuoteID       [16]byte
	Amount        AssetAmount
	Watermark     *UsageWatermark // key 4 (terms 3 only)
	MethodPayload []byte
	TimestampMs   uint64
	Phase         *uint64 // key 7 (terms 2 only), fail-closed {1,2}
}

// ToValue encodes the invoice.
func (inv PayInvoice) ToValue() cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(inv.InvoiceID[:])},
		{K: cbor.Uint(2), V: cbor.Bytes(inv.QuoteID[:])},
		{K: cbor.Uint(3), V: inv.Amount.ToValue()},
	}
	if inv.Watermark != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: inv.Watermark.ToValue()})
	}
	m = append(m, cbor.Pair{K: cbor.Uint(5), V: cbor.Bytes(inv.MethodPayload)})
	m = append(m, cbor.Pair{K: cbor.Uint(6), V: cbor.Uint(inv.TimestampMs)})
	if inv.Phase != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(7), V: cbor.Uint(*inv.Phase)})
	}
	return m
}

// PayInvoiceFromValue decodes an invoice; phase (key 7) is fail-closed ∈{1,2}.
func PayInvoiceFromValue(v cbor.Value) (PayInvoice, error) {
	var inv PayInvoice
	m, ok := v.(cbor.Map)
	if !ok {
		return inv, badField(0)
	}
	var err error
	if inv.InvoiceID, err = reqID16(m, 1); err != nil {
		return inv, err
	}
	if inv.QuoteID, err = reqID16(m, 2); err != nil {
		return inv, err
	}
	if inv.Amount, err = aaAt(m, 3); err != nil {
		return inv, err
	}
	if wv, ok := mapGet(m, 4); ok {
		w, err := UsageWatermarkFromValue(wv)
		if err != nil {
			return inv, err
		}
		inv.Watermark = &w
	}
	if inv.MethodPayload, err = reqBytes(m, 5); err != nil {
		return inv, err
	}
	if inv.TimestampMs, err = reqUint(m, 6); err != nil {
		return inv, err
	}
	if pv, ok := mapGet(m, 7); ok {
		p, ok := pv.(cbor.Uint)
		if !ok || (uint64(p) != PhaseHold && uint64(p) != PhaseRelease) {
			return inv, badField(7)
		}
		pp := uint64(p)
		inv.Phase = &pp
	}
	return inv, nil
}

// PayProof is msg-type 0x0403 (pay §4).
type PayProof struct {
	InvoiceID    [16]byte
	Method       string
	ProofPayload []byte
}

// ToValue encodes the proof.
func (p PayProof) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(p.InvoiceID[:])},
		{K: cbor.Uint(2), V: cbor.Text(p.Method)},
		{K: cbor.Uint(3), V: cbor.Bytes(p.ProofPayload)},
	}
}

// PayProofFromValue decodes a proof.
func PayProofFromValue(v cbor.Value) (PayProof, error) {
	var p PayProof
	m, ok := v.(cbor.Map)
	if !ok {
		return p, badField(0)
	}
	var err error
	if p.InvoiceID, err = reqID16(m, 1); err != nil {
		return p, err
	}
	if p.Method, err = reqText(m, 2); err != nil {
		return p, err
	}
	if p.ProofPayload, err = reqBytes(m, 3); err != nil {
		return p, err
	}
	return p, nil
}

// PayReceipt is msg-type 0x0404 (pay §4/§5.2.1).
type PayReceipt struct {
	InvoiceID   [16]byte
	Amount      AssetAmount
	TimestampMs uint64
	ProofDigest []byte  // key 4 (optional)
	Kind        *uint64 // key 5 (optional), fail-closed ≤3
}

// ToValue encodes the receipt.
func (r PayReceipt) ToValue() cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(r.InvoiceID[:])},
		{K: cbor.Uint(2), V: r.Amount.ToValue()},
		{K: cbor.Uint(3), V: cbor.Uint(r.TimestampMs)},
	}
	if r.ProofDigest != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: cbor.Bytes(r.ProofDigest)})
	}
	if r.Kind != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: cbor.Uint(*r.Kind)})
	}
	return m
}

// PayReceiptFromValue decodes a receipt; kind (key 5) is fail-closed ≤3.
func PayReceiptFromValue(v cbor.Value) (PayReceipt, error) {
	var r PayReceipt
	m, ok := v.(cbor.Map)
	if !ok {
		return r, badField(0)
	}
	var err error
	if r.InvoiceID, err = reqID16(m, 1); err != nil {
		return r, err
	}
	if r.Amount, err = aaAt(m, 2); err != nil {
		return r, err
	}
	if r.TimestampMs, err = reqUint(m, 3); err != nil {
		return r, err
	}
	if dv, ok := mapGet(m, 4); ok {
		b, ok := dv.(cbor.Bytes)
		if !ok {
			return r, badField(4)
		}
		r.ProofDigest = append([]byte(nil), b...)
	}
	if kv, ok := mapGet(m, 5); ok {
		k, ok := kv.(cbor.Uint)
		if !ok || uint64(k) > KindRefunded {
			return r, badField(5)
		}
		kk := uint64(k)
		r.Kind = &kk
	}
	return r, nil
}

// PayQuoteRef / PayAgreeRef are the (quote-id, digest) reference pairs (pay
// §3.1) — typed versions of the opaque cbor.Value carried in Offer/Quote/
// Assign task messages.
type PayQuoteRef struct {
	QuoteID [16]byte
	Digest  []byte
}

// PayAgreeRef references a PAY_AGREE by (quote-id, envelope digest).
type PayAgreeRef struct {
	QuoteID [16]byte
	Digest  []byte
}

func idDigestToValue(id [16]byte, digest []byte) cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(id[:])},
		{K: cbor.Uint(2), V: cbor.Bytes(digest)},
	}
}

// ToValue encodes {1: quote-id, 2: digest}.
func (r PayQuoteRef) ToValue() cbor.Value { return idDigestToValue(r.QuoteID, r.Digest) }

// ToValue encodes {1: quote-id, 2: digest}.
func (r PayAgreeRef) ToValue() cbor.Value { return idDigestToValue(r.QuoteID, r.Digest) }

func idDigestFromValue(v cbor.Value) ([16]byte, []byte, error) {
	var id [16]byte
	m, ok := v.(cbor.Map)
	if !ok {
		return id, nil, badField(0)
	}
	id, err := reqID16(m, 1)
	if err != nil {
		return id, nil, err
	}
	digest, err := reqBytes(m, 2)
	if err != nil {
		return id, nil, err
	}
	return id, digest, nil
}

// PayQuoteRefFromValue decodes a quote reference.
func PayQuoteRefFromValue(v cbor.Value) (PayQuoteRef, error) {
	id, d, err := idDigestFromValue(v)
	return PayQuoteRef{QuoteID: id, Digest: d}, err
}

// PayAgreeRefFromValue decodes an agreement reference.
func PayAgreeRefFromValue(v cbor.Value) (PayAgreeRef, error) {
	id, d, err := idDigestFromValue(v)
	return PayAgreeRef{QuoteID: id, Digest: d}, err
}
