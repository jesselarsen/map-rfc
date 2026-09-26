package dht

import (
	"crypto/ed25519"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/discovery"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// A3 regression: ids from other shards are dropped from a shard query.
func TestInShardDropsForeignIds(t *testing.T) {
	var ids []identity.AgentID
	for i := 0; i < 64; i++ {
		seed := make([]byte, 32)
		seed[0] = byte(i)
		a, _ := identity.FromPub(ed25519.NewKeyFromSeed(seed).Public().(ed25519.PublicKey))
		ids = append(ids, a)
	}
	target := discovery.Shard(ids[0])
	got := InShard(ids, target)
	if len(got) == 0 || len(got) == len(ids) {
		t.Fatalf("filter kept %d of %d", len(got), len(ids))
	}
	for _, id := range got {
		if discovery.Shard(id) != target {
			t.Fatal("foreign-shard id survived")
		}
	}
}
