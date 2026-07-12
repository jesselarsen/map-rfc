package conformance

import (
	"bytes"
	"encoding/hex"
	"errors"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/discovery"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/hello"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
)

const vectorsPath = "../../map-spec/test-vectors/map-test-vectors.md"

func loadVF(t *testing.T) *VectorFile {
	t.Helper()
	vf, err := LoadVectors(vectorsPath)
	if err != nil {
		t.Fatalf("loading frozen vectors: %v", err)
	}
	return vf
}

// TestProduceParity: every frozen vector is regenerated from seeds
// byte-for-byte (TASK-015 §FR-3, acceptance criterion "produce").
// Failures report the vector id and first-diff offset.
func TestProduceParity(t *testing.T) {
	vf := loadVF(t)
	produced := Produce()
	sections := []string{
		"TV-001", "TV-002", "TV-003", "TV-004", "TV-005", "TV-006", "TV-007",
		"TV-008", "TV-009", "TV-010", "TV-010b", "TV-011", "TV-012", "TV-013",
		"TV-014", "TV-015",
		"NV-001", "NV-002", "NV-003", "NV-004", "NV-005", "NV-006", "NV-007", "NV-008", "NV-009",
	}
	failed := false
	for _, id := range sections {
		frozen, err := vf.Envelope(id)
		if err != nil {
			t.Errorf("%s: %v", id, err)
			failed = true
			continue
		}
		got, ok := produced[id]
		if !ok {
			t.Errorf("%s: not produced", id)
			failed = true
			continue
		}
		if d := firstDiff(got, frozen); d != -1 {
			t.Errorf("%s: first diff at offset %d (got %d bytes, want %d)", id, d, len(got), len(frozen))
			failed = true
		}
	}
	if !failed {
		t.Logf("all %d frozen vectors reproduced byte-exact", len(sections))
	}
}

// TestConsume: every positive vector opens/verifies from its frozen bytes
// (acceptance criterion "consume").
func TestConsume(t *testing.T) {
	vf := loadVF(t)
	wantType := map[string]uint64{
		"TV-001": 0x0001, "TV-003": 0x0002, "TV-004": 0x0006, "TV-005": 0x0100,
		"TV-006": 0x0200, "TV-007": 0x0202, "TV-008": 0x0205, "TV-009": 0x020D,
		"TV-010": 0x0400, "TV-010b": 0x0400, "TV-011": 0x0502, "TV-013": 0x0401,
		"TV-014": 0x020F, "TV-015": 0x0210,
	}
	for id, mt := range wantType {
		env, err := vf.Envelope(id)
		if err != nil {
			t.Fatalf("%s: %v", id, err)
		}
		o, err := envelope.Open(env, nil)
		if err != nil {
			t.Errorf("%s: did not open: %v", id, err)
			continue
		}
		if o.Body.MsgType != mt {
			t.Errorf("%s: msg-type %#x, want %#x", id, o.Body.MsgType, mt)
		}
		if o.Body.Timestamp != TS {
			t.Errorf("%s: timestamp %d, want %d", id, o.Body.Timestamp, TS)
		}
	}

	// TV-002: dual-signed receipt verifies (both role-bound signatures).
	rec, err := vf.Envelope("TV-002")
	if err != nil {
		t.Fatal(err)
	}
	r, err := receipt.Verify(rec)
	if err != nil {
		t.Fatalf("TV-002 receipt did not verify: %v", err)
	}
	if r.Requester.Multibase() != "z6MkmJaZFLtdW8yn2t9b8An5D3bbYvziY7XabCVQ6xSCxah6" ||
		r.Provider.Multibase() != "z6Mkszo9kizvbjPPLe5KnKHedt9pCieVg3NCPoFdzD7446EG" {
		t.Errorf("TV-002 party multibase strings do not match the vector file")
	}

	// TV-012: delegated OFFER — chain validates, accountable root is Key A.
	env12, err := vf.Envelope("TV-012")
	if err != nil {
		t.Fatal(err)
	}
	o12, root, err := envelope.OpenDelegated(env12, nil, TS)
	if err != nil {
		t.Fatalf("TV-012 delegated open failed: %v", err)
	}
	aidA, _ := identity.Parse(aid(KeyA))
	if root != aidA {
		t.Errorf("TV-012 accountable root is not Key A")
	}
	aidDK, _ := identity.Parse(aid(KeyDK))
	if o12.KID != aidDK {
		t.Errorf("TV-012 kid is not the DK leaf")
	}
}

// requireClass asserts err is an *envelope.Error with the given core §9 code.
func requireClass(t *testing.T, id string, err error, code int) {
	t.Helper()
	if err == nil {
		t.Errorf("%s: accepted, must reject with class %d", id, code)
		return
	}
	var ee *envelope.Error
	if !errors.As(err, &ee) || ee.Code != code {
		t.Errorf("%s: got %v, want error class %d", id, err, code)
	}
}

// TestReject: every negative vector is rejected with the required class
// (acceptance criterion "reject"; classes per the vector file).
func TestReject(t *testing.T) {
	vf := loadVF(t)
	get := func(id string) []byte {
		env, err := vf.Envelope(id)
		if err != nil {
			t.Fatalf("%s: %v", id, err)
		}
		return env
	}

	// NV-001 tampered payload -> BAD_SIGNATURE (2000).
	_, err := envelope.Open(get("NV-001"), nil)
	requireClass(t, "NV-001", err, envelope.CodeBadSignature)

	// NV-002 wrong outer tag -> MALFORMED (1000).
	_, err = envelope.Open(get("NV-002"), nil)
	requireClass(t, "NV-002", err, envelope.CodeMalformed)

	// NV-003 indefinite-length body (valid signature!) -> MALFORMED (1000):
	// the deterministic-profile check, isolated from signature checking.
	_, err = envelope.Open(get("NV-003"), nil)
	requireClass(t, "NV-003", err, envelope.CodeMalformed)

	// NV-004 stale descriptor seq: opens fine (no wire error); the
	// discovery §5 LWW rule keeps the newer record.
	o4, err := envelope.Open(get("NV-004"), nil)
	if err != nil {
		t.Fatalf("NV-004 must open (policy reject, not wire error): %v", err)
	}
	seq := contentUint(t, o4.Body.Content, 2)
	if discovery.DescriptorFresher(seq, 1) {
		t.Errorf("NV-004: seq %d must be stale against cached seq 1", seq)
	}

	// NV-005 swapped receipt signatures -> BAD_SIGNATURE (2000).
	_, err = receipt.Verify(get("NV-005"))
	requireClass(t, "NV-005", err, envelope.CodeBadSignature)
	if _, perr := receipt.Parse(get("NV-005")); perr != nil {
		t.Errorf("NV-005 must remain structurally valid: %v", perr)
	}

	// NV-006 mismatched task binding: opens fine; pay §3.1 policy reject.
	o6, err := envelope.Open(get("NV-006"), nil)
	if err != nil {
		t.Fatalf("NV-006 must open (policy reject): %v", err)
	}
	binding := contentBytes(t, o6.Body.Content, 9)
	if bytes.Equal(binding, TaskID) {
		t.Errorf("NV-006: binding unexpectedly matches the OFFER TaskID")
	}

	// NV-007 scope-widening delegation chain -> BAD_DELEGATION (2003).
	_, _, err = envelope.OpenDelegated(get("NV-007"), nil, TS)
	requireClass(t, "NV-007", err, envelope.CodeBadDelegation)

	// NV-009 max-depth-exceeded chain -> BAD_DELEGATION (2003). Parent
	// declares max-depth 0; identical scopes isolate the depth bound.
	_, _, err = envelope.OpenDelegated(get("NV-009"), nil, TS)
	requireClass(t, "NV-009", err, envelope.CodeBadDelegation)

	// NV-008 lease outside the reserved window: opens fine; tasks §6
	// requires the requester to treat it as a rejection.
	o8, err := envelope.Open(get("NV-008"), nil)
	if err != nil {
		t.Fatalf("NV-008 must open (policy reject): %v", err)
	}
	win, ok := contentValue(o8.Body.Content, 3).(cbor.Array)
	if !ok || len(win) != 2 {
		t.Fatalf("NV-008: no granted window")
	}
	from, until := uint64(win[0].(cbor.Uint)), uint64(win[1].(cbor.Uint))
	if from >= TS && until <= DelegExpiry {
		t.Errorf("NV-008: window [%d,%d] unexpectedly inside the reservation", from, until)
	}
}

// TestDelegationDepthBoundary pins the VALID side of the max-depth rule
// (ARCHITECT RULING 2026-07-12: a cert with max-depth d admits at most d
// certs between itself and the leaf), the complement to NV-009's violation.
func TestDelegationDepthBoundary(t *testing.T) {
	aidA, _ := identity.Parse(aid(KeyA))

	// (a) parent max-depth 1, exactly one further hop -> valid.
	parent := mustSeal(KeyA, 0x0502, msgid(30), delegateBody(KeyA, KeyDK, []string{Cap}, 1))
	child := mustSeal(KeyDK, 0x0502, msgid(31), delegateBody(KeyDK, KeySK, []string{Cap}, -1))
	off, err := envelope.SealDelegated(KeySK, 0x0200, msgid(32), TS, offerContent(), [][]byte{child, parent}, nil)
	if err != nil {
		t.Fatal(err)
	}
	_, root, err := envelope.OpenDelegated(off, nil, TS)
	if err != nil {
		t.Fatalf("depth-1 chain with one hop must validate: %v", err)
	}
	if root != aidA {
		t.Errorf("accountable root != Key A")
	}

	// (b) single delegated cert declaring max-depth 0 -> valid (0 certs below).
	leaf := mustSeal(KeyA, 0x0502, msgid(33), delegateBody(KeyA, KeyDK, []string{Cap}, 0))
	off2, err := envelope.SealDelegated(KeyDK, 0x0200, msgid(34), TS, offerContent(), [][]byte{leaf}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := envelope.OpenDelegated(off2, nil, TS); err != nil {
		t.Fatalf("single depth-0 cert must validate: %v", err)
	}
}

// TestDerivedConstants: TH, CB, shard, DHT keys, transcript root
// (vector file "Derived constants").
func TestDerivedConstants(t *testing.T) {
	vf := loadVF(t)
	derived := vf.Sections["Derived"]
	if len(derived) != 3 {
		t.Fatalf("expected 3 pure-hex derived blobs (TH, CB, root), got %d", len(derived))
	}
	th := hello.TranscriptHash(mustProduced(t, "TV-001"), mustProduced(t, "TV-003"))
	if !bytes.Equal(th[:], derived[0]) {
		t.Errorf("transcript hash mismatch")
	}
	cb, err := hello.ChannelBinding(th, make([]byte, 32))
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(cb[:], derived[1]) {
		t.Errorf("channel binding mismatch")
	}
	root := TranscriptRoot([]Chunk{{Seq: 0, Kind: 0, Content: []byte("Hello, MAP!")}})
	if !bytes.Equal(root[:], derived[2]) {
		t.Errorf("transcript root mismatch")
	}

	// Sharding constants appear inline in the file; frozen values asserted here.
	aidB, _ := identity.Parse(aid(KeyB))
	if discovery.Shard(aidB) != 0x8d {
		t.Errorf("shard(B) = %#x, want 0x8d", discovery.Shard(aidB))
	}
	wantK, _ := hex.DecodeString("1220f38c3fea314f636e1fc1acb4b3a4aa9523f07a0039e7303c4e372884c553d8f7")
	wantKp, _ := hex.DecodeString("12201b919a17e7c7c9a97f432416e9b6590c06d21bba7bd1331889d506f276ebdeaf")
	if !bytes.Equal(discovery.CapKey(Cap), wantK) {
		t.Errorf("K(cap) mismatch")
	}
	if !bytes.Equal(discovery.ShardedCapKey(Cap, 0x8d), wantKp) {
		t.Errorf("K'(cap, shard) mismatch")
	}

	// Empty transcript root is well-defined (tasks §8.1) and differs from
	// any single-chunk root.
	empty := TranscriptRoot(nil)
	if bytes.Equal(empty[:], root[:]) {
		t.Errorf("empty root collides with single-chunk root")
	}
}

// TestAgentIDStrings: multibase/did:key forms match the vector file exactly
// (TASK-015 acceptance criterion 4).
func TestAgentIDStrings(t *testing.T) {
	aidA, _ := identity.Parse(aid(KeyA))
	aidB, _ := identity.Parse(aid(KeyB))
	if got := aidA.Multibase(); got != "z6MkmJaZFLtdW8yn2t9b8An5D3bbYvziY7XabCVQ6xSCxah6" {
		t.Errorf("Key A multibase = %s", got)
	}
	if got := aidB.Multibase(); got != "z6Mkszo9kizvbjPPLe5KnKHedt9pCieVg3NCPoFdzD7446EG" {
		t.Errorf("Key B multibase = %s", got)
	}
	if got := aidA.DID(); got != "did:key:z6MkmJaZFLtdW8yn2t9b8An5D3bbYvziY7XabCVQ6xSCxah6" {
		t.Errorf("Key A DID = %s", got)
	}
}

var producedCache map[string][]byte

func mustProduced(t *testing.T, id string) []byte {
	t.Helper()
	if producedCache == nil {
		producedCache = Produce()
	}
	b, ok := producedCache[id]
	if !ok {
		t.Fatalf("no produced vector %s", id)
	}
	return b
}

func contentValue(content cbor.Value, key uint64) cbor.Value {
	m, ok := content.(cbor.Map)
	if !ok {
		return nil
	}
	for _, p := range m {
		if k, ok := p.K.(cbor.Uint); ok && uint64(k) == key {
			return p.V
		}
	}
	return nil
}

func contentUint(t *testing.T, content cbor.Value, key uint64) uint64 {
	t.Helper()
	u, ok := contentValue(content, key).(cbor.Uint)
	if !ok {
		t.Fatalf("content key %d is not a uint", key)
	}
	return uint64(u)
}

func contentBytes(t *testing.T, content cbor.Value, key uint64) []byte {
	t.Helper()
	b, ok := contentValue(content, key).(cbor.Bytes)
	if !ok {
		t.Fatalf("content key %d is not a byte string", key)
	}
	return b
}
