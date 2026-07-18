package tasks

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// Outgoing is one message the runtime wants to send: a wire msg type plus its
// CBOR content (to be sealed by the session layer).
type Outgoing struct {
	MsgType uint64
	Content cbor.Value
}

// ReceiptInputs is the provider's own observation of a completed task — the
// trusted side of the later receipt tolerance check (TASK-008 §6/§11).
type ReceiptInputs struct {
	Task       TaskID
	Capability string
	StartedMs  uint64
	EndedMs    uint64
	Usage      Usage
	Outcome    uint8
}

// Executor produces a task's output chunks from its inline input (FR-2). It is
// the only thing a provider implements; the runtime drives the protocol.
type Executor interface {
	Execute(input []byte) []Chunk
}

// ProviderState enumerates the provider FSM states.
type ProviderState int

const (
	// ProviderAwaitAssign waits for ASSIGN after sending QUOTE.
	ProviderAwaitAssign ProviderState = iota
	// ProviderStreaming drains queued chunks under flow credit.
	ProviderStreaming
	// ProviderStarved is parked mid-stream waiting for FLOW_CREDIT.
	ProviderStarved
	// ProviderCompleted emitted COMPLETE.
	ProviderCompleted
	// ProviderCancelled received CANCEL.
	ProviderCancelled
	// ProviderFailed hit a malformed ASSIGN or terminal error.
	ProviderFailed
)

func (s ProviderState) isTerminal() bool {
	switch s {
	case ProviderCompleted, ProviderCancelled, ProviderFailed:
		return true
	default:
		return false
	}
}

// ProviderTask answers OFFER→QUOTE, then ASSIGN→ACCEPT + CHUNK(s)→COMPLETE,
// gating chunk sends on flow credit and committing a transcript root.
type ProviderTask struct {
	executor    Executor
	state       ProviderState
	task        TaskID
	capability  string
	flow        *FlowAccount
	transcript  *TranscriptBuilder
	pending     []ChunkMsg
	quoteTTFTMs uint64
	startedMs   uint64
	tokensIn    uint64
	tokensOut   uint64
	chunksSent  uint64
	observation *ReceiptInputs
}

// DefaultQuoteTTFTMs is the guaranteed TTFT a provider quotes.
const DefaultQuoteTTFTMs = 1000

// QuoteExpiryWindowMs is how long a quoted offer is valid.
const QuoteExpiryWindowMs = 60_000

// NewProviderTask builds a provider runtime around an executor.
func NewProviderTask(exec Executor) *ProviderTask {
	return &ProviderTask{
		executor:    exec,
		state:       ProviderAwaitAssign,
		flow:        NewFlowAccount(),
		transcript:  NewTranscript(),
		quoteTTFTMs: DefaultQuoteTTFTMs,
	}
}

// State returns the current FSM state.
func (p *ProviderTask) State() ProviderState { return p.state }

// ChunksSent returns the number of chunks emitted so far.
func (p *ProviderTask) ChunksSent() uint64 { return p.chunksSent }

// Observation returns the provider's own record of the finished task (once
// COMPLETE was emitted), for a later countersign check.
func (p *ProviderTask) Observation() *ReceiptInputs { return p.observation }

// PollDeadline returns no outgoing messages: a provider has no self-timed
// deadlines (present for API symmetry with the requester).
func (p *ProviderTask) PollDeadline(_ uint64) []Outgoing { return nil }

// OnIncoming advances the FSM on a received message, returning messages to send.
func (p *ProviderTask) OnIncoming(msgType uint64, content cbor.Value, nowMs uint64) []Outgoing {
	if p.state.isTerminal() {
		return nil // idempotent terminals
	}
	switch msgType {
	case MsgOffer:
		return p.onOffer(content, nowMs)
	case MsgAssign:
		return p.onAssign(content, nowMs)
	case MsgFlowCredit:
		return p.onFlowCredit(content, nowMs)
	case MsgCancel:
		p.state = ProviderCancelled
		return nil
	default:
		return nil
	}
}

func (p *ProviderTask) onOffer(content cbor.Value, nowMs uint64) []Outgoing {
	var o Offer
	if err := o.FromValue(content); err != nil {
		return nil
	}
	q := Quote{
		Task:     o.Task,
		TTFTMs:   p.quoteTTFTMs,
		ExpiryMs: nowMs + QuoteExpiryWindowMs,
	}
	return []Outgoing{{MsgType: MsgQuote, Content: q.ToValue()}}
}

func (p *ProviderTask) onAssign(content cbor.Value, nowMs uint64) []Outgoing {
	var a Assign
	if err := a.FromValue(content); err != nil {
		p.state = ProviderFailed
		return nil
	}
	p.task = a.Task
	p.capability = a.Capability
	p.startedMs = nowMs
	input := []byte(nil)
	if a.Input.hasInline {
		input = a.Input.Inline
	}
	p.tokensIn = uint64(len(input))
	p.flow.Grant(a.Flow.MsgCredits, a.Flow.ByteCredits)

	outputs := p.executor.Execute(input)
	n := len(outputs)
	for i, c := range outputs {
		p.pending = append(p.pending, ChunkMsg{
			Task:    a.Task,
			Seq:     uint64(i),
			Kind:    c.Kind,
			Content: c.Content,
			Fin:     i+1 == n,
		})
	}
	p.state = ProviderStreaming

	out := []Outgoing{{MsgType: MsgAccept, Content: Accept{}.ToValue()}}
	out = append(out, p.drain(nowMs)...)
	return out
}

func (p *ProviderTask) onFlowCredit(content cbor.Value, nowMs uint64) []Outgoing {
	var fc FlowCredit
	if err := fc.FromValue(content); err == nil {
		p.flow.Grant(fc.MsgCredits, fc.ByteCredits)
	}
	return p.drain(nowMs)
}

// drain sends as many queued chunks as credit allows; parks (Starved) when out,
// or emits COMPLETE once all are sent.
func (p *ProviderTask) drain(nowMs uint64) []Outgoing {
	var out []Outgoing
	for len(p.pending) > 0 {
		next := p.pending[0]
		if err := p.flow.TryConsume(len(next.Content)); err != nil {
			p.state = ProviderStarved
			return out
		}
		p.pending = p.pending[1:]
		if next.Kind.InTranscript() {
			p.transcript.Push(next.TranscriptChunk())
			p.tokensOut += uint64(len(next.Content))
		}
		p.chunksSent++
		out = append(out, Outgoing{MsgType: MsgChunk, Content: next.ToValue()})
	}

	usage := Usage{
		TokensIn:  p.tokensIn,
		TokensOut: p.tokensOut,
		TTFTMs:    0,
		WallMs:    nowMs - p.startedMs,
	}
	p.observation = &ReceiptInputs{
		Task:       p.task,
		Capability: p.capability,
		StartedMs:  p.startedMs,
		EndedMs:    nowMs,
		Usage:      usage,
		Outcome:    0,
	}
	complete := Complete{
		Task:           p.task,
		Outcome:        0,
		TranscriptRoot: p.transcript.Root(),
		Usage:          usage,
	}
	p.state = ProviderCompleted
	out = append(out, Outgoing{MsgType: MsgComplete, Content: complete.ToValue()})
	return out
}
