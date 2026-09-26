package a2a

// End-to-end accountability flow over A2A objects, in memory (port of
// map-a2a tests/flow_e2e.rs): provider A fails mid-task, the requester
// resumes on provider B from A's last countersigned checkpoint, and the task
// finishes with a dual-signed receipt naming what it resumed from.

import (
	"bytes"
	"errors"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/meter"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
	"github.com/jesselarsen/map-rfc/map-go/transcript"
)

const flowCap = "map:cap:inference/text-generation"
const t0 = uint64(1_790_000_000_000)

func part(text string) any { return map[string]any{"text": text, "mediaType": "text/plain"} }

func usage(n uint64) core.Usage {
	return core.Usage{TokensIn: 20, TokensOut: 5 * n, TTFTMs: 150, WallMs: 100 * n}
}

func cfg(zone uint8) ProviderConfig {
	return ProviderConfig{Capability: flowCap, Zone: zone, CheckpointEvery: 2}
}

func stream(t *testing.T, prov *ProviderTask, req *RequesterTask, texts []string) {
	t.Helper()
	for i, s := range texts {
		n := uint64(i + 1)
		p := part(s)
		proposal, err := prov.Deliver(p, usage(n), t0+n*100)
		if err != nil {
			t.Fatal(err)
		}
		if err := req.OnPart(p); err != nil {
			t.Fatal(err)
		}
		if proposal != nil {
			cs, err := req.OnCheckpoint(proposal, usage(n))
			if err != nil {
				t.Fatal(err)
			}
			if err := prov.OnCountersigned(cs); err != nil {
				t.Fatal(err)
			}
		}
	}
}

var nonceA, nonceB = [16]byte{7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7}, [16]byte{8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8}

func TestFailoverToASecondProvider(t *testing.T) {
	reqK, aK, bK := key(1), key(2), key(3)
	provA := StartProvider(aK, agentOf(reqK), nonceA, "a2a-task-A", cfg(2))
	reqA := StartRequester(reqK, agentOf(aK), nonceA, "a2a-task-A", flowCap, 2)
	if provA.TaskID() != reqA.TaskID() {
		t.Fatal("TaskIDs differ")
	}
	stream(t, provA, reqA, []string{"The ", "quick ", "brown ", "fox ", "jumps"})
	// C-5: A is owed its last countersigned checkpoint (4 Parts), not 5.
	if owed, ok := provA.OwedUsage(); !ok || owed != usage(4) {
		t.Fatalf("owed %v", owed)
	}
	reqB, pkg, err := reqA.ResumeOn(agentOf(bK), nonceB, "a2a-task-B")
	if err != nil {
		t.Fatal(err)
	}
	if len(pkg.PrefixParts) != 4 {
		t.Fatalf("prefix %d parts", len(pkg.PrefixParts))
	}
	provB, err := ResumeProvider(bK, agentOf(reqK), nonceB, "a2a-task-B", cfg(2), pkg.Checkpoint, pkg.PrefixParts)
	if err != nil {
		t.Fatal(err)
	}
	if provB.NextSeq() != 4 || provB.TaskID() != reqB.TaskID() || provB.TaskID() == provA.TaskID() {
		t.Fatal("successor state wrong (A2A-24/25)")
	}
	voucher := meter.DigestMPPTempo(bytes.Repeat([]byte{0xc4}, 32), meter.Amount{Exponent: -6, Mantissa: 5200}, "ref-9", "tempo")
	provB.SetVoucherDigest(voucher)
	stream(t, provB, reqB, []string{"over ", "the lazy dog."})
	proposal := provB.ProposeReceipt(receipt.OutcomeOK, usage(2), t0, t0+900)
	cs, err := reqB.OnReceipt(proposal, usage(2), t0+50, t0+950)
	if err != nil {
		t.Fatal(err)
	}
	r, err := provB.OnCountersignedReceipt(proposal, cs)
	if err != nil {
		t.Fatal(err)
	}
	v, err := r.Verify()
	if err != nil || !v.Dual {
		t.Fatalf("receipt not dual: %v", err)
	}
	cp, _ := transcript.DecodeArtifact(pkg.Checkpoint)
	root := reqB.TranscriptRoot()
	if !bytes.Equal(v.Core.ResumedFrom, cp.Digest()) || !bytes.Equal(v.Core.PaymentRef, voucher) ||
		*v.Core.TranscriptRoot != root || *v.Core.Zone != 2 {
		t.Fatal("receipt fields wrong")
	}
}

func TestRequesterRefusesInflatedUsageAndUnseenContent(t *testing.T) {
	reqK, aK := key(1), key(2)
	prov := StartProvider(aK, agentOf(reqK), nonceA, "t", cfg(2))
	req := StartRequester(reqK, agentOf(aK), nonceA, "t", flowCap, 2)
	_ = req.OnPart(part("one"))
	_, _ = prov.Deliver(part("one"), usage(1), t0)
	inflated := usage(2)
	inflated.TokensOut = 30
	p, _ := prov.Deliver(part("two"), inflated, t0+1)
	_ = req.OnPart(part("two"))
	if _, err := req.OnCheckpoint(p, usage(2)); !errors.Is(err, ErrOutsideTolerance) {
		t.Fatalf("inflated usage: %v", err)
	}
	prov2 := StartProvider(aK, agentOf(reqK), nonceA, "t", cfg(2))
	req2 := StartRequester(reqK, agentOf(aK), nonceA, "t", flowCap, 2)
	_, _ = prov2.Deliver(part("one"), usage(1), t0)
	p2, _ := prov2.Deliver(part("SECRETLY DIFFERENT"), usage(2), t0+1)
	_ = req2.OnPart(part("one"))
	_ = req2.OnPart(part("two"))
	if _, err := req2.OnCheckpoint(p2, usage(2)); !errors.Is(err, transcript.ErrRootMismatch) {
		t.Fatalf("unseen content: %v", err)
	}
}

func TestSuccessorCannotWidenZoneOrAcceptTamperedPrefix(t *testing.T) {
	reqK, aK, bK := key(1), key(2), key(3)
	provA := StartProvider(aK, agentOf(reqK), nonceA, "t", cfg(1))
	reqA := StartRequester(reqK, agentOf(aK), nonceA, "t", flowCap, 1)
	stream(t, provA, reqA, []string{"a", "b"})
	_, pkg, err := reqA.ResumeOn(agentOf(bK), nonceB, "t2")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := ResumeProvider(bK, agentOf(reqK), nonceB, "t2", cfg(2), pkg.Checkpoint, pkg.PrefixParts); !errors.Is(err, ErrZoneWidened) {
		t.Fatalf("zone widening: %v", err)
	}
	tampered := append([]any(nil), pkg.PrefixParts...)
	tampered[1] = part("B")
	if _, err := ResumeProvider(bK, agentOf(reqK), nonceB, "t2", cfg(1), pkg.Checkpoint, tampered); !errors.Is(err, transcript.ErrRootMismatch) {
		t.Fatalf("tampered prefix: %v", err)
	}
	if _, err := ResumeProvider(bK, agentOf(key(9)), nonceB, "t2", cfg(1), pkg.Checkpoint, pkg.PrefixParts); !errors.Is(err, ErrMismatch) {
		t.Fatalf("stranger requester: %v", err)
	}
}

func TestReceiptWithADifferentTranscriptIsRefused(t *testing.T) {
	reqK, aK := key(1), key(2)
	prov := StartProvider(aK, agentOf(reqK), nonceA, "t", cfg(2))
	req := StartRequester(reqK, agentOf(aK), nonceA, "t", flowCap, 2)
	stream(t, prov, req, []string{"x", "y"})
	_ = req.OnPart(part("z-only-the-requester-saw"))
	proposal := prov.ProposeReceipt(receipt.OutcomeOK, usage(2), t0, t0+200)
	if _, err := req.OnReceipt(proposal, usage(2), t0, t0+200); !errors.Is(err, ErrMismatch) {
		t.Fatalf("transcript mismatch: %v", err)
	}
}
