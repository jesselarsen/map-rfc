package session

import "github.com/jesselarsen/map-rfc/map-go/cbor"

// Core §9 error codes used by the session layer.
const (
	CodeMalformed          = 1000
	CodeUnsupportedVersion = 1001
	CodeReplay             = 1003
	CodeTooLarge           = 1004
	CodeStateViolation     = 1006
	CodeBadSignature       = 2000
	CodeBadDelegation      = 2003
)

// ErrorMsg is the ERROR/CLOSE content shape (core §9):
// {1 code, 2 retryable, ?3 text, ?4 msg-id, ?5 detail}.
type ErrorMsg struct {
	Code      uint16
	Retryable bool
	Text      string
	InReplyTo []byte
}

// ToValue encodes the ErrorMsg content.
func (e ErrorMsg) ToValue() cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(e.Code)},
		{K: cbor.Uint(2), V: cbor.Bool(e.Retryable)},
	}
	if e.Text != "" {
		m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Text(e.Text)})
	}
	if e.InReplyTo != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: cbor.Bytes(e.InReplyTo)})
	}
	return m
}

// ParseErrorMsg decodes ERROR/CLOSE content, tolerating optional keys.
func ParseErrorMsg(v cbor.Value) (ErrorMsg, bool) {
	m, ok := v.(cbor.Map)
	if !ok {
		return ErrorMsg{}, false
	}
	var e ErrorMsg
	var haveCode bool
	for _, p := range m {
		k, ok := p.K.(cbor.Uint)
		if !ok {
			continue
		}
		switch uint64(k) {
		case 1:
			if u, ok := p.V.(cbor.Uint); ok && uint64(u) <= 0xffff {
				e.Code, haveCode = uint16(u), true
			}
		case 2:
			if b, ok := p.V.(cbor.Bool); ok {
				e.Retryable = bool(b)
			}
		case 3:
			if t, ok := p.V.(cbor.Text); ok {
				e.Text = string(t)
			}
		case 4:
			if b, ok := p.V.(cbor.Bytes); ok {
				e.InReplyTo = b
			}
		}
	}
	return e, haveCode
}
