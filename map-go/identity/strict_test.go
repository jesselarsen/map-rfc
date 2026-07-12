package identity

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"
	"strings"
	"testing"
)

func testKey() ed25519.PrivateKey {
	seed := sha256.Sum256([]byte("MAP-TEST-VECTOR-KEY-A"))
	return ed25519.NewKeyFromSeed(seed[:])
}

func TestVerifyStrictAcceptsValid(t *testing.T) {
	priv := testKey()
	msg := []byte("strict-verify positive case")
	sig := ed25519.Sign(priv, msg)
	if err := VerifyStrict(priv.Public().(ed25519.PublicKey), msg, sig); err != nil {
		t.Fatalf("valid signature rejected: %v", err)
	}
}

// FR-4: reject a NON-CANONICAL point encoding. y = p (= 2^255-19, LE
// "edff…ff7f") encodes the same point as y = 0 — RFC 8032 §5.1.3 decoding
// requires y < p, so this encoding is non-canonical and MUST be rejected
// even though a lax decoder would accept it (mod-p reduction).
func TestVerifyStrictRejectsNonCanonicalKey(t *testing.T) {
	nonCanonical, _ := hex.DecodeString("edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f")
	err := VerifyStrict(ed25519.PublicKey(nonCanonical), []byte("m"), make([]byte, 64))
	if err == nil || !strings.Contains(err.Error(), "non-canonical") {
		t.Fatalf("non-canonical public key not rejected: %v", err)
	}
}

func TestVerifyStrictRejectsNonCanonicalR(t *testing.T) {
	priv := testKey()
	msg := []byte("m")
	sig := ed25519.Sign(priv, msg)
	bad := append([]byte(nil), sig...)
	// Overwrite R with the non-canonical y = p encoding.
	r, _ := hex.DecodeString("edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f")
	copy(bad[:32], r)
	err := VerifyStrict(priv.Public().(ed25519.PublicKey), msg, bad)
	if err == nil || !strings.Contains(err.Error(), "non-canonical R") {
		t.Fatalf("non-canonical R not rejected: %v", err)
	}
}

// FR-4: reject small-order A (and R). RFC 8032 admits torsion points;
// verify_strict does not.
func TestVerifyStrictRejectsSmallOrder(t *testing.T) {
	for i, p := range smallOrderPoints {
		err := VerifyStrict(ed25519.PublicKey(p), []byte("m"), make([]byte, 64))
		if err == nil || !strings.Contains(err.Error(), "small-order") {
			t.Errorf("small-order point %d not rejected as such: %v", i, err)
		}
	}
	priv := testKey()
	sig := ed25519.Sign(priv, []byte("m"))
	bad := append([]byte(nil), sig...)
	copy(bad[:32], smallOrderPoints[0]) // R = identity
	err := VerifyStrict(priv.Public().(ed25519.PublicKey), []byte("m"), bad)
	if err == nil || !strings.Contains(err.Error(), "small-order R") {
		t.Errorf("small-order R not rejected: %v", err)
	}
}

func TestVerifyStrictRejectsBadSig(t *testing.T) {
	priv := testKey()
	sig := ed25519.Sign(priv, []byte("m"))
	sig[63] ^= 0x01
	if err := VerifyStrict(priv.Public().(ed25519.PublicKey), []byte("m"), sig); err == nil {
		t.Fatal("tampered signature accepted")
	}
}

func TestPointCanonicalBoundary(t *testing.T) {
	// y = p-1 ("ecff…ff7f") is canonical; y = p ("edff…ff7f") is not.
	pm1, _ := hex.DecodeString("ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f")
	p, _ := hex.DecodeString("edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f")
	if !pointCanonical(pm1) {
		t.Error("y = p-1 must be canonical")
	}
	if pointCanonical(p) {
		t.Error("y = p must be non-canonical")
	}
	// Sign bit does not affect canonicality of y.
	pm1[31] |= 0x80
	if !pointCanonical(pm1) {
		t.Error("sign bit must not affect y canonicality")
	}
}
