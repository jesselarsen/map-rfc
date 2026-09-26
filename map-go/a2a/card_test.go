package a2a

import (
	"bytes"
	"crypto/ed25519"
	"errors"
	"testing"
)

func key(b byte) ed25519.PrivateKey { return ed25519.NewKeyFromSeed(bytes.Repeat([]byte{b}, 32)) }

func testParams() CardParams {
	return CardParams{
		Name: "Test provider", Description: "A MAP-capable test agent", Version: "1.0.0",
		JSONRPCURL:              "http://127.0.0.1:8080/a2a",
		Skills:                  []Skill{{"summarize", "Summarize", "Summarizes text", []string{"text"}}},
		SupportedZones:          []uint8{2, 3},
		CheckpointIntervalParts: 8,
		AcceptedVoucherRails:    []string{"pay:mpp-tempo"},
	}
}

func TestSignAndVerifyCard(t *testing.T) {
	kp := key(2)
	signed, _, err := SignCard(BuildCard(agentOf(kp), testParams()), kp)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := VerifyCard(signed, agentOf(kp)); err != nil {
		t.Fatal(err)
	}
	// Member order and whitespace do not matter (JCS): re-parse the JCS.
	j, _ := Canonicalize(signed)
	re, _ := ParseJSON(j)
	if _, err := VerifyCard(re, agentOf(kp)); err != nil {
		t.Fatal(err)
	}
}

func TestTamperWrongKeyAndExtensionMismatch(t *testing.T) {
	kp, other := key(2), key(3)
	signed, _, _ := SignCard(BuildCard(agentOf(kp), testParams()), kp)
	if _, err := VerifyCard(signed, agentOf(other)); !errors.Is(err, ErrNoMatchingSignature) {
		t.Fatalf("other identity: %v", err)
	}
	signed["description"] = "changed"
	if _, err := VerifyCard(signed, agentOf(kp)); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("tampered card: %v", err)
	}
	// Validly signed by kp, but the MAP extension names another agent (D-4).
	liar, _, _ := SignCard(BuildCard(agentOf(other), testParams()), kp)
	if _, err := VerifyCard(liar, agentOf(kp)); !errors.Is(err, ErrExtensionMismatch) {
		t.Fatalf("extension mismatch: %v", err)
	}
}

func mustJSON(t *testing.T, s string) any {
	t.Helper()
	v, err := ParseJSON([]byte(s))
	if err != nil {
		t.Fatal(err)
	}
	return v
}

// A2A §8.4.1 example: empty non-REQUIRED `extensions` is dropped, explicit
// optional booleans stay, REQUIRED empty `skills` stays.
func TestA2ASpecDefaultRemovalExample(t *testing.T) {
	card := mustJSON(t, `{"name":"Example Agent","description":"","capabilities":{"streaming":false,"pushNotifications":false,"extensions":[]},"skills":[]}`)
	got, err := SigningPayload(card)
	if err != nil {
		t.Fatal(err)
	}
	want := `{"capabilities":{"pushNotifications":false,"streaming":false},"description":"","name":"Example Agent","skills":[]}`
	if string(got) != want {
		t.Fatalf("got %s", got)
	}
}

func TestImplicitPresenceDefaultsAreDropped(t *testing.T) {
	card := mustJSON(t, `{"name":"n","supportedInterfaces":[{"url":"u","tenant":""}],"capabilities":{"extensions":[{"uri":"a","required":false},{"uri":"b","required":true,"description":""}]}}`)
	got, _ := SigningPayload(card)
	want := `{"capabilities":{"extensions":[{"uri":"a"},{"required":true,"uri":"b"}]},"name":"n","supportedInterfaces":[{"url":"u"}]}`
	if string(got) != want {
		t.Fatalf("got %s", got)
	}
}

// A2A-31: nothing a protobuf-based SDK would canonicalize differently.
func TestBuiltCardsStayInsideTheInteropProfile(t *testing.T) {
	kp := key(2)
	signed, _, _ := SignCard(BuildCard(agentOf(kp), testParams()), kp)
	var walk func(v any, inParams bool)
	walk = func(v any, inParams bool) {
		switch x := v.(type) {
		case string:
			for i := 0; i < len(x); i++ {
				if x[i] >= 0x80 {
					t.Fatalf("non-ASCII string %q", x)
				}
			}
			if x == "" {
				t.Fatal("empty string")
			}
		case []any:
			if len(x) == 0 {
				t.Fatal("empty array")
			}
			for _, e := range x {
				walk(e, inParams)
			}
		case map[string]any:
			if len(x) == 0 {
				t.Fatal("empty object")
			}
			if x["required"] == false {
				t.Fatal("required: false present")
			}
			for k, e := range x {
				walk(e, inParams || k == "params")
			}
		case bool, nil:
		default:
			if inParams {
				t.Fatalf("number inside a Struct: %v", x)
			}
		}
	}
	walk(signed, false)
}

func TestDescriptorSealOpenAndRejects(t *testing.T) {
	kp := key(2)
	const now = uint64(1_790_000_000_000)
	spec := DescriptorSpec{Seq: 3, Endpoints: []string{"http://127.0.0.1:8080/"}, Capabilities: []string{"map:cap:inference/text-generation"}, Zones: []uint8{2}}
	env, _, err := SealDescriptor(kp, BuildCard(agentOf(kp), testParams()), spec, [16]byte{7}, now, 3_600_000)
	if err != nil {
		t.Fatal(err)
	}
	d, card, err := OpenDescriptor(env, now+1000)
	if err != nil {
		t.Fatal(err)
	}
	if d.Seq != 3 || d.Agent != agentOf(kp) || MapExtensionAgent(card) != agentOf(kp).Multibase() {
		t.Fatal("descriptor fields wrong")
	}
	if _, _, err := OpenDescriptor(env, now+3_600_001); !errors.Is(err, ErrTTL) {
		t.Fatalf("expired descriptor: %v", err)
	}
	if _, _, err := OpenDescriptor(env, now-ClockSkewMs-1); !errors.Is(err, ErrTTL) {
		t.Fatalf("future descriptor: %v", err)
	}
	if _, _, err := SealDescriptor(kp, BuildCard(agentOf(kp), testParams()), spec, [16]byte{}, now, MaxValidityMs+1); !errors.Is(err, ErrTTL) {
		t.Fatalf("over-long ttl sealed: %v", err)
	}
	// A card whose MAP extension names someone else cannot be embedded
	// under this subject (D-4).
	env2, _, _ := SealDescriptor(kp, BuildCard(agentOf(key(9)), testParams()), spec, [16]byte{8}, now, 3_600_000)
	if _, _, err := OpenDescriptor(env2, now); !errors.Is(err, ErrExtensionMismatch) {
		t.Fatalf("card naming another agent: %v", err)
	}
}
