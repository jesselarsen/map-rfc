package identity

import (
	"crypto/ed25519"
	"encoding/hex"
	"errors"
)

// VerifyStrict is the documented strict equivalent of ed25519-dalek's
// verify_strict, per TASK-015 §FR-4. Go's stdlib ed25519.Verify already
// enforces, per RFC 8032 §5.1.7:
//   - the scalar S is canonical (S < L; stdlib SetCanonicalBytes rejects
//     otherwise), and
//   - the cofactorless verification equation [S]B = R + [k]A over the
//     exact encodings received.
//
// What stdlib does NOT reject, and this function adds explicitly:
//  1. Non-canonical point encodings for A (the public key) and R (the
//     first 32 signature bytes). RFC 8032 §5.1.3 decoding recovers
//     y from the low 255 bits; an encoding with y >= p (p = 2^255-19)
//     is a second representation of the same point — a malleability
//     vector. We reject any encoding whose y, with the sign bit
//     cleared, is >= p.
//  2. Small-order (torsion) A or R. RFC 8032 admits them; a small-order
//     public key makes signatures transferable across messages, and a
//     small-order R interacts with cofactored verifiers. We reject the
//     8 canonical encodings of the order-1/2/4/8 torsion points
//     (the canonical subset of libsodium's blacklist; combined with
//     check 1, non-canonical torsion encodings are also unreachable).
//
// Together these match verify_strict semantics: canonical A/R/S,
// no torsion component in A or R, cofactorless equation.
func VerifyStrict(pub ed25519.PublicKey, message, sig []byte) error {
	if len(pub) != ed25519.PublicKeySize {
		return errors.New("identity: bad public key length")
	}
	if len(sig) != ed25519.SignatureSize {
		return errors.New("identity: bad signature length")
	}
	if !pointCanonical(pub) {
		return errors.New("identity: non-canonical public key encoding")
	}
	if !pointCanonical(sig[:32]) {
		return errors.New("identity: non-canonical R encoding")
	}
	if smallOrder(pub) {
		return errors.New("identity: small-order public key")
	}
	if smallOrder(sig[:32]) {
		return errors.New("identity: small-order R")
	}
	if !ed25519.Verify(pub, message, sig) {
		return errors.New("identity: signature verification failed")
	}
	return nil
}

// pointCanonical reports whether a 32-byte point encoding is canonical:
// the little-endian y coordinate (bit 255 = x sign, cleared) is < p,
// p = 2^255 - 19 (RFC 8032 §5.1.3 decoding domain).
func pointCanonical(enc []byte) bool {
	// p in little-endian: ed ff ff ... ff 7f
	var y [32]byte
	copy(y[:], enc)
	y[31] &= 0x7f
	// Compare y against p from the most significant byte down.
	for i := 31; i >= 0; i-- {
		var pb byte
		switch i {
		case 31:
			pb = 0x7f
		case 0:
			pb = 0xed
		default:
			pb = 0xff
		}
		if y[i] < pb {
			return true
		}
		if y[i] > pb {
			return false
		}
	}
	return false // y == p: non-canonical
}

// smallOrderPoints are the canonical encodings of the 8 points of the
// order-8 torsion subgroup of edwards25519 (source: libsodium
// ed25519_blacklisted_points, canonical entries; ed25519-dalek
// EdwardsPoint::is_small_order agrees on these).
var smallOrderPoints = func() [][]byte {
	hexes := []string{
		"0100000000000000000000000000000000000000000000000000000000000000", // identity (0, 1)
		"ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f", // (0, -1), order 2
		"0000000000000000000000000000000000000000000000000000000000000000", // y = 0, order 4
		"0000000000000000000000000000000000000000000000000000000000000080", // y = 0, x-sign 1, order 4
		"c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac037a", // order 8
		"c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac03fa", // order 8, x-sign 1
		"26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc05", // order 8
		"26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc85", // order 8, x-sign 1
	}
	pts := make([][]byte, len(hexes))
	for i, h := range hexes {
		b, err := hex.DecodeString(h)
		if err != nil {
			panic(err)
		}
		pts[i] = b
	}
	return pts
}()

func smallOrder(enc []byte) bool {
	for _, p := range smallOrderPoints {
		match := true
		for i := range p {
			if enc[i] != p[i] {
				match = false
				break
			}
		}
		if match {
			return true
		}
	}
	return false
}
