package transcript

import (
	"bytes"
	"crypto/ed25519"
	"errors"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

func seedKey(b byte) ed25519.PrivateKey {
	seed := bytes.Repeat([]byte{b}, 32)
	return ed25519.NewKeyFromSeed(seed)
}

func aidOf(k ed25519.PrivateKey) identity.AgentID {
	a, err := identity.FromPub(k.Public().(ed25519.PublicKey))
	if err != nil {
		panic(err)
	}
	return a
}

func textChunks(texts ...string) []Chunk {
	out := make([]Chunk, len(texts))
	for i, s := range texts {
		out[i] = Chunk{Seq: uint64(i), Kind: ChunkOutputText, Content: []byte(s)}
	}
	return out
}

func testCore(req, prov ed25519.PrivateKey, prefix []Chunk, u core.Usage) CheckpointCore {
	b := NewTranscript()
	for _, c := range prefix {
		b.Push(c)
	}
	var task core.TaskID
	for i := range task {
		task[i] = 9
	}
	return CheckpointCore{
		Task: task, Requester: aidOf(req), Provider: aidOf(prov),
		ThroughSeq: uint64(len(prefix) - 1), Root: b.Root(), Usage: u,
		Zone: 2, TimestampMs: 1_790_000_000_000,
	}
}

func TestProposeCountersignResume(t *testing.T) {
	req, prov := seedKey(1), seedKey(2)
	prefix := textChunks("a", "b", "c")
	c := testCore(req, prov, prefix, core.Usage{TokensIn: 5, TokensOut: 3, TTFTMs: 1, WallMs: 9})
	art := Propose(prov, c)
	if _, err := art.VerifyResumptionPoint(); !errors.Is(err, ErrNotDualSigned) {
		t.Fatalf("provider-only artifact must not be a resumption point: %v", err)
	}
	if _, err := VerifyPrefix(c, prefix); err != nil {
		t.Fatal(err)
	}
	art.Countersign(req)
	wire, err := DecodeArtifact(art.Encode())
	if err != nil {
		t.Fatal(err)
	}
	r, err := Resume(wire, prefix)
	if err != nil {
		t.Fatal(err)
	}
	if r.NextSeq != 3 || !bytes.Equal(r.ResumedFrom, art.Digest()) {
		t.Fatalf("resume state wrong: next %d", r.NextSeq)
	}
}

func TestUsageIsCoveredByTheCountersignature(t *testing.T) {
	req, prov := seedKey(1), seedKey(2)
	prefix := textChunks("a")
	c := testCore(req, prov, prefix, core.Usage{TokensIn: 100, TokensOut: 80, TTFTMs: 1, WallMs: 1})
	art := Propose(prov, c)
	art.Countersign(req)
	edited := c
	edited.Usage.TokensIn = 1
	forged := CheckpointArtifact{CoreBytes: edited.Encode(), SigReq: art.SigReq, SigProv: art.SigProv}
	if _, err := forged.Verify(); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("edited usage must break verification: %v", err)
	}
}

func TestRootMismatchAndBadPrefix(t *testing.T) {
	req, prov := seedKey(1), seedKey(2)
	prefix := textChunks("a", "b")
	art := Propose(prov, testCore(req, prov, prefix, core.Usage{}))
	art.Countersign(req)
	if _, err := Resume(art, textChunks("a", "X")); !errors.Is(err, ErrRootMismatch) {
		t.Fatalf("want root mismatch, got %v", err)
	}
	if _, err := Resume(art, textChunks("a")); !errors.Is(err, ErrBadPrefix) {
		t.Fatalf("want bad prefix, got %v", err)
	}
	gap := textChunks("a", "b")
	gap[1].Seq = 5
	if _, err := Resume(art, gap); !errors.Is(err, ErrBadPrefix) {
		t.Fatalf("want bad prefix for a gap, got %v", err)
	}
}

func TestSelfDealingAndMonotonicity(t *testing.T) {
	req, prov := seedKey(1), seedKey(2)
	prefix := textChunks("a", "b")
	solo := Propose(prov, testCore(prov, prov, prefix, core.Usage{}))
	solo.Countersign(prov)
	if _, err := solo.Verify(); !errors.Is(err, ErrSelfDealing) {
		t.Fatalf("want self-dealing, got %v", err)
	}
	a := testCore(req, prov, prefix, core.Usage{TokensIn: 10, TokensOut: 10, TTFTMs: 1, WallMs: 10})
	b := a
	b.ThroughSeq = 3
	b.Usage.TokensOut = 12
	if err := CheckSuccessor(a, b); err != nil {
		t.Fatal(err)
	}
	stale := b
	stale.ThroughSeq = a.ThroughSeq
	if CheckSuccessor(a, stale) == nil {
		t.Fatal("stale through-seq accepted")
	}
	regress := b
	regress.Usage.TokensIn = 9
	if CheckSuccessor(a, regress) == nil {
		t.Fatal("usage regression accepted")
	}
}
