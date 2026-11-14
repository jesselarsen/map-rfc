# Comment on inamprotocol/inam-protocol#50 (offline verifier in Go or Rust)

Posted as a pointer, not an offer to implement (development on MAP is
suspended).

---

A pointer for whoever picks this up. MAP, an Apache-2.0 accountability layer over A2A that I've stopped developing, already has most of the pieces this verifier needs, in both Rust and Go:

- **did:key / AgentID.** The same encoding INAM uses: multicodec `0xed 0x01` plus a 32-byte Ed25519 key, base58btc `z`. Rust: [`map-identity`](https://github.com/jesselarsen/map-rfc/tree/6cc324e/map-rs/crates/map-identity). Go: [`map-go/identity`](https://github.com/jesselarsen/map-rfc/tree/6cc324e/map-go/identity).
- **Strict Ed25519 verification** with small-order key rejection, which SPEC §2 requires. Rust uses `ed25519-dalek`'s `verify_strict`. The Go version is written from RFC 8032, with canonical-encoding and small-order checks and tests for non-canonical inputs.
- **RFC 8785 JCS**, written for A2A Agent Card signatures and checked against the a2a-python SDK's verifier. Rust: [`map-a2a/src/jcs.rs`](https://github.com/jesselarsen/map-rfc/blob/6cc324e/map-rs/adapters/a2a/src/jcs.rs). Go: [`map-go/a2a/jcs.go`](https://github.com/jesselarsen/map-rfc/blob/6cc324e/map-go/a2a/jcs.go). It has been tested against adversarial escape cases, for example lowercase hex in control-character escapes.

What's left is INAM-specific: rebuild the canonical content exactly as `sdk-js/src/core/receiptContent.ts` does, then check the vectors from `scripts/interop-vectors.ts` and `interop/canonical/vectors.json`. I won't be doing this myself, but the code is free to reuse.
