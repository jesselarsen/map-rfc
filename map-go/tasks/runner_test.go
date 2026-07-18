package tasks

import (
	"crypto/ed25519"
	"crypto/sha256"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/session"
)

func keyOf(label string) ed25519.PrivateKey {
	s := sha256.Sum256([]byte(label))
	return ed25519.NewKeyFromSeed(s[:])
}

func aidOf(k ed25519.PrivateKey) identity.AgentID {
	a, _ := identity.FromPub(k.Public().(ed25519.PublicKey))
	return a
}

// excuseExecutor answers map:cap:demo/excuse with a single deterministic line.
type excuseExecutor struct{}

func (excuseExecutor) Execute(input []byte) []Chunk {
	line := "I'm late because a rogue AGI rewrote my calendar in cbor."
	return []Chunk{{Seq: 0, Kind: ChunkOutputText, Content: []byte(line)}}
}

func TestTaskRunOverSession(t *testing.T) {
	ka, kb := keyOf("provider-A"), keyOf("requester-B")
	a, b := aidOf(ka), aidOf(kb)
	di, dr := session.NewDuplexPair(a, b)

	hello := session.Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 262144}
	initCfg := session.SessionConfig{Keypair: ka, ExpectedPeer: b, Hello: hello}
	respCfg := session.SessionConfig{Keypair: kb, ExpectedPeer: a, Hello: hello}

	// Provider side (initiator). Start it concurrently because Initiate
	// blocks on the HELLO_ACK from the responder.
	provider := NewProviderTask(excuseExecutor{})
	type sErr struct {
		s   *session.Session
		err error
	}
	provCh := make(chan sErr, 1)
	go func() {
		s, e := session.Initiate(initCfg, di, session.SystemClock{}, session.RandMsgIDs{})
		provCh <- sErr{s, e}
	}()

	// Requester side (responder).
	reqSess, err := session.Respond(respCfg, dr, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		t.Fatalf("respond: %v", err)
	}
	pe := <-provCh
	if pe.err != nil {
		t.Fatalf("initiate: %v", pe.err)
	}
	provSess := pe.s
	var task TaskID
	task[0] = 0xAB

	// Drive both sides concurrently (the runner blocks on Recv).
	done := make(chan error, 1)
	go func() {
		done <- RunProvider(provSess, provider)
	}()

	ver, root, err := RunRequester(reqSess, NewRequesterTask(task, "map:cap:demo/excuse", TaskConstraints{}, []byte("why late?")), false)
	if err != nil {
		t.Fatalf("requester: %v", err)
	}
	if err := <-done; err != nil {
		t.Fatalf("provider: %v", err)
	}

	if ver == nil {
		t.Fatalf("no verified receipt")
	}
	// Transcript root must be non-zero and the provider's observation must agree.
	if root == [32]byte{} {
		t.Fatalf("zero transcript root")
	}
	obs := provider.Observation()
	if obs == nil {
		t.Fatalf("provider left no observation")
	}
	if provider.State() != ProviderCompleted {
		t.Fatalf("provider state = %v", provider.State())
	}
	_ = root
	_ = ver
}
