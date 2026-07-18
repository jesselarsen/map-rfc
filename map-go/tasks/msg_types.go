package tasks

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// TaskID is the 32-byte requester-chosen idempotency key (tasks §1).
type TaskID [32]byte

// MsgError enumerates codec failures.
type MsgError int

const (
	// MsgErrMissingField means a required map field was absent.
	MsgErrMissingField MsgError = iota
	// MsgErrBadField means a field had the wrong type or value.
	MsgErrBadField
)

func (e MsgError) Error() string {
	if e == MsgErrMissingField {
		return "tasks: message missing field"
	}
	return "tasks: message field invalid"
}

// --- cbor map helpers ------------------------------------------------------

func mapGet(m cbor.Map, k uint64) (cbor.Value, bool) {
	for _, p := range m {
		if u, ok := p.K.(cbor.Uint); ok && uint64(u) == k {
			return p.V, true
		}
	}
	return nil, false
}

func asUint(v cbor.Value) (uint64, bool) {
	if u, ok := v.(cbor.Uint); ok {
		return uint64(u), true
	}
	return 0, false
}

func asText(v cbor.Value) (string, bool) {
	if t, ok := v.(cbor.Text); ok {
		return string(t), true
	}
	return "", false
}

func asBytes(v cbor.Value) ([]byte, bool) {
	if b, ok := v.(cbor.Bytes); ok {
		return []byte(b), true
	}
	return nil, false
}

func asArray(v cbor.Value) ([]cbor.Value, bool) {
	if a, ok := v.(cbor.Array); ok {
		return []cbor.Value(a), true
	}
	return nil, false
}

func reqUint(m cbor.Map, k uint64) (uint64, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return 0, MsgErrMissingField
	}
	u, ok := asUint(v)
	if !ok {
		return 0, MsgErrBadField
	}
	return u, nil
}

func reqText(m cbor.Map, k uint64) (string, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return "", MsgErrMissingField
	}
	s, ok := asText(v)
	if !ok {
		return "", MsgErrBadField
	}
	return s, nil
}

func reqBytes(m cbor.Map, k uint64) ([]byte, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return nil, MsgErrMissingField
	}
	b, ok := asBytes(v)
	if !ok {
		return nil, MsgErrBadField
	}
	return b, nil
}

func optUint(m cbor.Map, k uint64) (uint64, bool, error) {
	v, ok := mapGet(m, k)
	if !ok {
		return 0, false, nil
	}
	u, ok := asUint(v)
	if !ok {
		return 0, false, MsgErrBadField
	}
	return u, true, nil
}

func taskAt(m cbor.Map, k uint64) (TaskID, error) {
	b, err := reqBytes(m, k)
	if err != nil {
		return TaskID{}, err
	}
	if len(b) != 32 {
		return TaskID{}, MsgErrBadField
	}
	var t TaskID
	copy(t[:], b)
	return t, nil
}

// --- shared sub-structures -------------------------------------------------

// TaskConstraints models tasks §2 negotiation constraints (all optional).
type TaskConstraints struct {
	MaxContextTokens uint64
	MaxOutputTokens  uint64
	DeadlineMs       uint64
	TTFTMs           uint64
	Zone             uint8
	MinSandbox       uint64
	ModelDigest      []byte
	Ext              cbor.Value

	hasMaxContext bool
	hasMaxOutput  bool
	hasDeadline   bool
	hasTTFT       bool
	hasZone       bool
	hasMinSandbox bool
	hasModel      bool
	hasExt        bool
}

// ToValue encodes constraints (only set keys emitted).
func (c TaskConstraints) ToValue() cbor.Value {
	var m cbor.Map
	if c.hasMaxContext {
		m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Uint(c.MaxContextTokens)})
	}
	if c.hasMaxOutput {
		m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Uint(c.MaxOutputTokens)})
	}
	if c.hasDeadline {
		m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Uint(c.DeadlineMs)})
	}
	if c.hasTTFT {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: cbor.Uint(c.TTFTMs)})
	}
	if c.hasZone {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: cbor.Uint(uint64(c.Zone))})
	}
	if c.hasMinSandbox {
		m = append(m, cbor.Pair{K: cbor.Uint(6), V: cbor.Uint(c.MinSandbox)})
	}
	if c.hasModel {
		m = append(m, cbor.Pair{K: cbor.Uint(7), V: cbor.Bytes(c.ModelDigest)})
	}
	if c.hasExt {
		m = append(m, cbor.Pair{K: cbor.Uint(8), V: c.Ext})
	}
	return m
}

// FromValue decodes constraints.
func (c *TaskConstraints) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	if x, ok, err := optUint(m, 1); err != nil {
		return err
	} else if ok {
		c.MaxContextTokens, c.hasMaxContext = x, true
	}
	if x, ok, err := optUint(m, 2); err != nil {
		return err
	} else if ok {
		c.MaxOutputTokens, c.hasMaxOutput = x, true
	}
	if x, ok, err := optUint(m, 3); err != nil {
		return err
	} else if ok {
		c.DeadlineMs, c.hasDeadline = x, true
	}
	if x, ok, err := optUint(m, 4); err != nil {
		return err
	} else if ok {
		c.TTFTMs, c.hasTTFT = x, true
	}
	if x, ok, err := optUint(m, 5); err != nil {
		return err
	} else if ok {
		if x > 255 {
			return MsgErrBadField
		}
		c.Zone, c.hasZone = uint8(x), true
	}
	if x, ok, err := optUint(m, 6); err != nil {
		return err
	} else if ok {
		c.MinSandbox, c.hasMinSandbox = x, true
	}
	if vv, ok := mapGet(m, 7); ok {
		b, ok := asBytes(vv)
		if !ok {
			return MsgErrBadField
		}
		c.ModelDigest, c.hasModel = b, true
	}
	if vv, ok := mapGet(m, 8); ok {
		c.Ext, c.hasExt = vv, true
	}
	return nil
}

// InputSpec models tasks §2.1: inline (1), blocks (2), context (3); at least
// one key MUST be present.
type InputSpec struct {
	Inline  []byte
	Blocks  [][]byte
	Context []cbor.Value

	hasInline  bool
	hasBlocks  bool
	hasContext bool
}

// NewInputInline builds an inline input spec.
func NewInputInline(b []byte) InputSpec {
	return InputSpec{Inline: append([]byte(nil), b...), hasInline: true}
}

// ToValue encodes the input spec (only present keys).
func (i InputSpec) ToValue() cbor.Value {
	var m cbor.Map
	if i.hasInline {
		m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(i.Inline)})
	}
	if i.hasBlocks {
		var arr cbor.Array
		for _, h := range i.Blocks {
			arr = append(arr, cbor.Bytes(h))
		}
		m = append(m, cbor.Pair{K: cbor.Uint(2), V: arr})
	}
	if i.hasContext {
		m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Array(i.Context)})
	}
	return m
}

// FromValue decodes the input spec, rejecting an all-absent or malformed form.
func (i *InputSpec) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	if vv, ok := mapGet(m, 1); ok {
		b, ok := asBytes(vv)
		if !ok {
			return MsgErrBadField
		}
		i.Inline, i.hasInline = b, true
	}
	if vv, ok := mapGet(m, 2); ok {
		arr, ok := asArray(vv)
		if !ok || len(arr) == 0 {
			return MsgErrBadField
		}
		var blocks [][]byte
		for _, h := range arr {
			b, ok := asBytes(h)
			if !ok {
				return MsgErrBadField
			}
			blocks = append(blocks, b)
		}
		i.Blocks, i.hasBlocks = blocks, true
	}
	if vv, ok := mapGet(m, 3); ok {
		arr, ok := asArray(vv)
		if !ok || len(arr) == 0 {
			return MsgErrBadField
		}
		i.Context, i.hasContext = arr, true
	}
	if !i.hasInline && !i.hasBlocks && !i.hasContext {
		return MsgErrMissingField
	}
	return nil
}

// FlowInit is the initial flow credit (tasks §2.1).
type FlowInit struct {
	MsgCredits  uint64
	ByteCredits uint64
}

func (f FlowInit) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(f.MsgCredits)},
		{K: cbor.Uint(2), V: cbor.Uint(f.ByteCredits)},
	}
}

func (f *FlowInit) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	msgs, err := reqUint(m, 1)
	if err != nil {
		return err
	}
	bytes, err := reqUint(m, 2)
	if err != nil {
		return err
	}
	f.MsgCredits, f.ByteCredits = msgs, bytes
	return nil
}

// Usage is the metered usage (tasks §4).
type Usage struct {
	TokensIn  uint64
	TokensOut uint64
	TTFTMs    uint64
	WallMs    uint64
}

func (u Usage) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(u.TokensIn)},
		{K: cbor.Uint(2), V: cbor.Uint(u.TokensOut)},
		{K: cbor.Uint(3), V: cbor.Uint(u.TTFTMs)},
		{K: cbor.Uint(4), V: cbor.Uint(u.WallMs)},
	}
}

func (u *Usage) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	for _, kv := range []struct {
		k uint64
		p *uint64
	}{
		{1, &u.TokensIn}, {2, &u.TokensOut}, {3, &u.TTFTMs}, {4, &u.WallMs},
	} {
		x, err := reqUint(m, kv.k)
		if err != nil {
			return err
		}
		*kv.p = x
	}
	return nil
}
