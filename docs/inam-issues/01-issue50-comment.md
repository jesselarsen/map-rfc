# Comment on inamprotocol/inam-protocol#50 (offline verifier in Go or Rust)

Post this as a comment on issue #50 before you write code, so the
maintainer can say whether they still want it and which language.

---

I'd like to take this one, in Rust, with a Go version to follow if useful.

I maintain a related protocol (MAP, an accountability layer over A2A) that
uses the same identity encoding: did:key over Ed25519, multicodec `0xed01`,
base58btc `z`. It also has RFC 8785 JCS implementations in Rust and Go,
written for A2A Agent Card signatures and checked against the a2a-sdk. Most
of the pieces already exist; the work is wiring them to your receipt format.

Plan, following the issue's three steps:

1. **`receiptId`.** Rebuild the canonical content from `jobId, agentA,
   agentB, task, result, settlement, verification` as SPEC §4.2 lists, run
   JCS, and compare `"sha256:" + hex(sha256(...))` with the receipt's
   `receiptId`. I'll follow `sdk-js/src/core/receiptContent.ts` exactly,
   including how it treats absent optional fields.
2. **did:key.** Decode `z` + base58btc, require the `0xed 0x01` prefix and a
   32-byte key, and reject small-order keys before verifying, as SPEC §2
   requires.
3. **Signatures.** Verify `signatures.agentB` over the draft content and
   `signatures.agentA` over the same content (SPEC §4.3), using strict
   Ed25519 verification (`ed25519-dalek` `verify_strict`).

Deliverable: `examples/verifier-rust/`, a small library and a CLI
(`inam-verify receipt.json` exits 0 or 1 and says which check failed),
tested against the vectors from `npx tsx scripts/interop-vectors.ts` and
`interop/canonical/vectors.json`. I'll also add negative cases: a tampered
field, a swapped signature, a small-order key, and a wrong multicodec
prefix. Commits will be DCO signed off.

Two questions before I start:
- Should the CLI also check Verifications (§12.1-12.2) and transparency-log
  inclusion proofs (§13), or stay at receipts as the issue says?
- Is a Rust crate under `examples/` fine without a CI job, or do you want a
  workflow added?

---

## PR description outline (for later, once the code exists)

- Title: `examples: offline receipt verifier in Rust (#50)`
- What it checks (the three steps), and what it doesn't (registry status,
  disputes, verifier grants, freshness: all registry claims, SPEC §5.4).
- How it was tested: which vector files, how many cases, the negative cases.
- Build and run: `cargo test`, `cargo run -- <receipt.json>`.
- Closes #50.
