// Command map-a2a-provider is the Go MAP-over-A2A provider. Its flags and
// behavior match the Rust map-a2a-provider so the interop driver can mix
// implementations:
//
//	map-a2a-provider --listen 127.0.0.1:8701 --seed 2 [--parts 24]
//	    [--checkpoint-every 4] [--zones 1,2] [--fail-after-parts N]
package main

import (
	"bytes"
	"crypto/ed25519"
	"flag"
	"log"
	"net/http"
	"strconv"
	"strings"

	"github.com/jesselarsen/map-rfc/map-go/a2a"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

const capability = "map:cap:inference/text-generation"

func main() {
	listen := flag.String("listen", "127.0.0.1:8701", "listen address")
	seed := flag.Int("seed", 2, "key seed byte (test keys)")
	parts := flag.Int("parts", 24, "Parts per task")
	every := flag.Uint64("checkpoint-every", 4, "checkpoint cadence in Parts")
	zonesFlag := flag.String("zones", "1,2", "accepted zone labels")
	failAfter := flag.Uint64("fail-after-parts", 0, "crash after producing this many Parts (0 = never)")
	flag.Parse()
	log.SetPrefix("")
	log.SetFlags(0)

	var zones []uint8
	for _, z := range strings.Split(*zonesFlag, ",") {
		if n, err := strconv.Atoi(strings.TrimSpace(z)); err == nil {
			zones = append(zones, uint8(n))
		}
	}
	key := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{byte(*seed)}, 32))
	agent, _ := identity.FromPub(key.Public().(ed25519.PublicKey))
	params := a2a.CardParams{
		Name:        "MAP demo provider " + strconv.Itoa(*seed) + " (Go)",
		Description: "Deterministic text generator with MAP accountability",
		Version:     "0.1.0",
		JSONRPCURL:  "http://" + *listen + "/a2a",
		Skills: []a2a.Skill{{ID: "text-generation", Name: "Text generation",
			Description: "Generates a deterministic word stream from a prompt", Tags: []string{"text", "demo"}}},
		SupportedZones:          zones,
		CheckpointIntervalParts: *every,
		AcceptedVoucherRails:    []string{"pay:mpp-tempo"},
	}
	card, _, err := a2a.SignCard(a2a.BuildCard(agent, params), key)
	if err != nil {
		log.Fatalf("[provider] card: %v", err)
	}
	n := *parts
	p := a2a.NewProvider(a2a.ServerConfig{
		Key: key, Card: card, Capability: capability, SupportedZones: zones,
		CheckpointEvery: *every, FailAfterParts: *failAfter,
		Workload: func(prompt string) []string { return a2a.Generate(prompt, n) },
	})
	log.Printf("[provider] %s listening on http://%s (Go)", agent.Multibase(), *listen)
	log.Fatal(http.ListenAndServe(*listen, p))
}
