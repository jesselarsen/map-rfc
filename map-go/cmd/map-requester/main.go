// Command map-requester is the Go demo task requester. It dials a MAP provider
// over libp2p/QUIC, runs one task for the given capability, and prints the
// streamed output plus the verified transcript root (proving the cross-impl
// transcript commitment).
//
// Usage:
//
//	map-requester --seed <hex> --addr "<provider-addr>/p2p/<provider-peerid>" \
//	              --cap map:cap:demo/excuse --input "why are you late?"
package main

import (
	"context"
	"crypto/ed25519"
	"encoding/hex"
	"flag"
	"fmt"
	"os"

	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/net"
	"github.com/jesselarsen/map-rfc/map-go/session"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
	"github.com/libp2p/go-libp2p/core/peer"
)

func die(scenario string, err error) {
	fmt.Printf("FAIL %s: %v\n", scenario, err)
	os.Exit(1)
}

func main() {
	seedHex := flag.String("seed", "", "32-byte hex ed25519 seed")
	addr := flag.String("addr", "", "provider dial multiaddr (<addr>/p2p/<peerid>)")
	capURI := flag.String("cap", "map:cap:demo/excuse", "capability URI requested")
	input := flag.String("input", "the demo", "inline task input")
	direct := flag.Bool("direct", false, "skip OFFER/QUOTE, send ASSIGN directly")
	flag.Parse()

	if *seedHex == "" || *addr == "" {
		die("request", fmt.Errorf("--seed and --addr required"))
	}
	seed, err := hex.DecodeString(*seedHex)
	if err != nil || len(seed) != 32 {
		die("request", fmt.Errorf("bad --seed"))
	}

	// Extract the expected peer id from the /p2p/ suffix.
	var peerID string
	if i := lastIndex(*addr, "/p2p/"); i >= 0 {
		peerID = (*addr)[i+len("/p2p/"):]
	}
	expected, err := agentIDFromPeerString(peerID)
	if err != nil {
		die("request", fmt.Errorf("bad /p2p/ peer id: %w", err))
	}

	hello := session.Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 262144}
	node, err := net.New(seed, hello, "")
	if err != nil {
		die("request", err)
	}
	defer node.Close()

	sess, err := node.Dial(context.Background(), *addr, expected)
	if err != nil {
		die("request", err)
	}

	var task tasks.TaskID
	task[0] = 0x01
	req := tasks.NewRequesterTask(task, *capURI, tasks.TaskConstraints{}, []byte(*input))

	ver, root, err := tasks.RunRequester(sess, req, *direct)
	if err != nil {
		die("request", err)
	}
	fmt.Printf("OK result: %s\n", ver.Capability)
	fmt.Printf("OK transcript-root: %x\n", root)
	fmt.Printf("OK chunks: %d credits-granted: %d\n", req.ChunksReceived(), req.CreditsGranted())
}

func lastIndex(s, sub string) int {
	for i := len(s) - len(sub); i >= 0; i-- {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}

// agentIDFromPeerString decodes a libp2p peer-id string into a MAP AgentID,
// asserting the ed25519 key recovers (FR-3).
func agentIDFromPeerString(s string) (identity.AgentID, error) {
	pid, err := peer.Decode(s)
	if err != nil {
		return identity.AgentID{}, err
	}
	pub, err := pid.ExtractPublicKey()
	if err != nil {
		return identity.AgentID{}, err
	}
	raw, err := pub.Raw()
	if err != nil {
		return identity.AgentID{}, err
	}
	return identity.FromPub(ed25519.PublicKey(raw))
}
