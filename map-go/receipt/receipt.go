// Package receipt implements the dual-signed ExecutionReceipt of
// draft-map-trust-00 §3 (TV-002 / NV-005). The receipt core is kept as
// received bytes (invariant I1): signatures are computed and verified
// over enc(["MAPReceipt", role, core-bytes]) with the exact core bytes.
package receipt

import (
	"crypto/ed25519"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// Signer roles in the MAPReceipt signing structure (trust §3).
const (
	RoleRequester = 1
	RoleProvider  = 2
)

// Receipt is a parsed receipt object {1: core, 2: sig_req, 3: sig_prov}.
type Receipt struct {
	Core      []byte // exact received core bytes (I1)
	SigReq    []byte
	SigProv   []byte
	Requester identity.AgentID // ReceiptCore key 2
	Provider  identity.AgentID // ReceiptCore key 3
}

func signingBytes(role uint64, core []byte) []byte {
	return cbor.Encode(cbor.Array{cbor.Text("MAPReceipt"), cbor.Uint(role), cbor.Bytes(core)})
}

// Sign produces the role-bound signature over the exact core bytes.
func Sign(priv ed25519.PrivateKey, role uint64, core []byte) []byte {
	return ed25519.Sign(priv, signingBytes(role, core))
}

// Encode assembles the receipt object {1: core, 2: sig_req, 3: sig_prov}.
func Encode(core, sigReq, sigProv []byte) []byte {
	return cbor.Encode(cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(core)},
		{K: cbor.Uint(2), V: cbor.Bytes(sigReq)},
		{K: cbor.Uint(3), V: cbor.Bytes(sigProv)},
	})
}

// Parse strictly decodes a receipt object and extracts the party
// AgentIDs from its core, without verifying signatures (structural
// validity, trust §3).
func Parse(data []byte) (*Receipt, error) {
	v, err := cbor.Decode(data)
	if err != nil {
		return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: err.Error()}
	}
	m, ok := v.(cbor.Map)
	if !ok || len(m) != 3 {
		return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt must be a 3-entry map"}
	}
	r := &Receipt{}
	for _, p := range m {
		k, ok := p.K.(cbor.Uint)
		if !ok {
			return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt: non-uint key"}
		}
		b, ok := p.V.(cbor.Bytes)
		if !ok {
			return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt: values must be byte strings"}
		}
		switch uint64(k) {
		case 1:
			r.Core = b
		case 2:
			r.SigReq = b
		case 3:
			r.SigProv = b
		default:
			return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt: unknown key"}
		}
	}
	if r.Core == nil || len(r.SigReq) != 64 || len(r.SigProv) != 64 {
		return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt: missing core or malformed signatures"}
	}
	// Extract the parties from ReceiptCore (trust §3: 2 requester, 3 provider).
	cv, err := cbor.Decode(r.Core)
	if err != nil {
		return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt core: " + err.Error()}
	}
	cm, ok := cv.(cbor.Map)
	if !ok {
		return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt core must be a map"}
	}
	var haveReq, haveProv bool
	for _, p := range cm {
		k, ok := p.K.(cbor.Uint)
		if !ok {
			continue
		}
		if uint64(k) == 2 || uint64(k) == 3 {
			b, ok := p.V.(cbor.Bytes)
			if !ok {
				return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt core: party must be AgentID bytes"}
			}
			aid, err := identity.Parse(b)
			if err != nil {
				return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt core: " + err.Error()}
			}
			if uint64(k) == 2 {
				r.Requester, haveReq = aid, true
			} else {
				r.Provider, haveProv = aid, true
			}
		}
	}
	if !haveReq || !haveProv {
		return nil, &envelope.Error{Code: envelope.CodeMalformed, Msg: "receipt core missing requester/provider"}
	}
	return r, nil
}

// Verify parses data and verifies BOTH role-bound signatures (the trust-use
// requirement; a single-signed receipt is only structurally valid — use
// Parse for that). Role binding: requester signs role 1, provider role 2,
// so swapped signatures (NV-005) fail both checks.
func Verify(data []byte) (*Receipt, error) {
	r, err := Parse(data)
	if err != nil {
		return nil, err
	}
	if err := identity.VerifyStrict(r.Requester.Pub(), signingBytes(RoleRequester, r.Core), r.SigReq); err != nil {
		return nil, &envelope.Error{Code: envelope.CodeBadSignature, Msg: fmt.Sprintf("requester signature: %v", err)}
	}
	if err := identity.VerifyStrict(r.Provider.Pub(), signingBytes(RoleProvider, r.Core), r.SigProv); err != nil {
		return nil, &envelope.Error{Code: envelope.CodeBadSignature, Msg: fmt.Sprintf("provider signature: %v", err)}
	}
	return r, nil
}
