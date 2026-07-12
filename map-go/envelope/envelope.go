// Package envelope implements the MAP COSE_Sign1 message envelope
// (draft-map-core-00 §4): Seal produces deterministic, signed envelopes;
// Open verifies over the exact received bytes (invariant I1 — never
// re-encode to verify) with the strict Ed25519 profile.
package envelope

import (
	"crypto/ed25519"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// Error carries the core §9 error class for NV assertions.
type Error struct {
	Code int // core §9 code: 1000 malformed, 2000 bad signature, 2003 bad delegation, ...
	Msg  string
}

func (e *Error) Error() string { return fmt.Sprintf("envelope: [%d] %s", e.Code, e.Msg) }

// Core §9 codes used by this package.
const (
	CodeMalformed     = 1000
	CodeBadSignature  = 2000
	CodeExpired       = 2001
	CodeBadDelegation = 2003
)

func malformed(format string, args ...any) *Error {
	return &Error{Code: CodeMalformed, Msg: fmt.Sprintf(format, args...)}
}

const (
	coseTagSign1 = 18
	coseAlgEdDSA = -8
	hdrAlg       = 1
	hdrKid       = 4
	hdrChain     = 33 // delegation chain, identity §6
	versionV1    = 0x0100
)

// Body is the parsed envelope body (core §4.1). Payload field numbers:
// 0 version, 1 msg-type, 2 msg-id, 3 timestamp, 10 content.
type Body struct {
	Version   uint64
	MsgType   uint64
	MsgID     []byte
	Timestamp uint64
	Content   cbor.Value
}

// Opened is a verified envelope. Payload preserves the received payload
// bytes (I1); Chain holds raw delegation-cert envelopes leaf..root, or nil.
type Opened struct {
	KID       identity.AgentID
	Protected []byte
	Payload   []byte
	Signature []byte
	Chain     [][]byte
	Body      Body
}

// buildBody encodes the standard Body map (core §4.1) for msg-type/content.
func buildBody(msgType uint64, msgID []byte, ts uint64, content cbor.Value) []byte {
	return cbor.Encode(cbor.Map{
		{K: cbor.Uint(0), V: cbor.Uint(versionV1)},
		{K: cbor.Uint(1), V: cbor.Uint(msgType)},
		{K: cbor.Uint(2), V: cbor.Bytes(msgID)},
		{K: cbor.Uint(3), V: cbor.Uint(ts)},
		{K: cbor.Uint(10), V: content},
	})
}

func sigStructure(protected, externalAAD, payload []byte) []byte {
	if externalAAD == nil {
		externalAAD = []byte{}
	}
	return cbor.Encode(cbor.Array{
		cbor.Text("Signature1"), cbor.Bytes(protected),
		cbor.Bytes(externalAAD), cbor.Bytes(payload),
	})
}

func seal(priv ed25519.PrivateKey, protected, payload, externalAAD []byte) []byte {
	sig := ed25519.Sign(priv, sigStructure(protected, externalAAD, payload))
	return cbor.Encode(cbor.Tag{Num: coseTagSign1, Content: cbor.Array{
		cbor.Bytes(protected), cbor.Map{}, cbor.Bytes(payload), cbor.Bytes(sig),
	}})
}

// Seal builds and signs a standard envelope (core §4.1).
func Seal(priv ed25519.PrivateKey, msgType uint64, msgID []byte, ts uint64, content cbor.Value, externalAAD []byte) ([]byte, error) {
	kid, err := identity.FromPub(priv.Public().(ed25519.PublicKey))
	if err != nil {
		return nil, err
	}
	protected := cbor.Encode(cbor.Map{
		{K: cbor.Uint(hdrAlg), V: cbor.Int(coseAlgEdDSA)},
		{K: cbor.Uint(hdrKid), V: cbor.Bytes(kid[:])},
	})
	return seal(priv, protected, buildBody(msgType, msgID, ts, content), externalAAD), nil
}

// SealDelegated is Seal with a delegation chain (raw cert-envelope byte
// strings, leaf..root) in protected-header key 33 (identity §6). The chain
// sits inside the signed protected header, so the leaf signature covers it.
func SealDelegated(priv ed25519.PrivateKey, msgType uint64, msgID []byte, ts uint64, content cbor.Value, chain [][]byte, externalAAD []byte) ([]byte, error) {
	kid, err := identity.FromPub(priv.Public().(ed25519.PublicKey))
	if err != nil {
		return nil, err
	}
	certs := make(cbor.Array, len(chain))
	for i, c := range chain {
		certs[i] = cbor.Bytes(c)
	}
	protected := cbor.Encode(cbor.Map{
		{K: cbor.Uint(hdrAlg), V: cbor.Int(coseAlgEdDSA)},
		{K: cbor.Uint(hdrKid), V: cbor.Bytes(kid[:])},
		{K: cbor.Uint(hdrChain), V: certs},
	})
	return seal(priv, protected, buildBody(msgType, msgID, ts, content), externalAAD), nil
}

// Open verifies an envelope over its exact received bytes (core §4.3
// steps 1, 2, 7) and parses the body. It does NOT validate a delegation
// chain — see OpenDelegated. Delegated envelopes still verify here against
// the leaf key named by kid.
func Open(env, externalAAD []byte) (*Opened, error) {
	top, err := cbor.Decode(env)
	if err != nil {
		return nil, malformed("%v", err)
	}
	tag, ok := top.(cbor.Tag)
	if !ok || tag.Num != coseTagSign1 {
		return nil, malformed("not a COSE_Sign1 (tag 18) envelope")
	}
	arr, ok := tag.Content.(cbor.Array)
	if !ok || len(arr) != 4 {
		return nil, malformed("COSE_Sign1 must be a 4-element array")
	}
	protected, ok := arr[0].(cbor.Bytes)
	if !ok {
		return nil, malformed("protected header must be a byte string")
	}
	unprot, ok := arr[1].(cbor.Map)
	if !ok || len(unprot) != 0 {
		return nil, malformed("unprotected header must be an empty map")
	}
	payload, ok := arr[2].(cbor.Bytes)
	if !ok {
		return nil, malformed("payload must be a byte string")
	}
	sig, ok := arr[3].(cbor.Bytes)
	if !ok || len(sig) != 64 {
		return nil, malformed("signature must be a 64-byte string")
	}

	kid, chain, err := parseProtected(protected)
	if err != nil {
		return nil, err
	}
	// Verify over the exact received byte strings (I1): Sig_structure is
	// rebuilt around them, never around re-encoded objects.
	if err := identity.VerifyStrict(kid.Pub(), sigStructure(protected, externalAAD, payload), sig); err != nil {
		return nil, &Error{Code: CodeBadSignature, Msg: err.Error()}
	}
	body, err := parseBody(payload)
	if err != nil {
		return nil, err
	}
	return &Opened{
		KID: kid, Protected: protected, Payload: payload,
		Signature: sig, Chain: chain, Body: body,
	}, nil
}

func parseProtected(protected []byte) (identity.AgentID, [][]byte, error) {
	var zero identity.AgentID
	v, err := cbor.Decode(protected)
	if err != nil {
		return zero, nil, malformed("protected header: %v", err)
	}
	m, ok := v.(cbor.Map)
	if !ok {
		return zero, nil, malformed("protected header must be a map")
	}
	var kid identity.AgentID
	var haveAlg, haveKid bool
	var chain [][]byte
	for _, p := range m {
		k, ok := p.K.(cbor.Uint)
		if !ok {
			return zero, nil, malformed("protected header: non-uint key")
		}
		switch uint64(k) {
		case hdrAlg:
			alg, ok := p.V.(cbor.Int)
			if !ok || alg != coseAlgEdDSA {
				return zero, nil, malformed("alg must be EdDSA (-8)")
			}
			haveAlg = true
		case hdrKid:
			b, ok := p.V.(cbor.Bytes)
			if !ok {
				return zero, nil, malformed("kid must be a byte string")
			}
			kid, err = identity.Parse(b)
			if err != nil {
				return zero, nil, malformed("kid: %v", err)
			}
			haveKid = true
		case hdrChain:
			arr, ok := p.V.(cbor.Array)
			if !ok || len(arr) == 0 {
				return zero, nil, malformed("delegation chain must be a non-empty array")
			}
			for _, c := range arr {
				cb, ok := c.(cbor.Bytes)
				if !ok {
					return zero, nil, malformed("delegation chain entries must be byte strings")
				}
				chain = append(chain, cb)
			}
		default:
			return zero, nil, malformed("unknown protected header key %d", uint64(k))
		}
	}
	if !haveAlg || !haveKid {
		return zero, nil, malformed("protected header missing alg or kid")
	}
	return kid, chain, nil
}

func parseBody(payload []byte) (Body, error) {
	var b Body
	v, err := cbor.Decode(payload)
	if err != nil {
		return b, malformed("payload: %v", err)
	}
	m, ok := v.(cbor.Map)
	if !ok {
		return b, malformed("body must be a map")
	}
	var have0, have1, have2, have3 bool
	for _, p := range m {
		k, ok := p.K.(cbor.Uint)
		if !ok {
			return b, malformed("body: non-uint key")
		}
		switch uint64(k) {
		case 0:
			u, ok := p.V.(cbor.Uint)
			if !ok {
				return b, malformed("body: version must be uint")
			}
			b.Version, have0 = uint64(u), true
		case 1:
			u, ok := p.V.(cbor.Uint)
			if !ok {
				return b, malformed("body: msg-type must be uint")
			}
			b.MsgType, have1 = uint64(u), true
		case 2:
			bs, ok := p.V.(cbor.Bytes)
			if !ok || len(bs) != 16 {
				return b, malformed("body: msg-id must be a 16-byte string")
			}
			b.MsgID, have2 = bs, true
		case 3:
			u, ok := p.V.(cbor.Uint)
			if !ok {
				return b, malformed("body: timestamp must be uint")
			}
			b.Timestamp, have3 = uint64(u), true
		case 10:
			b.Content = p.V
		default:
			// Optional body keys 4..9 (core §4.1): accepted, not modeled here.
		}
	}
	if !have0 || !have1 || !have2 || !have3 || b.Content == nil {
		return b, malformed("body missing a required key (0,1,2,3,10)")
	}
	return b, nil
}
