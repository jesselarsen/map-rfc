package tasks

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// ReqState enumerates the requester FSM states (tasks §13).
type ReqState int

const (
	// ReqCreated is the initial state.
	ReqCreated ReqState = iota
	// ReqOfferSent: OFFER sent, awaiting QUOTE.
	ReqOfferSent
	// ReqQuoted: QUOTE received.
	ReqQuoted
	// ReqPending: ASSIGN sent, awaiting ACCEPT/REJECT.
	ReqPending
	// ReqActive: ACCEPT received, streaming chunks.
	ReqActive
	// ReqSuspended: provider lost, resumable.
	ReqSuspended
	// ReqDelivered: COMPLETE received, verifying transcript.
	ReqDelivered
	// ReqSettling: transcript verified.
	ReqSettling
	// ReqClosed: settled.
	ReqClosed
	// ReqCancelled: cancelled.
	ReqCancelled
	// ReqFailed: failed.
	ReqFailed
	// ReqDisputed: transcript mismatch (routed here per spec; reference impl
	// currently routes mismatch to Failed — see note in requester.onComplete).
	ReqDisputed
)

func (s ReqState) isTerminal() bool {
	switch s {
	case ReqClosed, ReqCancelled, ReqFailed, ReqDisputed:
		return true
	default:
		return false
	}
}

// ReqEvent enumerates FSM transitions.
type ReqEvent int

const (
	evSendOffer ReqEvent = iota
	evRecvQuote
	evSendAssign
	evRecvAccept
	evRecvReject
	evRecvChunk
	evProviderLost
	evResume
	evCancel
	evRecvCompleteOk
	evTranscriptVerified
	evTranscriptMismatch
	evSettled
	evFatal
)

// step applies the tasks §13 transition table. Go has no tuple-switch, so the
// (state, event) table is flattened into explicit comparisons.
func step(s ReqState, e ReqEvent) (ReqState, bool) {
	if s.isTerminal() {
		return s, false
	}
	switch s {
	case ReqCreated:
		switch e {
		case evSendOffer:
			return ReqOfferSent, true
		case evSendAssign:
			return ReqPending, true
		}
	case ReqOfferSent:
		if e == evRecvQuote {
			return ReqQuoted, true
		}
	case ReqQuoted:
		if e == evSendAssign {
			return ReqPending, true
		}
	case ReqPending:
		switch e {
		case evRecvAccept:
			return ReqActive, true
		case evRecvReject:
			return ReqFailed, true
		}
	case ReqActive:
		switch e {
		case evRecvChunk:
			return ReqActive, true
		case evProviderLost:
			return ReqSuspended, true
		case evRecvCompleteOk:
			return ReqDelivered, true
		case evCancel:
			return ReqCancelled, true
		}
	case ReqSuspended:
		switch e {
		case evResume:
			return ReqActive, true
		case evCancel:
			return ReqCancelled, true
		}
	case ReqDelivered:
		switch e {
		case evTranscriptVerified:
			return ReqSettling, true
		case evTranscriptMismatch:
			return ReqDisputed, true
		}
	case ReqSettling:
		if e == evSettled {
			return ReqClosed, true
		}
	}
	if e == evFatal {
		return ReqFailed, true
	}
	return s, false
}

// Credit re-granted per FLOW_CREDIT and as the initial FlowInit (§8).
const (
	grantMsgs  uint64 = 64
	grantBytes uint64 = 1 << 20
)

// TTFT slack: cancel if the first chunk misses quoted × 1.1 (tasks §2).
const (
	ttftSlackNum uint64 = 11
	ttftSlackDen uint64 = 10
)

// Requester error codes (tasks §4/§13).
const (
	ErrTranscriptMismatch uint16 = 5000
	ErrTTFTMissed         uint16 = 5001
	ErrSeqGap             uint16 = 1003
)

// RequesterTask drives the requester side: send OFFER (or direct ASSIGN),
// evaluate QUOTE, stream CHUNKs (feeding its own transcript, re-granting credit),
// and verify root == own transcript root on COMPLETE.
type RequesterTask struct {
	state           ReqState
	task            TaskID
	capability      string
	constraints     TaskConstraints
	input           []byte
	transcript      *TranscriptBuilder
	expectedSeq     uint64
	grantedMsgs     uint64
	grantedBytes    uint64
	usedMsgs        uint64
	usedBytes       uint64
	quotedTTFTMs    uint64
	hasQuotedTTFT   bool
	ttftDeadlineMs  uint64
	hasTTFTDeadline bool
	firstChunkSeen  bool
	startedMs       uint64
	endedMs         uint64
	verified        *ReceiptInputs
	verifiedRoot    [32]byte
	errorCode       *uint16
	chunksReceived  uint64
	creditsGranted  uint64
}

// NewRequesterTask builds a requester for one task.
func NewRequesterTask(task TaskID, capability string, constraints TaskConstraints, input []byte) *RequesterTask {
	return &RequesterTask{
		state:       ReqCreated,
		task:        task,
		capability:  capability,
		constraints: constraints,
		input:       input,
		transcript:  NewTranscript(),
	}
}

// State returns the FSM state.
func (r *RequesterTask) State() ReqState { return r.state }

// Verified returns the verified receipt inputs + transcript root, once verified.
func (r *RequesterTask) Verified() (*ReceiptInputs, [32]byte) {
	if r.verified == nil {
		return nil, [32]byte{}
	}
	return r.verified, r.verifiedRoot
}

// IsVerified reports whether the transcript has been verified (the task's
// terminal success condition for a runner).
func (r *RequesterTask) IsVerified() bool { return r.verified != nil }

// ErrorCode returns the failure wire code, if any.
func (r *RequesterTask) ErrorCode() *uint16 { return r.errorCode }

// ChunksReceived returns the count of chunks received.
func (r *RequesterTask) ChunksReceived() uint64 { return r.chunksReceived }

// CreditsGranted returns the number of FLOW_CREDIT re-grants issued.
func (r *RequesterTask) CreditsGranted() uint64 { return r.creditsGranted }

// StartOffer begins the negotiated flow: send OFFER (Created → OfferSent).
func (r *RequesterTask) StartOffer(nowMs uint64) []Outgoing {
	r.startedMs = nowMs
	if !r.tryStep(evSendOffer) {
		return nil
	}
	o := Offer{Task: r.task, Capability: r.capability, Constraints: r.constraints}
	return []Outgoing{{MsgType: MsgOffer, Content: o.ToValue()}}
}

// StartDirect skips OFFER/QUOTE straight to ASSIGN (Created → Pending). No TTFT
// deadline is armed on this path.
func (r *RequesterTask) StartDirect(nowMs uint64) []Outgoing {
	r.startedMs = nowMs
	if !r.tryStep(evSendAssign) {
		return nil
	}
	return []Outgoing{{MsgType: MsgAssign, Content: r.assignMsg().ToValue()}}
}

// PollDeadline cancels if the first chunk misses the TTFT deadline.
func (r *RequesterTask) PollDeadline(nowMs uint64) []Outgoing {
	if r.state == ReqActive && !r.firstChunkSeen && r.hasTTFTDeadline {
		if nowMs > r.ttftDeadlineMs {
			code := ErrTTFTMissed
			r.errorCode = &code
			r.tryStep(evCancel)
			return []Outgoing{{MsgType: MsgCancel, Content: Cancel{Task: r.task, Code: code}.ToValue()}}
		}
	}
	return nil
}

// OnIncoming advances the FSM on a received message, returning messages to send.
func (r *RequesterTask) OnIncoming(msgType uint64, content cbor.Value, nowMs uint64) []Outgoing {
	if r.isDone() {
		return nil
	}
	switch msgType {
	case MsgQuote:
		return r.onQuote(content, nowMs)
	case MsgAccept:
		return r.onAccept(nowMs)
	case MsgReject:
		return r.onReject(content)
	case MsgChunk:
		return r.onChunk(content)
	case MsgComplete:
		return r.onComplete(content, nowMs)
	case MsgCancel:
		r.tryStep(evCancel)
		return nil
	default:
		return nil
	}
}

func (r *RequesterTask) onQuote(content cbor.Value, nowMs uint64) []Outgoing {
	var q Quote
	if err := q.FromValue(content); err != nil {
		return r.fail(nil)
	}
	if nowMs > q.ExpiryMs {
		return r.fail(uint16Ptr(6000)) // QUOTE_EXPIRED
	}
	if r.constraints.hasTTFT && q.TTFTMs > r.constraints.TTFTMs {
		return r.fail(nil) // local policy reject
	}
	r.quotedTTFTMs, r.hasQuotedTTFT = q.TTFTMs, true
	if !r.tryStep(evRecvQuote) || !r.tryStep(evSendAssign) {
		return nil
	}
	return []Outgoing{{MsgType: MsgAssign, Content: r.assignMsg().ToValue()}}
}

func (r *RequesterTask) onAccept(nowMs uint64) []Outgoing {
	if !r.tryStep(evRecvAccept) {
		return nil
	}
	if r.hasQuotedTTFT {
		r.ttftDeadlineMs = nowMs + r.quotedTTFTMs*ttftSlackNum/ttftSlackDen
		r.hasTTFTDeadline = true
	}
	return nil
}

func (r *RequesterTask) onReject(content cbor.Value) []Outgoing {
	var rej Reject
	if err := rej.FromValue(content); err == nil {
		c := rej.Code
		r.errorCode = &c
	}
	r.tryStep(evRecvReject)
	return nil
}

func (r *RequesterTask) onChunk(content cbor.Value) []Outgoing {
	if r.state != ReqActive {
		return r.fail(nil)
	}
	var c ChunkMsg
	if err := c.FromValue(content); err != nil {
		return r.fail(nil)
	}
	// chunk-seq density is a content-level check (field 2), separate from the
	// envelope seq map-session already enforces.
	if c.Seq != r.expectedSeq {
		return r.fail(uint16Ptr(ErrSeqGap))
	}
	r.expectedSeq++
	r.firstChunkSeen = true
	r.chunksReceived++
	r.usedMsgs++
	r.usedBytes += uint64(len(c.Content))
	if c.Kind.InTranscript() {
		r.transcript.Push(c.TranscriptChunk())
	}
	if !r.tryStep(evRecvChunk) {
		return nil
	}
	return r.maybeRegrant()
}

func (r *RequesterTask) onComplete(content cbor.Value, nowMs uint64) []Outgoing {
	var c Complete
	if err := c.FromValue(content); err != nil {
		return r.fail(nil)
	}
	r.endedMs = nowMs
	if !r.tryStep(evRecvCompleteOk) {
		return nil
	}
	if c.TranscriptRoot == r.transcript.Root() {
		r.tryStep(evTranscriptVerified)
		r.verified = &ReceiptInputs{
			Task:       r.task,
			Capability: r.capability,
			StartedMs:  r.startedMs,
			EndedMs:    r.endedMs,
			Usage:      c.Usage,
			Outcome:    c.Outcome,
		}
		r.verifiedRoot = c.TranscriptRoot
	} else {
		// Reference impl routes mismatch to Failed (see tasks/requester.rs note).
		r.fail(uint16Ptr(ErrTranscriptMismatch))
	}
	return nil
}

func (r *RequesterTask) assignMsg() Assign {
	r.grantedMsgs = grantMsgs
	r.grantedBytes = grantBytes
	return Assign{
		Task:        r.task,
		Capability:  r.capability,
		Constraints: r.constraints,
		Input:       NewInputInline(r.input),
		Flow:        FlowInit{MsgCredits: grantMsgs, ByteCredits: grantBytes},
	}
}

// maybeRegrant grants another FlowInit-worth when either unit drops below 25%.
func (r *RequesterTask) maybeRegrant() []Outgoing {
	remMsgs := r.grantedMsgs - r.usedMsgs
	remBytes := r.grantedBytes - r.usedBytes
	if remMsgs*4 < grantMsgs || remBytes*4 < grantBytes {
		r.grantedMsgs += grantMsgs
		r.grantedBytes += grantBytes
		r.creditsGranted++
		fc := FlowCredit{Scope: r.task, MsgCredits: grantMsgs, ByteCredits: grantBytes}
		return []Outgoing{{MsgType: MsgFlowCredit, Content: fc.ToValue()}}
	}
	return nil
}

func (r *RequesterTask) tryStep(e ReqEvent) bool {
	ns, ok := step(r.state, e)
	r.state = ns
	return ok
}

func (r *RequesterTask) fail(code *uint16) []Outgoing {
	if code != nil {
		r.errorCode = code
	}
	r.state = ReqFailed
	return nil
}

func (r *RequesterTask) isDone() bool {
	return r.state.isTerminal() || r.verified != nil
}

func uint16Ptr(x uint16) *uint16 { return &x }
