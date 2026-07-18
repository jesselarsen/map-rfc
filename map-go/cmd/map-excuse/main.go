// Command map-excuse is the Go implementation #2 demo provider:
// "MAP-Excuse-As-A-Service". It listens on libp2p/QUIC, accepts MAP sessions,
// and answers tasks for the demo capability map:cap:demo/excuse with a single
// deterministic output chunk. It is a real task-answerer (not a stub) and is
// wire-compatible with the Rust rfc-title-o-matic provider.
//
// Usage:
//
//	map-excuse --seed <hex> --listen "/ip4/0.0.0.0/udp/0/quic-v1" \
//	           --cap map:cap:demo/excuse
package main

import (
	"context"
	"encoding/hex"
	"flag"
	"fmt"
	"os"

	"github.com/jesselarsen/map-rfc/map-go/dht"
	"github.com/jesselarsen/map-rfc/map-go/discovery"
	"github.com/jesselarsen/map-rfc/map-go/net"
	"github.com/jesselarsen/map-rfc/map-go/session"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

func die(scenario string, err error) {
	fmt.Printf("FAIL %s: %v\n", scenario, err)
	os.Exit(1)
}

// excuseExecutor answers the demo excuse capability.
type excuseExecutor struct{}

func (excuseExecutor) Execute(input []byte) []tasks.Chunk {
	var prompt string
	if len(input) > 0 {
		prompt = string(input)
	}
	line := fmt.Sprintf("Excuse: I'm late because %q triggered a cascade in the mesh.", prompt)
	return []tasks.Chunk{{Seq: 0, Kind: tasks.ChunkOutputText, Content: []byte(line)}}
}

func main() {
	seedHex := flag.String("seed", "", "32-byte hex ed25519 seed")
	listen := flag.String("listen", "/ip4/0.0.0.0/udp/0/quic-v1", "libp2p listen multiaddr")
	capURI := flag.String("cap", "map:cap:demo/excuse", "capability URI served")
	flag.Parse()

	if *seedHex == "" {
		die("serve", fmt.Errorf("--seed required"))
	}
	seed, err := hex.DecodeString(*seedHex)
	if err != nil || len(seed) != 32 {
		die("serve", fmt.Errorf("bad --seed"))
	}
	if !discovery.ValidCapURI(*capURI) {
		die("serve", fmt.Errorf("invalid cap uri %q", *capURI))
	}

	hello := session.Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 262144}
	node, err := net.New(seed, hello, *listen)
	if err != nil {
		die("serve", err)
	}
	defer node.Close()

	addr, err := node.ListenAddr()
	if err != nil {
		die("serve", err)
	}
	fmt.Printf("LISTENING %s/p2p/%s\n", addr.String(), node.PeerID())

	// Publish a descriptor so peers can discover this provider (discovery §2/§6).
	desc := &discovery.Descriptor{
		Agent:        node.LocalAgent(),
		Seq:          1,
		Versions:     []uint16{0x0100},
		Suites:       []uint16{1},
		Addrs:        []string{addr.String() + "/p2p/" + node.PeerID()},
		Capabilities: []string{*capURI},
		Zones:        []uint8{2},
	}
	if _, err := dht.NewRouter(node).Publish(context.Background(), desc); err == nil {
		fmt.Printf("PUBLISHED descriptor for %s\n", *capURI)
	} else {
		fmt.Fprintf(os.Stderr, "descriptor publish skipped: %v\n", err)
	}

	// Accept loop: each inbound session runs one provider task.
	for {
		sess := node.Accept()
		if sess == nil {
			return
		}
		go func(s *session.Session) {
			provider := tasks.NewProviderTask(excuseExecutor{})
			if err := tasks.RunProvider(s, provider); err != nil {
				fmt.Fprintf(os.Stderr, "task error: %v\n", err)
			}
			_, _ = s.Recv() // await requester CLOSE before teardown
			_ = s.Close(0, "done")
		}(sess)
	}
}
