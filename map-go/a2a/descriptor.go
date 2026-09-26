package a2a

// v2 descriptors that embed a signed A2A Agent Card (layer §11.2, D-4/D-5),
// mirroring map-a2a::descriptor. Opening one checks everything a querier
// must check before shortlisting: the envelope signature and signer ==
// subject; D-5 validity (ttl in (0, 24 h], timestamp not in the future
// beyond 120 s, not expired); the embedded card is canonical JSON and its JWS
// verifies under the subject's own key with the MAP extension naming the
// subject; and key 7 is exactly the verifying AgentCardSignature.

import (
	"bytes"
	"crypto/ed25519"
	"errors"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/discovery"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// Descriptor validity rules.
const (
	MaxValidityMs = 24 * 3_600_000
	ClockSkewMs   = 120_000
)

// Descriptor errors.
var (
	ErrWrongMsgType          = errors.New("descriptor: wrong msg-type")
	ErrSignerNotSubject      = errors.New("descriptor: signer is not the subject")
	ErrTTL                   = errors.New("descriptor: ttl")
	ErrCardNotCanonical      = errors.New("descriptor: embedded card is not canonical JSON")
	ErrCardSignatureMismatch = errors.New("descriptor: key 7 is not the verifying card signature")
)

// DescriptorSpec is everything a provider publishes besides its keys.
type DescriptorSpec struct {
	Seq          uint64
	Endpoints    []string
	Capabilities []string
	Zones        []uint8
}

// SealDescriptor signs unsignedCard, embeds it and seals the descriptor
// envelope. It returns the envelope and the signed card (to serve at
// /.well-known/agent-card.json).
func SealDescriptor(priv ed25519.PrivateKey, unsignedCard any, spec DescriptorSpec, msgID [16]byte, nowMs, ttlMs uint64) ([]byte, map[string]any, error) {
	if ttlMs == 0 || ttlMs > MaxValidityMs {
		return nil, nil, fmt.Errorf("%w: out of range (0, 24h]", ErrTTL)
	}
	signed, entry, err := SignCard(unsignedCard, priv)
	if err != nil {
		return nil, nil, err
	}
	cardJCS, err := Canonicalize(signed)
	if err != nil {
		return nil, nil, err
	}
	sigJCS, err := Canonicalize(entry)
	if err != nil {
		return nil, nil, err
	}
	agent, err := identity.FromPub(priv.Public().(ed25519.PublicKey))
	if err != nil {
		return nil, nil, err
	}
	ttl := ttlMs
	d := discovery.DescriptorV2{
		Agent: agent, Seq: spec.Seq, Endpoints: spec.Endpoints, Capabilities: spec.Capabilities,
		Zones: spec.Zones, CardJCS: cardJCS, CardSignature: sigJCS, ValidityMs: &ttl,
	}
	env, err := envelope.SealBody(priv, envelope.Body{
		Version: 0x0100, MsgType: discovery.MsgDescriptorV2, MsgID: msgID[:],
		Timestamp: nowMs, TTLMs: &ttl, Content: d.ToValue(),
	}, nil)
	if err != nil {
		return nil, nil, err
	}
	return env, signed, nil
}

// OpenDescriptor opens and fully validates a v2 descriptor envelope.
func OpenDescriptor(env []byte, nowMs uint64) (discovery.DescriptorV2, map[string]any, error) {
	var zero discovery.DescriptorV2
	o, err := envelope.Open(env, nil)
	if err != nil {
		return zero, nil, err
	}
	if o.Body.MsgType != discovery.MsgDescriptorV2 {
		return zero, nil, fmt.Errorf("%w: %#x", ErrWrongMsgType, o.Body.MsgType)
	}
	d, err := discovery.DescriptorV2FromValue(o.Body.Content)
	if err != nil {
		return zero, nil, err
	}
	if d.Agent != o.KID {
		return zero, nil, ErrSignerNotSubject
	}
	if o.Body.TTLMs == nil {
		return zero, nil, fmt.Errorf("%w: missing", ErrTTL)
	}
	ttl := *o.Body.TTLMs
	if ttl == 0 || ttl > MaxValidityMs {
		return zero, nil, fmt.Errorf("%w: out of range (0, 24h]", ErrTTL)
	}
	if o.Body.Timestamp > nowMs+ClockSkewMs {
		return zero, nil, fmt.Errorf("%w: timestamp in the future", ErrTTL)
	}
	if nowMs > o.Body.Timestamp+ttl {
		return zero, nil, fmt.Errorf("%w: expired", ErrTTL)
	}
	cv, err := ParseJSON(d.CardJCS)
	if err != nil {
		return zero, nil, ErrCardNotCanonical
	}
	canon, err := Canonicalize(cv)
	if err != nil || !bytes.Equal(canon, d.CardJCS) {
		return zero, nil, ErrCardNotCanonical
	}
	entry, err := VerifyCard(cv, d.Agent)
	if err != nil {
		return zero, nil, err
	}
	sigJCS, err := Canonicalize(entry)
	if err != nil || !bytes.Equal(sigJCS, d.CardSignature) {
		return zero, nil, ErrCardSignatureMismatch
	}
	card, _ := cv.(map[string]any)
	return d, card, nil
}
