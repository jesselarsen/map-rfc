// Package dht is the libp2p/Kademlia transport binding for MAP descriptor
// publish/lookup (discovery §5/§6). It is deliberately SEPARATE from the
// `discovery` package: `discovery` holds the stdlib-only descriptor codec and
// key derivation (the independence-critical conformance surface), while this
// package pulls the libp2p ecosystem via `net`. Keeping the split means the
// conformance suite never links libp2p.
package dht

import (
	"context"
	"encoding/hex"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/discovery"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/net"
)

// Router publishes and resolves descriptors over a libp2p/Kademlia DHT
// (discovery §5/§6), using a net.MapNode as the transport. It mirrors the
// Rust map-discovery provider-record scheme at the key-derivation level:
// descriptors are stored under their content CID, and capability lookups use
// the sharded capability key K'(cap, shard).
type Router struct {
	node *net.MapNode
}

// NewRouter wraps a MapNode for descriptor operations.
func NewRouter(node *net.MapNode) *Router { return &Router{node: node} }

// Publish encodes and advertises a descriptor, returning its CID. The node's
// listen address is attached to the descriptor's addrs so a resolver can dial
// it directly (discovery §2/§6.4).
func (r *Router) Publish(ctx context.Context, desc *discovery.Descriptor) ([]byte, error) {
	if len(desc.Capabilities) == 0 {
		return nil, fmt.Errorf("dht: descriptor has no capabilities")
	}
	payload, err := desc.Encode()
	if err != nil {
		return nil, fmt.Errorf("dht: encode: %w", err)
	}
	cid, err := r.node.PublishDescriptor(ctx, payload)
	if err != nil {
		return nil, fmt.Errorf("dht: publish: %w", err)
	}
	return cid, nil
}

// Fetch retrieves a descriptor by its CID.
func (r *Router) Fetch(ctx context.Context, cid []byte) (*discovery.Descriptor, error) {
	payload, err := r.node.FetchDescriptor(ctx, cid)
	if err != nil {
		return nil, fmt.Errorf("dht: fetch: %w", err)
	}
	return discovery.Decode(payload)
}

// FindProviders returns agents that may provide cap, sampling the shard chosen
// by the querying agent (discovery §6). The returned ids are the DHT's closest
// peers to K'(cap, shard); the caller verifies by fetching a descriptor.
func (r *Router) FindProviders(ctx context.Context, cap string, shard uint8) ([]identity.AgentID, error) {
	key := hex.EncodeToString(discovery.ShardedCapKey(cap, shard))
	ids, err := r.node.FindProviders(ctx, key)
	if err != nil {
		return nil, fmt.Errorf("dht: find: %w", err)
	}
	return ids, nil
}
