// Command map-interop is the Go peer for the live Rust<->Go interop matrix
// (TASK-020). It runs ONE scenario in a --role of dial or listen and prints
// exactly one final line: "OK <scenario>" or "FAIL <scenario>: <detail>",
// exiting 0 or 1. A listener additionally prints "LISTENING <addr>" once bound
// so the driver can hand the port to the dialer.
//
// The scenario contract is shared verbatim with the Rust peer (map-cli
// interop); see interop/MATRIX.md for the table it feeds.
package main

import (
	"context"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net"
	"os"
	"time"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	mapnet "github.com/jesselarsen/map-rfc/map-go/net"
	"github.com/jesselarsen/map-rfc/map-go/pay"
	"github.com/jesselarsen/map-rfc/map-go/session"
	"github.com/jesselarsen/map-rfc/map-go/tasks"
)

const testMsgType = 0xF000

type flags struct {
	role         string
	addr         string
	seed         []byte
	peerSeed     []byte
	issuerSeed   []byte // optional; delegated scenario
	operatorSeed []byte // optional; pay-* scenarios (credit-ledger operator)
	scenario     string
	transport    string // "tcp" (default) or "libp2p"
}

func die(scenario string, err error) {
	fmt.Printf("FAIL %s: %v\n", scenario, err)
	os.Exit(1)
}

func main() {
	f, err := parseArgs(os.Args[1:])
	if err != nil {
		fmt.Printf("FAIL ?: %v\n", err)
		os.Exit(2)
	}
	if err := run(f); err != nil {
		die(f.scenario, err)
	}
	fmt.Printf("OK %s\n", f.scenario)
}

func parseArgs(args []string) (flags, error) {
	var f flags
	for i := 0; i < len(args); i++ {
		val := func() string {
			i++
			if i < len(args) {
				return args[i]
			}
			return ""
		}
		switch args[i] {
		case "--role":
			f.role = val()
		case "--addr":
			f.addr = val()
		case "--seed":
			f.seed = mustHex(val())
		case "--peer-seed":
			f.peerSeed = mustHex(val())
		case "--issuer-seed":
			f.issuerSeed = mustHex(val())
		case "--operator-seed":
			f.operatorSeed = mustHex(val())
		case "--scenario":
			f.scenario = val()
		case "--transport":
			f.transport = val()
		default:
			return f, fmt.Errorf("unknown flag %q", args[i])
		}
	}
	if f.role == "" || f.addr == "" || len(f.seed) != 32 || len(f.peerSeed) != 32 || f.scenario == "" {
		return f, fmt.Errorf("missing/invalid required flags")
	}
	return f, nil
}

func mustHex(s string) []byte {
	b, err := hex.DecodeString(s)
	if err != nil {
		return nil
	}
	return b
}

func key(seed []byte) ed25519.PrivateKey { return ed25519.NewKeyFromSeed(seed) }

func aidOf(k ed25519.PrivateKey) identity.AgentID {
	a, err := identity.FromPub(k.Public().(ed25519.PublicKey))
	if err != nil {
		panic(err)
	}
	return a
}

func stdHello() session.Hello {
	return session.Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 262144}
}

func run(f flags) error {
	if f.transport == "libp2p" {
		return runLibp2p(f)
	}
	if f.role == "listen" {
		return runListen(f)
	}
	return runDial(f)
}

// --- transport setup -------------------------------------------------------

func dialTransport(f flags) (*session.TCPTransport, error) {
	conn, err := net.DialTimeout("tcp", f.addr, 5*time.Second)
	if err != nil {
		return nil, err
	}
	_ = conn.SetDeadline(time.Now().Add(5 * time.Second))
	// Exporter order is dialer(initiator) || listener(responder).
	return session.NewTCPTransport(conn, aidOf(key(f.seed)), aidOf(key(f.peerSeed)), 262144), nil
}

func listenSetup(f flags) (net.Listener, error) {
	ln, err := net.Listen("tcp", f.addr)
	if err != nil {
		return nil, err
	}
	fmt.Printf("LISTENING %s\n", ln.Addr().String())
	return ln, nil
}

func acceptTransport(ln net.Listener, f flags) (*session.TCPTransport, error) {
	conn, err := ln.Accept()
	if err != nil {
		return nil, err
	}
	_ = conn.SetDeadline(time.Now().Add(5 * time.Second))
	// Exporter order is dialer(initiator=peer) || listener(responder=me).
	return session.NewTCPTransport(conn, aidOf(key(f.peerSeed)), aidOf(key(f.seed)), 262144), nil
}

func dialCfg(f flags) session.SessionConfig {
	cfg := session.SessionConfig{Keypair: key(f.seed), ExpectedPeer: aidOf(key(f.peerSeed)), Hello: stdHello()}
	if f.scenario == "versiondisjoint" {
		cfg.Hello = session.Hello{Versions: []uint16{0x0200}, Suites: []uint16{1}, MaxMessageSize: 262144}
	}
	if f.scenario == "delegated" && len(f.issuerSeed) == 32 {
		cfg.DelegationChain = [][]byte{delegationCert(key(f.issuerSeed), key(f.seed))}
	}
	return cfg
}

func listenCfg(f flags) session.SessionConfig {
	return session.SessionConfig{Keypair: key(f.seed), ExpectedPeer: aidOf(key(f.peerSeed)), Hello: stdHello()}
}

func delegationCert(issuer, subject ed25519.PrivateKey) []byte {
	body := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(aidOf(issuer).Bytes())},
		{K: cbor.Uint(2), V: cbor.Bytes(aidOf(subject).Bytes())},
		{K: cbor.Uint(3), V: cbor.Uint(0)},
		{K: cbor.Uint(4), V: cbor.Uint(1 << 62)},
		{K: cbor.Uint(5), V: cbor.Array{cbor.Text("map:cap:inference/text-generation")}},
		{K: cbor.Uint(7), V: cbor.Uint(1)},
	}
	env, err := envelope.Seal(issuer, 0x0502, make([]byte, 16), 0, body, nil)
	if err != nil {
		panic(err)
	}
	return env
}

// craftFrame seals an in-session envelope with a chosen seq/session-id under
// the session's channel binding (harness use).
func craftFrame(s *session.Session, f flags, seq uint64, sid []byte, content cbor.Value) []byte {
	recipient := aidOf(key(f.peerSeed))
	cb := s.ChannelBinding()
	body := envelope.Body{
		Version:   uint64(s.Negotiated().Version),
		MsgType:   testMsgType,
		MsgID:     []byte{0xC0, byte(seq), 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16},
		Timestamp: uint64(time.Now().UnixMilli()),
		Recipient: &recipient,
		SessionID: sid,
		Seq:       &seq,
		Content:   content,
	}
	env, err := envelope.SealBody(key(f.seed), body, cb[:])
	if err != nil {
		panic(err)
	}
	return env
}

func echoContent() cbor.Value { return cbor.Map{{K: cbor.Uint(1), V: cbor.Text("echo")}} }

// --- dialer scenarios ------------------------------------------------------

func runDial(f flags) error {
	switch f.scenario {
	case "versiondisjoint":
		tr, err := dialTransport(f)
		if err != nil {
			return err
		}
		_, err = session.Initiate(dialCfg(f), tr, session.SystemClock{}, session.RandMsgIDs{})
		return expectPeerCode(err, session.CodeUnsupportedVersion)
	case "relay":
		return runDialRelay(f)
	}

	tr, err := dialTransport(f)
	if err != nil {
		return err
	}
	s, err := session.Initiate(dialCfg(f), tr, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		return fmt.Errorf("handshake: %w", err)
	}
	switch f.scenario {
	case "echo", "delegated":
		if _, err := s.Send(testMsgType, echoContent()); err != nil {
			return err
		}
		got, err := s.Recv()
		if err != nil {
			return fmt.Errorf("recv echo: %w", err)
		}
		if !echoOK(got.Content) {
			return fmt.Errorf("echo mismatch")
		}
		return s.Close(0, "done")
	case "replay":
		raw, err := s.Send(testMsgType, echoContent())
		if err != nil {
			return err
		}
		if err := s.SendRaw(raw); err != nil {
			return err
		}
		return expectPeerCode(recvErr(s), session.CodeReplay)
	case "seqgap":
		// Skip seq 0: first in-session frame carries seq 1.
		frame := craftFrame(s, f, 1, s.SessionID(), echoContent())
		if err := s.SendRaw(frame); err != nil {
			return err
		}
		return expectPeerCode(recvErr(s), session.CodeReplay)
	case "tamper":
		raw, err := s.Send(testMsgType, echoContent())
		if err != nil {
			return err
		}
		bad := append([]byte(nil), raw...)
		bad[len(bad)-1] ^= 0xFF
		if err := s.SendRaw(bad); err != nil {
			return err
		}
		return expectPeerCode(recvErr(s), session.CodeBadSignature)
	case "wrongsid":
		wrong := append([]byte(nil), s.SessionID()...)
		wrong[0] ^= 0xFF
		frame := craftFrame(s, f, 0, wrong, echoContent())
		if err := s.SendRaw(frame); err != nil {
			return err
		}
		return expectPeerCode(recvErr(s), session.CodeStateViolation)
	default:
		return fmt.Errorf("unknown scenario")
	}
}

// runDialRelay opens two sessions: it captures a frame bound to session A's CB,
// then replays it into a fresh session B, which MUST reject it (2000) because
// the channel binding differs (the live §7.3 anti-relay property).
func runDialRelay(f flags) error {
	trA, err := dialTransport(f)
	if err != nil {
		return err
	}
	sa, err := session.Initiate(dialCfg(f), trA, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		return fmt.Errorf("handshake A: %w", err)
	}
	rawA, err := sa.Send(testMsgType, echoContent())
	if err != nil {
		return err
	}
	_ = sa.Close(0, "done")

	trB, err := dialTransport(f)
	if err != nil {
		return err
	}
	sb, err := session.Initiate(dialCfg(f), trB, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		return fmt.Errorf("handshake B: %w", err)
	}
	if err := sb.SendRaw(rawA); err != nil {
		return err
	}
	return expectPeerCode(recvErr(sb), session.CodeBadSignature)
}

// --- listener scenarios ----------------------------------------------------

func runListen(f flags) error {
	ln, err := listenSetup(f)
	if err != nil {
		return err
	}
	defer ln.Close()

	if f.scenario == "relay" {
		return runListenRelay(f, ln)
	}

	tr, err := acceptTransport(ln, f)
	if err != nil {
		return err
	}
	s, err := session.Respond(listenCfg(f), tr, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		// versiondisjoint: Respond fails with the handshake error — expected.
		if f.scenario == "versiondisjoint" {
			return expectLocalCode(err, session.CodeUnsupportedVersion)
		}
		return fmt.Errorf("respond: %w", err)
	}
	switch f.scenario {
	case "versiondisjoint":
		return fmt.Errorf("handshake unexpectedly succeeded")
	case "echo":
		return echoLoop(s)
	case "delegated":
		if s.AccountablePeer() != aidOf(key(f.issuerSeed)) {
			return fmt.Errorf("accountable peer != issuer root")
		}
		return echoLoop(s)
	case "replay":
		if _, err := s.Recv(); err != nil {
			return fmt.Errorf("first msg must be ok: %w", err)
		}
		return expectLocalCode(recvErr(s), session.CodeReplay)
	case "seqgap":
		return expectLocalCode(recvErr(s), session.CodeReplay)
	case "tamper":
		if _, err := s.Recv(); err != nil {
			return fmt.Errorf("good frame must be ok: %w", err)
		}
		return expectLocalCode(recvErr(s), session.CodeBadSignature)
	case "wrongsid":
		return expectLocalCode(recvErr(s), session.CodeStateViolation)
	default:
		return fmt.Errorf("unknown scenario")
	}
}

func runListenRelay(f flags, ln net.Listener) error {
	// Connection A: the victim session; receive one good message, then close.
	trA, err := acceptTransport(ln, f)
	if err != nil {
		return err
	}
	sa, err := session.Respond(listenCfg(f), trA, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		return fmt.Errorf("respond A: %w", err)
	}
	if _, err := sa.Recv(); err != nil {
		return fmt.Errorf("session A first msg: %w", err)
	}
	_, _ = sa.Recv() // drain peer CLOSE

	// Connection B: the relayed frame (bound to A's CB) MUST be rejected.
	trB, err := acceptTransport(ln, f)
	if err != nil {
		return err
	}
	sb, err := session.Respond(listenCfg(f), trB, session.SystemClock{}, session.RandMsgIDs{})
	if err != nil {
		return fmt.Errorf("respond B: %w", err)
	}
	return expectLocalCode(recvErr(sb), session.CodeBadSignature)
}

// --- helpers ---------------------------------------------------------------

func echoLoop(s *session.Session) error {
	for {
		m, err := s.Recv()
		if err != nil {
			if se, ok := err.(*session.SessionError); ok && se.Kind == "peer_closed" {
				return nil
			}
			return err
		}
		if _, err := s.Send(testMsgType, m.Content); err != nil {
			return err
		}
	}
}

func recvErr(s *session.Session) error {
	_, err := s.Recv()
	return err
}

func echoOK(v cbor.Value) bool {
	m, ok := v.(cbor.Map)
	if !ok || len(m) != 1 {
		return false
	}
	t, ok := m[0].V.(cbor.Text)
	return ok && t == "echo"
}

func expectPeerCode(err error, code uint16) error {
	se, ok := err.(*session.SessionError)
	if !ok {
		return fmt.Errorf("expected peer error %d, got %v", code, err)
	}
	if se.Kind != "peer_error" || se.Code != code {
		return fmt.Errorf("expected peer_error %d, got kind=%s code=%d", code, se.Kind, se.Code)
	}
	return nil
}

func expectLocalCode(err error, code uint16) error {
	se, ok := err.(*session.SessionError)
	if !ok || se.Code != code {
		return fmt.Errorf("expected local error %d, got %v", code, err)
	}
	return nil
}

// --- libp2p task scenarios ---------------------------------------------------
//
// These scenarios run the live task protocol (tasks §1-§13) over a real
// libp2p/QUIC session (core §10 / net). The provider and requester are both the
// Go implementation #2, proving impl #2's task runtime end-to-end over its own
// transport. The wire format is identical to Rust by construction (the
// transcript codec is cross-impl-verified in tasks/transcript_test.go), so the
// same scenarios run Rust<->Go once the Rust interop peer gains a libp2p
// task-mode (see ERRATA PENDING-ARCHITECT).

const libp2pCapURI = "map:cap:demo/excuse"

// scenarioExecutor emits a deterministic chunk set so a task's transcript root
// is reproducible. It stands in for a real capability handler.
type scenarioExecutor struct{ scenario string }

func (e scenarioExecutor) Execute(input []byte) []tasks.Chunk {
	var prompt string
	if len(input) > 0 {
		prompt = string(input)
	}
	switch e.scenario {
	case "task-stream":
		return []tasks.Chunk{{Seq: 0, Kind: tasks.ChunkOutputText, Content: []byte("Excuse: " + prompt)}}
	case "task-multichunk":
		var out []tasks.Chunk
		for i := 0; i < 20; i++ {
			out = append(out, tasks.Chunk{
				Seq:     uint64(i),
				Kind:    tasks.ChunkOutputText,
				Content: []byte(fmt.Sprintf("chunk-%d: %s", i, prompt)),
			})
		}
		return out
	case "task-transcript":
		return []tasks.Chunk{
			{Seq: 0, Kind: tasks.ChunkOutputText, Content: []byte("alpha " + prompt)},
			{Seq: 1, Kind: tasks.ChunkOutputByte, Content: []byte{0x01, 0x02, 0x03}},
			{Seq: 2, Kind: tasks.ChunkOutputText, Content: []byte("omega " + prompt)},
		}
	default:
		return []tasks.Chunk{{Seq: 0, Kind: tasks.ChunkOutputText, Content: []byte(prompt)}}
	}
}

func runLibp2p(f flags) error {
	if f.role == "listen" {
		return runLibp2pListen(f)
	}
	return runLibp2pDial(f)
}

func runLibp2pListen(f flags) error {
	hello := session.Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 262144}
	node, err := mapnet.New(f.seed, hello, "/ip4/127.0.0.1/udp/0/quic-v1")
	if err != nil {
		return err
	}
	defer node.Close()
	addr, err := node.ListenAddr()
	if err != nil {
		return err
	}
	fmt.Printf("LISTENING %s/p2p/%s\n", addr.String(), node.PeerID())

	sess := node.Accept()
	if sess == nil {
		return fmt.Errorf("accept closed")
	}
	provider := tasks.NewProviderTask(scenarioExecutor{scenario: f.scenario})
	if err := tasks.RunProvider(sess, provider); err != nil {
		return err
	}
	// pay-* scenarios: after the task's transcript is verified, run the pay
	// exchange over the same reliable stream (the provider is the payee).
	if isPayScenario(f.scenario) {
		if err := providerPayExchange(sess, f); err != nil {
			return err
		}
	}
	// Await the requester's CLOSE before tearing down, so the last frame is
	// fully flushed and the requester has verified before the stream closes.
	_, _ = sess.Recv()
	_ = sess.Close(0, "done")
	return nil
}

func runLibp2pDial(f flags) error {
	switch f.scenario {
	case "task-stream", "task-multichunk", "task-transcript", "pay-postpaid", "pay-metered":
	default:
		return fmt.Errorf("libp2p transport only supports task-*/pay-* scenarios, got %q", f.scenario)
	}
	expected := aidOf(key(f.peerSeed))
	hello := session.Hello{Versions: []uint16{0x0100}, Suites: []uint16{1}, MaxMessageSize: 262144}
	node, err := mapnet.New(f.seed, hello, "/ip4/127.0.0.1/udp/0/quic-v1")
	if err != nil {
		return err
	}
	defer node.Close()

	ctx := context.Background()
	sess, err := node.Dial(ctx, f.addr, expected)
	if err != nil {
		return err
	}
	tid := tasks.TaskID(sha256.Sum256([]byte(f.scenario)))
	r := tasks.NewRequesterTask(tid, libp2pCapURI, tasks.TaskConstraints{}, []byte("traffic"))
	ver, root, err := tasks.RunRequester(sess, r, false)
	if err != nil {
		return err
	}
	if ver == nil {
		return fmt.Errorf("task did not verify")
	}
	// pay-* scenarios: after the transcript verifies, run the pay exchange
	// over the same stream (the requester is the payer + ledger operator host).
	if isPayScenario(f.scenario) {
		if err := requesterPayExchange(sess, f); err != nil {
			return err
		}
	}
	_ = sess.Close(0, "done")
	fmt.Printf("OK transcript-root: %x\n", root)
	return nil
}

// --- pay scenarios (over libp2p, after the task transcript verifies) --------

const (
	payQuoteID = 0x10
	payExpiry  = uint64(1) << 62
	payNow     = uint64(1)
)

func isPayScenario(s string) bool { return s == "pay-postpaid" || s == "pay-metered" }

func id16(b byte) [16]byte { var x [16]byte; x[0] = b; return x }

// payErr extracts the wire code from an ERROR outgoing for a diagnostic.
func payErr(where string, out []tasks.Outgoing) error {
	if len(out) == 1 && out[0].MsgType == 0x0006 {
		if m, ok := out[0].Content.(cbor.Map); ok {
			for _, p := range m {
				if k, ok := p.K.(cbor.Uint); ok && uint64(k) == 1 {
					if c, ok := p.V.(cbor.Uint); ok {
						return fmt.Errorf("pay %s: ERROR %d", where, uint64(c))
					}
				}
			}
		}
	}
	return fmt.Errorf("pay %s: unexpected output", where)
}

func meteredQuote(task [32]byte) pay.PayQuote {
	terms := pay.TermsMetered
	total := pay.AssetAmount{Asset: "iso4217:USD", Amount: pay.Amount{Exponent: -6, Mantissa: 10_000_000}}
	tb := task
	micro := func(m int64) pay.AssetAmount {
		return pay.AssetAmount{Asset: "iso4217:USD", Amount: pay.Amount{Exponent: -6, Mantissa: m}}
	}
	return pay.PayQuote{
		QuoteID: id16(payQuoteID), PricingModel: 1,
		RateTable: []pay.RateEntry{{Unit: pay.UnitTokenIn, Amount: micro(3000)}, {Unit: pay.UnitTokenOut, Amount: micro(5000)}},
		Total:     &total,
		Methods:   []pay.MethodOffer{{URI: pay.CreditLedgerMethod, Params: []byte{}}},
		ExpiryMs:  payExpiry, Terms: &terms, TaskBinding: &tb,
	}
}

// providerPayExchange runs the payee side of a pay-* scenario.
func providerPayExchange(sess *session.Session, f flags) error {
	if len(f.operatorSeed) != 32 {
		return fmt.Errorf("pay scenario needs --operator-seed")
	}
	provAID := aidOf(key(f.seed))
	reqAID := aidOf(key(f.peerSeed))
	opAID := aidOf(key(f.operatorSeed))
	task := [32]byte(sha256.Sum256([]byte(f.scenario)))

	if f.scenario == "pay-metered" {
		q := meteredQuote(task)
		mp := pay.NewMeteredProviderPayment(provAID, reqAID, opAID, task, q)
		raw, err := sess.Send(pay.MsgPayQuote, q.ToValue())
		if err != nil {
			return err
		}
		mp.BindQuoteEnvelope(raw)
		inc, err := sess.Recv()
		if err != nil {
			return err
		}
		if out := mp.OnAgree(inc.Content); len(out) != 0 {
			return payErr("metered on_agree", out)
		}
		usages := []tasks.Usage{{TokensIn: 100, TokensOut: 40, WallMs: 1000}, {TokensIn: 200, TokensOut: 80, WallMs: 2000}}
		seqs := []uint64{4, 9}
		ids := []byte{0x21, 0x22}
		for i := 0; i < 2; i++ {
			for _, o := range mp.Bill(usages[i], seqs[i], id16(ids[i]), payNow) {
				if _, err := sess.Send(o.MsgType, o.Content); err != nil {
					return err
				}
			}
			inc, err := sess.Recv() // PAY_PROOF
			if err != nil {
				return err
			}
			rout := mp.OnProof(inc.Content, payNow)
			if len(rout) != 1 || rout[0].MsgType != pay.MsgPayReceipt {
				return payErr("metered on_proof", rout)
			}
			if _, err := sess.Send(rout[0].MsgType, rout[0].Content); err != nil {
				return err
			}
		}
		if len(mp.Receipts()) != 2 {
			return fmt.Errorf("metered: want 2 settlements, got %d", len(mp.Receipts()))
		}
		return nil
	}

	// pay-postpaid.
	pp := pay.NewProviderPayment(provAID, reqAID, opAID)
	total := pay.AssetAmount{Asset: "iso4217:USD", Amount: pay.Amount{Exponent: -2, Mantissa: 500}}
	qout := pp.IssueQuote(id16(payQuoteID), task, total, payExpiry)
	raw, err := sess.Send(qout[0].MsgType, qout[0].Content)
	if err != nil {
		return err
	}
	pp.BindQuoteEnvelope(raw)
	inc, err := sess.Recv() // PAY_AGREE
	if err != nil {
		return err
	}
	if out := pp.OnAgree(inc.Content); len(out) != 0 {
		return payErr("on_agree", out)
	}
	iout := pp.Invoice(id16(0x20), payNow)
	if _, err := sess.Send(iout[0].MsgType, iout[0].Content); err != nil {
		return err
	}
	inc, err = sess.Recv() // PAY_PROOF
	if err != nil {
		return err
	}
	rout := pp.OnProof(inc.Content, payNow)
	if len(rout) != 1 || rout[0].MsgType != pay.MsgPayReceipt {
		return payErr("on_proof", rout)
	}
	if _, err := sess.Send(rout[0].MsgType, rout[0].Content); err != nil {
		return err
	}
	if pp.State() != pay.ProvSettled {
		return fmt.Errorf("postpaid: provider not settled")
	}
	return nil
}

// requesterPayExchange runs the payer side (and hosts the credit-ledger
// operator, pre-credited) of a pay-* scenario.
func requesterPayExchange(sess *session.Session, f flags) error {
	if len(f.operatorSeed) != 32 {
		return fmt.Errorf("pay scenario needs --operator-seed")
	}
	myAID := aidOf(key(f.seed))
	provAID := aidOf(key(f.peerSeed))
	task := [32]byte(sha256.Sum256([]byte(f.scenario)))

	if f.scenario == "pay-metered" {
		op, err := pay.NewLedgerOperator(f.operatorSeed, -6)
		if err != nil {
			return err
		}
		op.Credit(myAID, 2_000_000)
		mr := pay.NewMeteredRequesterPayment(task, myAID, provAID)
		inc, err := sess.Recv() // PAY_QUOTE
		if err != nil {
			return err
		}
		aout := mr.OnQuote(inc.Content, inc.Raw, payNow)
		if len(aout) != 1 || aout[0].MsgType != pay.MsgPayAgree {
			return payErr("metered on_quote", aout)
		}
		if _, err := sess.Send(aout[0].MsgType, aout[0].Content); err != nil {
			return err
		}
		for i := 0; i < 2; i++ {
			cinc, err := sess.Recv() // CHUNK (kind-3 usage)
			if err != nil {
				return err
			}
			mr.OnChunk(cinc.Content)
			iinc, err := sess.Recv() // PAY_INVOICE
			if err != nil {
				return err
			}
			pout := mr.OnInvoice(iinc.Content, op, payNow)
			if len(pout) != 1 || pout[0].MsgType != pay.MsgPayProof {
				return payErr("metered on_invoice", pout)
			}
			if _, err := sess.Send(pout[0].MsgType, pout[0].Content); err != nil {
				return err
			}
			rinc, err := sess.Recv() // PAY_RECEIPT
			if err != nil {
				return err
			}
			mr.OnReceipt(rinc.Content)
		}
		if len(mr.Receipts()) != 2 {
			return fmt.Errorf("metered: want 2 receipts, got %d", len(mr.Receipts()))
		}
		return nil
	}

	// pay-postpaid.
	op, err := pay.NewLedgerOperator(f.operatorSeed, -2)
	if err != nil {
		return err
	}
	op.Credit(myAID, 1000)
	rp := pay.NewRequesterPayment(&task, myAID, provAID)
	inc, err := sess.Recv() // PAY_QUOTE
	if err != nil {
		return err
	}
	aout := rp.OnQuote(inc.Content, inc.Raw, payNow)
	if len(aout) != 1 || aout[0].MsgType != pay.MsgPayAgree {
		return payErr("on_quote", aout)
	}
	if _, err := sess.Send(aout[0].MsgType, aout[0].Content); err != nil {
		return err
	}
	iinc, err := sess.Recv() // PAY_INVOICE
	if err != nil {
		return err
	}
	pout := rp.OnInvoice(iinc.Content, op, payNow)
	if len(pout) != 1 || pout[0].MsgType != pay.MsgPayProof {
		return payErr("on_invoice", pout)
	}
	if _, err := sess.Send(pout[0].MsgType, pout[0].Content); err != nil {
		return err
	}
	rinc, err := sess.Recv() // PAY_RECEIPT
	if err != nil {
		return err
	}
	rp.OnReceipt(rinc.Content)
	if len(rp.ReceiptBytes()) == 0 {
		return fmt.Errorf("postpaid: no receipt received")
	}
	return nil
}
