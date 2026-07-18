// Package tasks implements the MAP task protocol runtime (draft-map-tasks-01):
// the wire message codecs (msg.go), the transcript commitment (transcript.go),
// flow-credit accounting (flow.go), and the provider/requester state machines
// (provider.go, requester.go). It is a faithful port of the Rust map-tasks +
// map-taskrun crates, reproduced so the two implementations interoperate
// byte-for-byte on the transcript root.
//
// Determinism note: the transcript root depends only on SHA-256 over the
// deterministic CBOR codec (map-go/cbor, RFC 8949 §4.2.1). Because Go's CBOR
// encoder sorts map keys bytewise and produces minimal heads — identical to the
// Rust profile — the leaf encoding [seq, kind, content] (an array, no maps)
// matches exactly, so both sides commit to the same root.
package tasks

// Message types (core §5, tasks §2). Mirrors map-core/src/msg_type.rs.
const (
	MsgOffer      = 0x0200
	MsgQuote      = 0x0201
	MsgAssign     = 0x0202
	MsgAccept     = 0x0203
	MsgReject     = 0x0204
	MsgChunk      = 0x0205
	MsgCancel     = 0x020C
	MsgComplete   = 0x020D
	MsgReserve    = 0x020F
	MsgLease      = 0x0210
	MsgLeaseRel   = 0x0211
	MsgFlowCredit = 0x0007
)

// sha256MultihashPrefix is the 0x12/0x20 multihash prefix wrapping the 32-byte
// transcript root inside Complete key 3 (core §3, tasks §4).
var sha256MultihashPrefix = []byte{0x12, 0x20}
