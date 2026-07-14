package envelope

import (
	"strings"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// Delegation-chain validation, draft-map-identity-01 §4 / §6.
//
// A DelegationCert is itself an envelope (msg-type 0x0502) whose kid is the
// ISSUER; its Body content is DelegateBody:
//   1 issuer, 2 subject, 3 not-before, 4 not-after,
//   ?5 [CapURI] scope (absent = all), ?6 [ZoneLabel], ?7 max further
//   delegation depth, ?8 caveats (unknown caveat key ⇒ chain invalid,
//   fail-closed).
//
// Chains are carried leaf..root and MUST NOT exceed 8 certs. Scopes only
// ever attenuate: a child scope MUST be a subset of the parent's (⇒ 2003).
// Per the identity §4 editorial note (frozen by TV-012), the data
// envelope's signature verifies against the LEAF key (kid == leaf
// subject); the chain resolves the accountable ROOT identity.

const msgTypeDelegate = 0x0502

// DelegateBody is a parsed delegation certificate body (identity §4).
type DelegateBody struct {
	Issuer    identity.AgentID
	Subject   identity.AgentID
	NotBefore uint64
	NotAfter  uint64
	Scope     []string // nil = all capabilities
	HasScope  bool
	MaxDepth  uint64
	HasDepth  bool
}

func badDelegation(msg string) *Error {
	return &Error{Code: CodeBadDelegation, Msg: msg}
}

// OpenDelegated opens env and, if it carries a chain in protected key 33,
// validates the chain at verification time `at` (ms). Returns the opened
// envelope plus the accountable root AgentID (the kid itself when no
// chain is present).
func OpenDelegated(env, externalAAD []byte, at uint64) (*Opened, identity.AgentID, error) {
	o, err := Open(env, externalAAD)
	if err != nil {
		return nil, identity.AgentID{}, err
	}
	if len(o.Chain) == 0 {
		return o, o.KID, nil
	}
	if len(o.Chain) > 8 {
		return nil, identity.AgentID{}, badDelegation("chain longer than 8 certificates")
	}
	certs := make([]DelegateBody, len(o.Chain))
	for i, raw := range o.Chain {
		co, err := Open(raw, nil) // certs are out-of-session: empty external_aad
		if err != nil {
			return nil, identity.AgentID{}, badDelegation("cert did not open: " + err.Error())
		}
		if co.Body.MsgType != msgTypeDelegate {
			return nil, identity.AgentID{}, badDelegation("chain entry is not a DELEGATE (0x0502)")
		}
		db, err := parseDelegateBody(co.Body.Content)
		if err != nil {
			return nil, identity.AgentID{}, err
		}
		// The issuer key signs its cert envelope: kid must be the issuer.
		if co.KID != db.Issuer {
			return nil, identity.AgentID{}, badDelegation("cert kid is not its issuer")
		}
		if at < db.NotBefore || at > db.NotAfter {
			return nil, identity.AgentID{}, badDelegation("cert validity window does not cover verification time")
		}
		certs[i] = db
	}
	// Linkage (leaf..root): the data envelope's kid is the leaf subject;
	// each cert's issuer is the next (parent) cert's subject.
	if o.KID != certs[0].Subject {
		return nil, identity.AgentID{}, badDelegation("envelope kid is not the leaf subject")
	}
	for i := 0; i+1 < len(certs); i++ {
		if certs[i].Issuer != certs[i+1].Subject {
			return nil, identity.AgentID{}, badDelegation("broken issuer/subject linkage")
		}
	}
	// Tighten-only attenuation (identity §4): each child scope must be a
	// subset of its parent's. certs[i+1] is the parent of certs[i].
	for i := 0; i+1 < len(certs); i++ {
		if !scopeSubset(certs[i], certs[i+1]) {
			return nil, identity.AgentID{}, badDelegation("child scope widens parent scope")
		}
	}
	// Max further-delegation depth: cert at index i has i certs delegated
	// below it; a declared depth must admit them.
	for i, c := range certs {
		if c.HasDepth && uint64(i) > c.MaxDepth {
			return nil, identity.AgentID{}, badDelegation("max delegation depth exceeded")
		}
	}
	return o, certs[len(certs)-1].Issuer, nil
}

// scopeSubset reports whether child's scope attenuates parent's. An absent
// scope means "all capabilities": any child is a subset of an absent parent
// scope; an absent child scope is a subset only of an absent parent scope.
func scopeSubset(child, parent DelegateBody) bool {
	if !parent.HasScope {
		return true
	}
	if !child.HasScope {
		return false // child claims all, parent is scoped: widening
	}
	for _, c := range child.Scope {
		ok := false
		for _, p := range parent.Scope {
			if capWithin(c, p) {
				ok = true
				break
			}
		}
		if !ok {
			return false
		}
	}
	return true
}

// capWithin reports whether CapURI c is equal to or a sub-capability of p.
// Capability paths are '/'-separated (core §3 CapURI), so
// "map:cap:inference/text" covers "map:cap:inference/text-…"? No: only an
// exact segment boundary extends a scope — "map:cap:a" covers "map:cap:a/b"
// but not "map:cap:ab".
func capWithin(c, p string) bool {
	return c == p || strings.HasPrefix(c, p+"/")
}

func parseDelegateBody(v cbor.Value) (DelegateBody, error) {
	var d DelegateBody
	m, ok := v.(cbor.Map)
	if !ok {
		return d, badDelegation("DelegateBody must be a map")
	}
	var have1, have2, have3, have4 bool
	for _, p := range m {
		k, ok := p.K.(cbor.Uint)
		if !ok {
			return d, badDelegation("DelegateBody: non-uint key")
		}
		switch uint64(k) {
		case 1, 2:
			b, ok := p.V.(cbor.Bytes)
			if !ok {
				return d, badDelegation("DelegateBody: issuer/subject must be AgentID bytes")
			}
			aid, err := identity.Parse(b)
			if err != nil {
				return d, badDelegation("DelegateBody: " + err.Error())
			}
			if uint64(k) == 1 {
				d.Issuer, have1 = aid, true
			} else {
				d.Subject, have2 = aid, true
			}
		case 3, 4:
			u, ok := p.V.(cbor.Uint)
			if !ok {
				return d, badDelegation("DelegateBody: timestamps must be uint")
			}
			if uint64(k) == 3 {
				d.NotBefore, have3 = uint64(u), true
			} else {
				d.NotAfter, have4 = uint64(u), true
			}
		case 5:
			arr, ok := p.V.(cbor.Array)
			if !ok || len(arr) == 0 {
				return d, badDelegation("DelegateBody: scope must be a non-empty array")
			}
			for _, e := range arr {
				t, ok := e.(cbor.Text)
				if !ok {
					return d, badDelegation("DelegateBody: scope entries must be CapURIs")
				}
				d.Scope = append(d.Scope, string(t))
			}
			d.HasScope = true
		case 6:
			// Zone scope: structurally accepted; zone attenuation is
			// enforced by zone-aware consumers, out of TASK-015 scope.
			if _, ok := p.V.(cbor.Array); !ok {
				return d, badDelegation("DelegateBody: zones must be an array")
			}
		case 7:
			u, ok := p.V.(cbor.Uint)
			if !ok {
				return d, badDelegation("DelegateBody: max-depth must be uint")
			}
			d.MaxDepth, d.HasDepth = uint64(u), true
		case 8:
			// Caveats: registered keys are 1 (task-pinning) and 2
			// (audience); an UNKNOWN caveat key invalidates the whole
			// chain (identity §4, fail-closed).
			cm, ok := p.V.(cbor.Map)
			if !ok {
				return d, badDelegation("DelegateBody: caveats must be a map")
			}
			for _, cp := range cm {
				ck, ok := cp.K.(cbor.Uint)
				if !ok || (uint64(ck) != 1 && uint64(ck) != 2) {
					return d, badDelegation("DelegateBody: unknown caveat key (fail-closed)")
				}
			}
		default:
			return d, badDelegation("DelegateBody: unknown key (fail-closed)")
		}
	}
	if !have1 || !have2 || !have3 || !have4 {
		return d, badDelegation("DelegateBody missing a required key (1,2,3,4)")
	}
	return d, nil
}
