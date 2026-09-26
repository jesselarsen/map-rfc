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
	"crypto/ed25519"
	"encoding/hex"
	"fmt"
	"net"
	"os"
	"time"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/envelope"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/session"
)

const testMsgType = 0xF000

type flags struct {
	role     string
	addr     string
	seed     []byte
	peerSeed []byte
	scenario string
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
		case "--scenario":
			f.scenario = val()
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
	return cfg
}

func listenCfg(f flags) session.SessionConfig {
	return session.SessionConfig{Keypair: key(f.seed), ExpectedPeer: aidOf(key(f.peerSeed)), Hello: stdHello()}
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
	case "echo":
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
