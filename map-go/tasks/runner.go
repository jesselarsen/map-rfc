package tasks

import (
	"time"

	"github.com/jesselarsen/map-rfc/map-go/session"
)

func nowMs() uint64 { return uint64(time.Now().UnixMilli()) }

// RunProvider drives a ProviderTask to completion over an established session.
// It pumps inbound task messages through the FSM and sends every Outgoing back
// through the session's CB-bound channel, returning when the task reaches a
// terminal state (Completed / Cancelled / Failed) or the session closes.
func RunProvider(sess *session.Session, p *ProviderTask) error {
	for {
		if p.State().isTerminal() {
			return nil
		}
		inc, err := sess.Recv()
		if err != nil {
			return err
		}
		out := p.OnIncoming(inc.MsgType, inc.Content, nowMs())
		if err := sendAll(sess, out); err != nil {
			return err
		}
	}
}

// RunRequester drives a RequesterTask to completion over an established session
// and returns the verified receipt inputs + transcript root (or an error if the
// task failed, e.g. transcript mismatch or TTFT miss). direct skips
// OFFER/QUOTE and starts with ASSIGN (matching StartDirect).
func RunRequester(sess *session.Session, r *RequesterTask, direct bool) (*ReceiptInputs, [32]byte, error) {
	n := nowMs()
	var first []Outgoing
	if direct {
		first = r.StartDirect(n)
	} else {
		first = r.StartOffer(n)
	}
	if err := sendAll(sess, first); err != nil {
		return nil, [32]byte{}, err
	}
	for {
		if r.State().isTerminal() || r.IsVerified() {
			break
		}
		// Honour a TTFT deadline before blocking on Recv.
		if d := r.PollDeadline(nowMs()); d != nil {
			if err := sendAll(sess, d); err != nil {
				return nil, [32]byte{}, err
			}
		}
		inc, err := sess.Recv()
		if err != nil {
			return nil, [32]byte{}, err
		}
		out := r.OnIncoming(inc.MsgType, inc.Content, nowMs())
		if err := sendAll(sess, out); err != nil {
			return nil, [32]byte{}, err
		}
	}
	if ver, root := r.Verified(); ver != nil {
		return ver, root, nil
	}
	if code := r.ErrorCode(); code != nil {
		return nil, [32]byte{}, &TaskError{Code: *code}
	}
	return nil, [32]byte{}, &TaskError{Code: 0, msg: "requester did not verify"}
}

// TaskError is returned by RunRequester when the task fails protocol-side.
type TaskError struct {
	Code uint16
	msg  string
}

func (e *TaskError) Error() string {
	if e.msg != "" {
		return e.msg
	}
	return "task failed"
}

func sendAll(sess *session.Session, out []Outgoing) error {
	for _, o := range out {
		if _, err := sess.Send(o.MsgType, o.Content); err != nil {
			return err
		}
	}
	return nil
}
