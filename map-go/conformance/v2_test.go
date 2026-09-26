package conformance

import (
	"bufio"
	"bytes"
	"encoding/hex"
	"errors"
	"os"
	"regexp"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/a2a"
	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
	"github.com/jesselarsen/map-rfc/map-go/transcript"
	"github.com/jesselarsen/map-rfc/map-go/trust"
)

// v2 vectors: TV-021 (CheckpointArtifact), NV-013/NV-014 (checkpoint
// rejects) and TV-023 (receipt admissibility). Rust map-conformance checks
// the same frozen bytes.

var tv021Prefix = []string{"Hello, ", "accountable ", "MAP!"}

func tv021Chunks() []transcript.Chunk {
	out := make([]transcript.Chunk, len(tv021Prefix))
	for i, s := range tv021Prefix {
		out[i] = transcript.Chunk{Seq: uint64(i), Kind: transcript.ChunkOutputText, Content: []byte(s)}
	}
	return out
}

func mustAID(t *testing.T, b []byte) identity.AgentID {
	t.Helper()
	a, err := identity.Parse(b)
	if err != nil {
		t.Fatal(err)
	}
	return a
}

// TV-021 produce: the transcript root, the core bytes and the dual-signed
// artifact reproduce byte-for-byte from the seeds.
func TestTV021Produce(t *testing.T) {
	vf := loadVF(t)
	blobs := vf.Sections["TV-021"]
	if len(blobs) != 3 {
		t.Fatalf("TV-021: want 3 blobs (root, core, artifact), got %d", len(blobs))
	}
	b := transcript.NewTranscript()
	for _, c := range tv021Chunks() {
		b.Push(c)
	}
	root := b.Root()
	if !bytes.Equal(root[:], blobs[0]) {
		t.Fatal("TV-021 root differs")
	}
	task, _ := core.TaskIDFromBytes(TaskID)
	cc := transcript.CheckpointCore{
		Task: task, Requester: mustAID(t, aid(KeyA)), Provider: mustAID(t, aid(KeyB)),
		ThroughSeq: 2, Root: root,
		Usage: core.Usage{TokensIn: 10, TokensOut: 3, TTFTMs: 120, WallMs: 900},
		Zone:  2, TimestampMs: TS + 900,
	}
	if d := firstDiff(cc.Encode(), blobs[1]); d != -1 {
		t.Fatalf("TV-021 core: first diff at %d", d)
	}
	art := transcript.Propose(KeyB, cc)
	art.Countersign(KeyA)
	if d := firstDiff(art.Encode(), blobs[2]); d != -1 {
		t.Fatalf("TV-021 artifact: first diff at %d", d)
	}
}

// TV-021 consume: the frozen artifact is a valid resumption point and
// resumes against its prefix at seq 3.
func TestTV021Consume(t *testing.T) {
	env, err := loadVF(t).Envelope("TV-021")
	if err != nil {
		t.Fatal(err)
	}
	art, err := transcript.DecodeArtifact(env)
	if err != nil {
		t.Fatal(err)
	}
	r, err := transcript.Resume(art, tv021Chunks())
	if err != nil {
		t.Fatalf("TV-021 resume: %v", err)
	}
	if r.NextSeq != 3 || r.Core.Zone != 2 || r.Core.Usage.TokensOut != 3 {
		t.Fatalf("TV-021 resume state: %+v", r.Core)
	}
}

// NV-013: corrupted requester countersignature -> not a resumption point.
func TestNV013BadCountersignature(t *testing.T) {
	env, err := loadVF(t).Envelope("NV-013")
	if err != nil {
		t.Fatal(err)
	}
	art, err := transcript.DecodeArtifact(env)
	if err != nil {
		t.Fatalf("NV-013 must decode (signature reject, not structure): %v", err)
	}
	if _, err := art.VerifyResumptionPoint(); !errors.Is(err, transcript.ErrBadSignature) {
		t.Fatalf("NV-013: want bad signature, got %v", err)
	}
}

// NV-014: dual-signed, but key 5 does not match the prefix -> reject resume.
func TestNV014RootMismatch(t *testing.T) {
	env, err := loadVF(t).Envelope("NV-014")
	if err != nil {
		t.Fatal(err)
	}
	art, err := transcript.DecodeArtifact(env)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := art.VerifyResumptionPoint(); err != nil {
		t.Fatalf("NV-014 signatures must verify: %v", err)
	}
	if _, err := transcript.Resume(art, tv021Chunks()); !errors.Is(err, transcript.ErrRootMismatch) {
		t.Fatalf("NV-014: want root mismatch, got %v", err)
	}
}

const tv023NowMs = 1_790_000_000_000

type tv023Case struct {
	verdict string
	receipt []byte
}

// loadTV023 reads the "case NN <verdict> -- ..." lines and the hex receipt
// under each from the vector file.
func loadTV023(t *testing.T) []tv023Case {
	t.Helper()
	f, err := os.Open(vectorsPath)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	caseLine := regexp.MustCompile(`^case (\d\d) (\S+) -- `)
	var out []tv023Case
	in, pending := false, ""
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 64*1024), 64*1024)
	for sc.Scan() {
		line := sc.Text()
		switch {
		case len(line) > 3 && line[:3] == "## ":
			in = len(line) > 9 && line[:9] == "## TV-023"
		case !in:
		case caseLine.MatchString(line):
			pending = caseLine.FindStringSubmatch(line)[2]
		case pending != "":
			b, err := hex.DecodeString(string(bytes.TrimSpace([]byte(line))))
			if err != nil {
				t.Fatalf("TV-023 case hex: %v", err)
			}
			out = append(out, tv023Case{pending, b})
			pending = ""
		}
	}
	if len(out) != 13 {
		t.Fatalf("TV-023: want 13 cases, got %d", len(out))
	}
	return out
}

func verdict(err error) string {
	switch {
	case err == nil:
		return "count"
	case errors.Is(err, trust.ErrSelfDealing):
		return "self-dealing"
	case errors.Is(err, trust.ErrFutureDated):
		return "future-dated"
	case errors.Is(err, trust.ErrNotAdmissible):
		return "not-admissible"
	case errors.Is(err, trust.ErrCancelled):
		return "cancelled"
	case errors.Is(err, trust.ErrZeroWeight):
		return "zero-weight"
	}
	return "error: " + err.Error()
}

// TV-023: every receipt reaches the pinned admissibility verdict.
func TestTV023Admissibility(t *testing.T) {
	for i, c := range loadTV023(t) {
		r, err := receipt.Decode(c.receipt)
		if err != nil {
			t.Fatalf("case %02d: decode: %v", i+1, err)
		}
		v, err := r.Verify()
		if err != nil {
			t.Fatalf("case %02d: verify: %v", i+1, err)
		}
		if got := verdict(trust.New(trust.DefaultParams()).Apply(v, tv023NowMs, 1)); got != c.verdict {
			t.Errorf("case %02d: got %s, want %s", i+1, got, c.verdict)
		}
	}
}

// tv022Params are the card inputs the generator states for TV-022.
func tv022Params() a2a.CardParams {
	return a2a.CardParams{
		Name: "MAP test vector provider", Description: "TV-022 Agent Card", Version: "1.0.0",
		JSONRPCURL:              "https://provider.example.org/a2a",
		Skills:                  []a2a.Skill{{ID: "generate", Name: "Generate", Description: "Text generation", Tags: []string{"text"}}},
		SupportedZones:          []uint8{2, 3},
		CheckpointIntervalParts: 8,
		AcceptedVoucherRails:    []string{"pay:mpp-tempo"},
	}
}

func lastBlob(t *testing.T, id string) []byte {
	t.Helper()
	b, err := loadVF(t).Envelope(id)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

// TV-022 produce: the Go-signed card's canonical bytes equal the vector.
func TestTV022Produce(t *testing.T) {
	signed, _, err := a2a.SignCard(a2a.BuildCard(mustAID(t, aid(KeyB)), tv022Params()), KeyB)
	if err != nil {
		t.Fatal(err)
	}
	got, err := a2a.Canonicalize(signed)
	if err != nil {
		t.Fatal(err)
	}
	if d := firstDiff(got, lastBlob(t, "TV-022")); d != -1 {
		t.Fatalf("TV-022: first diff at %d", d)
	}
}

// TV-022 consume: the Python-signed card verifies and is canonical.
func TestTV022Consume(t *testing.T) {
	b := lastBlob(t, "TV-022")
	card, err := a2a.ParseJSON(b)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := a2a.VerifyCard(card, mustAID(t, aid(KeyB))); err != nil {
		t.Fatalf("TV-022 verify: %v", err)
	}
	if c, _ := a2a.Canonicalize(card); !bytes.Equal(c, b) {
		t.Fatal("TV-022 is not canonical")
	}
}

const tv024TTL = 3_600_000

func tv024Spec() a2a.DescriptorSpec {
	return a2a.DescriptorSpec{
		Seq: 1, Endpoints: []string{"https://provider.example.org/a2a"},
		Capabilities: []string{Cap}, Zones: []uint8{2, 3},
	}
}

// TV-024 produce: sealing TV-022's card reproduces the envelope.
func TestTV024Produce(t *testing.T) {
	var id [16]byte
	for i := range id {
		id[i] = 0x24
	}
	env, _, err := a2a.SealDescriptor(KeyB, a2a.BuildCard(mustAID(t, aid(KeyB)), tv022Params()), tv024Spec(), id, TS, tv024TTL)
	if err != nil {
		t.Fatal(err)
	}
	if d := firstDiff(env, lastBlob(t, "TV-024")); d != -1 {
		t.Fatalf("TV-024: first diff at %d", d)
	}
}

// TV-024 consume: opens inside its window, refused once expired.
func TestTV024Consume(t *testing.T) {
	env := lastBlob(t, "TV-024")
	d, card, err := a2a.OpenDescriptor(env, TS+1000)
	if err != nil {
		t.Fatalf("TV-024 open: %v", err)
	}
	if d.Agent != mustAID(t, aid(KeyB)) || d.Seq != 1 || len(d.Zones) != 2 {
		t.Fatal("TV-024 fields differ")
	}
	if c, _ := a2a.Canonicalize(card); !bytes.Equal(c, lastBlob(t, "TV-022")) {
		t.Fatal("TV-024 embeds a card other than TV-022")
	}
	if _, _, err := a2a.OpenDescriptor(env, TS+tv024TTL+1); err == nil {
		t.Fatal("TV-024 accepted after expiry")
	}
}

// TV-025: A2A TaskID (A2A-8) and the Part -> leaf rule (A2A-12..14) over
// non-canonical Part JSON with a metadata member the rule must drop.
func TestTV025TaskIDAndPartLeaves(t *testing.T) {
	blobs := loadVF(t).Sections["TV-025"]
	if len(blobs) != 8 {
		t.Fatalf("TV-025: want 8 blobs, got %d", len(blobs))
	}
	var nonce [16]byte
	for i := range nonce {
		nonce[i] = 7
	}
	task := a2a.TaskIDFor(nonce, mustAID(t, aid(KeyB)), "task-1")
	if !bytes.Equal(task[:], blobs[0]) {
		t.Fatal("TV-025 TaskID differs")
	}
	b := transcript.NewTranscript()
	for i := 0; i < 3; i++ {
		part, err := a2a.ParseJSON(blobs[2+2*i])
		if err != nil {
			t.Fatalf("part %d: %v", i, err)
		}
		leaf, err := a2a.PartLeafContent(part)
		if err != nil {
			t.Fatal(err)
		}
		if d := firstDiff(leaf, blobs[3+2*i]); d != -1 {
			t.Fatalf("part %d leaf: first diff at %d (%s)", i, d, leaf)
		}
		c, _ := a2a.PartChunk(uint64(i), part)
		b.Push(c)
	}
	if root := b.Root(); !bytes.Equal(root[:], blobs[1]) {
		t.Fatal("TV-025 root differs")
	}
}
