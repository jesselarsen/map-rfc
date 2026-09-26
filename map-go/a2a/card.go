package a2a

// A2A Agent Cards (A2A v1.0 §4.4, §8.4) with the MAP extension block
// (binding A2A-1..A2A-3) and JWS signatures, mirroring Rust map-a2a::card.
//
// Signing follows A2A §8.4.2 with the payload detached: the signature is
// over BASE64URL(protected) || "." || BASE64URL(payload), where payload is
// the JCS of the card without `signatures` after §8.4.1 default-value
// removal. alg is EdDSA (RFC 8037) with kid = the provider's AgentID in
// multibase form, so the card's signing key IS the AgentID key (layer D-4).
// Cards built here stay inside the A2A-31 interop profile.

import (
	"crypto/ed25519"
	"encoding/base64"
	"errors"
	"strconv"

	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// MapExtensionURI is the MAP extension URI (binding A2A-1). PLACEHOLDER:
// not registered.
const MapExtensionURI = "https://map-protocol.org/ext/accountability/v2"

// b64 is unpadded base64url, as JWS uses.
var b64 = base64.RawURLEncoding

// Card errors.
var (
	ErrNotObject           = errors.New("card: not a JSON object")
	ErrNoSignatures        = errors.New("card: no signatures")
	ErrNoMatchingSignature = errors.New("card: no EdDSA signature with the expected kid")
	ErrBadEncoding         = errors.New("card: bad base64url or protected header")
	ErrBadSignature        = errors.New("card: bad signature")
	ErrExtensionMismatch   = errors.New("card: MAP extension missing or names another agent")
)

// requiredRepeated are A2A repeated fields marked REQUIRED, kept even when
// empty (§8.4.1).
var requiredRepeated = map[string]bool{
	"supportedInterfaces": true, "defaultInputModes": true, "defaultOutputModes": true,
	"skills": true, "tags": true,
}

// Skill is one AgentSkill.
type Skill struct {
	ID, Name, Description string
	Tags                  []string
}

// CardParams are the inputs for a MAP provider's Agent Card.
type CardParams struct {
	Name, Description, Version string
	JSONRPCURL                 string
	Skills                     []Skill
	SupportedZones             []uint8
	CheckpointIntervalParts    uint64
	AcceptedVoucherRails       []string
}

func strs(xs []string) []any {
	out := make([]any, len(xs))
	for i, s := range xs {
		out[i] = s
	}
	return out
}

// BuildCard returns an unsigned Agent Card declaring the MAP extension.
// `required: false` is an implicit-presence default and so omitted (A2A-2);
// integers in params are decimal strings (A2A-3a).
func BuildCard(agent identity.AgentID, p CardParams) map[string]any {
	skills := make([]any, len(p.Skills))
	for i, s := range p.Skills {
		skills[i] = map[string]any{"id": s.ID, "name": s.Name, "description": s.Description, "tags": strs(s.Tags)}
	}
	zones := make([]string, len(p.SupportedZones))
	for i, z := range p.SupportedZones {
		zones[i] = strconv.Itoa(int(z))
	}
	return map[string]any{
		"name":        p.Name,
		"description": p.Description,
		"version":     p.Version,
		"supportedInterfaces": []any{map[string]any{
			"url": p.JSONRPCURL, "protocolBinding": "JSONRPC", "protocolVersion": "1.0",
		}},
		"capabilities": map[string]any{
			"streaming": false,
			"extensions": []any{map[string]any{
				"uri":         MapExtensionURI,
				"description": "MAP accountability layer: dual-signed receipts, countersigned checkpoints, zone labels, metering evidence.",
				"params": map[string]any{
					"agentId":                 agent.Multibase(),
					"supportedZones":          strs(zones),
					"checkpointIntervalParts": strconv.FormatUint(p.CheckpointIntervalParts, 10),
					"acceptedVoucherRails":    strs(p.AcceptedVoucherRails),
				},
			}},
		},
		"defaultInputModes":  []any{"text/plain"},
		"defaultOutputModes": []any{"text/plain"},
		"skills":             skills,
	}
}

// deepCopy copies the `any` JSON model.
func deepCopy(v any) any {
	switch x := v.(type) {
	case map[string]any:
		m := make(map[string]any, len(x))
		for k, e := range x {
			m[k] = deepCopy(e)
		}
		return m
	case []any:
		a := make([]any, len(x))
		for i, e := range x {
			a[i] = deepCopy(e)
		}
		return a
	default:
		return v
	}
}

func dropEmptyRepeated(v any) {
	switch x := v.(type) {
	case map[string]any:
		for k, e := range x {
			if a, ok := e.([]any); ok && len(a) == 0 && !requiredRepeated[k] {
				delete(x, k)
				continue
			}
			dropEmptyRepeated(e)
		}
	case []any:
		for _, e := range x {
			dropEmptyRepeated(e)
		}
	}
}

// dropImplicitDefaults removes implicit-presence scalars holding their
// default from every object in list.
func dropImplicitDefaults(list any, defaults map[string]any) {
	arr, ok := list.([]any)
	if !ok {
		return
	}
	for _, it := range arr {
		m, ok := it.(map[string]any)
		if !ok {
			continue
		}
		for k, d := range defaults {
			if v, present := m[k]; present && v == d {
				delete(m, k)
			}
		}
	}
}

// SigningPayload is the detached JWS payload: the JCS of the card without
// `signatures`, after §8.4.1 default-value removal. The implicit-presence
// scalars in the card schema are AgentExtension.{uri, description,
// required} and AgentInterface.tenant.
func SigningPayload(card any) ([]byte, error) {
	c, ok := deepCopy(card).(map[string]any)
	if !ok {
		return nil, ErrNotObject
	}
	delete(c, "signatures")
	if caps, ok := c["capabilities"].(map[string]any); ok {
		dropImplicitDefaults(caps["extensions"], map[string]any{"uri": "", "description": "", "required": false})
	}
	dropImplicitDefaults(c["supportedInterfaces"], map[string]any{"tenant": ""})
	dropEmptyRepeated(c)
	return Canonicalize(c)
}

func protectedHeader(agent identity.AgentID) (string, error) {
	h, err := Canonicalize(map[string]any{"alg": "EdDSA", "typ": "JOSE", "kid": agent.Multibase()})
	if err != nil {
		return "", err
	}
	return b64.EncodeToString(h), nil
}

// SignCard signs card with priv, replacing any existing signatures. It
// returns the signed card and the AgentCardSignature entry.
func SignCard(card any, priv ed25519.PrivateKey) (map[string]any, map[string]any, error) {
	payload, err := SigningPayload(card)
	if err != nil {
		return nil, nil, err
	}
	agent, err := identity.FromPub(priv.Public().(ed25519.PublicKey))
	if err != nil {
		return nil, nil, err
	}
	protected, err := protectedHeader(agent)
	if err != nil {
		return nil, nil, err
	}
	sig := ed25519.Sign(priv, []byte(protected+"."+b64.EncodeToString(payload)))
	entry := map[string]any{"protected": protected, "signature": b64.EncodeToString(sig)}
	signed := deepCopy(card).(map[string]any)
	signed["signatures"] = []any{entry}
	return signed, entry, nil
}

// VerifyCard checks that card carries a valid EdDSA signature by expected
// and that its MAP extension names the same AgentID. It returns the
// verifying signature entry.
func VerifyCard(card any, expected identity.AgentID) (map[string]any, error) {
	c, ok := card.(map[string]any)
	if !ok {
		return nil, ErrNotObject
	}
	sigs, _ := c["signatures"].([]any)
	if len(sigs) == 0 {
		return nil, ErrNoSignatures
	}
	payload, err := SigningPayload(card)
	if err != nil {
		return nil, err
	}
	payloadB64 := b64.EncodeToString(payload)
	wantKid := expected.Multibase()
	for _, e := range sigs {
		entry, ok := e.(map[string]any)
		if !ok {
			continue
		}
		protected, ok1 := entry["protected"].(string)
		signature, ok2 := entry["signature"].(string)
		if !ok1 || !ok2 {
			continue
		}
		hb, err := b64.DecodeString(protected)
		if err != nil {
			return nil, ErrBadEncoding
		}
		hv, err := ParseJSON(hb)
		if err != nil {
			return nil, ErrBadEncoding
		}
		h, _ := hv.(map[string]any)
		if h["alg"] != "EdDSA" || h["kid"] != wantKid {
			continue
		}
		sig, err := b64.DecodeString(signature)
		if err != nil {
			return nil, ErrBadEncoding
		}
		if identity.VerifyStrict(expected.Pub(), []byte(protected+"."+payloadB64), sig) != nil {
			return nil, ErrBadSignature
		}
		if MapExtensionAgent(card) != wantKid {
			return nil, ErrExtensionMismatch
		}
		return entry, nil
	}
	return nil, ErrNoMatchingSignature
}

// MapExtensionParams returns the MAP extension's params, if declared.
func MapExtensionParams(card any) map[string]any {
	c, _ := card.(map[string]any)
	caps, _ := c["capabilities"].(map[string]any)
	exts, _ := caps["extensions"].([]any)
	for _, e := range exts {
		m, _ := e.(map[string]any)
		if m["uri"] == MapExtensionURI {
			p, _ := m["params"].(map[string]any)
			return p
		}
	}
	return nil
}

// MapExtensionAgent is params.agentId, or "".
func MapExtensionAgent(card any) string {
	s, _ := MapExtensionParams(card)["agentId"].(string)
	return s
}
