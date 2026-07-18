package pay

// Metered-incremental (terms 3) pay:credit-ledger flow (pay §4/§4.1), a
// byte-for-byte port of map-rs/crates/map-taskrun/src/metered.rs. The provider
// meters as it streams — a kind-3 usage-update CHUNK carrying cumulative Usage,
// then a PAY_INVOICE with the delta amount and the cumulative UsageWatermark —
// and the requester runs the six pay §4.1 checks IN ORDER before paying.
//
// Pause/resume/cancel (pay §4.2): no new message type. Exhaustion is proof
// absence; the provider's Pause emits ERROR 6001 + {1: UsageWatermark} detail;
// resume is the covering PAY_PROOF (the provider clears its paused flag on
// OnProof and streams on). While paused Bill is a no-op.

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

// MeteredProviderState is the metered provider FSM state.
type MeteredProviderState int

// Metered provider states.
const (
	MeteredProvAwaitAgree MeteredProviderState = iota
	MeteredProvStreaming
	MeteredProvPaused
	MeteredProvCancelled
)

type billedInvoice struct {
	amount    AssetAmount
	watermark UsageWatermark
}

// MeteredProviderPayment is the terms-3 payee runtime.
type MeteredProviderPayment struct {
	providerAID identity.AgentID
	requesterAI identity.AgentID
	operatorAID identity.AgentID
	task        [32]byte
	quote       PayQuote
	quoteDigest []byte
	settled     map[[16]byte]bool
	receipts    []PayReceipt
	billed      map[[16]byte]billedInvoice
	emitted     map[[16]byte][]tasks.Outgoing
	lastCumul   int64
	pausedOn    *UsageWatermark
	state       MeteredProviderState
}

// NewMeteredProviderPayment builds a terms-3 provider from a terms-3, metered
// (model 1/2/3) quote.
func NewMeteredProviderPayment(provider, requester, operator identity.AgentID, task [32]byte, quote PayQuote) *MeteredProviderPayment {
	return &MeteredProviderPayment{
		providerAID: provider, requesterAI: requester, operatorAID: operator, task: task, quote: quote,
		settled: make(map[[16]byte]bool), billed: make(map[[16]byte]billedInvoice),
		emitted: make(map[[16]byte][]tasks.Outgoing), state: MeteredProvAwaitAgree,
	}
}

// State returns the FSM state.
func (p *MeteredProviderPayment) State() MeteredProviderState { return p.state }

// Receipts returns the emitted receipts.
func (p *MeteredProviderPayment) Receipts() []PayReceipt { return p.receipts }

// BindQuoteEnvelope records the sealed PAY_QUOTE bytes (I1).
func (p *MeteredProviderPayment) BindQuoteEnvelope(envelope []byte) {
	p.quoteDigest = PayRefDigest(envelope)
}

// OnAgree verifies the PAY_AGREE binds this quote and picks the method.
func (p *MeteredProviderPayment) OnAgree(content cbor.Value) []tasks.Outgoing {
	agree, err := PayAgreeFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if agree.Method != CreditLedgerMethod {
		return errOut(CodeMethodUnsupported)
	}
	if agree.QuoteID != p.quote.QuoteID || !bytesEqual(agree.QuoteDigest, p.quoteDigest) {
		return errOut(CodeAgreementDigestMismat)
	}
	p.state = MeteredProvStreaming
	return nil
}

func (p *MeteredProviderPayment) billCumulative(usage tasks.Usage) AssetAmount {
	aa, err := Billed(p.quote.PricingModel, p.quote.RateTable, usage)
	if err != nil {
		panic("provider rates bill exactly")
	}
	return aa
}

// Bill emits a kind-3 usage-update CHUNK (cumulative Usage) followed by a
// PAY_INVOICE for the delta since the last invoice, carrying the cumulative
// watermark. Idempotent by invoiceID: a re-billed id re-emits identical
// outgoings and does not re-advance the cumulative basis. No-op while paused.
func (p *MeteredProviderPayment) Bill(usage tasks.Usage, throughSeq uint64, invoiceID [16]byte, nowMs uint64) []tasks.Outgoing {
	if out, ok := p.emitted[invoiceID]; ok {
		return out
	}
	if p.state == MeteredProvPaused {
		return nil
	}
	cumulative := p.billCumulative(usage)
	delta := AssetAmount{Asset: cumulative.Asset, Amount: Amount{Exponent: cumulative.Amount.Exponent, Mantissa: cumulative.Amount.Mantissa - p.lastCumul}}
	watermark := UsageWatermark{Task: p.task, ThroughSeq: throughSeq, Usage: usage, CumulativeBille: cumulative}
	chunk := tasks.ChunkMsg{Task: tasks.TaskID(p.task), Seq: throughSeq, Kind: tasks.ChunkUsage, Content: cbor.Encode(usage.ToValue()), Fin: false}
	inv := PayInvoice{InvoiceID: invoiceID, QuoteID: p.quote.QuoteID, Amount: delta, Watermark: &watermark, MethodPayload: []byte{}, TimestampMs: nowMs}
	out := []tasks.Outgoing{
		{MsgType: tasks.MsgChunk, Content: chunk.ToValue()},
		{MsgType: MsgPayInvoice, Content: inv.ToValue()},
	}
	p.billed[invoiceID] = billedInvoice{amount: delta, watermark: watermark}
	p.emitted[invoiceID] = out
	p.lastCumul = cumulative.Amount.Mantissa
	return out
}

// OnProof verifies a PAY_PROOF's ledger entry, settles the invoice once, and
// replies PAY_RECEIPT. Clears a pause when the covering invoice settles.
func (p *MeteredProviderPayment) OnProof(content cbor.Value, nowMs uint64) []tasks.Outgoing {
	proof, err := PayProofFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	bi, ok := p.billed[proof.InvoiceID]
	if !ok {
		return errOut(CodeSettlementFailed)
	}
	entry, err := EntryProofFromBytes(proof.ProofPayload)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if !VerifyEntry(entry, p.operatorAID, proof.InvoiceID, p.requesterAI, p.providerAID, bi.amount) {
		return errOut(CodeInvalidPaymentProof)
	}
	var out []tasks.Outgoing
	if p.settled[proof.InvoiceID] {
		for i := range p.receipts {
			if p.receipts[i].InvoiceID == proof.InvoiceID {
				out = []tasks.Outgoing{{MsgType: MsgPayReceipt, Content: p.receipts[i].ToValue()}}
				break
			}
		}
		if out == nil {
			out = errOut(CodeSettlementFailed)
		}
	} else {
		p.settled[proof.InvoiceID] = true
		rcpt := PayReceipt{InvoiceID: proof.InvoiceID, Amount: bi.amount, TimestampMs: nowMs, ProofDigest: PayRefDigest(proof.ProofPayload)}
		p.receipts = append(p.receipts, rcpt)
		out = []tasks.Outgoing{{MsgType: MsgPayReceipt, Content: rcpt.ToValue()}}
	}
	// Resume: if the settled invoice was the one we paused on, un-pause.
	if p.pausedOn != nil && watermarkEqual(*p.pausedOn, bi.watermark) {
		p.pausedOn = nil
		p.state = MeteredProvStreaming
	}
	return out
}

// Pause pauses an exhausted stream (pay §4.1): ERROR 6001 (retryable) whose
// detail carries the final unpaid watermark as {1: UsageWatermark}.
func (p *MeteredProviderPayment) Pause(invoiceID [16]byte) []tasks.Outgoing {
	bi, ok := p.billed[invoiceID]
	if !ok {
		return nil
	}
	w := bi.watermark
	p.pausedOn = &w
	p.state = MeteredProvPaused
	detail := cbor.Map{{K: cbor.Uint(1), V: w.ToValue()}}
	return []tasks.Outgoing{{MsgType: msgError, Content: cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(CodePaymentRequired)},
		{K: cbor.Uint(2), V: cbor.Bool(true)},
		{K: cbor.Uint(5), V: detail},
	}}}
}

// OwedAtCheckpoint is the amount owed if the task is CANCELled now, billed from
// the last countersigned checkpoint's usage (pay §4.1; tasks §8.2 key 5).
func (p *MeteredProviderPayment) OwedAtCheckpoint(usage tasks.Usage) AssetAmount {
	return p.billCumulative(usage)
}

func watermarkEqual(a, b UsageWatermark) bool {
	return a.Task == b.Task && a.ThroughSeq == b.ThroughSeq && a.Usage == b.Usage && a.CumulativeBille.Equal(b.CumulativeBille)
}

// ---- requester ------------------------------------------------------------

// MeteredRequesterState is the terms-3 requester FSM state.
type MeteredRequesterState int

// Metered requester states.
const (
	MeteredReqAwaitQuote MeteredRequesterState = iota
	MeteredReqAgreed
	MeteredReqPaying
	MeteredReqInsufficient
	MeteredReqFailed
)

type meterReading struct {
	seq   uint64
	usage tasks.Usage
}

// MeteredRequesterPayment is the terms-3 payer runtime.
type MeteredRequesterPayment struct {
	expectedTask  [32]byte
	requesterAI   identity.AgentID
	providerAID   identity.AgentID
	model         uint64
	rateTable     []RateEntry
	authorizedCap *AssetAmount
	quoteID       *[16]byte
	meters        []meterReading
	lastAccepted  *UsageWatermark
	accepted      map[[16]byte]AssetAmount
	pendingUnpaid *PayInvoice
	receipts      []PayReceipt
	state         MeteredRequesterState
}

// NewMeteredRequesterPayment builds a terms-3 requester runtime.
func NewMeteredRequesterPayment(expectedTask [32]byte, requester, provider identity.AgentID) *MeteredRequesterPayment {
	return &MeteredRequesterPayment{
		expectedTask: expectedTask, requesterAI: requester, providerAID: provider,
		accepted: make(map[[16]byte]AssetAmount), state: MeteredReqAwaitQuote,
	}
}

// State returns the FSM state.
func (r *MeteredRequesterPayment) State() MeteredRequesterState { return r.state }

// Receipts returns the received receipts.
func (r *MeteredRequesterPayment) Receipts() []PayReceipt { return r.receipts }

// OnQuote evaluates a metered PAY_QUOTE (terms 3) and agrees; records the
// billing basis. The authorized cap is the quote's maximum total.
func (r *MeteredRequesterPayment) OnQuote(content cbor.Value, raw []byte, nowMs uint64) []tasks.Outgoing {
	q, err := PayQuoteFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if q.TaskBinding == nil || *q.TaskBinding != r.expectedTask {
		r.state = MeteredReqFailed
		return errOut(CodeTaskBindingMismatch)
	}
	if nowMs > q.ExpiryMs {
		r.state = MeteredReqFailed
		return errOut(CodeQuoteExpired)
	}
	if !hasMethod(q.Methods, CreditLedgerMethod) {
		r.state = MeteredReqFailed
		return errOut(CodeMethodUnsupported)
	}
	r.model = q.PricingModel
	r.rateTable = q.RateTable
	r.authorizedCap = q.Total
	qid := q.QuoteID
	r.quoteID = &qid
	r.state = MeteredReqAgreed
	agree := PayAgree{QuoteID: q.QuoteID, QuoteDigest: PayRefDigest(raw), Method: CreditLedgerMethod, AuthorizedCap: q.Total}
	return []tasks.Outgoing{{MsgType: MsgPayAgree, Content: agree.ToValue()}}
}

// OnChunk records a kind-3 usage-update CHUNK — the meter reading pay §4.1
// check 4 compares invoices against. Non-usage chunks are ignored.
func (r *MeteredRequesterPayment) OnChunk(content cbor.Value) {
	var c tasks.ChunkMsg
	if err := c.FromValue(content); err != nil {
		return
	}
	if c.Kind != tasks.ChunkUsage {
		return
	}
	uv, err := cbor.Decode(c.Content)
	if err != nil {
		return
	}
	var u tasks.Usage
	if u.FromValue(uv) != nil {
		return
	}
	r.meters = append(r.meters, meterReading{seq: c.Seq, usage: u})
}

// meteredUsage returns the latest metered usage at or before throughSeq.
func (r *MeteredRequesterPayment) meteredUsage(throughSeq uint64) (tasks.Usage, bool) {
	var best tasks.Usage
	found := false
	var bestSeq uint64
	for _, m := range r.meters {
		if m.seq <= throughSeq && (!found || m.seq >= bestSeq) {
			best, bestSeq, found = m.usage, m.seq, true
		}
	}
	return best, found
}

// OnInvoice verifies a metered PAY_INVOICE against the six pay §4.1 checks IN
// ORDER and, on pass, pays via the ledger operator and replies PAY_PROOF.
func (r *MeteredRequesterPayment) OnInvoice(content cbor.Value, operator *LedgerOperator, nowMs uint64) []tasks.Outgoing {
	inv, err := PayInvoiceFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if r.quoteID == nil || inv.QuoteID != *r.quoteID {
		return errOut(CodeSettlementFailed)
	}
	if _, ok := r.accepted[inv.InvoiceID]; ok {
		return r.payInvoice(inv, operator, nowMs)
	}
	// 1. presence.
	if inv.Watermark == nil {
		return errOut(CodeWatermarkViolation)
	}
	w := *inv.Watermark
	// 2. task binding.
	if w.Task != r.expectedTask {
		return errOut(CodeTaskBindingMismatch)
	}
	// 3. monotonicity vs the last accepted watermark.
	if r.lastAccepted != nil {
		prev := *r.lastAccepted
		usageMonotone := w.Usage.TokensIn >= prev.Usage.TokensIn &&
			w.Usage.TokensOut >= prev.Usage.TokensOut &&
			w.Usage.TTFTMs >= prev.Usage.TTFTMs &&
			w.Usage.WallMs >= prev.Usage.WallMs
		billedMonotone, _ := le(prev.CumulativeBille, w.CumulativeBille)
		if w.ThroughSeq < prev.ThroughSeq || !usageMonotone || !billedMonotone {
			return errOut(CodeWatermarkViolation)
		}
	}
	// 4. meter consistency: the watermark's usage MUST equal a usage we were
	//    actually metered at or before through-seq.
	mu, ok := r.meteredUsage(w.ThroughSeq)
	if !ok || mu != w.Usage {
		return errOut(CodeWatermarkViolation)
	}
	// 5. arithmetic: cumulative-billed == rate × usage, and the invoice amount
	//    is the delta since the last accepted watermark.
	computed, berr := Billed(r.model, r.rateTable, w.Usage)
	if berr != nil || !computed.Equal(w.CumulativeBille) {
		return errOut(CodeWatermarkViolation)
	}
	var prevBilled int64
	if r.lastAccepted != nil {
		prevBilled = r.lastAccepted.CumulativeBille.Amount.Mantissa
	}
	expectedDelta := w.CumulativeBille.Amount.Mantissa - prevBilled
	amountOK := expectedDelta >= 0 &&
		inv.Amount.Asset == w.CumulativeBille.Asset &&
		inv.Amount.Amount.Exponent == w.CumulativeBille.Amount.Exponent &&
		inv.Amount.Amount.Mantissa == expectedDelta
	if !amountOK {
		return errOut(CodeWatermarkViolation)
	}
	// 6. cap.
	if r.authorizedCap != nil {
		ok, comparable := le(w.CumulativeBille, *r.authorizedCap)
		if !comparable || !ok {
			return errOut(CodeCapExceeded)
		}
	}
	r.accepted[inv.InvoiceID] = inv.Amount
	wc := w
	r.lastAccepted = &wc
	return r.payInvoice(inv, operator, nowMs)
}

func (r *MeteredRequesterPayment) payInvoice(inv PayInvoice, operator *LedgerOperator, nowMs uint64) []tasks.Outgoing {
	entry, perr := operator.Pay(inv.InvoiceID, r.requesterAI, r.providerAID, inv.Amount, nowMs)
	switch perr {
	case nil:
		r.pendingUnpaid = nil
		r.state = MeteredReqPaying
		proof := PayProof{InvoiceID: inv.InvoiceID, Method: CreditLedgerMethod, ProofPayload: entry.ToBytes()}
		return []tasks.Outgoing{{MsgType: MsgPayProof, Content: proof.ToValue()}}
	case ErrInsufficient:
		ic := inv
		r.pendingUnpaid = &ic
		r.state = MeteredReqInsufficient
		return nil
	default:
		r.state = MeteredReqFailed
		return errOut(CodeCapExceeded)
	}
}

// RetryPending retries the invoice we could not cover, after a top-up.
func (r *MeteredRequesterPayment) RetryPending(operator *LedgerOperator, nowMs uint64) []tasks.Outgoing {
	if r.pendingUnpaid == nil {
		return nil
	}
	return r.payInvoice(*r.pendingUnpaid, operator, nowMs)
}

// OnReceipt stores a received receipt.
func (r *MeteredRequesterPayment) OnReceipt(content cbor.Value) []tasks.Outgoing {
	if rcpt, err := PayReceiptFromValue(content); err == nil {
		r.receipts = append(r.receipts, rcpt)
	}
	return nil
}
