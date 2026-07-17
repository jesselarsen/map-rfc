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
		"TV-016", "TV-017", "TV-018", "TV-019", "TV-020",
		"NV-001", "NV-002", "NV-003", "NV-004", "NV-005", "NV-006", "NV-007", "NV-008", "NV-009", "NV-010", "NV-011", "NV-012",
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
		"TV-014": 0x020F, "TV-015": 0x0210, "TV-016": 0x0402, "TV-017": 0x0402,
		"TV-018": 0x0404, "TV-019": 0x0405, "TV-020": 0x0406,
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

	// NV-010 metered invoice with off-by-one billing: opens + verifies as an
	// envelope; the pay §4.1 arithmetic check (below) is what rejects it.
	o10, err := envelope.Open(get("NV-010"), nil)
	if err != nil {
		t.Fatalf("NV-010 must open (arithmetic reject, not wire): %v", err)
	}
	if watermarkBillsCorrectly(t, o10.Body.Content) {
		t.Errorf("NV-010: off-by-one watermark unexpectedly passed the billing check")
	}

	// NV-011 receipt with an unrecognized kind: opens + verifies as an
	// envelope; the pay §5.2.1 kind-registry check (fail-closed, 6010 class)
	// is what rejects it.
	o11, err := envelope.Open(get("NV-011"), nil)
	if err != nil {
		t.Fatalf("NV-011 must open (kind-registry reject, not wire): %v", err)
	}
	if receiptKindRecognized(o11.Body.Content) {
		t.Errorf("NV-011: unrecognized receipt kind 9 unexpectedly accepted")
	}
}

// receiptKindRecognized is the pay §5.2.1 receipt-kind registry check, written
// from the spec (independence rule): key 5 absent (= 0 settled) or 0..3;
// anything else MUST be rejected fail-closed.
func receiptKindRecognized(content cbor.Value) bool {
	v := contentValue(content, 5)
	if v == nil {
		return true // absent = kind 0 (settled)
	}
	k, ok := v.(cbor.Uint)
	return ok && uint64(k) <= 3
}

// disputeClaimRecognized is the pay §6.1 claim-code registry check, written
// from the spec (independence rule): key 3 present and 0..4; anything else
// MUST be rejected fail-closed.
func disputeClaimRecognized(content cbor.Value) bool {
	v := contentValue(content, 3)
	k, ok := v.(cbor.Uint)
	return ok && uint64(k) <= 4
}

// TestReceiptKinds: TV-018's kind (1, held) is recognized; NV-011's (9) is
// not — the executable pay §5.2.1 fail-closed parity (TASK-024).
func TestReceiptKinds(t *testing.T) {
	vf := loadVF(t)
	openContent := func(id string) cbor.Value {
		env, err := vf.Envelope(id)
		if err != nil {
			t.Fatalf("%s: %v", id, err)
		}
		o, err := envelope.Open(env, nil)
		if err != nil {
			t.Fatalf("%s: %v", id, err)
		}
		return o.Body.Content
	}
	c18 := openContent("TV-018")
	if !receiptKindRecognized(c18) {
		t.Errorf("TV-018 kind 1 (held) should be recognized")
	}
	if got := contentUint(t, c18, 5); got != 1 {
		t.Errorf("TV-018 kind = %d, want 1", got)
	}
	if receiptKindRecognized(openContent("NV-011")) {
		t.Errorf("NV-011 kind 9 should be rejected fail-closed")
	}
	// Disputes (pay §6.1, TASK-026): TV-019 carries a recognized claim (3);
	// NV-012 carries 9 and must be rejected fail-closed; TV-020's single
	// evidence item is the frozen TV-010b envelope VERBATIM (I1).
	if got := contentUint(t, openContent("TV-019"), 3); got != 3 {
		t.Errorf("TV-019 claim = %d, want 3", got)
	}
	if disputeClaimRecognized(openContent("NV-012")) {
		t.Errorf("NV-012 claim 9 should be rejected fail-closed")
	}
	items, ok := contentValue(openContent("TV-020"), 2).(cbor.Array)
	if !ok || len(items) != 1 {
		t.Fatalf("TV-020: want exactly one evidence item")
	}
	itemBytes := contentBytes(t, items[0], 2)
	vf2 := loadVF(t)
	frozen, err := vf2.Envelope("TV-010b")
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(itemBytes, frozen) {
		t.Errorf("TV-020 item bytes != frozen TV-010b envelope (I1 verbatim-carry)")
	}
	// TV-017: hold invoice — phase key 7 = 1, no watermark (key 4 absent).
	c17 := openContent("TV-017")
	if got := contentUint(t, c17, 7); got != 1 {
		t.Errorf("TV-017 phase = %d, want 1 (hold)", got)
	}
	if contentValue(c17, 4) != nil {
		t.Errorf("TV-017 must not carry a watermark (presence iff terms 3)")
	}
}

// billedModel1 is the pay §4.1 model-1 billing arithmetic, written from the
// spec (NOT ported from map-rs — independence rule): tokens_in × rate_in +
// tokens_out × rate_out, exact integer product on µUSD mantissas. The rates are
// the agreed quote's (3000/5000 µUSD), not carried in the invoice.
func billedModel1(tokensIn, tokensOut uint64) int64 {
	const rateIn, rateOut = 3000, 5000
	return int64(tokensIn)*rateIn + int64(tokensOut)*rateOut
}

// watermarkBillsCorrectly reports whether a PAY_INVOICE's watermark
// cumulative-billed (key 4 of key 4) equals rate × usage (pay §4.1 check 5).
func watermarkBillsCorrectly(t *testing.T, invoice cbor.Value) bool {
	t.Helper()
	w, ok := contentValue(invoice, 4).(cbor.Map)
	if !ok {
		t.Fatalf("invoice missing UsageWatermark (key 4)")
	}
	usage, ok := mapGet(w, 3).(cbor.Map)
	if !ok {
		t.Fatalf("watermark missing Usage (key 3)")
	}
	tokensIn := uint64(mapGet(usage, 1).(cbor.Uint))
	tokensOut := uint64(mapGet(usage, 2).(cbor.Uint))
	claimed := decimalMantissa(t, mapGet(w, 4))
	return billedModel1(tokensIn, tokensOut) == claimed
}

// mapGet returns the value for a uint key in a cbor.Map, or nil.
func mapGet(m cbor.Map, key uint64) cbor.Value {
	for _, p := range m {
		if k, ok := p.K.(cbor.Uint); ok && uint64(k) == key {
			return p.V
		}
	}
	return nil
}

// decimalMantissa extracts the mantissa from an [Asset, 4([exp, mantissa])] pair.
func decimalMantissa(t *testing.T, v cbor.Value) int64 {
	t.Helper()
	arr, ok := v.(cbor.Array)
	if !ok || len(arr) != 2 {
		t.Fatalf("not an [Asset, Amount] pair")
	}
	tag, ok := arr[1].(cbor.Tag)
	if !ok || tag.Num != 4 {
		t.Fatalf("amount is not a tag-4 decimal fraction")
	}
	inner, ok := tag.Content.(cbor.Array)
	if !ok || len(inner) != 2 {
		t.Fatalf("decimal fraction is not [exp, mantissa]")
	}
	m, ok := inner[1].(cbor.Uint)
	if !ok {
		t.Fatalf("mantissa is not a uint")
	}
	return int64(m)
}

// TestWatermarkBilling: TV-016's watermark bills exactly (independent Go
// arithmetic), NV-010's does not — the executable pay §4.1 check-5 parity.
func TestWatermarkBilling(t *testing.T) {
	produced := Produce()
	if got := billedModel1(1523, 812); got != 8_629_000 {
		t.Fatalf("model-1 arithmetic = %d, want 8_629_000", got)
	}
	openContent := func(id string) cbor.Value {
		o, err := envelope.Open(produced[id], nil)
		if err != nil {
			t.Fatalf("%s open: %v", id, err)
		}
		return o.Body.Content
	}
	if !watermarkBillsCorrectly(t, openContent("TV-016")) {
		t.Errorf("TV-016 watermark should bill correctly")
	}
	if watermarkBillsCorrectly(t, openContent("NV-010")) {
		t.Errorf("NV-010 watermark should NOT bill correctly (off by one)")
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
