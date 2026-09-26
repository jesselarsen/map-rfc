package conformance

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
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
	out["TV-002"] = receipt.Receipt{CoreBytes: core, SigReq: sigReq, SigProv: sigProv}.Encode()

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
	out["NV-005"] = receipt.Receipt{CoreBytes: core, SigReq: sigProv, SigProv: sigReq}.Encode()

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
