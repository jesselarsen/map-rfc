// Command map-a2a-requester is the Go MAP-over-A2A requester. Flags, checks
// and the final "MAP-DEMO-RESULT {json}" line match the Rust
// map-a2a-requester so the interop driver can mix implementations:
//
//	map-a2a-requester --providers http://127.0.0.1:8701,http://127.0.0.1:8702
//	    [--seed 1] [--zone 2] [--prompt "..."] [--plain]
//
// It runs one task across the providers in order, resuming on the next one
// from the last countersigned checkpoint whenever a provider fails
// (binding A2A-23), and checks that the final receipt is dual-signed, names
// what it resumed from, covers every Part received, and that the voucher
// watermarks sum to exactly the Parts delivered.
package main

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"os"
	"sort"
	"strings"

	"github.com/jesselarsen/map-rfc/map-go/a2a"
	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/meter"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
)

const capability = "map:cap:inference/text-generation"

func fail(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "[requester] FAIL: "+format+"\n", args...)
	os.Exit(1)
}

func result(v any) {
	b, _ := json.Marshal(v)
	fmt.Println("MAP-DEMO-RESULT " + string(b))
}

func plain(base, prompt string) {
	ep, err := a2a.Discover(base)
	if err != nil {
		fail("discover %s: %v", base, err)
	}
	msg := map[string]any{"messageId": "plain-1", "role": "ROLE_USER", "parts": []any{map[string]any{"text": prompt}}}
	res, err := a2a.RPC(ep.RPCURL, "SendMessage", map[string]any{"message": msg}, false)
	if err != nil {
		fail("%v", err)
	}
	task, _ := res["task"].(map[string]any)
	status, _ := task["status"].(map[string]any)
	arts, _ := task["artifacts"].([]any)
	n := 0
	if len(arts) > 0 {
		a0, _ := arts[0].(map[string]any)
		ps, _ := a0["parts"].([]any)
		n = len(ps)
	}
	if status["state"] != "TASK_STATE_COMPLETED" || task["metadata"] != nil {
		fail("plain client got state %v, metadata %v", status["state"], task["metadata"])
	}
	result(map[string]any{"mode": "plain", "state": status["state"], "parts": n})
}

func main() {
	providers := flag.String("providers", "http://127.0.0.1:8701,http://127.0.0.1:8702", "provider base URLs, in failover order")
	seed := flag.Int("seed", 1, "key seed byte (test keys)")
	zone := flag.Uint("zone", 2, "task zone label")
	prompt := flag.String("prompt", "Write a short status report", "task prompt")
	plainMode := flag.Bool("plain", false, "act as a plain A2A client against the first provider")
	flag.Parse()
	log.SetFlags(0)
	bases := strings.Split(*providers, ",")
	if *plainMode {
		plain(bases[0], *prompt)
		return
	}
	key := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{byte(*seed)}, 32))

	// The payment-rail stand-in, as in the Rust requester: one MPP-Tempo
	// session per provider, per-token pricing in a 6-decimal stablecoin.
	asset := meter.ERC20("eip155:8453", "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913")
	rates := []meter.Rate{
		{Unit: meter.UnitTokenIn, Price: meter.AssetAmount{Asset: asset, Amount: meter.Amount{Exponent: -6, Mantissa: 2}}},
		{Unit: meter.UnitTokenOut, Price: meter.AssetAmount{Asset: asset, Amount: meter.Amount{Exponent: -6, Mantissa: 10}}},
	}
	type paid struct {
		usage  core.Usage
		amount uint64
	}
	ledger := map[string]paid{}
	pay := func(p identity.AgentID, u core.Usage) []byte {
		amt, err := meter.Billed(1, rates, u)
		if err != nil {
			fail("billing: %v", err)
		}
		ch := sha256.Sum256(p[:])
		ledger[p.Multibase()] = paid{u, amt.Amount.Mantissa}
		return meter.DigestMPPTempo(ch[:], amt.Amount, "map-demo", "tempo")
	}

	var resume *a2a.Resume
	var finished *a2a.Outcome
	var hops []map[string]any
	for _, base := range bases {
		ep, err := a2a.Discover(base)
		if err != nil {
			log.Printf("[requester] skip %s: %v", base, err)
			continue
		}
		log.Printf("[requester] task on %s (%s)", base, ep.Agent.Multibase())
		out := a2a.RunTask(ep, key, *prompt, capability, uint8(*zone), resume, pay)
		hop := map[string]any{"provider": base, "a2aTaskId": out.A2ATaskID, "acked": nil, "error": nil}
		if out.Acked != nil {
			hop["acked"] = out.Acked.TokensOut
		}
		if out.Err != nil {
			hop["error"] = out.Err.Error()
		}
		hops = append(hops, hop)
		if out.Receipt != nil {
			finished = &out
			break
		}
		log.Printf("[requester] %s stopped: %v", base, out.Err)
		if out.Task != nil {
			if _, err := out.Task.ResumePackage(); err == nil {
				resume = &a2a.Resume{From: out.Task, FromA2ATaskID: out.A2ATaskID}
			}
		}
	}
	if finished == nil {
		fail("no provider completed the task")
	}
	r, err := receipt.Decode(finished.Receipt)
	if err != nil {
		fail("receipt: %v", err)
	}
	v, err := r.Verify()
	if err != nil {
		fail("receipt does not verify: %v", err)
	}
	if !v.Dual {
		fail("receipt is not dual-signed")
	}
	root := finished.Task.TranscriptRoot()
	if v.Core.TranscriptRoot == nil || *v.Core.TranscriptRoot != root {
		fail("receipt transcript root differs from the Parts received")
	}
	if (resume != nil) != (v.Core.ResumedFrom != nil) {
		fail("receipt resumed-from does not match what happened")
	}
	delivered := uint64(finished.Task.PartsReceived())
	var tokens, atomic uint64
	names := make([]string, 0, len(ledger))
	for k, p := range ledger {
		tokens += p.usage.TokensOut
		atomic += p.amount
		names = append(names, k)
	}
	if tokens != delivered {
		fail("voucher watermarks sum to %d Parts but %d were delivered", tokens, delivered)
	}
	sort.Strings(names)
	var vouchers []map[string]any
	for _, k := range names {
		vouchers = append(vouchers, map[string]any{"provider": k, "tokensOut": ledger[k].usage.TokensOut, "cumulativeAtomic": ledger[k].amount})
	}
	hexOrNil := func(b []byte) any {
		if b == nil {
			return nil
		}
		return hex.EncodeToString(b)
	}
	result(map[string]any{
		"mode": "map", "impl": "go", "dualSigned": v.Dual,
		"resumedFrom": hexOrNil(v.Core.ResumedFrom), "voucherDigestInReceipt": hexOrNil(v.Core.PaymentRef),
		"transcriptRoot": hex.EncodeToString(root[:]), "partsDelivered": delivered,
		"vouchers": vouchers, "voucherTotalAtomic": atomic, "hops": hops,
	})
}
