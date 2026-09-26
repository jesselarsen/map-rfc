package a2a

// The deterministic demo workload and the usage both parties derive from
// it (mirrors map-a2a::workload): a successor provider reproduces the same
// output, so resumption by replay (layer §7.4) is observable, and requester
// and provider compute usage independently for the tolerance check.

import (
	"crypto/sha256"
	"fmt"
	"strings"
	"time"

	"github.com/jesselarsen/map-rfc/map-go/core"
)

// Generate returns n output words derived from the prompt: w001-<6 hex>...
// The hash input is prompt || 0x00 || decimal(i), as in Rust.
func Generate(prompt string, n int) []string {
	out := make([]string, n)
	for i := 1; i <= n; i++ {
		in := append([]byte(prompt), 0)
		in = append(in, []byte(fmt.Sprint(i))...)
		h := sha256.Sum256(in)
		out[i-1] = fmt.Sprintf("w%03d-%02x%02x%02x", i, h[0], h[1], h[2])
	}
	return out
}

// ObservedUsage: one input token per prompt word, one output token per Part.
func ObservedUsage(promptWords, parts uint64) core.Usage {
	return core.Usage{TokensIn: promptWords, TokensOut: parts}
}

// PromptWords counts whitespace-separated words.
func PromptWords(prompt string) uint64 { return uint64(len(strings.Fields(prompt))) }

// NowMs is the wall clock in ms.
func NowMs() uint64 { return uint64(time.Now().UnixMilli()) }
