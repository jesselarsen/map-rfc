package conformance

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/discovery"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
)

// Deterministic test keys (vector file "Keys" section): seeds are
// SHA-256 of fixed labels. Testing only — never production keys.
func keyFromLabel(label string) ed25519.PrivateKey {
	seed := sha256.Sum256([]byte(label))
	return ed25519.NewKeyFromSeed(seed[:])
}

var (
	KeyA  = keyFromLabel("MAP-TEST-VECTOR-KEY-A")
	KeyB  = keyFromLabel("MAP-TEST-VECTOR-KEY-B")
	KeyDK = keyFromLabel("MAP-TEST-VECTOR-KEY-A-DK") // delegated key, TV-011/012
	KeySK = keyFromLabel("MAP-TEST-VECTOR-KEY-A-SK") // sub-delegated key, NV-007
)

const (
	// TS is the fixed vector timestamp (ms).
	TS = 1780000000000
	// DelegExpiry is the shared not-after / quote expiry.
	DelegExpiry = 1780000600000
	// Cap is the capability every vector exercises.
	Cap = "map:cap:inference/text-generation"
)

// TaskID is the shared 32-byte task id (BLAKE2b-256("example-task");
// hardcoded per TASK-015 §FR-2 — blake2 is not otherwise needed).
var TaskID = mustHex("d06d15802fbb8ac7c5b07129175d7b461c8e74b69ca0ad7b7a84f890c0f05e25")

func mustHex(s string) []byte {
	b, err := hex.DecodeString(s)
	if err != nil {
		panic(err)
	}
	return b
}

func msgid(n byte) []byte {
	b := make([]byte, 16)
	for i := range b {
		b[i] = n
	}
	return b
}

func aid(priv ed25519.PrivateKey) []byte {
	pub := priv.Public().(ed25519.PublicKey)
	return append([]byte{0xED, 0x01}, pub...)
}

func mustSeal(priv ed25519.PrivateKey, msgType uint64, msgID []byte, content cbor.Value) []byte {
	env, err := envelope.Seal(priv, msgType, msgID, TS, content, nil)
	if err != nil {
		panic(err)
	}
	return env
}

// usdAmount is ["iso4217:USD", 4([-6, 4200000])] (pay §3: Asset + tag-4
// decimal-fraction Amount).
func usdAmount() cbor.Value {
	return cbor.Array{cbor.Text("iso4217:USD"),
		cbor.Tag{Num: 4, Content: cbor.Array{cbor.Int(-6), cbor.Uint(4200000)}}}
}

func payQuoteContent(taskBinding []byte) cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x10))},
		{K: cbor.Uint(2), V: cbor.Uint(0)},
		{K: cbor.Uint(3), V: cbor.Map{{K: cbor.Uint(0), V: usdAmount()}}},
		{K: cbor.Uint(5), V: cbor.Array{cbor.Map{
			{K: cbor.Uint(1), V: cbor.Text("pay:credit-ledger")},
			{K: cbor.Uint(2), V: cbor.Bytes{}},
		}}},
		{K: cbor.Uint(6), V: cbor.Uint(DelegExpiry)},
	}
	if taskBinding != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(9), V: cbor.Bytes(taskBinding)})
	}
	return m
}

func offerContent() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(TaskID)},
		{K: cbor.Uint(2), V: cbor.Text(Cap)},
		{K: cbor.Uint(3), V: cbor.Map{}},
	}
}

func delegateBody(issuer, subject ed25519.PrivateKey, scope []string, maxDepth int) cbor.Value {
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(aid(issuer))},
		{K: cbor.Uint(2), V: cbor.Bytes(aid(subject))},
		{K: cbor.Uint(3), V: cbor.Uint(TS)},
		{K: cbor.Uint(4), V: cbor.Uint(DelegExpiry)},
	}
	caps := make(cbor.Array, len(scope))
	for i, s := range scope {
		caps[i] = cbor.Text(s)
	}
	m = append(m, cbor.Pair{K: cbor.Uint(5), V: caps})
	if maxDepth >= 0 {
		m = append(m, cbor.Pair{K: cbor.Uint(7), V: cbor.Uint(uint64(maxDepth))})
	}
	return m
}

// Produce regenerates every frozen vector from seeds (TASK-015 §FR-3).
// Keys are the section ids of the vector file ("TV-001".."TV-015",
// "TV-010b", "NV-001".."NV-008").
func Produce() map[string][]byte {
	out := map[string][]byte{}

	// TV-001 HELLO (core §6.1: version/suite LISTS).
	tv001 := mustSeal(KeyA, 0x0001, mustHex("000102030405060708090a0b0c0d0e0f"), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Array{cbor.Uint(0x0100)}},
		{K: cbor.Uint(2), V: cbor.Array{cbor.Uint(1)}},
		{K: cbor.Uint(3), V: cbor.Map{}},
		{K: cbor.Uint(4), V: cbor.Uint(262144)},
	})
	out["TV-001"] = tv001

	// TV-002 ExecutionReceipt (trust §3).
	core := cbor.Encode(cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(TaskID)},
		{K: cbor.Uint(2), V: cbor.Bytes(aid(KeyA))},
		{K: cbor.Uint(3), V: cbor.Bytes(aid(KeyB))},
		{K: cbor.Uint(4), V: cbor.Text(Cap)},
		{K: cbor.Uint(5), V: cbor.Uint(1780000000000)},
		{K: cbor.Uint(6), V: cbor.Uint(1780000042000)},
		{K: cbor.Uint(7), V: cbor.Uint(0)},
		{K: cbor.Uint(8), V: usageMap()},
		{K: cbor.Uint(9), V: cbor.Uint(92)},
	})
	sigReq := receipt.Sign(KeyA, receipt.RoleRequester, core)
	sigProv := receipt.Sign(KeyB, receipt.RoleProvider, core)
	out["TV-002"] = receipt.Encode(core, sigReq, sigProv)

	// TV-003 HELLO_ACK (core §6.1: SINGLETON version/suite).
	tv003 := mustSeal(KeyB, 0x0002, msgid(3), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(0x0100)},
		{K: cbor.Uint(2), V: cbor.Uint(1)},
		{K: cbor.Uint(3), V: cbor.Map{}},
		{K: cbor.Uint(4), V: cbor.Uint(262144)},
	})
	out["TV-003"] = tv003

	// TV-004 ERROR (core §9).
	out["TV-004"] = mustSeal(KeyB, 0x0006, msgid(4), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(1003)},
		{K: cbor.Uint(2), V: cbor.Bool(false)},
		{K: cbor.Uint(3), V: cbor.Text("replayed msg-id")},
		{K: cbor.Uint(4), V: cbor.Bytes(mustHex("000102030405060708090a0b0c0d0e0f"))},
	})

	// TV-005 DESCRIPTOR (discovery §2), seq 1.
	out["TV-005"] = mustSeal(KeyB, 0x0100, msgid(5), descriptorContent(1))

	// TV-006 OFFER (tasks §2).
	out["TV-006"] = mustSeal(KeyA, 0x0200, msgid(6), offerContent())

	// TV-007 ASSIGN (tasks §2.1): InputSpec key 2 = [+Multihash]
	// (content-addressed inputs), FlowInit {1: msgs, 2: bytes}.
	out["TV-007"] = mustSeal(KeyA, 0x0202, msgid(7), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(TaskID)},
		{K: cbor.Uint(2), V: cbor.Text(Cap)},
		{K: cbor.Uint(3), V: cbor.Map{}},
		{K: cbor.Uint(4), V: cbor.Map{{K: cbor.Uint(2),
			V: cbor.Array{cbor.Bytes(discovery.MultihashSHA256([]byte("MAP-TV-INPUT")))}}}},
		{K: cbor.Uint(5), V: cbor.Map{
			{K: cbor.Uint(1), V: cbor.Uint(64)},
			{K: cbor.Uint(2), V: cbor.Uint(1048576)},
		}},
	})

	// TV-008 CHUNK (tasks §3): seq 0, kind 0, "Hello, MAP!".
	chunkContent := []byte("Hello, MAP!")
	out["TV-008"] = mustSeal(KeyB, 0x0205, msgid(8), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(TaskID)},
		{K: cbor.Uint(2), V: cbor.Uint(0)},
		{K: cbor.Uint(3), V: cbor.Uint(0)},
		{K: cbor.Uint(4), V: cbor.Bytes(chunkContent)},
	})

	// TV-009 COMPLETE (tasks §4 / §8.1): transcript root over the single
	// TV-008 output chunk, wrapped as Multihash.
	root := TranscriptRoot([]Chunk{{Seq: 0, Kind: 0, Content: chunkContent}})
	out["TV-009"] = mustSeal(KeyB, 0x020D, msgid(9), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(TaskID)},
		{K: cbor.Uint(2), V: cbor.Uint(0)},
		{K: cbor.Uint(3), V: cbor.Bytes(append([]byte{0x12, 0x20}, root[:]...))},
		{K: cbor.Uint(4), V: usageMap()},
	})

	// TV-010 PAY_QUOTE unbound; TV-010b bound to the TV-006 TaskID (pay §3.1).
	out["TV-010"] = mustSeal(KeyB, 0x0400, msgid(10), payQuoteContent(nil))
	tv010b := mustSeal(KeyB, 0x0400, msgid(11), payQuoteContent(TaskID))
	out["TV-010b"] = tv010b

	// TV-011 DelegationCert (identity §4): RIK-A -> DK, scope Cap, depth 1.
	tv011 := mustSeal(KeyA, 0x0502, msgid(13), delegateBody(KeyA, KeyDK, []string{Cap}, 1))
	out["TV-011"] = tv011

	// TV-012 delegated OFFER signed by DK, chain [TV-011] in ProtHdr 33.
	tv012, err := envelope.SealDelegated(KeyDK, 0x0200, msgid(14), TS, offerContent(), [][]byte{tv011}, nil)
	if err != nil {
		panic(err)
	}
	out["TV-012"] = tv012

	// TV-013 PAY_AGREE (pay §3.1): key 2 = Multihash of the received
	// TV-010b envelope bytes (I1).
	tv013 := mustSeal(KeyA, 0x0401, msgid(18), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x10))},
		{K: cbor.Uint(2), V: cbor.Bytes(discovery.MultihashSHA256(tv010b))},
		{K: cbor.Uint(3), V: cbor.Text("pay:credit-ledger")},
		{K: cbor.Uint(4), V: usdAmount()},
	})
	out["TV-013"] = tv013

	// TV-014 RESERVE (tasks §6): window [TS, DelegExpiry], zone-1 constraint.
	rsvID := msgid(0x20)
	out["TV-014"] = mustSeal(KeyA, 0x020F, msgid(19), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(rsvID)},
		{K: cbor.Uint(2), V: cbor.Text(Cap)},
		{K: cbor.Uint(3), V: cbor.Uint(TS)},
		{K: cbor.Uint(4), V: cbor.Uint(DelegExpiry)},
		{K: cbor.Uint(5), V: cbor.Map{{K: cbor.Uint(5), V: cbor.Uint(1)}}},
	})

	// TV-015 LEASE granted (tasks §6): window inside TV-014's.
	out["TV-015"] = mustSeal(KeyB, 0x0210, msgid(20), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(rsvID)},
		{K: cbor.Uint(2), V: cbor.Uint(0)},
		{K: cbor.Uint(3), V: cbor.Array{cbor.Uint(1780000100000), cbor.Uint(1780000500000)}},
	})

	// TV-016 metered PAY_INVOICE (pay §4/§4.1): first incremental invoice, so
	// amount (key 3) == watermark cumulative-billed (key 4). Model-1 rates
	// 3000/5000 µUSD ⇒ 1523×3000 + 812×5000 = 8_629_000 µUSD.
	out["TV-016"] = mustSeal(KeyB, 0x0402, msgid(25), payInvoiceContent(cumBilled(8_629_000)))

	// TV-017 escrow hold PAY_INVOICE (pay §5.2.1/§5.2.3, TASK-024): phase
	// key 7 = 1 (hold), amount = the quote key-4 total (500 cents), NO
	// watermark. Method payload (pay §5.2.4): the demanded hold expiry-ts
	// as deterministic-CBOR uint.
	out["TV-017"] = mustSeal(KeyB, 0x0402, msgid(27), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x31))},
		{K: cbor.Uint(2), V: cbor.Bytes(msgid(0x10))},
		{K: cbor.Uint(3), V: centUSD(500)},
		{K: cbor.Uint(5), V: cbor.Bytes(cbor.Encode(cbor.Uint(DelegExpiry)))},
		{K: cbor.Uint(6), V: cbor.Uint(TS)},
		{K: cbor.Uint(7), V: cbor.Uint(1)},
	})

	// TV-018 held PAY_RECEIPT (pay §5.2.1): kind key 5 = 1 (held); key 4
	// (proof digest) deliberately absent.
	out["TV-018"] = mustSeal(KeyB, 0x0404, msgid(28), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x31))},
		{K: cbor.Uint(2), V: centUSD(500)},
		{K: cbor.Uint(3), V: cbor.Uint(TS)},
		{K: cbor.Uint(5), V: cbor.Uint(1)},
	})

	// ---- Negative vectors ----

	// NV-001: envelope byte 60 (payload byte 13) XOR 0xFF, original signature.
	nv001 := append([]byte(nil), tv001...)
	nv001[60] ^= 0xFF
	out["NV-001"] = nv001

	// NV-002: outer tag 18 -> 17.
	nv002 := append([]byte(nil), tv001...)
	nv002[0] = 0xD1
	out["NV-002"] = nv002

	// NV-003: indefinite-length body head, RE-SIGNED (only the
	// deterministic-encoding profile is violated).
	out["NV-003"] = buildNV003(tv001)

	// NV-004: TV-005 re-issued with stale seq 0.
	out["NV-004"] = mustSeal(KeyB, 0x0100, msgid(5), descriptorContent(0))

	// NV-005: TV-002 with the two signatures swapped.
	out["NV-005"] = receipt.Encode(core, sigProv, sigReq)

	// NV-006: bound PAY_QUOTE whose key 9 != the OFFER TaskID (first byte
	// flipped).
	badTask := append([]byte(nil), TaskID...)
	badTask[0] ^= 0xFF
	out["NV-006"] = mustSeal(KeyB, 0x0400, msgid(12), payQuoteContent(badTask))

	// NV-007: 2-link chain RIK->DK ("…/text") then DK->SK ("…inference",
	// WIDER); OFFER signed by SK carries [child, parent].
	nv7Parent := mustSeal(KeyA, 0x0502, msgid(15), delegateBody(KeyA, KeyDK, []string{"map:cap:inference/text"}, -1))
	nv7Child := mustSeal(KeyDK, 0x0502, msgid(16), delegateBody(KeyDK, KeySK, []string{"map:cap:inference"}, -1))
	nv007, err := envelope.SealDelegated(KeySK, 0x0200, msgid(17), TS, offerContent(), [][]byte{nv7Child, nv7Parent}, nil)
	if err != nil {
		panic(err)
	}
	out["NV-007"] = nv007

	// NV-008: LEASE granted entirely after the reservation's `until`.
	out["NV-008"] = mustSeal(KeyB, 0x0210, msgid(21), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(rsvID)},
		{K: cbor.Uint(2), V: cbor.Uint(0)},
		{K: cbor.Uint(3), V: cbor.Array{cbor.Uint(DelegExpiry + 3600000), cbor.Uint(DelegExpiry + 7200000)}},
	})

	// NV-009: 2-link chain RIK->DK->SK whose PARENT declares max-depth 0
	// (key 7) yet DK delegates one hop further. Both links carry the
	// IDENTICAL scope (Cap), so scope is NOT widened — only the identity §4
	// depth bound is violated. OFFER signed by SK carries [child, parent].
	nv9Parent := mustSeal(KeyA, 0x0502, msgid(22), delegateBody(KeyA, KeyDK, []string{Cap}, 0))
	nv9Child := mustSeal(KeyDK, 0x0502, msgid(23), delegateBody(KeyDK, KeySK, []string{Cap}, -1))
	nv009, err := envelope.SealDelegated(KeySK, 0x0200, msgid(24), TS, offerContent(), [][]byte{nv9Child, nv9Parent}, nil)
	if err != nil {
		panic(err)
	}
	out["NV-009"] = nv009

	// NV-010: TV-016 with the watermark cumulative-billed off by one
	// (8_629_001 vs the 8_629_000 the rates bill); amount (key 3) stays the
	// correct 8_629_000, so exactly one mantissa digit differs. MUST reject
	// 6009 (pay §4.1 arithmetic).
	out["NV-010"] = mustSeal(KeyB, 0x0402, msgid(26), payInvoiceContentBilled(cumBilled(8_629_000), cumBilled(8_629_001)))

	// TV-019 PAY_DISPUTE (pay §6.1): claim 3 (overbilling) over the TV-016
	// invoice, anchored to TV-013 by envelope-digest PayAgreeRef (I1), bound
	// to the TV-006 TaskID. No statement (never evidence).
	disputeContent := func(claim uint64) cbor.Value {
		return cbor.Map{
			{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x32))},
			{K: cbor.Uint(2), V: cbor.Bytes(msgid(0x30))},
			{K: cbor.Uint(3), V: cbor.Uint(claim)},
			{K: cbor.Uint(4), V: cbor.Map{
				{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x10))},
				{K: cbor.Uint(2), V: cbor.Bytes(discovery.MultihashSHA256(tv013))},
			}},
			{K: cbor.Uint(5), V: cbor.Bytes(TaskID)},
			{K: cbor.Uint(7), V: cbor.Uint(TS)},
		}
	}
	out["TV-019"] = mustSeal(KeyA, 0x0405, msgid(30), disputeContent(3))

	// TV-020 PAY_DISPUTE_EVIDENCE (pay §6.1): ONE kind-0 item carrying the
	// frozen TV-010b envelope VERBATIM (I1 verbatim-carry); CB absent
	// (out-of-session, pay §6.2).
	out["TV-020"] = mustSeal(KeyA, 0x0406, msgid(31), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x32))},
		{K: cbor.Uint(2), V: cbor.Array{cbor.Map{
			{K: cbor.Uint(1), V: cbor.Uint(0)},
			{K: cbor.Uint(2), V: cbor.Bytes(tv010b)},
		}}},
	})

	// NV-011: TV-018 with kind = 9 — pay §5.2.1 registers only kinds 0..3;
	// receivers MUST reject unrecognized kinds fail-closed (6010 class).
	out["NV-011"] = mustSeal(KeyB, 0x0404, msgid(29), cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(msgid(0x31))},
		{K: cbor.Uint(2), V: centUSD(500)},
		{K: cbor.Uint(3), V: cbor.Uint(TS)},
		{K: cbor.Uint(5), V: cbor.Uint(9)},
	})

	// NV-012: TV-019 with claim = 9 — pay §6.1 registers only claims 0..4;
	// receivers MUST reject unknown codes fail-closed.
	out["NV-012"] = mustSeal(KeyA, 0x0405, msgid(33), disputeContent(9))

	return out
}

// centUSD is ["iso4217:USD", 4([-2, mantissa])] — the TV-017/018 escrow total.
func centUSD(mantissa int64) cbor.Value {
	return cbor.Array{cbor.Text("iso4217:USD"),
		cbor.Tag{Num: 4, Content: cbor.Array{cbor.Int(-2), cbor.Uint(uint64(mantissa))}}}
}

// microUSD is ["iso4217:USD", 4([-6, mantissa])].
func microUSD(mantissa int64) cbor.Value {
	return cbor.Array{cbor.Text("iso4217:USD"),
		cbor.Tag{Num: 4, Content: cbor.Array{cbor.Int(-6), cbor.Uint(uint64(mantissa))}}}
}

// cumBilled is the cumulative-billed [Asset, Amount] at a µUSD mantissa.
func cumBilled(mantissa int64) cbor.Value { return microUSD(mantissa) }

// watermarkContent is the TV-016 UsageWatermark (pay §4.1): task = the TV-006
// TaskID, through-chunk-seq 8, cumulative Usage {1523,812,431,42000}.
func watermarkContent(cumulativeBilled cbor.Value) cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(TaskID)},
		{K: cbor.Uint(2), V: cbor.Uint(8)},
		{K: cbor.Uint(3), V: cbor.Map{
			{K: cbor.Uint(1), V: cbor.Uint(1523)},
			{K: cbor.Uint(2), V: cbor.Uint(812)},
			{K: cbor.Uint(3), V: cbor.Uint(431)},
			{K: cbor.Uint(4), V: cbor.Uint(42000)},
		}},
		{K: cbor.Uint(4), V: cumulativeBilled},
	}
}

// payInvoiceContent is a metered PAY_INVOICE whose amount and watermark
// cumulative-billed agree (the valid first-invoice shape).
func payInvoiceContent(amount cbor.Value) cbor.Value {
	return payInvoiceContentBilled(amount, amount)
}

// payInvoiceContentBilled lets amount (key 3) and the watermark's
// cumulative-billed (key 4) differ — used by NV-010 to break the arithmetic.
func payInvoiceContentBilled(amount, cumulativeBilled cbor.Value) cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(msgidBytes(0x30))},
		{K: cbor.Uint(2), V: cbor.Bytes(msgidBytes(0x10))},
		{K: cbor.Uint(3), V: amount},
		{K: cbor.Uint(4), V: watermarkContent(cumulativeBilled)},
		{K: cbor.Uint(5), V: cbor.Bytes{}},
		{K: cbor.Uint(6), V: cbor.Uint(TS)},
	}
}

// msgidBytes is a 16-byte id filled with b (invoice/quote ids in TV-016).
func msgidBytes(b byte) []byte {
	out := make([]byte, 16)
	for i := range out {
		out[i] = b
	}
	return out
}

func usageMap() cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Uint(1523)},
		{K: cbor.Uint(2), V: cbor.Uint(812)},
		{K: cbor.Uint(3), V: cbor.Uint(431)},
		{K: cbor.Uint(4), V: cbor.Uint(42000)},
	}
}

func descriptorContent(seq uint64) cbor.Value {
	return cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(aid(KeyB))},
		{K: cbor.Uint(2), V: cbor.Uint(seq)},
		{K: cbor.Uint(3), V: cbor.Array{cbor.Uint(0x0100)}},
		{K: cbor.Uint(4), V: cbor.Array{cbor.Uint(1)}},
		{K: cbor.Uint(5), V: cbor.Array{cbor.Text("/map/1.0.0")}},
		{K: cbor.Uint(6), V: cbor.Array{cbor.Map{{K: cbor.Uint(1), V: cbor.Text(Cap)}}}},
		{K: cbor.Uint(12), V: cbor.Array{cbor.Uint(2)}},
	}
}

// buildNV003 rebuilds TV-001's payload with an indefinite-length map head
// (0xbf … 0xff) and re-signs it with Key A, so the signature is VALID and
// only the deterministic-encoding profile is violated.
func buildNV003(tv001 []byte) []byte {
	// TV-001 layout: d2 84 58 28 <40B protected> a0 58 35 <53B payload> 58 40 <sig>.
	protected := tv001[4:44]
	payload := tv001[47 : 47+0x35]
	indef := append([]byte{0xBF}, payload[1:]...)
	indef = append(indef, 0xFF)
	sigStruct := cbor.Encode(cbor.Array{
		cbor.Text("Signature1"), cbor.Bytes(protected), cbor.Bytes{}, cbor.Bytes(indef),
	})
	sig := ed25519.Sign(KeyA, sigStruct)
	return cbor.Encode(cbor.Tag{Num: 18, Content: cbor.Array{
		cbor.Bytes(protected), cbor.Map{}, cbor.Bytes(indef), cbor.Bytes(sig),
	}})
}
