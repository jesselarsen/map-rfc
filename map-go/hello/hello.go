// Package hello implements the negotiation transcript hash and channel
// binding of draft-map-core-01 §6.2 / §7.3 (MAP-SUITE-1: SHA-256, HKDF).
package hello

import (
	"crypto/hkdf"
	"crypto/sha256"
)

// TranscriptHash computes TH = H("MAPv1-transcript" || HELLO-envelope-bytes
// || HELLO_ACK-envelope-bytes) (core §6.2), over the exact envelope bytes.
func TranscriptHash(helloEnv, helloAckEnv []byte) [32]byte {
	h := sha256.New()
	h.Write([]byte("MAPv1-transcript"))
	h.Write(helloEnv)
	h.Write(helloAckEnv)
	var th [32]byte
	copy(th[:], h.Sum(nil))
	return th
}

// ChannelBinding derives CB = HKDF(TH, transport-exporter, "map cb", 32)
// (core §7.3). TH is the HKDF salt; the transport exporter is the IKM.
func ChannelBinding(th [32]byte, transportExporter []byte) ([32]byte, error) {
	var cb [32]byte
	out, err := hkdf.Key(sha256.New, transportExporter, th[:], "map cb", 32)
	if err != nil {
		return cb, err
	}
	copy(cb[:], out)
	return cb, nil
}
