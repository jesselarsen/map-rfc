package net

import (
	"context"
	"crypto/ed25519"
	"fmt"
	"sync"

	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/session"
	"github.com/multiformats/go-multiaddr"

	gocid "github.com/ipfs/go-cid"
	"github.com/libp2p/go-libp2p"
	dht "github.com/libp2p/go-libp2p-kad-dht"
	"github.com/libp2p/go-libp2p/core/crypto"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/network"
	"github.com/libp2p/go-libp2p/core/peer"
)

// MapNode is the Go counterpart of the Rust map-net MapNode: a blocking
// front-end over a background libp2p host running the /map/1.0.0 protocol over
// QUIC, with a Kademlia DHT for descriptor publication and lookup (discovery
// §6). It deliberately matches the Rust API shape: New / LocalAgent / Listen /
// Dial / Accept / PublishDescriptor / FindProviders / FetchDescriptor.
type MapNode struct {
	host host.Host
	dht  *dht.IpfsDHT
	seed []byte
	priv ed25519.PrivateKey

	local identity.AgentID
	hello session.Hello

	inbound chan *session.Session
	mu      sync.Mutex
	closed  bool
	cancel  context.CancelFunc

	// Local descriptor store (LAN-grade). Descriptors are immutable per CID,
	// so a local map suffices for the demo; cross-node retrieval in production
	// layers the DHT provider-records on top (discovery §6).
	storeMu     sync.Mutex
	descriptors map[string][]byte
}

// New constructs a MapNode from a 32-byte ed25519 seed. The libp2p identity is
// New constructs a MapNode from a 32-byte ed25519 seed. If listenAddr is
// non-empty it is passed to libp2p as a listen address at host construction
// time (libp2p requires listen addresses at construction to advertise them
// correctly; a later Network().Listen does not register the address for
// dialing). The libp2p identity is derived from the seed, and FR-3 is asserted:
// the libp2p PeerID is cryptographically bound to the same ed25519 key, so
// PeerID ≡ AgentID.
func New(seed []byte, hello session.Hello, listenAddr string) (*MapNode, error) {
	if len(seed) != 32 {
		return nil, fmt.Errorf("net: seed must be 32 bytes, got %d", len(seed))
	}
	full := ed25519.NewKeyFromSeed(seed)
	priv, err := crypto.UnmarshalEd25519PrivateKey(full)
	if err != nil {
		return nil, fmt.Errorf("net: bad ed25519 seed: %w", err)
	}
	pub := priv.GetPublic()
	rawPub, err := pub.Raw()
	if err != nil {
		return nil, fmt.Errorf("net: raw public: %w", err)
	}
	aid, err := identity.FromPub(ed25519.PublicKey(rawPub))
	if err != nil {
		return nil, fmt.Errorf("net: agent id: %w", err)
	}
	pid, err := peer.IDFromPublicKey(pub)
	if err != nil {
		return nil, fmt.Errorf("net: peer id: %w", err)
	}
	// FR-3: AgentID must round-trip through PeerID.
	recPub, err := pid.ExtractPublicKey()
	if err != nil {
		return nil, fmt.Errorf("net: extract key: %w", err)
	}
	recRaw, err := recPub.Raw()
	if err != nil {
		return nil, fmt.Errorf("net: raw recovered: %w", err)
	}
	recAID, err := identity.FromPub(ed25519.PublicKey(recRaw))
	if err != nil {
		return nil, fmt.Errorf("net: recover agent id: %w", err)
	}
	if recAID != aid {
		return nil, fmt.Errorf("net: FR-3 violation: agent id %s != recovered %s", aid, recAID)
	}

	opts := []libp2p.Option{libp2p.Identity(priv)}
	if listenAddr != "" {
		opts = append(opts, libp2p.ListenAddrStrings(listenAddr))
	}
	ctx, cancel := context.WithCancel(context.Background())
	h, err := libp2p.New(opts...)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("net: host: %w", err)
	}
	kdht, err := dht.New(ctx, h)
	if err != nil {
		cancel()
		_ = h.Close()
		return nil, fmt.Errorf("net: dht: %w", err)
	}

	n := &MapNode{
		host:        h,
		dht:         kdht,
		seed:        seed,
		priv:        full,
		local:       aid,
		hello:       hello,
		inbound:     make(chan *session.Session, 8),
		cancel:      cancel,
		descriptors: make(map[string][]byte),
	}
	h.SetStreamHandler(mapProtocol, n.handleStream)
	return n, nil
}

// LocalAgent returns the node's AgentID (= libp2p PeerID).
func (n *MapNode) LocalAgent() identity.AgentID { return n.local }

// PeerID returns the libp2p peer id string, for building a dial multiaddr of
// the form <listen-addr>/p2p/<peerid>.
func (n *MapNode) PeerID() string { return n.host.ID().String() }

func (n *MapNode) handleStream(s network.Stream) {
	// NOTE: do not close s here. The Libp2pTransport owns the stream for the
	// session's lifetime; closing it would tear down the session mid-handshake.
	// Remote authenticated via Noise: peer.ID == AgentID (FR-3).
	remotePeer := s.Conn().RemotePeer()
	recPub, err := remotePeer.ExtractPublicKey()
	if err != nil {
		return
	}
	rawPub, err := recPub.Raw()
	if err != nil {
		return
	}
	remoteAID, err := identity.FromPub(ed25519.PublicKey(rawPub))
	if err != nil {
		return
	}
	tr := NewLibp2pTransport(s, remoteAID, n.local, defaultMaxMsgSize)
	cfg := session.SessionConfig{
		Keypair:      n.priv,
		ExpectedPeer: remoteAID,
		Hello:        n.hello,
	}
	sess, err := session.Respond(cfg, tr, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		return
	}
	select {
	case n.inbound <- sess:
	default:
	}
}

// ListenAddr returns the first advertised listen address matching the IP and
// transport used at construction (so the caller can build a dial multiaddr of
// the form <addr>/p2p/<peerid>). Listening is performed at construction time
// via libp2p.ListenAddrStrings; this merely surfaces the bound address.
func (n *MapNode) ListenAddr() (multiaddr.Multiaddr, error) {
	addrs := n.host.Addrs()
	if len(addrs) == 0 {
		return nil, fmt.Errorf("net: node has no advertised addresses")
	}
	for _, a := range addrs {
		if ip, err := a.ValueForProtocol(multiaddr.P_IP4); err == nil && ip == "127.0.0.1" {
			return a, nil
		}
	}
	return addrs[0], nil
}

// Dial connects to a remote peer at addr and asserts its AgentID equals the
// expected value (FR-2.5/FR-3).
func (n *MapNode) Dial(ctx context.Context, addr string, expected identity.AgentID) (*session.Session, error) {
	ma, err := multiaddr.NewMultiaddr(addr)
	if err != nil {
		return nil, fmt.Errorf("net: bad addr %q: %w", addr, err)
	}
	info, err := peer.AddrInfoFromP2pAddr(ma)
	if err != nil {
		return nil, fmt.Errorf("net: addr info: %w", err)
	}
	recPub, err := info.ID.ExtractPublicKey()
	if err != nil {
		return nil, fmt.Errorf("net: remote has no embedded key: %w", err)
	}
	rawPub, err := recPub.Raw()
	if err != nil {
		return nil, fmt.Errorf("net: remote raw key: %w", err)
	}
	remoteAID, err := identity.FromPub(ed25519.PublicKey(rawPub))
	if err != nil {
		return nil, fmt.Errorf("net: remote agent id: %w", err)
	}
	if remoteAID != expected {
		return nil, fmt.Errorf("net: agent id mismatch: got %s expected %s", remoteAID, expected)
	}
	// Connect registers the peer's address with the swarm; a bare NewStream
	// on an unconnected peer reports "no addresses" on some libp2p builds.
	if err := n.host.Connect(ctx, *info); err != nil {
		return nil, fmt.Errorf("net: connect: %w", err)
	}
	s, err := n.host.NewStream(ctx, info.ID, mapProtocol)
	if err != nil {
		return nil, fmt.Errorf("net: new stream: %w", err)
	}
	tr := NewLibp2pTransport(s, n.local, remoteAID, defaultMaxMsgSize)
	cfg := session.SessionConfig{
		Keypair:      n.priv,
		ExpectedPeer: remoteAID,
		Hello:        n.hello,
	}
	return session.Initiate(cfg, tr, session.SystemClock{}, session.RandMsgIDs{})
}

// Accept blocks until the next inbound session arrives.
func (n *MapNode) Accept() *session.Session {
	return <-n.inbound
}

// PublishDescriptor stores a descriptor's bytes under its content CID in the
// local store and announces the node as a provider of that CID on the DHT
// (discovery §6.4). It returns the CID for later retrieval. Cross-node
// retrieval in the LAN-grade scope is performed by the caller exchanging the
// descriptor bytes directly after discovering the provider via FindProviders.
func (n *MapNode) PublishDescriptor(ctx context.Context, payload []byte) ([]byte, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	cid := descriptorCID(payload)
	n.storeMu.Lock()
	n.descriptors[dhtKey(cid)] = payload
	n.storeMu.Unlock()
	// Best-effort provider announcement; ignore errors (no connected peers yet).
	if c, err := gocid.Cast(cid); err == nil {
		_ = n.dht.Provide(ctx, c, true)
	}
	return cid, nil
}

// FetchDescriptor retrieves a descriptor previously published under cid from
// the local store.
func (n *MapNode) FetchDescriptor(_ context.Context, cid []byte) ([]byte, error) {
	n.storeMu.Lock()
	val, ok := n.descriptors[dhtKey(cid)]
	n.storeMu.Unlock()
	if !ok {
		return nil, fmt.Errorf("net: fetch: descriptor not found locally")
	}
	return val, nil
}

// Bootstrap refreshes the DHT routing table (discovery §6). In a multi-node
// deployment this connects to known peers; in the LAN-grade 2-node demo it
// simply triggers a self-refresh so recent connections enter the table.
func (n *MapNode) Bootstrap(ctx context.Context) error {
	return n.dht.Bootstrap(ctx)
}

// FindProviders returns peers closest to the given DHT key (discovery §6). The
// caller supplies the already-derived key (e.g. discovery.ShardedCapabilityKey
// hex), so the routing layer stays transport-agnostic. It does not guarantee
// the returned peers actually provide the capability (the caller verifies via
// a descriptor fetch) — this matches the LAN-grade scope.
func (n *MapNode) FindProviders(ctx context.Context, keyHex string) ([]identity.AgentID, error) {
	peers, err := n.dht.GetClosestPeers(ctx, keyHex)
	if err != nil {
		return nil, fmt.Errorf("net: find: %w", err)
	}
	var out []identity.AgentID
	for _, p := range peers {
		recPub, err := p.ExtractPublicKey()
		if err != nil {
			continue
		}
		rawPub, err := recPub.Raw()
		if err != nil {
			continue
		}
		aid, err := identity.FromPub(ed25519.PublicKey(rawPub))
		if err != nil {
			continue
		}
		out = append(out, aid)
	}
	return out, nil
}

// Close shuts the node down.
func (n *MapNode) Close() error {
	n.mu.Lock()
	if n.closed {
		n.mu.Unlock()
		return nil
	}
	n.closed = true
	n.mu.Unlock()
	n.cancel()
	_ = n.dht.Close()
	return n.host.Close()
}
