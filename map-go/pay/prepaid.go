package pay

// Prepaid (terms 1) pay:credit-ledger flow (pay §5.2.2), the terms-1 subset of
// map-rs/crates/map-taskrun/src/escrow.rs (terms-2 escrow + disputes are NOT
// ported — TASK-029 scope). One pre-delivery invoice equal to the quoted
// total; the provider's ExecutionPermitted() gate stays false until settled —
// the "no pay, no work" gate the task pump consults before billable work.

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

// acceptQuote runs the shared terms-1/2 quote checks (task binding, expiry,
// method, required terms + total) and returns the agreed terms or a wire code.
func acceptQuote(q PayQuote, expectedTask [32]byte, requiredTerms uint64, nowMs uint64) (AgreedTerms, uint16) {
	if q.TaskBinding == nil || *q.TaskBinding != expectedTask {
		return AgreedTerms{}, CodeTaskBindingMismatch
	}
	if nowMs > q.ExpiryMs {
		return AgreedTerms{}, CodeQuoteExpired
	}
	if !hasMethod(q.Methods, CreditLedgerMethod) {
		return AgreedTerms{}, CodeMethodUnsupported
	}
	if q.Terms == nil || *q.Terms != requiredTerms || q.Total == nil {
		return AgreedTerms{}, CodeEscrowViolation
	}
	return AgreedTerms{
		Task: expectedTask, PricingModel: q.PricingModel, RateTable: q.RateTable,
		Terms: requiredTerms, AuthorizedCap: q.Total, QuotedTotal: q.Total,
	}, 0
}

// verifyPrepaidInvoice is the terms-1 invoice check (pay §5.2.2): no watermark,
// no phase key, amount == the quoted total, within cap.
func verifyPrepaidInvoice(agreed AgreedTerms, inv PayInvoice) uint16 {
	if inv.Watermark != nil {
		return CodeWatermarkViolation
	}
	if inv.Phase != nil {
		return CodeEscrowViolation
	}
	if agreed.QuotedTotal == nil || !inv.Amount.Equal(*agreed.QuotedTotal) {
		return CodeEscrowViolation
	}
	if agreed.AuthorizedCap != nil {
		if ok, comparable := le(inv.Amount, *agreed.AuthorizedCap); !comparable || !ok {
			return CodeCapExceeded
		}
	}
	return 0
}

// PrepaidProviderState is the terms-1 provider FSM state.
type PrepaidProviderState int

// Prepaid provider states.
const (
	PrepaidProvAwaitAgree PrepaidProviderState = iota
	PrepaidProvAwaitPayment
	PrepaidProvSettled
)

// PrepaidProviderPayment is the terms-1 payee runtime.
type PrepaidProviderPayment struct {
	providerAID identity.AgentID
	requesterAI identity.AgentID
	operatorAID identity.AgentID
	quote       PayQuote
	quoteDigest []byte
	settled     map[[16]byte]bool
	invoiced    *pendingInvoice
	receipts    []PayReceipt
	state       PrepaidProviderState
}

// NewPrepaidProviderPayment builds a terms-1 provider from a terms-1 quote
// (key 4 total present).
func NewPrepaidProviderPayment(provider, requester, operator identity.AgentID, quote PayQuote) *PrepaidProviderPayment {
	return &PrepaidProviderPayment{
		providerAID: provider, requesterAI: requester, operatorAID: operator, quote: quote,
		settled: make(map[[16]byte]bool), state: PrepaidProvAwaitAgree,
	}
}

// State returns the FSM state.
func (p *PrepaidProviderPayment) State() PrepaidProviderState { return p.state }

// Receipts returns the emitted receipts.
func (p *PrepaidProviderPayment) Receipts() []PayReceipt { return p.receipts }

// ExecutionPermitted is the §5.2.2 gate: billable execution MUST NOT start
// before *settled*.
func (p *PrepaidProviderPayment) ExecutionPermitted() bool { return p.state == PrepaidProvSettled }

// BindQuoteEnvelope records the sealed PAY_QUOTE bytes (I1).
func (p *PrepaidProviderPayment) BindQuoteEnvelope(envelope []byte) {
	p.quoteDigest = PayRefDigest(envelope)
}

// OnAgree verifies the PAY_AGREE binds this quote and picks the method.
func (p *PrepaidProviderPayment) OnAgree(content cbor.Value) []tasks.Outgoing {
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
	p.state = PrepaidProvAwaitPayment
	return nil
}

// Invoice emits the single pre-delivery PAY_INVOICE equal to the quoted total.
func (p *PrepaidProviderPayment) Invoice(invoiceID [16]byte, nowMs uint64) []tasks.Outgoing {
	total := *p.quote.Total
	p.invoiced = &pendingInvoice{id: invoiceID, amount: total}
	inv := PayInvoice{InvoiceID: invoiceID, QuoteID: p.quote.QuoteID, Amount: total, MethodPayload: []byte{}, TimestampMs: nowMs}
	return []tasks.Outgoing{{MsgType: MsgPayInvoice, Content: inv.ToValue()}}
}

// OnProof verifies the proof, settles once, replies a settled (kind-absent)
// receipt, and opens the execution gate.
func (p *PrepaidProviderPayment) OnProof(content cbor.Value, nowMs uint64) []tasks.Outgoing {
	proof, err := PayProofFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if p.invoiced == nil || proof.InvoiceID != p.invoiced.id {
		return errOut(CodeSettlementFailed)
	}
	invID, amount := p.invoiced.id, p.invoiced.amount
	entry, err := EntryProofFromBytes(proof.ProofPayload)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if !VerifyEntry(entry, p.operatorAID, invID, p.requesterAI, p.providerAID, amount) {
		return errOut(CodeInvalidPaymentProof)
	}
	if p.settled[invID] {
		for i := range p.receipts {
			if p.receipts[i].InvoiceID == invID {
				return []tasks.Outgoing{{MsgType: MsgPayReceipt, Content: p.receipts[i].ToValue()}}
			}
		}
		return errOut(CodeSettlementFailed)
	}
	p.settled[invID] = true
	rcpt := PayReceipt{InvoiceID: invID, Amount: amount, TimestampMs: nowMs, ProofDigest: PayRefDigest(proof.ProofPayload)}
	p.receipts = append(p.receipts, rcpt)
	p.state = PrepaidProvSettled
	return []tasks.Outgoing{{MsgType: MsgPayReceipt, Content: rcpt.ToValue()}}
}

// ---- requester ------------------------------------------------------------

// PrepaidRequesterState is the terms-1 requester FSM state.
type PrepaidRequesterState int

// Prepaid requester states.
const (
	PrepaidReqAwaitQuote PrepaidRequesterState = iota
	PrepaidReqAgreed
	PrepaidReqPaid
	PrepaidReqFailed
)

// PrepaidRequesterPayment is the terms-1 payer runtime.
type PrepaidRequesterPayment struct {
	expectedTask [32]byte
	requesterAI  identity.AgentID
	providerAID  identity.AgentID
	agreed       *AgreedTerms
	quoteID      *[16]byte
	receipts     []PayReceipt
	state        PrepaidRequesterState
}

// NewPrepaidRequesterPayment builds a terms-1 requester runtime.
func NewPrepaidRequesterPayment(expectedTask [32]byte, requester, provider identity.AgentID) *PrepaidRequesterPayment {
	return &PrepaidRequesterPayment{
		expectedTask: expectedTask, requesterAI: requester, providerAID: provider,
		state: PrepaidReqAwaitQuote,
	}
}

// State returns the FSM state.
func (r *PrepaidRequesterPayment) State() PrepaidRequesterState { return r.state }

// Receipts returns the received receipts.
func (r *PrepaidRequesterPayment) Receipts() []PayReceipt { return r.receipts }

// OnQuote evaluates a terms-1 PAY_QUOTE and emits PAY_AGREE (raw = the received
// envelope bytes, I1).
func (r *PrepaidRequesterPayment) OnQuote(content cbor.Value, raw []byte, nowMs uint64) []tasks.Outgoing {
	q, err := PayQuoteFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	agreed, code := acceptQuote(q, r.expectedTask, TermsPrepaid, nowMs)
	if code != 0 {
		r.state = PrepaidReqFailed
		return errOut(code)
	}
	qid := q.QuoteID
	r.quoteID = &qid
	r.agreed = &agreed
	r.state = PrepaidReqAgreed
	agree := PayAgree{QuoteID: q.QuoteID, QuoteDigest: PayRefDigest(raw), Method: CreditLedgerMethod, AuthorizedCap: agreed.QuotedTotal}
	return []tasks.Outgoing{{MsgType: MsgPayAgree, Content: agree.ToValue()}}
}

// OnInvoice verifies (pay §5.2.2) and pays the single prepaid invoice.
func (r *PrepaidRequesterPayment) OnInvoice(content cbor.Value, operator *LedgerOperator, nowMs uint64) []tasks.Outgoing {
	inv, err := PayInvoiceFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if r.quoteID == nil || inv.QuoteID != *r.quoteID {
		return errOut(CodeSettlementFailed)
	}
	if code := verifyPrepaidInvoice(*r.agreed, inv); code != 0 {
		return errOut(code)
	}
	entry, perr := operator.Pay(inv.InvoiceID, r.requesterAI, r.providerAID, inv.Amount, nowMs)
	switch perr {
	case nil:
		r.state = PrepaidReqPaid
		proof := PayProof{InvoiceID: inv.InvoiceID, Method: CreditLedgerMethod, ProofPayload: entry.ToBytes()}
		return []tasks.Outgoing{{MsgType: MsgPayProof, Content: proof.ToValue()}}
	case ErrInsufficient:
		return nil // proof absence, as postpaid
	default:
		return errOut(CodeSettlementFailed)
	}
}

// OnReceipt stores a received receipt.
func (r *PrepaidRequesterPayment) OnReceipt(content cbor.Value) []tasks.Outgoing {
	if rcpt, err := PayReceiptFromValue(content); err == nil {
		r.receipts = append(r.receipts, rcpt)
	}
	return nil
}
