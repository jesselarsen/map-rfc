package net

import (
	"crypto/sha256"
	"encoding/hex"

	"github.com/multiformats/go-multihash"
)

// defaultMaxMsgSize mirrors the Rust map-net default envelope ceiling (core
// §10). Excessive for demo use but matches the Rust default so a Rust and Go
// node negotiate the same limit.
const defaultMaxMsgSize uint32 = 4 << 20

// descriptorCID returns the descriptor's content CID: a SHA2-256 multihash of
// the descriptor bytes (discovery §6.4). It is the key under which the
// descriptor is stored/retrieved on the DHT.
func descriptorCID(payload []byte) []byte {
	h := sha256.Sum256(payload)
	mh, err := multihash.Encode(h[:], multihash.SHA2_256)
	if err != nil {
		// multihash.Encode over a fixed algorithm cannot fail.
		panic(err)
	}
	return mh
}

// dhtKey encodes a CID into the DHT record key under the "map" namespace. The
// namespace lets us register a permissive record validator (see mapValidator)
// so arbitrary descriptor/provider payloads can be stored without the default
// public-key signing requirement.
func dhtKey(cid []byte) string { return "/map/" + hex.EncodeToString(cid) }
