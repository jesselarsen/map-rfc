package pay_test

// In-process runtime legs mirroring the Rust payments_flow / metered_flow /
// prepaid escrow_flow test matrices. The two FSMs are pure pumps, so cases
// drive them by shuttling (msg-type, content) directly; the shared "envelope"
// bytes both sides digest is a synthetic deterministic frame (the digest only
// needs to match between the provider's BindQuoteEnvelope and the requester's
// OnQuote raw — exactly as the live session's sealed bytes do).

import (
	"crypto/ed25519"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/pay"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

func aid(t *testing.T, seed byte) identity.AgentID {
	t.Helper()
	s := make([]byte, 32)
	s[0] = seed
	priv := ed25519.NewKeyFromSeed(s)
	a, err := identity.FromPub(priv.Public().(ed25519.PublicKey))
	if err != nil {
		t.Fatal(err)
	}
	return a
}

func operator(t *testing.T, seed byte, exp int32, payer identity.AgentID, credit int64) *pay.LedgerOperator {
	t.Helper()
	s := make([]byte, 32)
	s[0] = seed
	op, err := pay.NewLedgerOperator(s, exp)
	if err != nil {
		t.Fatal(err)
	}
	op.Credit(payer, credit)
	return op
}

func usd2(m int64) pay.AssetAmount {
	return pay.AssetAmount{Asset: "iso4217:USD", Amount: pay.Amount{Exponent: -2, Mantissa: m}}
}
func micro(m int64) pay.AssetAmount {
	return pay.AssetAmount{Asset: "iso4217:USD", Amount: pay.Amount{Exponent: -6, Mantissa: m}}
}

func errCode(t *testing.T, out []tasks.Outgoing) uint16 {
	t.Helper()
	if len(out) != 1 || out[0].MsgType != 0x0006 {
		t.Fatalf("expected one ERROR, got %+v", out)
	}
	m := out[0].Content.(cbor.Map)
	for _, p := range m {
		if k, ok := p.K.(cbor.Uint); ok && uint64(k) == 1 {
			return uint16(p.V.(cbor.Uint))
		}
	}
	t.Fatal("ERROR has no code")
	return 0
}

// --- postpaid --------------------------------------------------------------

func TestPostpaidHappyPathAndIdempotency(t *testing.T) {
	prov, req, op := aid(t, 2), aid(t, 1), aid(t, 4)
	pp := pay.NewProviderPayment(prov, req, op)
	task := [32]byte{0xAB}
	rp := pay.NewRequesterPayment(&task, req, prov)
	ledger := operator(t, 4, -2, req, 1000)

	// Quote (bind the synthetic sealed bytes) → agree.
	qout := pp.IssueQuote([16]byte{0x10}, task, usd2(500), 1<<62)
	qraw := cbor.Encode(qout[0].Content)
	pp.BindQuoteEnvelope(qraw)
	aout := rp.OnQuote(qout[0].Content, qraw, 1)
	if len(aout) != 1 || aout[0].MsgType != pay.MsgPayAgree {
		t.Fatalf("expected PAY_AGREE, got %+v", aout)
	}
	if out := pp.OnAgree(aout[0].Content); out != nil {
		t.Fatalf("on_agree should be silent, got %+v", out)
	}
	// Invoice → pay → receipt.
	iout := pp.Invoice([16]byte{0x20}, 2)
	pout := rp.OnInvoice(iout[0].Content, ledger, 3)
	if len(pout) != 1 || pout[0].MsgType != pay.MsgPayProof {
		t.Fatalf("expected PAY_PROOF, got %+v", pout)
	}
	rout := pp.OnProof(pout[0].Content, 4)
	if len(rout) != 1 || rout[0].MsgType != pay.MsgPayReceipt {
		t.Fatalf("expected PAY_RECEIPT, got %+v", rout)
	}
	if ledger.Balance(req) != 500 || ledger.Balance(prov) != 500 {
		t.Errorf("balances not moved once: payer=%d payee=%d", ledger.Balance(req), ledger.Balance(prov))
	}
	// Idempotent re-proof: same receipt, no second settlement.
	rout2 := pp.OnProof(pout[0].Content, 5)
	if len(rout2) != 1 || rout2[0].MsgType != pay.MsgPayReceipt {
		t.Fatalf("re-proof must re-yield receipt, got %+v", rout2)
	}
	if ledger.Balance(req) != 500 {
		t.Errorf("re-proof moved money again: %d", ledger.Balance(req))
	}
}

func TestPostpaidOverCapRejected6005(t *testing.T) {
	prov, req, op := aid(t, 2), aid(t, 1), aid(t, 4)
	pp := pay.NewProviderPayment(prov, req, op)
	task := [32]byte{0xAB}
	rp := pay.NewRequesterPayment(&task, req, prov)
	ledger := operator(t, 4, -2, req, 1000)
	qout := pp.IssueQuote([16]byte{0x10}, task, usd2(500), 1<<62)
	qraw := cbor.Encode(qout[0].Content)
	pp.BindQuoteEnvelope(qraw)
	rp.OnQuote(qout[0].Content, qraw, 1)
	// Provider (mis)invoices 999 > cap 500 → requester refuses, no money moves.
	bad := pay.PayInvoice{InvoiceID: [16]byte{0x20}, QuoteID: [16]byte{0x10}, Amount: usd2(999), MethodPayload: []byte{}, TimestampMs: 3}
	out := rp.OnInvoice(bad.ToValue(), ledger, 3)
	if errCode(t, out) != pay.CodeCapExceeded {
		t.Errorf("want 6005, got %d", errCode(t, out))
	}
	if ledger.Balance(req) != 1000 {
		t.Errorf("money moved on over-cap: %d", ledger.Balance(req))
	}
}

// --- prepaid ---------------------------------------------------------------

func prepaidQuote(task [32]byte) pay.PayQuote {
	terms := pay.TermsPrepaid
	total := usd2(500)
	tb := task
	return pay.PayQuote{
		QuoteID: [16]byte{0x50}, PricingModel: 0,
		RateTable: []pay.RateEntry{{Unit: 0, Amount: total}}, Total: &total,
		Methods:  []pay.MethodOffer{{URI: pay.CreditLedgerMethod, Params: []byte{}}},
		ExpiryMs: 1 << 62, Terms: &terms, TaskBinding: &tb,
	}
}

func TestPrepaidGateAndSettlement(t *testing.T) {
	prov, req, op := aid(t, 2), aid(t, 1), aid(t, 4)
	task := [32]byte{0xE}
	q := prepaidQuote(task)
	pp := pay.NewPrepaidProviderPayment(prov, req, op, q)
	rp := pay.NewPrepaidRequesterPayment(task, req, prov)
	ledger := operator(t, 4, -2, req, 1000)

	if pp.ExecutionPermitted() {
		t.Fatal("gate open before payment")
	}
	qraw := cbor.Encode(q.ToValue())
	pp.BindQuoteEnvelope(qraw)
	aout := rp.OnQuote(q.ToValue(), qraw, 1)
	if len(aout) != 1 || aout[0].MsgType != pay.MsgPayAgree {
		t.Fatalf("expected PAY_AGREE, got %+v", aout)
	}
	pp.OnAgree(aout[0].Content)
	iout := pp.Invoice([16]byte{0x60}, 2)
	pout := rp.OnInvoice(iout[0].Content, ledger, 3)
	if len(pout) != 1 || pout[0].MsgType != pay.MsgPayProof {
		t.Fatalf("expected PAY_PROOF, got %+v", pout)
	}
	rout := pp.OnProof(pout[0].Content, 4)
	if len(rout) != 1 || rout[0].MsgType != pay.MsgPayReceipt {
		t.Fatalf("expected PAY_RECEIPT, got %+v", rout)
	}
	if r, _ := pay.PayReceiptFromValue(rout[0].Content); r.Kind != nil {
		t.Error("prepaid receipt must be settled (kind absent)")
	}
	if !pp.ExecutionPermitted() {
		t.Error("gate did not open after settlement")
	}
	if ledger.Balance(req) != 500 || ledger.Balance(prov) != 500 {
		t.Errorf("balances wrong: %d/%d", ledger.Balance(req), ledger.Balance(prov))
	}
}

func TestPrepaidWrongAmountRejected6010(t *testing.T) {
	prov, req, op := aid(t, 2), aid(t, 1), aid(t, 4)
	task := [32]byte{0xE}
	q := prepaidQuote(task)
	pp := pay.NewPrepaidProviderPayment(prov, req, op, q)
	rp := pay.NewPrepaidRequesterPayment(task, req, prov)
	ledger := operator(t, 4, -2, req, 1000)
	qraw := cbor.Encode(q.ToValue())
	pp.BindQuoteEnvelope(qraw)
	aout := rp.OnQuote(q.ToValue(), qraw, 1)
	pp.OnAgree(aout[0].Content)
	bad := pay.PayInvoice{InvoiceID: [16]byte{0x61}, QuoteID: [16]byte{0x50}, Amount: usd2(499), MethodPayload: []byte{}, TimestampMs: 3}
	out := rp.OnInvoice(bad.ToValue(), ledger, 3)
	if errCode(t, out) != pay.CodeEscrowViolation {
		t.Errorf("want 6010, got %d", errCode(t, out))
	}
	if ledger.Balance(req) != 1000 {
		t.Error("money moved on wrong-amount prepaid")
	}
}

// --- metered ---------------------------------------------------------------

func meteredQuote(task [32]byte) pay.PayQuote {
	terms := pay.TermsMetered
	total := micro(10_000_000)
	tb := task
	return pay.PayQuote{
		QuoteID: [16]byte{0x40}, PricingModel: 1,
		RateTable: []pay.RateEntry{{Unit: pay.UnitTokenIn, Amount: micro(3000)}, {Unit: pay.UnitTokenOut, Amount: micro(5000)}},
		Total:     &total,
		Methods:   []pay.MethodOffer{{URI: pay.CreditLedgerMethod, Params: []byte{}}},
		ExpiryMs:  1 << 62, Terms: &terms, TaskBinding: &tb,
	}
}

func TestMeteredTwoIncrementalInvoices(t *testing.T) {
	prov, req, opAID := aid(t, 2), aid(t, 1), aid(t, 4)
	task := [32]byte{0x9}
	q := meteredQuote(task)
	pp := pay.NewMeteredProviderPayment(prov, req, opAID, task, q)
	rp := pay.NewMeteredRequesterPayment(task, req, prov)
	ledger := operator(t, 4, -6, req, 5_000_000)

	qraw := cbor.Encode(q.ToValue())
	pp.BindQuoteEnvelope(qraw)
	aout := rp.OnQuote(q.ToValue(), qraw, 1)
	pp.OnAgree(aout[0].Content)

	round := func(u tasks.Usage, seq uint64, id byte) {
		billed := pp.Bill(u, seq, [16]byte{id}, 1)
		if len(billed) != 2 || billed[0].MsgType != tasks.MsgChunk || billed[1].MsgType != pay.MsgPayInvoice {
			t.Fatalf("bill must emit CHUNK+INVOICE, got %+v", billed)
		}
		rp.OnChunk(billed[0].Content)
		pout := rp.OnInvoice(billed[1].Content, ledger, 1)
		if len(pout) != 1 || pout[0].MsgType != pay.MsgPayProof {
			t.Fatalf("expected PAY_PROOF, got %+v", pout)
		}
		rout := pp.OnProof(pout[0].Content, 1)
		if len(rout) != 1 || rout[0].MsgType != pay.MsgPayReceipt {
			t.Fatalf("expected PAY_RECEIPT, got %+v", rout)
		}
		rp.OnReceipt(rout[0].Content)
	}
	// Two cumulative watermarks: 100/40 then 200/80 tokens.
	round(tasks.Usage{TokensIn: 100, TokensOut: 40, WallMs: 1000}, 4, 0x01) // 100*3000+40*5000 = 500_000
	round(tasks.Usage{TokensIn: 200, TokensOut: 80, WallMs: 2000}, 9, 0x02) // 1_000_000 cumulative; delta 500_000
	if len(pp.Receipts()) != 2 || len(rp.Receipts()) != 2 {
		t.Fatalf("want 2 receipts each, got %d/%d", len(pp.Receipts()), len(rp.Receipts()))
	}
	// payer paid 1_000_000 total.
	if ledger.Balance(req) != 4_000_000 || ledger.Balance(prov) != 1_000_000 {
		t.Errorf("metered balances: payer=%d payee=%d", ledger.Balance(req), ledger.Balance(prov))
	}
}

func TestMeteredInflatedUsageRejected6009(t *testing.T) {
	prov, req, opAID := aid(t, 2), aid(t, 1), aid(t, 4)
	task := [32]byte{0x9}
	q := meteredQuote(task)
	pp := pay.NewMeteredProviderPayment(prov, req, opAID, task, q)
	rp := pay.NewMeteredRequesterPayment(task, req, prov)
	ledger := operator(t, 4, -6, req, 10_000_000)
	qraw := cbor.Encode(q.ToValue())
	pp.BindQuoteEnvelope(qraw)
	rp.OnQuote(q.ToValue(), qraw, 1)
	pp.OnAgree(pay.PayAgree{QuoteID: q.QuoteID, QuoteDigest: pay.PayRefDigest(qraw), Method: pay.CreditLedgerMethod}.ToValue())

	// Meter 100/40 but the provider bills a watermark claiming 200/80 (not metered).
	billed := pp.Bill(tasks.Usage{TokensIn: 100, TokensOut: 40, WallMs: 1000}, 4, [16]byte{0x01}, 1)
	rp.OnChunk(billed[0].Content)
	forged := pay.PayInvoice{
		InvoiceID: [16]byte{0x02}, QuoteID: q.QuoteID, Amount: micro(1_000_000),
		Watermark:     &pay.UsageWatermark{Task: task, ThroughSeq: 4, Usage: tasks.Usage{TokensIn: 200, TokensOut: 80, WallMs: 1000}, CumulativeBille: micro(1_000_000)},
		MethodPayload: []byte{}, TimestampMs: 1,
	}
	out := rp.OnInvoice(forged.ToValue(), ledger, 1)
	if errCode(t, out) != pay.CodeWatermarkViolation {
		t.Errorf("meter-consistency: want 6009, got %d", errCode(t, out))
	}
}
