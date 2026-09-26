// Package receipt implements the v2 ExecutionReceipt (spec/layer.md §6):
// a ReceiptCore signed by the requester (role 1) and/or the provider
// (role 2) over enc(["MAPReceipt", role, core-bytes]). The core is kept as
// received bytes (invariant I1): signatures are verified over the exact core
// bytes, never a re-serialization. Which parties signed decides
// admissibility (layer R-4, enforced by package trust).
package receipt

import (
	"crypto/ed25519"
	"crypto/sha256"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// Signer roles in the MAPReceipt signing structure.
const (
	RoleRequester = 1
	RoleProvider  = 2
)

// Outcome codes (ReceiptCore key 7).
const (
	OutcomeOK        = 0
	OutcomeFailed    = 1
	OutcomeTimeout   = 2
	OutcomeCancelled = 3
	OutcomeDisputed  = 4
)

func malformed(format string, args ...any) error {
	return &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt: " + fmt.Sprintf(format, args...)}
}

// ReceiptCore is the signed fact. Keys 1..8 are required; 9..13 optional
// (nil pointers / nil slices = absent). Metrics share Usage's wire shape.
type ReceiptCore struct {
	Task           core.TaskID
	Requester      identity.AgentID
	Provider       identity.AgentID
	Capability     string
	StartedMs      uint64
	EndedMs        uint64
	Outcome        uint64
	Metrics        core.Usage
	Rating         *uint64   // 9, 0..100, a subjective assertion
	PaymentRef     []byte    // 10, voucher-digest Multihash when metered
	TranscriptRoot *[32]byte // 11
	Zone           *uint8    // 12
	ResumedFrom    []byte    // 13, Multihash of a CheckpointArtifact
}

// ToValue encodes the core map.
func (c ReceiptCore) ToValue() cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(c.Task[:])},
		{K: cbor.Uint(2), V: cbor.Bytes(c.Requester[:])},
		{K: cbor.Uint(3), V: cbor.Bytes(c.Provider[:])},
		{K: cbor.Uint(4), V: cbor.Text(c.Capability)},
		{K: cbor.Uint(5), V: cbor.Uint(c.StartedMs)},
		{K: cbor.Uint(6), V: cbor.Uint(c.EndedMs)},
		{K: cbor.Uint(7), V: cbor.Uint(c.Outcome)},
		{K: cbor.Uint(8), V: c.Metrics.ToValue()},
	}
	if c.Rating != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(9), V: cbor.Uint(*c.Rating)})
	}
	if c.PaymentRef != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(10), V: cbor.Bytes(c.PaymentRef)})
	}
	if c.TranscriptRoot != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(11), V: cbor.Bytes(c.TranscriptRoot[:])})
	}
	if c.Zone != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(12), V: cbor.Uint(*c.Zone)})
	}
	if c.ResumedFrom != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(13), V: cbor.Bytes(c.ResumedFrom)})
	}
	return m
}

// Encode returns the deterministic core bytes.
func (c ReceiptCore) Encode() []byte { return cbor.Encode(c.ToValue()) }

// CoreFromValue decodes and validates a ReceiptCore.
func CoreFromValue(v cbor.Value) (ReceiptCore, error) {
	var c ReceiptCore
	m, ok := v.(cbor.Map)
	if !ok {
		return c, malformed("core must be a map")
	}
	bytesF := func(k uint64) ([]byte, error) {
		x, ok := m.Get(k)
		b, isB := x.(cbor.Bytes)
		if !ok || !isB {
			return nil, malformed("field %d missing or not bytes", k)
		}
		return b, nil
	}
	uintF := func(k uint64) (uint64, error) {
		x, ok := m.Get(k)
		u, isU := x.(cbor.Uint)
		if !ok || !isU {
			return 0, malformed("field %d missing or not a uint", k)
		}
		return uint64(u), nil
	}
	b, err := bytesF(1)
	if err != nil {
		return c, err
	}
	if c.Task, ok = core.TaskIDFromBytes(b); !ok {
		return c, malformed("field 1 must be 32 bytes")
	}
	for _, k := range []uint64{2, 3} {
		b, err := bytesF(k)
		if err != nil {
			return c, err
		}
		aid, err := identity.Parse(b)
		if err != nil {
			return c, malformed("field %d: %v", k, err)
		}
		if k == 2 {
			c.Requester = aid
		} else {
			c.Provider = aid
		}
	}
	capV, ok := m.Get(4)
	capT, isT := capV.(cbor.Text)
	if !ok || !isT {
		return c, malformed("field 4 missing or not text")
	}
	c.Capability = string(capT)
	if c.StartedMs, err = uintF(5); err != nil {
		return c, err
	}
	if c.EndedMs, err = uintF(6); err != nil {
		return c, err
	}
	if c.EndedMs < c.StartedMs {
		return c, malformed("ended before started")
	}
	if c.Outcome, err = uintF(7); err != nil || c.Outcome > OutcomeDisputed {
		return c, malformed("field 7 missing or not an outcome")
	}
	mv, ok := m.Get(8)
	if !ok {
		return c, malformed("field 8 missing")
	}
	if c.Metrics, err = core.UsageFromValue(mv); err != nil {
		return c, malformed("field 8: %v", err)
	}
	if _, ok := m.Get(9); ok {
		r, err := uintF(9)
		if err != nil || r > 100 {
			return c, malformed("field 9 must be 0..100")
		}
		c.Rating = &r
	}
	if _, ok := m.Get(10); ok {
		if c.PaymentRef, err = bytesF(10); err != nil {
			return c, err
		}
	}
	if _, ok := m.Get(11); ok {
		b, err := bytesF(11)
		if err != nil || len(b) != 32 {
			return c, malformed("field 11 must be 32 bytes")
		}
		var root [32]byte
		copy(root[:], b)
		c.TranscriptRoot = &root
	}
	if _, ok := m.Get(12); ok {
		z, err := uintF(12)
		if err != nil || z > 3 {
			return c, malformed("field 12 must be a zone 0..3")
		}
		zz := uint8(z)
		c.Zone = &zz
	}
	if _, ok := m.Get(13); ok {
		b, err := bytesF(13)
		if err != nil || len(b) != 34 || b[0] != 0x12 || b[1] != 0x20 {
			return c, malformed("field 13 must be a sha2-256 Multihash")
		}
		c.ResumedFrom = b
	}
	return c, nil
}

// SigningInput is enc(["MAPReceipt", role, core-bytes]).
func SigningInput(role uint64, coreBytes []byte) []byte {
	return cbor.Encode(cbor.Array{cbor.Text("MAPReceipt"), cbor.Uint(role), cbor.Bytes(coreBytes)})
}

// Sign produces the role-bound signature over the exact core bytes.
func Sign(priv ed25519.PrivateKey, role uint64, coreBytes []byte) []byte {
	return ed25519.Sign(priv, SigningInput(role, coreBytes))
}

// Receipt is the wire object {1: core-bytes, ?2: sig_req, ?3: sig_prov}.
type Receipt struct {
	CoreBytes []byte
	SigReq    []byte // nil = absent
	SigProv   []byte // nil = absent
}

// Encode returns the wire bytes (TV-002's form when both signatures exist).
func (r Receipt) Encode() []byte {
	m := cbor.Map{{K: cbor.Uint(1), V: cbor.Bytes(r.CoreBytes)}}
	if r.SigReq != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(2), V: cbor.Bytes(r.SigReq)})
	}
	if r.SigProv != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(3), V: cbor.Bytes(r.SigProv)})
	}
	return cbor.Encode(m)
}

// Decode strictly decodes wire bytes (structure only; no signature checks).
func Decode(data []byte) (Receipt, error) {
	var r Receipt
	v, err := cbor.Decode(data)
	if err != nil {
		return r, &envelope.Error{Code: envelope.CodeMalformed, Msg: err.Error()}
	}
	m, ok := v.(cbor.Map)
	if !ok {
		return r, malformed("must be a map")
	}
	cv, ok := m.Get(1)
	cb, isB := cv.(cbor.Bytes)
	if !ok || !isB {
		return r, malformed("key 1 (core) missing or not bytes")
	}
	r.CoreBytes = cb
	for _, k := range []uint64{2, 3} {
		sv, present := m.Get(k)
		if !present {
			continue
		}
		s, isB := sv.(cbor.Bytes)
		if !isB || len(s) != 64 {
			return r, malformed("key %d must be a 64-byte signature", k)
		}
		if k == 2 {
			r.SigReq = s
		} else {
			r.SigProv = s
		}
	}
	return r, nil
}

// Core decodes the core bytes.
func (r Receipt) Core() (ReceiptCore, error) {
	v, err := cbor.Decode(r.CoreBytes)
	if err != nil {
		return ReceiptCore{}, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt core: " + err.Error()}
	}
	return CoreFromValue(v)
}

// Verified is a receipt whose present signatures verified against the
// identities in its core (layer R-1).
type Verified struct {
	Core            ReceiptCore
	Dual            bool // both signatures present and valid
	RequesterSigned bool
	ProviderSigned  bool
}

// Verify checks every present signature (at least one is required).
func (r Receipt) Verify() (Verified, error) {
	c, err := r.Core()
	if err != nil {
		return Verified{}, err
	}
	if r.SigReq == nil && r.SigProv == nil {
		return Verified{}, malformed("no signatures")
	}
	if r.SigReq != nil {
		if err := identity.VerifyStrict(c.Requester.Pub(), SigningInput(RoleRequester, r.CoreBytes), r.SigReq); err != nil {
			return Verified{}, &envelope.Error{Code: envelope.CodeBadSignature, Msg: fmt.Sprintf("requester signature: %v", err)}
		}
	}
	if r.SigProv != nil {
		if err := identity.VerifyStrict(c.Provider.Pub(), SigningInput(RoleProvider, r.CoreBytes), r.SigProv); err != nil {
			return Verified{}, &envelope.Error{Code: envelope.CodeBadSignature, Msg: fmt.Sprintf("provider signature: %v", err)}
		}
	}
	return Verified{
		Core: c, Dual: r.SigReq != nil && r.SigProv != nil,
		RequesterSigned: r.SigReq != nil, ProviderSigned: r.SigProv != nil,
	}, nil
}

// ContentAddress is SHA-256 of the encoded receipt (the store's dedup key).
func (r Receipt) ContentAddress() [32]byte { return sha256.Sum256(r.Encode()) }
