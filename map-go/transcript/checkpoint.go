package transcript

// Checkpoints and cross-provider resumption (spec/layer.md §7).
//
// A CheckpointArtifact mirrors an ExecutionReceipt: both parties sign the
// exact same core bytes with a role-tagged input
// enc(["MAPCheckpoint", role, core-bytes]), so the cumulative usage, zone and
// voucher digest are covered by both signatures.

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"errors"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// Signer roles in the MAPCheckpoint signing input.
const (
	RoleRequester = 1
	RoleProvider  = 2
)

// Checkpoint errors. Field errors wrap ErrField with the key.
var (
	ErrField         = errors.New("checkpoint: missing or invalid field")
	ErrNoSignatures  = errors.New("checkpoint: no signatures")
	ErrSelfDealing   = errors.New("checkpoint: requester == provider")
	ErrNotDualSigned = errors.New("checkpoint: not dual-signed (not a resumption point)")
	ErrNotMonotone   = errors.New("checkpoint: not a valid successor")
	ErrRootMismatch  = errors.New("checkpoint: transcript root mismatch")
	ErrBadPrefix     = errors.New("checkpoint: prefix is not seq 0..through-seq")
	ErrBadSignature  = errors.New("checkpoint: bad signature")
)

func fieldErr(k uint64) error { return fmt.Errorf("%w %d", ErrField, k) }

// MultihashSHA256 is 0x12 0x20 || SHA-256(data).
func MultihashSHA256(data []byte) []byte {
	d := sha256.Sum256(data)
	return append([]byte{0x12, 0x20}, d[:]...)
}

func isMultihash(b []byte) bool { return len(b) == 34 && b[0] == 0x12 && b[1] == 0x20 }

// CheckpointCore (layer §7.1). Keys: 1 TaskID, 2 requester, 3 provider,
// 4 through-seq (inclusive), 5 transcript root (Multihash), 6 cumulative
// Usage, 7 zone, 8 timestamp, ?9 voucher-digest, ?10 resumed-from.
type CheckpointCore struct {
	Task          core.TaskID
	Requester     identity.AgentID
	Provider      identity.AgentID
	ThroughSeq    uint64
	Root          [32]byte
	Usage         core.Usage
	Zone          uint8
	TimestampMs   uint64
	VoucherDigest []byte // nil = absent
	ResumedFrom   []byte // nil = absent
}

// ToValue encodes the core map.
func (c CheckpointCore) ToValue() cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(c.Task[:])},
		{K: cbor.Uint(2), V: cbor.Bytes(c.Requester[:])},
		{K: cbor.Uint(3), V: cbor.Bytes(c.Provider[:])},
		{K: cbor.Uint(4), V: cbor.Uint(c.ThroughSeq)},
		{K: cbor.Uint(5), V: cbor.Bytes(append([]byte{0x12, 0x20}, c.Root[:]...))},
		{K: cbor.Uint(6), V: c.Usage.ToValue()},
		{K: cbor.Uint(7), V: cbor.Uint(c.Zone)},
		{K: cbor.Uint(8), V: cbor.Uint(c.TimestampMs)},
	}
	if c.VoucherDigest != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(9), V: cbor.Bytes(c.VoucherDigest)})
	}
	if c.ResumedFrom != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(10), V: cbor.Bytes(c.ResumedFrom)})
	}
	return m
}

// Encode returns the deterministic core bytes.
func (c CheckpointCore) Encode() []byte { return cbor.Encode(c.ToValue()) }

func getBytes(m cbor.Map, k uint64) ([]byte, error) {
	v, ok := m.Get(k)
	if !ok {
		return nil, fieldErr(k)
	}
	b, ok := v.(cbor.Bytes)
	if !ok {
		return nil, fieldErr(k)
	}
	return b, nil
}

func getUint(m cbor.Map, k uint64) (uint64, error) {
	v, ok := m.Get(k)
	if !ok {
		return 0, fieldErr(k)
	}
	u, ok := v.(cbor.Uint)
	if !ok {
		return 0, fieldErr(k)
	}
	return uint64(u), nil
}

func optMultihash(m cbor.Map, k uint64) ([]byte, error) {
	if _, ok := m.Get(k); !ok {
		return nil, nil
	}
	b, err := getBytes(m, k)
	if err != nil || !isMultihash(b) {
		return nil, fieldErr(k)
	}
	return b, nil
}

// CheckpointCoreFromValue decodes and validates a core map.
func CheckpointCoreFromValue(v cbor.Value) (CheckpointCore, error) {
	var c CheckpointCore
	m, ok := v.(cbor.Map)
	if !ok {
		return c, fmt.Errorf("%w: core is not a map", ErrField)
	}
	b, err := getBytes(m, 1)
	if err != nil {
		return c, err
	}
	if c.Task, ok = core.TaskIDFromBytes(b); !ok {
		return c, fieldErr(1)
	}
	for _, k := range []uint64{2, 3} {
		b, err := getBytes(m, k)
		if err != nil {
			return c, err
		}
		aid, err := identity.Parse(b)
		if err != nil {
			return c, fieldErr(k)
		}
		if k == 2 {
			c.Requester = aid
		} else {
			c.Provider = aid
		}
	}
	if c.ThroughSeq, err = getUint(m, 4); err != nil {
		return c, err
	}
	root, err := getBytes(m, 5)
	if err != nil || !isMultihash(root) {
		return c, fieldErr(5)
	}
	copy(c.Root[:], root[2:])
	uv, ok := m.Get(6)
	if !ok {
		return c, fieldErr(6)
	}
	if c.Usage, err = core.UsageFromValue(uv); err != nil {
		return c, fieldErr(6)
	}
	zone, err := getUint(m, 7)
	if err != nil || zone > 3 {
		return c, fieldErr(7)
	}
	c.Zone = uint8(zone)
	if c.TimestampMs, err = getUint(m, 8); err != nil {
		return c, err
	}
	if c.VoucherDigest, err = optMultihash(m, 9); err != nil {
		return c, err
	}
	if c.ResumedFrom, err = optMultihash(m, 10); err != nil {
		return c, err
	}
	return c, nil
}

// CheckpointSigningInput is enc(["MAPCheckpoint", role, core-bytes]).
func CheckpointSigningInput(role uint64, coreBytes []byte) []byte {
	return cbor.Encode(cbor.Array{cbor.Text("MAPCheckpoint"), cbor.Uint(role), cbor.Bytes(coreBytes)})
}

// SignCheckpoint signs the role-tagged input over the exact core bytes.
func SignCheckpoint(priv ed25519.PrivateKey, role uint64, coreBytes []byte) []byte {
	return ed25519.Sign(priv, CheckpointSigningInput(role, coreBytes))
}

// CheckpointArtifact is the wire object {1: core-bytes, ?2: sig_req,
// ?3: sig_prov}. Signatures are verified over the received core bytes,
// never a re-serialization.
type CheckpointArtifact struct {
	CoreBytes []byte
	SigReq    []byte // nil = absent
	SigProv   []byte // nil = absent
}

// Propose builds the provider-signed artifact (C-1).
func Propose(provider ed25519.PrivateKey, c CheckpointCore) CheckpointArtifact {
	cb := c.Encode()
	return CheckpointArtifact{CoreBytes: cb, SigProv: SignCheckpoint(provider, RoleProvider, cb)}
}

// Countersign adds the requester signature (C-2). The caller MUST first
// have checked the root against the content it received (VerifyPrefix).
func (a *CheckpointArtifact) Countersign(requester ed25519.PrivateKey) {
	a.SigReq = SignCheckpoint(requester, RoleRequester, a.CoreBytes)
}

// Encode returns the wire bytes.
func (a CheckpointArtifact) Encode() []byte {
	m := cbor.Map{{K: cbor.Uint(1), V: cbor.Bytes(a.CoreBytes)}}
	if a.SigReq != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Bytes(a.SigReq)})
	}
	if a.SigProv != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Bytes(a.SigProv)})
	}
	return cbor.Encode(m)
}

// DecodeArtifact strictly decodes the wire bytes.
func DecodeArtifact(b []byte) (CheckpointArtifact, error) {
	var a CheckpointArtifact
	v, err := cbor.Decode(b)
	if err != nil {
		return a, err
	}
	m, ok := v.(cbor.Map)
	if !ok {
		return a, fmt.Errorf("%w: artifact is not a map", ErrField)
	}
	if a.CoreBytes, err = getBytes(m, 1); err != nil {
		return a, err
	}
	for _, k := range []uint64{2, 3} {
		if _, present := m.Get(k); !present {
			continue
		}
		s, err := getBytes(m, k)
		if err != nil || len(s) != 64 {
			return a, fieldErr(k)
		}
		if k == 2 {
			a.SigReq = s
		} else {
			a.SigProv = s
		}
	}
	return a, nil
}

// Core decodes the core bytes.
func (a CheckpointArtifact) Core() (CheckpointCore, error) {
	v, err := cbor.Decode(a.CoreBytes)
	if err != nil {
		return CheckpointCore{}, err
	}
	return CheckpointCoreFromValue(v)
}

// Verified is a checkpoint whose present signatures verified.
type Verified struct {
	Core CheckpointCore
	// Dual: both signatures present and valid, a valid resumption point.
	Dual bool
}

// Verify checks every present signature against the identity it claims and
// rejects self-dealing. It does not require dual signing.
func (a CheckpointArtifact) Verify() (Verified, error) {
	c, err := a.Core()
	if err != nil {
		return Verified{}, err
	}
	if a.SigReq == nil && a.SigProv == nil {
		return Verified{}, ErrNoSignatures
	}
	if c.Requester == c.Provider {
		return Verified{}, ErrSelfDealing
	}
	if a.SigReq != nil {
		if err := identity.VerifyStrict(c.Requester.Pub(), CheckpointSigningInput(RoleRequester, a.CoreBytes), a.SigReq); err != nil {
			return Verified{}, fmt.Errorf("%w: requester: %v", ErrBadSignature, err)
		}
	}
	if a.SigProv != nil {
		if err := identity.VerifyStrict(c.Provider.Pub(), CheckpointSigningInput(RoleProvider, a.CoreBytes), a.SigProv); err != nil {
			return Verified{}, fmt.Errorf("%w: provider: %v", ErrBadSignature, err)
		}
	}
	return Verified{Core: c, Dual: a.SigReq != nil && a.SigProv != nil}, nil
}

// VerifyResumptionPoint: a valid resumption point is dual-signed and
// verifies (layer §7.2).
func (a CheckpointArtifact) VerifyResumptionPoint() (CheckpointCore, error) {
	v, err := a.Verify()
	if err != nil {
		return CheckpointCore{}, err
	}
	if !v.Dual {
		return CheckpointCore{}, ErrNotDualSigned
	}
	return v.Core, nil
}

// Digest is the Multihash of the encoded artifact: the resumed-from value
// a successor's receipt (key 13) and checkpoints (key 10) carry.
func (a CheckpointArtifact) Digest() []byte { return MultihashSHA256(a.Encode()) }

// CheckSuccessor enforces C-3/C-4: same task and parties, through-seq
// advances, and no usage component regresses.
func CheckSuccessor(prev, next CheckpointCore) error {
	if next.Task != prev.Task || next.Requester != prev.Requester || next.Provider != prev.Provider {
		return fmt.Errorf("%w: task or parties changed", ErrNotMonotone)
	}
	if next.ThroughSeq <= prev.ThroughSeq {
		return fmt.Errorf("%w: through-seq did not advance", ErrNotMonotone)
	}
	if !next.Usage.Dominates(prev.Usage) {
		return fmt.Errorf("%w: usage regressed", ErrNotMonotone)
	}
	return nil
}

// VerifyPrefix recomputes the transcript root over prefix (exactly the
// chunks with seq 0..through-seq, in order) and compares it with the
// checkpoint's root (C-2 for a requester; §7.4 step 2 for a successor).
func VerifyPrefix(c CheckpointCore, prefix []Chunk) (*TranscriptBuilder, error) {
	b := NewTranscript()
	for i, ch := range prefix {
		if ch.Seq != uint64(i) {
			return nil, fmt.Errorf("%w: seq not contiguous from 0", ErrBadPrefix)
		}
		b.Push(ch)
	}
	if len(prefix) == 0 || uint64(len(prefix)-1) != c.ThroughSeq {
		return nil, fmt.Errorf("%w: prefix does not end at through-seq", ErrBadPrefix)
	}
	root := b.Root()
	if !bytes.Equal(root[:], c.Root[:]) {
		return nil, ErrRootMismatch
	}
	return b, nil
}

// Resumed is the state a successor provider continues from.
type Resumed struct {
	Core        CheckpointCore
	Builder     *TranscriptBuilder
	NextSeq     uint64
	ResumedFrom []byte
}

// Resume (§7.4): accept artifact only if it is a valid resumption point AND
// its root matches prefix.
func Resume(a CheckpointArtifact, prefix []Chunk) (Resumed, error) {
	c, err := a.VerifyResumptionPoint()
	if err != nil {
		return Resumed{}, err
	}
	b, err := VerifyPrefix(c, prefix)
	if err != nil {
		return Resumed{}, err
	}
	return Resumed{Core: c, Builder: b, NextSeq: c.ThroughSeq + 1, ResumedFrom: a.Digest()}, nil
}
