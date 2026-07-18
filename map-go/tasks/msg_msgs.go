package tasks

import (
	"github.com/jesselarsen/map-rfc/map-go/cbor"
)

// Offer (0x0200) — tasks §2.
type Offer struct {
	Task        TaskID
	Capability  string
	Constraints TaskConstraints
	PayRef      cbor.Value // optional (nil = omitted)
	ExpiryMs    uint64
	hasExpiry   bool
}

func (o Offer) ToValue() cbor.Value {
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(o.Task[:])})
	m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Text(o.Capability)})
	m = append(m, cbor.Pair{K: cbor.Uint(3), V: o.Constraints.ToValue()})
	if o.PayRef != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: o.PayRef})
	}
	if o.hasExpiry {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: cbor.Uint(o.ExpiryMs)})
	}
	return m
}

func (o *Offer) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	t, err := taskAt(m, 1)
	if err != nil {
		return err
	}
	cap, err := reqText(m, 2)
	if err != nil {
		return err
	}
	vv, ok := mapGet(m, 3)
	if !ok {
		return MsgErrMissingField
	}
	var c TaskConstraints
	if err := c.FromValue(vv); err != nil {
		return err
	}
	if vv, ok := mapGet(m, 4); ok {
		o.PayRef = vv
	}
	if x, ok, err := optUint(m, 5); err != nil {
		return err
	} else if ok {
		o.ExpiryMs, o.hasExpiry = x, true
	}
	o.Task, o.Capability, o.Constraints = t, cap, c
	return nil
}

// Quote (0x0201) — tasks §2.
type Quote struct {
	Task        TaskID
	PayQuoteRef cbor.Value // optional
	TTFTMs      uint64
	ExpiryMs    uint64
	Ext         cbor.Value // optional
	hasExt      bool
}

func (q Quote) ToValue() cbor.Value {
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(q.Task[:])})
	if q.PayQuoteRef != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(2), V: q.PayQuoteRef})
	}
	m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Uint(q.TTFTMs)})
	m = append(m, cbor.Pair{K: cbor.Uint(4), V: cbor.Uint(q.ExpiryMs)})
	if q.hasExt {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: q.Ext})
	}
	return m
}

func (q *Quote) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	t, err := taskAt(m, 1)
	if err != nil {
		return err
	}
	if vv, ok := mapGet(m, 2); ok {
		q.PayQuoteRef = vv
	}
	ttft, err := reqUint(m, 3)
	if err != nil {
		return err
	}
	exp, err := reqUint(m, 4)
	if err != nil {
		return err
	}
	if vv, ok := mapGet(m, 5); ok {
		q.Ext, q.hasExt = vv, true
	}
	q.Task, q.TTFTMs, q.ExpiryMs = t, ttft, exp
	return nil
}

// Assign (0x0202) — tasks §2.1.
type Assign struct {
	Task        TaskID
	Capability  string
	Constraints TaskConstraints
	Input       InputSpec
	Flow        FlowInit
	PayAgreeRef cbor.Value // optional
	Ext         cbor.Value // optional
	hasExt      bool
}

func (a Assign) ToValue() cbor.Value {
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(a.Task[:])})
	m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Text(a.Capability)})
	m = append(m, cbor.Pair{K: cbor.Uint(3), V: a.Constraints.ToValue()})
	m = append(m, cbor.Pair{K: cbor.Uint(4), V: a.Input.ToValue()})
	m = append(m, cbor.Pair{K: cbor.Uint(5), V: a.Flow.ToValue()})
	if a.PayAgreeRef != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(6), V: a.PayAgreeRef})
	}
	if a.hasExt {
		m = append(m, cbor.Pair{K: cbor.Uint(7), V: a.Ext})
	}
	return m
}

func (a *Assign) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	t, err := taskAt(m, 1)
	if err != nil {
		return err
	}
	cap, err := reqText(m, 2)
	if err != nil {
		return err
	}
	cv, ok := mapGet(m, 3)
	if !ok {
		return MsgErrMissingField
	}
	var c TaskConstraints
	if err := c.FromValue(cv); err != nil {
		return err
	}
	iv, ok := mapGet(m, 4)
	if !ok {
		return MsgErrMissingField
	}
	var inp InputSpec
	if err := inp.FromValue(iv); err != nil {
		return err
	}
	fv, ok := mapGet(m, 5)
	if !ok {
		return MsgErrMissingField
	}
	var f FlowInit
	if err := f.FromValue(fv); err != nil {
		return err
	}
	if vv, ok := mapGet(m, 6); ok {
		a.PayAgreeRef = vv
	}
	if vv, ok := mapGet(m, 7); ok {
		a.Ext, a.hasExt = vv, true
	}
	a.Task, a.Capability, a.Constraints, a.Input, a.Flow = t, cap, c, inp, f
	return nil
}

// Accept (0x0203) — empty content map.
type Accept struct{}

func (Accept) ToValue() cbor.Value { return cbor.Map{} }

func (Accept) FromValue(_ cbor.Value) error { return nil }

// Reject (0x0204) — {1 => reason-code}.
type Reject struct{ Code uint16 }

func (r Reject) ToValue() cbor.Value {
	return cbor.Map{{K: cbor.Uint(1), V: cbor.Uint(uint64(r.Code))}}
}

func (r *Reject) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	x, err := reqUint(m, 1)
	if err != nil {
		return err
	}
	if x > 0xFFFF {
		return MsgErrBadField
	}
	r.Code = uint16(x)
	return nil
}

// ChunkMsg (0x0205) — tasks §3. Distinct from the transcript Chunk: it carries
// the TaskID and fin; the transcript leaf uses only (seq, kind, content).
type ChunkMsg struct {
	Task    TaskID
	Seq     uint64
	Kind    ChunkKind
	Content []byte
	Fin     bool
}

// TranscriptChunk returns the transcript-level view.
func (c ChunkMsg) TranscriptChunk() Chunk {
	return Chunk{Seq: c.Seq, Kind: c.Kind, Content: c.Content}
}

func (c ChunkMsg) ToValue() cbor.Value {
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(c.Task[:])})
	m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Uint(c.Seq)})
	m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Uint(uint64(c.Kind))})
	m = append(m, cbor.Pair{K: cbor.Uint(4), V: cbor.Bytes(c.Content)})
	if c.Fin {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: cbor.Bool(true)})
	}
	return m
}

func (c *ChunkMsg) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	t, err := taskAt(m, 1)
	if err != nil {
		return err
	}
	seq, err := reqUint(m, 2)
	if err != nil {
		return err
	}
	kindU, err := reqUint(m, 3)
	if err != nil {
		return err
	}
	if kindU > 255 {
		return MsgErrBadField
	}
	content, err := reqBytes(m, 4)
	if err != nil {
		return err
	}
	fin := false
	if vv, ok := mapGet(m, 5); ok {
		if b, ok := vv.(cbor.Bool); ok && bool(b) {
			fin = true
		}
	}
	c.Task, c.Seq, c.Kind, c.Content, c.Fin = t, seq, ChunkKind(kindU), content, fin
	return nil
}

// Complete (0x020D) — tasks §4. TranscriptRoot is the raw 32-byte root (the
// 0x12,0x20 multihash prefix is stripped on decode).
type Complete struct {
	Task           TaskID
	Outcome        uint8
	TranscriptRoot [32]byte
	Usage          Usage
	Ext            cbor.Value
	hasExt         bool
}

func (c Complete) ToValue() cbor.Value {
	mh := make([]byte, 0, 34)
	mh = append(mh, sha256MultihashPrefix...)
	mh = append(mh, c.TranscriptRoot[:]...)
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(c.Task[:])})
	m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Uint(uint64(c.Outcome))})
	m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Bytes(mh)})
	m = append(m, cbor.Pair{K: cbor.Uint(4), V: c.Usage.ToValue()})
	if c.hasExt {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: c.Ext})
	}
	return m
}

func (c *Complete) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	t, err := taskAt(m, 1)
	if err != nil {
		return err
	}
	outU, err := reqUint(m, 2)
	if err != nil {
		return err
	}
	if outU > 255 {
		return MsgErrBadField
	}
	raw, err := reqBytes(m, 3)
	if err != nil {
		return err
	}
	if len(raw) != 34 || raw[0] != 0x12 || raw[1] != 0x20 {
		return MsgErrBadField
	}
	var root [32]byte
	copy(root[:], raw[2:])
	uv, ok := mapGet(m, 4)
	if !ok {
		return MsgErrMissingField
	}
	var u Usage
	if err := u.FromValue(uv); err != nil {
		return err
	}
	if vv, ok := mapGet(m, 5); ok {
		c.Ext, c.hasExt = vv, true
	}
	c.Task, c.Outcome, c.TranscriptRoot, c.Usage = t, uint8(outU), root, u
	return nil
}

// Cancel (0x020C) — {1 => TaskID, 2 => reason-code}.
type Cancel struct {
	Task TaskID
	Code uint16
}

func (c Cancel) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(c.Task[:])},
		{K: cbor.Uint(2), V: cbor.Uint(uint64(c.Code))},
	}
}

func (c *Cancel) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	t, err := taskAt(m, 1)
	if err != nil {
		return err
	}
	x, err := reqUint(m, 2)
	if err != nil {
		return err
	}
	if x > 0xFFFF {
		return MsgErrBadField
	}
	c.Task, c.Code = t, uint16(x)
	return nil
}

// FlowCredit (0x0007) — core §8.1.
type FlowCredit struct {
	Scope       TaskID
	MsgCredits  uint64
	ByteCredits uint64
}

func (f FlowCredit) ToValue() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(f.Scope[:])},
		{K: cbor.Uint(2), V: cbor.Uint(f.MsgCredits)},
		{K: cbor.Uint(3), V: cbor.Uint(f.ByteCredits)},
	}
}

func (f *FlowCredit) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	b, err := reqBytes(m, 1)
	if err != nil {
		return err
	}
	if len(b) != 32 {
		return MsgErrBadField
	}
	msgs, err := reqUint(m, 2)
	if err != nil {
		return err
	}
	bytes, err := reqUint(m, 3)
	if err != nil {
		return err
	}
	var scope TaskID
	copy(scope[:], b)
	f.Scope, f.MsgCredits, f.ByteCredits = scope, msgs, bytes
	return nil
}

// --- reservations (tasks §6) ----------------------------------------------

// Reserve (0x020F).
type Reserve struct {
	RsvID       [16]byte
	Capability  string
	FromMs      uint64
	UntilMs     uint64
	Constraints *TaskConstraints
	PayRef      cbor.Value
	Ext         cbor.Value
	hasExt      bool
}

func (r Reserve) ToValue() cbor.Value {
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(r.RsvID[:])})
	m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Text(r.Capability)})
	m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Uint(r.FromMs)})
	m = append(m, cbor.Pair{K: cbor.Uint(4), V: cbor.Uint(r.UntilMs)})
	if r.Constraints != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: r.Constraints.ToValue()})
	}
	if r.PayRef != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(6), V: r.PayRef})
	}
	if r.hasExt {
		m = append(m, cbor.Pair{K: cbor.Uint(7), V: r.Ext})
	}
	return m
}

func (r *Reserve) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	b, err := reqBytes(m, 1)
	if err != nil {
		return err
	}
	if len(b) != 16 {
		return MsgErrBadField
	}
	cap, err := reqText(m, 2)
	if err != nil {
		return err
	}
	from, err := reqUint(m, 3)
	if err != nil {
		return err
	}
	until, err := reqUint(m, 4)
	if err != nil {
		return err
	}
	if vv, ok := mapGet(m, 5); ok {
		var c TaskConstraints
		if err := c.FromValue(vv); err != nil {
			return err
		}
		r.Constraints = &c
	}
	if vv, ok := mapGet(m, 6); ok {
		r.PayRef = vv
	}
	if vv, ok := mapGet(m, 7); ok {
		r.Ext, r.hasExt = vv, true
	}
	copy(r.RsvID[:], b)
	r.Capability, r.FromMs, r.UntilMs = cap, from, until
	return nil
}

// Lease (0x0210). Status: 0 granted / 1 rejected / 2 revoked.
type Lease struct {
	RsvID  [16]byte
	Status uint8
	Window *[2]uint64
	PayRef cbor.Value
	Ext    cbor.Value
	hasExt bool
}

func (l Lease) ToValue() cbor.Value {
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(l.RsvID[:])})
	m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Uint(uint64(l.Status))})
	if l.Window != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Array{cbor.Uint(l.Window[0]), cbor.Uint(l.Window[1])}})
	}
	if l.PayRef != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(4), V: l.PayRef})
	}
	if l.hasExt {
		m = append(m, cbor.Pair{K: cbor.Uint(5), V: l.Ext})
	}
	return m
}

func (l *Lease) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	b, err := reqBytes(m, 1)
	if err != nil {
		return err
	}
	if len(b) != 16 {
		return MsgErrBadField
	}
	st, err := reqUint(m, 2)
	if err != nil {
		return err
	}
	if st > 255 {
		return MsgErrBadField
	}
	if vv, ok := mapGet(m, 3); ok {
		arr, ok := asArray(vv)
		if !ok || len(arr) != 2 {
			return MsgErrBadField
		}
		a, ok := asUint(arr[0])
		if !ok {
			return MsgErrBadField
		}
		b2, ok := asUint(arr[1])
		if !ok {
			return MsgErrBadField
		}
		w := [2]uint64{a, b2}
		l.Window = &w
	}
	if vv, ok := mapGet(m, 4); ok {
		l.PayRef = vv
	}
	if vv, ok := mapGet(m, 5); ok {
		l.Ext, l.hasExt = vv, true
	}
	copy(l.RsvID[:], b)
	l.Status = uint8(st)
	return nil
}

// GrantedWindowWithin reports whether a granted window falls within the
// requested [from, until] (tasks §6).
func (l Lease) GrantedWindowWithin(reqFrom, reqUntil uint64) bool {
	if l.Window == nil {
		return true
	}
	a, b := l.Window[0], l.Window[1]
	return a <= b && a >= reqFrom && b <= reqUntil
}

// LeaseRelease (0x0211).
type LeaseRelease struct {
	RsvID  [16]byte
	Reason *uint64
}

func (r LeaseRelease) ToValue() cbor.Value {
	var m cbor.Map
	m = append(m, cbor.Pair{K: cbor.Uint(1), V: cbor.Bytes(r.RsvID[:])})
	if r.Reason != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Uint(*r.Reason)})
	}
	return m
}

func (r *LeaseRelease) FromValue(v cbor.Value) error {
	m, ok := v.(cbor.Map)
	if !ok {
		return MsgErrBadField
	}
	b, err := reqBytes(m, 1)
	if err != nil {
		return err
	}
	if len(b) != 16 {
		return MsgErrBadField
	}
	if x, ok, err := optUint(m, 2); err != nil {
		return err
	} else if ok {
		r.Reason = &x
	}
	copy(r.RsvID[:], b)
	return nil
}
