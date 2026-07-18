package tasks

// FlowError enumerates flow-credit exhaustion causes (core §8.1).
type FlowError int

const (
	// FlowErrNoMessageCredit means no message credits remain.
	FlowErrNoMessageCredit FlowError = iota
	// FlowErrNoByteCredit means insufficient byte credits for the next chunk.
	FlowErrNoByteCredit
)

func (e FlowError) Error() string {
	if e == FlowErrNoMessageCredit {
		return "tasks: no message credit"
	}
	return "tasks: no byte credit"
}

// FlowAccount enforces the sender-side flow budget in both units (core §8.1):
// a sender must not exceed outstanding message or byte credits.
type FlowAccount struct {
	msgCredits  uint64
	byteCredits uint64
}

// NewFlowAccount returns an empty account.
func NewFlowAccount() *FlowAccount { return &FlowAccount{} }

// Grant adds credits (saturating) in both units.
func (f *FlowAccount) Grant(msgs, bytes uint64) {
	f.msgCredits += msgs
	f.byteCredits += bytes
}

// TryConsume deducts one message credit and contentLen byte credits on success.
// It MUST NOT overrun: any shortage is an error and nothing is deducted.
func (f *FlowAccount) TryConsume(contentLen int) error {
	if f.msgCredits == 0 {
		return FlowErrNoMessageCredit
	}
	if f.byteCredits < uint64(contentLen) {
		return FlowErrNoByteCredit
	}
	f.msgCredits--
	f.byteCredits -= uint64(contentLen)
	return nil
}

// Remaining reports the current credit levels.
func (f *FlowAccount) Remaining() (uint64, uint64) { return f.msgCredits, f.byteCredits }
