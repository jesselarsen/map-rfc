package pay_test

// Codec tests use the FROZEN conformance vectors as byte oracles: decode each
// pay vector with the pay codec, assert the expected fields, and re-encode to
// prove byte-for-byte round-trip against the sealed content. This is the
// cross-impl guarantee — the Go pay codec produces exactly the bytes the Rust
// map-pay codec and the Python vector generator produce.

import (
	"bytes"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/conformance"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/pay"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

func openContent(t *testing.T, vectors map[string][]byte, id string) cbor.Value {
	t.Helper()
	env, ok := vectors[id]
	if !ok {
		t.Fatalf("%s: not produced", id)
	}
	o, err := envelope.Open(env, nil)
	if err != nil {
		t.Fatalf("%s: open: %v", id, err)
	}
	return o.Body.Content
}

// roundTrips re-encodes a decoded pay message and asserts byte-equality with
// the sealed content bytes.
func roundTrips(t *testing.T, id string, content cbor.Value, reencoded cbor.Value) {
	t.Helper()
	if !bytes.Equal(cbor.Encode(content), cbor.Encode(reencoded)) {
		t.Errorf("%s: pay codec did not round-trip byte-for-byte", id)
	}
}

func TestTV016MeteredInvoice(t *testing.T) {
	v := conformance.Produce()
	content := openContent(t, v, "TV-016")
	inv, err := pay.PayInvoiceFromValue(content)
	if err != nil {
		t.Fatalf("TV-016 decode: %v", err)
	}
	if inv.Watermark == nil {
		t.Fatal("TV-016 missing watermark (key 4)")
	}
	// Model-1 rates 3000/5000 µUSD: 1523*3000 + 812*5000 = 8_629_000.
	if got := inv.Watermark.CumulativeBille.Amount.Mantissa; got != 8_629_000 {
		t.Errorf("TV-016 cumulative-billed = %d, want 8629000", got)
	}
	if inv.Amount.Amount.Exponent != -6 {
		t.Errorf("TV-016 amount exponent = %d, want -6", inv.Amount.Amount.Exponent)
	}
	roundTrips(t, "TV-016", content, inv.ToValue())
}

func TestTV017EscrowHoldInvoice(t *testing.T) {
	v := conformance.Produce()
	content := openContent(t, v, "TV-017")
	inv, err := pay.PayInvoiceFromValue(content)
	if err != nil {
		t.Fatalf("TV-017 decode: %v", err)
	}
	if inv.Phase == nil || *inv.Phase != pay.PhaseHold {
		t.Errorf("TV-017 phase != hold: %v", inv.Phase)
	}
	if inv.Watermark != nil {
		t.Error("TV-017 must not carry a watermark (presence iff terms 3)")
	}
	if inv.Amount.Amount.Exponent != -2 || inv.Amount.Amount.Mantissa != 500 {
		t.Errorf("TV-017 amount = %+v, want [-2,500]", inv.Amount.Amount)
	}
	roundTrips(t, "TV-017", content, inv.ToValue())
}

func TestTV018HeldReceipt(t *testing.T) {
	v := conformance.Produce()
	content := openContent(t, v, "TV-018")
	r, err := pay.PayReceiptFromValue(content)
	if err != nil {
		t.Fatalf("TV-018 decode: %v", err)
	}
	if r.Kind == nil || *r.Kind != pay.KindHeld {
		t.Errorf("TV-018 kind != held: %v", r.Kind)
	}
	roundTrips(t, "TV-018", content, r.ToValue())
}

func TestTV013PayAgreeAndTV010bQuote(t *testing.T) {
	v := conformance.Produce()
	q, err := pay.PayQuoteFromValue(openContent(t, v, "TV-010b"))
	if err != nil {
		t.Fatalf("TV-010b quote decode: %v", err)
	}
	if q.TaskBinding == nil {
		t.Error("TV-010b bound quote must carry task binding (key 9)")
	}
	roundTrips(t, "TV-010b", openContent(t, v, "TV-010b"), q.ToValue())

	a, err := pay.PayAgreeFromValue(openContent(t, v, "TV-013"))
	if err != nil {
		t.Fatalf("TV-013 agree decode: %v", err)
	}
	if a.Method != pay.CreditLedgerMethod {
		t.Errorf("TV-013 method = %q, want credit-ledger", a.Method)
	}
	roundTrips(t, "TV-013", openContent(t, v, "TV-013"), a.ToValue())
}

// NV-011 shape: a receipt with kind = 9 MUST be rejected fail-closed (6010).
func TestReceiptKindFailClosed(t *testing.T) {
	v := conformance.Produce()
	nv011, ok := v["NV-011"]
	if !ok {
		t.Skip("NV-011 not produced")
	}
	o, err := envelope.Open(nv011, nil)
	if err != nil {
		t.Fatalf("NV-011 must open (kind-registry reject, not wire): %v", err)
	}
	if _, err := pay.PayReceiptFromValue(o.Body.Content); err == nil {
		t.Error("NV-011 kind 9 must be rejected fail-closed")
	}
}

// The postpaid PayInvoice (no watermark, no phase) round-trips and is
// byte-stable — the regression that the optional keys never appear when unset.
func TestPostpaidInvoiceRoundTrip(t *testing.T) {
	usd := pay.AssetAmount{Asset: "iso4217:USD", Amount: pay.Amount{Exponent: -2, Mantissa: 500}}
	inv := pay.PayInvoice{
		InvoiceID: [16]byte{2}, QuoteID: [16]byte{1}, Amount: usd,
		MethodPayload: []byte{}, TimestampMs: 1,
	}
	got, err := pay.PayInvoiceFromValue(inv.ToValue())
	if err != nil {
		t.Fatal(err)
	}
	if got.Watermark != nil || got.Phase != nil {
		t.Error("postpaid invoice must decode with no watermark/phase")
	}
	if !bytes.Equal(cbor.Encode(inv.ToValue()), cbor.Encode(got.ToValue())) {
		t.Error("postpaid invoice not byte-stable")
	}
}

// Billing arithmetic parity: the frozen constant and the NV-010 off-by-one.
func TestBillingArithmetic(t *testing.T) {
	usd := func(m int64) pay.AssetAmount {
		return pay.AssetAmount{Asset: "iso4217:USD", Amount: pay.Amount{Exponent: -6, Mantissa: m}}
	}
	rates := []pay.RateEntry{{Unit: pay.UnitTokenIn, Amount: usd(3000)}, {Unit: pay.UnitTokenOut, Amount: usd(5000)}}
	usage := tasks.Usage{TokensIn: 1523, TokensOut: 812, TTFTMs: 431, WallMs: 42000}
	got, err := pay.Billed(1, rates, usage)
	if err != nil {
		t.Fatal(err)
	}
	if got.Amount.Mantissa != 8_629_000 {
		t.Errorf("billed = %d, want 8629000", got.Amount.Mantissa)
	}
	// A watermark claiming 8_629_001 must be rejected 6009 (NV-010 shape).
	agreed := pay.AgreedTerms{Task: [32]byte{0xAB}, PricingModel: 1, RateTable: rates, Terms: 3}
	w := pay.UsageWatermark{Task: [32]byte{0xAB}, ThroughSeq: 8, Usage: usage, CumulativeBille: usd(8_629_001)}
	inv := pay.PayInvoice{InvoiceID: [16]byte{1}, QuoteID: [16]byte{2}, Amount: usd(8_629_001), Watermark: &w}
	if code := pay.VerifyWatermark(agreed, nil, inv); code != pay.CodeWatermarkViolation {
		t.Errorf("NV-010 shape: got code %d, want 6009", code)
	}
	// The correct watermark passes.
	w.CumulativeBille = usd(8_629_000)
	inv.Amount = usd(8_629_000)
	inv.Watermark = &w
	if code := pay.VerifyWatermark(agreed, nil, inv); code != 0 {
		t.Errorf("correct watermark rejected with %d", code)
	}
}
