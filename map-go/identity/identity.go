// Package identity implements MAP AgentIDs (draft-map-identity-01 §3)
// and the strict Ed25519 verification profile required by TASK-015 §FR-4.
package identity

import (
	"crypto/ed25519"
	"errors"
	"fmt"
)

// AgentID is a multicodec-prefixed public key (identity §3.1).
// For ed25519-pub the prefix is the varint 0xED 0x01, total 34 bytes.
type AgentID [34]byte

var errBadAgentID = errors.New("identity: not an ed25519-pub AgentID (want 34 bytes, prefix ed 01)")

// FromPub builds the AgentID for an Ed25519 public key (identity §3.1).
func FromPub(pub ed25519.PublicKey) (AgentID, error) {
	var a AgentID
	if len(pub) != ed25519.PublicKeySize {
		return a, fmt.Errorf("identity: bad public key length %d", len(pub))
	}
	a[0], a[1] = 0xED, 0x01
	copy(a[2:], pub)
	return a, nil
}

// Parse validates and copies a byte-form AgentID.
func Parse(b []byte) (AgentID, error) {
	var a AgentID
	if len(b) != 34 || b[0] != 0xED || b[1] != 0x01 {
		return a, errBadAgentID
	}
	copy(a[:], b)
	return a, nil
}

// Pub returns the raw Ed25519 public key carried in the AgentID
// (strip the 2-byte multicodec prefix; test-vectors "Verification procedure" step 3).
func (a AgentID) Pub() ed25519.PublicKey {
	return ed25519.PublicKey(a[2:])
}

// Bytes returns a copy of the 34-byte AgentID as a slice.
func (a AgentID) Bytes() []byte {
	b := make([]byte, len(a))
	copy(b, a[:])
	return b
}

// Multibase returns the text form: multibase base58btc (identity §3.1), "z6Mk...".
func (a AgentID) Multibase() string {
	return "z" + base58Encode(a[:])
}

// DID returns the did:key form (identity §3.1).
func (a AgentID) DID() string {
	return "did:key:" + a.Multibase()
}

// base58Encode implements Bitcoin-alphabet base58 (in-package per TASK-015 §FR-2).
func base58Encode(b []byte) string {
	const alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
	// Count leading zero bytes: each encodes as '1'.
	zeros := 0
	for zeros < len(b) && b[zeros] == 0 {
		zeros++
	}
	// Repeated division of the big-endian integer by 58.
	digits := make([]byte, 0, len(b)*2)
	num := append([]byte(nil), b...)
	for len(num) > 0 {
		rem := 0
		next := num[:0]
		for _, d := range num {
			acc := rem*256 + int(d)
			q := acc / 58
			rem = acc % 58
			if len(next) > 0 || q != 0 {
				next = append(next, byte(q))
			}
		}
		digits = append(digits, alphabet[rem])
		num = next
	}
	out := make([]byte, 0, zeros+len(digits))
	for i := 0; i < zeros; i++ {
		out = append(out, '1')
	}
	for i := len(digits) - 1; i >= 0; i-- {
		out = append(out, digits[i])
	}
	return string(out)
}
