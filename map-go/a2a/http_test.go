package a2a

// In-process HTTP failover (the P4 demo's scenario, Go <-> Go): provider A
// "crashes" after producing 10 Parts, the requester resumes on B, and the
// receipt, transcript and voucher watermarks line up.

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"log"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/meter"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
)

type crashed struct{}

func startProvider(t *testing.T, k ed25519.PrivateKey, failAfter uint64) *httptest.Server {
	t.Helper()
	p := &Provider{tasks: map[string]*taskState{}, nextID: 1}
	srv := httptest.NewServer(p)
	params := testParams()
	params.JSONRPCURL = srv.URL + "/a2a"
	params.SupportedZones = []uint8{1, 2}
	card, _, err := SignCard(BuildCard(agentOf(k), params), k)
	if err != nil {
		t.Fatal(err)
	}
	p.cfg = ServerConfig{
		Key: k, Card: card, Capability: flowCap, SupportedZones: []uint8{1, 2},
		CheckpointEvery: 4, FailAfterParts: failAfter,
		Workload: func(prompt string) []string { return Generate(prompt, 24) },
		Crash:    func() { panic(http.ErrAbortHandler) },
	}
	return srv
}

func TestHTTPFailoverGoToGo(t *testing.T) {
	log.SetOutput(bytes.NewBuffer(nil)) // quiet provider logs
	reqK, aK, bK := key(1), key(2), key(3)
	a, b := startProvider(t, aK, 10), startProvider(t, bK, 0)
	defer a.Close()
	defer b.Close()

	rates := []meter.Rate{{Unit: meter.UnitTokenIn, Price: usdc(2)}, {Unit: meter.UnitTokenOut, Price: usdc(10)}}
	paid := map[identity.AgentID]core.Usage{}
	pay := func(p identity.AgentID, u core.Usage) []byte {
		amt, err := meter.Billed(1, rates, u)
		if err != nil {
			t.Fatal(err)
		}
		paid[p] = u
		ch := sha256.Sum256(p[:])
		return meter.DigestMPPTempo(ch[:], amt.Amount, "map-demo", "tempo")
	}
	const prompt = "Write a short status report"

	epA, err := Discover(a.URL)
	if err != nil {
		t.Fatal(err)
	}
	outA := RunTask(epA, reqK, prompt, flowCap, 2, nil, pay)
	if outA.Err == nil || outA.Receipt != nil {
		t.Fatalf("provider A should have died mid-task: %v", outA.Err)
	}
	if outA.Acked == nil || outA.Acked.TokensOut != 8 {
		t.Fatalf("A acked %+v, want 8 Parts (two checkpoints)", outA.Acked)
	}
	epB, err := Discover(b.URL)
	if err != nil {
		t.Fatal(err)
	}
	outB := RunTask(epB, reqK, prompt, flowCap, 2, &Resume{From: outA.Task, FromA2ATaskID: outA.A2ATaskID}, pay)
	if outB.Err != nil {
		t.Fatal(outB.Err)
	}
	r, err := receipt.Decode(outB.Receipt)
	if err != nil {
		t.Fatal(err)
	}
	v, err := r.Verify()
	if err != nil || !v.Dual || v.Core.ResumedFrom == nil || v.Core.PaymentRef == nil {
		t.Fatalf("receipt: dual %v, resumed %x, voucher %x, err %v", v.Dual, v.Core.ResumedFrom, v.Core.PaymentRef, err)
	}
	if *v.Core.TranscriptRoot != outB.Task.TranscriptRoot() || outB.Task.PartsReceived() != 24 {
		t.Fatal("receipt transcript does not cover all 24 Parts")
	}
	if paid[agentOf(aK)].TokensOut+paid[agentOf(bK)].TokensOut != 24 {
		t.Fatalf("voucher watermarks %v do not sum to the 24 Parts delivered", paid)
	}
}

func TestHTTPPlainClientGetsAPlainTask(t *testing.T) {
	log.SetOutput(bytes.NewBuffer(nil))
	b := startProvider(t, key(3), 0)
	defer b.Close()
	ep, err := Discover(b.URL)
	if err != nil {
		t.Fatal(err)
	}
	msg := map[string]any{"messageId": "plain-1", "role": "ROLE_USER", "parts": []any{map[string]any{"text": "hi"}}}
	res, err := RPC(ep.RPCURL, "SendMessage", map[string]any{"message": msg}, false)
	if err != nil {
		t.Fatal(err)
	}
	task, _ := res["task"].(map[string]any)
	status, _ := task["status"].(map[string]any)
	if status["state"] != "TASK_STATE_COMPLETED" || task["metadata"] != nil {
		t.Fatalf("plain client got %v (metadata %v)", status["state"], task["metadata"])
	}
}

func TestHTTPZoneViolationIsABindingError(t *testing.T) {
	log.SetOutput(bytes.NewBuffer(nil))
	b := startProvider(t, key(3), 0)
	defer b.Close()
	ep, _ := Discover(b.URL)
	out := RunTask(ep, key(1), "hi", flowCap, 3, nil, func(identity.AgentID, core.Usage) []byte { return nil })
	if out.Err == nil || !bytes.Contains([]byte(out.Err.Error()), []byte(`"code":3`)) {
		t.Fatalf("zone 3 must be refused with binding code 3: %v", out.Err)
	}
}

func usdc(m uint64) meter.AssetAmount {
	return meter.AssetAmount{Asset: meter.ERC20("eip155:8453", "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"), Amount: meter.Amount{Exponent: -6, Mantissa: m}}
}
