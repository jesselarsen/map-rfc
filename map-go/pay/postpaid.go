package pay

// Postpaid pay:credit-ledger flow (pay §2–§5.1), pump-driven and composed
// alongside the task runtime — a byte-for-byte port of
// map-rs/crates/map-taskrun/src/payments.rs:
//   provider PAY_QUOTE (flat, task-bound) → requester PAY_AGREE → task runs →
//   provider PAY_INVOICE → requester pays via the ledger operator, PAY_PROOF →
//   provider verifies + settles once → PAY_RECEIPT.
//
// Digest binding is over the RECEIVED ENVELOPE bytes (invariant I1, pay §3.1):
// the requester digests Incoming.Raw; the provider digests the sealed
// PAY_QUOTE bytes fed back via BindQuoteEnvelope after session.Send.

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

const msgError uint64 = 0x0006

// errOut builds a non-retryable ERROR outgoing ({1 code, 2 false}), matching
// the Rust ErrorMsg::new(code,false) content byte-for-byte.
func errOut(code uint16) []tasks.Outgoing {
	return []tasks.Outgoing{{MsgType: msgError, Content: cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(code)},
		{K: cbor.Uint(2), V: cbor.Bool(false)},
	}}}
}

// amountWithin reports a ≤ cap in the same asset and exponent (no
// cross-exponent normalization — a mismatch is a rejection, §9).
func amountWithin(a, cap AssetAmount) bool {
	return a.Asset == cap.Asset && a.Amount.Exponent == cap.Amount.Exponent && a.Amount.Mantissa <= cap.Amount.Mantissa
}

// ProviderPayState is the postpaid provider FSM state.
type ProviderPayState int

// Postpaid provider states.
const (
	ProvAwaitAgree ProviderPayState = iota
	ProvAgreed
	ProvInvoiced
	ProvSettled
	ProvFailed
)

// ProviderPayment is the postpaid payee (provider) runtime.
type ProviderPayment struct {
	providerAID identity.AgentID
	requesterAI identity.AgentID
	operatorAID identity.AgentID
	quote       *PayQuote
	quoteDigest []byte
	pendingInv  *pendingInvoice
	settled     map[[16]byte]bool
	receipt     *PayReceipt
	state       ProviderPayState
}

type pendingInvoice struct {
	id     [16]byte
	amount AssetAmount
}

// NewProviderPayment builds a postpaid provider runtime.
func NewProviderPayment(provider, requester, operator identity.AgentID) *ProviderPayment {
	return &ProviderPayment{
		providerAID: provider, requesterAI: requester, operatorAID: operator,
		settled: make(map[[16]byte]bool), state: ProvAwaitAgree,
	}
}

// State returns the FSM state.
func (p *ProviderPayment) State() ProviderPayState { return p.state }

// Receipt returns the settled receipt, if any.
func (p *ProviderPayment) Receipt() *PayReceipt { return p.receipt }

// IssueQuote emits a flat (model 0) PAY_QUOTE bound to task, alongside the task
// QUOTE. The caller MUST feed the sealed bytes back via BindQuoteEnvelope
// (the value session.Send returned) before processing the PAY_AGREE.
func (p *ProviderPayment) IssueQuote(quoteID [16]byte, task [32]byte, total AssetAmount, expiryMs uint64) []tasks.Outgoing {
	terms := TermsPostpaid
	tb := task
	q := PayQuote{
		QuoteID: quoteID, PricingModel: 0,
		RateTable: []RateEntry{{Unit: 0, Amount: total}},
		Total:     &total,
		Methods:   []MethodOffer{{URI: CreditLedgerMethod, Params: []byte{}}},
		ExpiryMs:  expiryMs, Terms: &terms, TaskBinding: &tb,
	}
	p.quoteDigest = nil
	p.quote = &q
	return []tasks.Outgoing{{MsgType: MsgPayQuote, Content: q.ToValue()}}
}

// BindQuoteEnvelope records the sealed PAY_QUOTE envelope bytes (what
// session.Send returned) so the incoming PAY_AGREE digest can be checked
// against the exact frame the requester received (I1, pay §3.1).
func (p *ProviderPayment) BindQuoteEnvelope(envelope []byte) {
	p.quoteDigest = PayRefDigest(envelope)
}

// OnAgree verifies PAY_AGREE binds THIS quote (id + envelope digest) and picks
// the offered method. Digest mismatch → 6006 (price switch); unoffered
// method → 6004.
func (p *ProviderPayment) OnAgree(content cbor.Value) []tasks.Outgoing {
	agree, err := PayAgreeFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if p.quote == nil {
		return errOut(CodeSettlementFailed)
	}
	if agree.Method != CreditLedgerMethod {
		p.state = ProvFailed
		return errOut(CodeMethodUnsupported)
	}
	if agree.QuoteID != p.quote.QuoteID || !bytesEqual(agree.QuoteDigest, p.quoteDigest) {
		p.state = ProvFailed
		return errOut(CodeAgreementDigestMismat)
	}
	p.state = ProvAgreed
	return nil
}

// Invoice emits the flat total PAY_INVOICE after a verified COMPLETE
// (idempotent by id).
func (p *ProviderPayment) Invoice(invoiceID [16]byte, nowMs uint64) []tasks.Outgoing {
	if p.state != ProvAgreed && p.state != ProvInvoiced {
		return nil
	}
	amount := *p.quote.Total
	p.pendingInv = &pendingInvoice{id: invoiceID, amount: amount}
	inv := PayInvoice{
		InvoiceID: invoiceID, QuoteID: p.quote.QuoteID, Amount: amount,
		MethodPayload: []byte{}, TimestampMs: nowMs,
	}
	p.state = ProvInvoiced
	return []tasks.Outgoing{{MsgType: MsgPayInvoice, Content: inv.ToValue()}}
}

// OnProof verifies the ledger-entry proof (operator sig + invoice binding +
// amount), settles once, and replies PAY_RECEIPT. Idempotent: a re-sent PROOF
// gets the same receipt with no second settlement.
func (p *ProviderPayment) OnProof(content cbor.Value, nowMs uint64) []tasks.Outgoing {
	proof, err := PayProofFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if p.pendingInv == nil {
		return errOut(CodeSettlementFailed)
	}
	invID, amount := p.pendingInv.id, p.pendingInv.amount
	if proof.InvoiceID != invID {
		return errOut(CodeSettlementFailed)
	}
	entry, err := EntryProofFromBytes(proof.ProofPayload)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if !VerifyEntry(entry, p.operatorAID, invID, p.requesterAI, p.providerAID, amount) {
		return errOut(CodeInvalidPaymentProof)
	}
	if p.settled[invID] {
		if p.receipt != nil {
			return []tasks.Outgoing{{MsgType: MsgPayReceipt, Content: p.receipt.ToValue()}}
		}
		return errOut(CodeSettlementFailed)
	}
	p.settled[invID] = true
	digest := PayRefDigest(proof.ProofPayload)
	rcpt := PayReceipt{InvoiceID: invID, Amount: amount, TimestampMs: nowMs, ProofDigest: digest}
	p.receipt = &rcpt
	p.state = ProvSettled
	return []tasks.Outgoing{{MsgType: MsgPayReceipt, Content: rcpt.ToValue()}}
}

// ---- requester ------------------------------------------------------------

// RequesterPayState is the postpaid requester FSM state.
type RequesterPayState int

// Postpaid requester states.
const (
	ReqAwaitQuote RequesterPayState = iota
	ReqAgreed
	ReqPaid
	ReqSettled
	ReqInsufficient
	ReqFailed
)

// RequesterPayment is the postpaid payer (requester) runtime.
type RequesterPayment struct {
	expectedTask *[32]byte
	requesterAI  identity.AgentID
	providerAID  identity.AgentID
	agreedCap    *AssetAmount
	quoteID      *[16]byte
	receiptBytes []byte
	state        RequesterPayState
}

// NewRequesterPayment builds a postpaid requester runtime. expectedTask non-nil
// binds to a task-bound quote (key 9 required); nil accepts a standing quote.
func NewRequesterPayment(expectedTask *[32]byte, requester, provider identity.AgentID) *RequesterPayment {
	return &RequesterPayment{
		expectedTask: expectedTask, requesterAI: requester, providerAID: provider,
		state: ReqAwaitQuote,
	}
}

// State returns the FSM state.
func (r *RequesterPayment) State() RequesterPayState { return r.state }

// ReceiptBytes returns the stored settled-receipt content bytes, if any.
func (r *RequesterPayment) ReceiptBytes() []byte { return r.receiptBytes }

// OnQuote evaluates a PAY_QUOTE and emits PAY_AGREE. raw is the RECEIVED
// envelope bytes (I1) the digest is computed over. Task binding (key 9) MUST
// equal the expected TaskID when set.
func (r *RequesterPayment) OnQuote(content cbor.Value, raw []byte, nowMs uint64) []tasks.Outgoing {
	q, err := PayQuoteFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if r.expectedTask != nil {
		if q.TaskBinding == nil || *q.TaskBinding != *r.expectedTask {
			r.state = ReqFailed
			return errOut(CodeTaskBindingMismatch)
		}
	}
	if nowMs > q.ExpiryMs {
		r.state = ReqFailed
		return errOut(CodeQuoteExpired)
	}
	if !hasMethod(q.Methods, CreditLedgerMethod) {
		r.state = ReqFailed
		return errOut(CodeMethodUnsupported)
	}
	digest := PayRefDigest(raw)
	r.agreedCap = q.Total
	qid := q.QuoteID
	r.quoteID = &qid
	r.state = ReqAgreed
	agree := PayAgree{QuoteID: q.QuoteID, QuoteDigest: digest, Method: CreditLedgerMethod, AuthorizedCap: q.Total}
	return []tasks.Outgoing{{MsgType: MsgPayAgree, Content: agree.ToValue()}}
}

// OnInvoice pays a PAY_INVOICE via the ledger operator and replies PAY_PROOF.
// Over-cap → 6005; insufficient balance → no proof (provider may re-invoice).
func (r *RequesterPayment) OnInvoice(content cbor.Value, operator *LedgerOperator, nowMs uint64) []tasks.Outgoing {
	inv, err := PayInvoiceFromValue(content)
	if err != nil {
		return errOut(CodeMalformed)
	}
	if r.quoteID == nil || inv.QuoteID != *r.quoteID {
		return errOut(CodeSettlementFailed)
	}
	if r.agreedCap == nil {
		return errOut(CodeSettlementFailed)
	}
	if !amountWithin(inv.Amount, *r.agreedCap) {
		r.state = ReqFailed
		return errOut(CodeCapExceeded)
	}
	entry, perr := operator.Pay(inv.InvoiceID, r.requesterAI, r.providerAID, inv.Amount, nowMs)
	switch perr {
	case nil:
		proof := PayProof{InvoiceID: inv.InvoiceID, Method: CreditLedgerMethod, ProofPayload: entry.ToBytes()}
		r.state = ReqPaid
		return []tasks.Outgoing{{MsgType: MsgPayProof, Content: proof.ToValue()}}
	case ErrInsufficient:
		r.state = ReqInsufficient
		return nil
	default:
		r.state = ReqFailed
		return errOut(CodeCapExceeded)
	}
}

// OnReceipt stores the settled receipt content and marks the flow settled.
func (r *RequesterPayment) OnReceipt(content cbor.Value) []tasks.Outgoing {
	if _, err := PayReceiptFromValue(content); err == nil {
		r.receiptBytes = cbor.Encode(content)
		r.state = ReqSettled
	}
	return nil
}

func hasMethod(methods []MethodOffer, uri string) bool {
	for _, m := range methods {
		if m.URI == uri {
			return true
		}
	}
	return false
}

func bytesEqual(a, b []byte) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
