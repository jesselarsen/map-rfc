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
	"github.com/jesselarsen/map-rfc/map-go/pay"
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
	payFlag := flag.Bool("pay", false, "pay the provider's prepaid quote before running the task")
	operatorHex := flag.String("operator-seed", "", "32-byte hex seed of the credit-ledger operator (with --pay)")
	budget := flag.Int64("budget", 0, "max cents willing to prepay (with --pay); 0 = unlimited")
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

	// Prepaid gate (pay §5.2.2): pay the provider's terms-1 quote BEFORE the
	// task, so a priced provider does no work until settlement.
	if *payFlag {
		if len(*operatorHex) != 64 {
			die("request", fmt.Errorf("--pay requires --operator-seed <32-byte hex>"))
		}
		opSeed, e := hex.DecodeString(*operatorHex)
		if e != nil || len(opSeed) != 32 {
			die("request", fmt.Errorf("bad --operator-seed"))
		}
		myAID, e := identity.FromPub(ed25519.NewKeyFromSeed(seed).Public().(ed25519.PublicKey))
		if e != nil {
			die("request", e)
		}
		if e := payProvider(sess, task, myAID, expected, opSeed, *budget); e != nil {
			die("request", e)
		}
	}

	req := tasks.NewRequesterTask(task, *capURI, tasks.TaskConstraints{}, []byte(*input))
	ver, root, err := tasks.RunRequester(sess, req, *direct)
	if err != nil {
		die("request", err)
	}
	fmt.Printf("OK result: %s\n", ver.Capability)
	fmt.Printf("OK transcript-root: %x\n", root)
	fmt.Printf("OK chunks: %d credits-granted: %d\n", req.ChunksReceived(), req.CreditsGranted())
}

// payProvider runs the terms-1 prepaid payer exchange over the dialed session,
// hosting the in-process credit-ledger operator (pre-credited to --budget). It
// refuses cleanly, before agreeing and before any money moves, if the quoted
// total exceeds the budget.
func payProvider(sess *session.Session, task tasks.TaskID, me, provider identity.AgentID, opSeed []byte, budget int64) error {
	inc, err := sess.Recv() // PAY_QUOTE
	if err != nil {
		return fmt.Errorf("no prepaid quote (provider free/unreachable?): %w", err)
	}
	if inc.MsgType != pay.MsgPayQuote {
		return fmt.Errorf("expected PAY_QUOTE, got %#06x", inc.MsgType)
	}
	q, err := pay.PayQuoteFromValue(inc.Content)
	if err != nil {
		return fmt.Errorf("bad quote: %w", err)
	}
	priced := int64(0)
	if q.Total != nil {
		priced = q.Total.Amount.Mantissa
	}
	if budget > 0 && priced > budget {
		return fmt.Errorf("OVER-BUDGET: quote %d cents > budget %d cents — refusing, no work, no payment", priced, budget)
	}
	credit := priced
	if budget > 0 {
		credit = budget
	}
	op, err := pay.NewLedgerOperator(opSeed, -2)
	if err != nil {
		return err
	}
	op.Credit(me, credit)
	rp := pay.NewPrepaidRequesterPayment([32]byte(task), me, provider)

	aout := rp.OnQuote(inc.Content, inc.Raw, 1)
	if len(aout) != 1 || aout[0].MsgType != pay.MsgPayAgree {
		return fmt.Errorf("quote rejected")
	}
	if _, err := sess.Send(aout[0].MsgType, aout[0].Content); err != nil {
		return err
	}
	iinc, err := sess.Recv() // PAY_INVOICE
	if err != nil {
		return err
	}
	pout := rp.OnInvoice(iinc.Content, op, 1)
	if len(pout) != 1 || pout[0].MsgType != pay.MsgPayProof {
		return fmt.Errorf("invoice rejected / could not pay")
	}
	if _, err := sess.Send(pout[0].MsgType, pout[0].Content); err != nil {
		return err
	}
	rinc, err := sess.Recv() // PAY_RECEIPT
	if err != nil {
		return err
	}
	rp.OnReceipt(rinc.Content)
	rcpts := rp.Receipts()
	if len(rcpts) != 1 {
		return fmt.Errorf("no receipt received")
	}
	fmt.Printf("OK paid: %d cents (iso4217:USD)\n", priced)
	fmt.Printf("OK receipt: %x\n", rcpts[0].InvoiceID)
	return nil
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
