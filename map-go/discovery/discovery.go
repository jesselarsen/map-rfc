// Package discovery implements capability index keys and sharding
// (draft-map-discovery-00 §6) and the descriptor freshness rule (§5).
package discovery

import (
	"crypto/sha256"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// MultihashSHA256 wraps a SHA-256 digest as a multihash
// (0x12 sha2-256, 0x20 length 32).
func MultihashSHA256(data []byte) []byte {
	d := sha256.Sum256(data)
	return append([]byte{0x12, 0x20}, d[:]...)
}

// Shard returns the provider's shard: the first byte of SHA-256(AgentID)
// (discovery §6).
func Shard(a identity.AgentID) uint8 {
	d := sha256.Sum256(a[:])
	return d[0]
}

// CapKey is the unsharded capability index key K(cap) = Multihash of
// "map:idx:1:<cap>" (discovery §6).
func CapKey(cap string) []byte {
	return MultihashSHA256([]byte("map:idx:1:" + cap))
}

// ShardedCapKey is K'(cap, shard) = Multihash of
// "map:idx:1:<cap>:<shard>" with the shard in decimal (discovery §6).
func ShardedCapKey(cap string, shard uint8) []byte {
	return MultihashSHA256([]byte(fmt.Sprintf("map:idx:1:%s:%d", cap, shard)))
}

// DescriptorFresher implements the discovery §5 last-writer-wins rule:
// a received descriptor replaces the cached one only when its seq is
// strictly newer. NV-004 (stale seq) is the executable counterpart —
// a stale descriptor is not a wire error; the receiver keeps the newer.
func DescriptorFresher(receivedSeq, cachedSeq uint64) bool {
	return receivedSeq > cachedSeq
}
