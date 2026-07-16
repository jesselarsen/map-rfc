# TASK-023 — pay:x402 settlement adapter (feature-gated) + gateway demo

Status: READY · Priority: P4 · Target: `map-rs` (new adapter crate
`adapters/x402-adapter`; map-cli demo subcommand; map-pay untouched)
Depends on: TASK-021 (pay codecs), TASK-009 (payment flow shape) ·
Blocks: nothing (periphery — a rail, not protocol core)

Authored by the 2026-07-14 architect session. Normative ground truth: the
`pay:x402` method definition in pay §5.1 (added in the same session — READ
IT FIRST; every MUST below is defined there), pay §3 (asset/amount forms),
and x402's public spec (x402.org / github.com/coinbase/x402) for the
PaymentPayload JSON and facilitator `/verify`+`/settle` API shapes.

---

## 1. Problem Statement

`pay:x402` is registered but has no implementation. Build the adapter that
lets a MAP payee accept x402-settled invoices and a MAP payer produce them,
WITHOUT contaminating the core workspace: EVM signing (secp256k1/EIP-712)
and facilitator HTTP are alien dependencies that MUST stay behind a feature
gate, exactly like TASK-013's `real-backend`. Everything testable without a
chain runs always-on through trait seams with canned implementations.

## 2. Functional Requirements

### FR-1 crate layout + feature gate

New `adapters/x402-adapter` crate. Default features: NO new external deps
(map-core, map-pay, map-identity, sha2 only). Feature `real-x402` gates:
`ureq` (facilitator HTTP; already precedented by TASK-013), `serde_json`
(the x402 payload/facilitator bodies are JSON — an external format, so the
no-serde house rule for MAP wire data does not apply, but keep it inside
the gate), and ONE secp256k1/EIP-712 signing dep (prefer `k256` with
minimal features; document the exact choice and why in the crate README —
this is the workspace's first non-Ed25519 crypto dep, gate it hard).

### FR-2 always-on codec (no gate)

`X402Params { version: u64, scheme: String, network: String, pay_to:
String, facilitator: Option<String>, max_validity_s: Option<u64> }` and
`X402Proof { payload: Vec<u8> }` with `to_bytes`/`from_bytes` as
deterministic-CBOR maps per the pay §5.1 CDDL (these travel inside
`MethodOffer.2` / `PayProof.3`, which are opaque bstrs to the rest of MAP).
Constants: `X402_METHOD: &str = "pay:x402"`, `X402_VERSION: u64 = 1`,
`SCHEME_EXACT: &str = "exact"`.

### FR-3 the invoice-binding nonce (no gate)

`x402_nonce(invoice_id: &[u8; 16]) -> [u8; 32]` =
`SHA-256("MAP-x402-nonce" || invoice-id)` — the pay §5.1 MUST. Both sides
use this one function; a payee-side `check_nonce` rejects a payload whose
authorization nonce differs.

### FR-4 trait seams (no gate) + real impls (gated)

- `trait EvmSigner { fn address(&self) -> String; fn sign_authorization(
  &self, auth: &TransferAuthorization) -> Result<Vec<u8>, X402Error>; }`
  where `TransferAuthorization { from, to, value_atomic: u128, valid_after,
  valid_before, nonce: [u8; 32], token, chain_id }`. Real impl (`k256`,
  EIP-712 domain/type hashing per EIP-3009) behind `real-x402`.
- `trait Facilitator { fn verify(&self, payload: &[u8], params:
  &X402Params, expected: &ExpectedPayment) -> Result<(), X402Error>;
  fn settle(&self, payload: &[u8], params: &X402Params) ->
  Result<Settlement, X402Error>; }` with `Settlement { tx_ref: String }`.
  Real impl (`ureq` against the params' facilitator URL, x402 facilitator
  API) behind `real-x402`. A `CannedFacilitator` (always-on, test-only
  semantics: verifies the CBOR/nonce/amount bindings locally and "settles"
  by recording) ships in the crate for tests and the demo's offline mode.

### FR-5 payer + payee flows (no gate; generic over the traits)

- Payer: `build_proof(signer, params, quote_asset, invoice) ->
  Result<PayProofBytes, X402Error>` — maps the invoice Amount mantissa to
  `value_atomic` (exponent MUST equal the token's negated decimals — take
  decimals from params-adjacent config; mismatch is an error), sets the
  FR-3 nonce, obtains the signature via the seam, assembles the x402
  PaymentPayload JSON, wraps it as `X402Proof`.
- Payee: `verify_and_settle(facilitator, params, quote_asset, invoice,
  proof_bytes) -> Result<Settlement, u16>` running the pay §5.1 payee
  procedure IN ORDER (version/scheme/network/payTo, token vs Asset, value
  vs invoice mantissa, the nonce MUST, validity window) with the specific
  wire codes: structural failures → 6008 INVALID_PAYMENT_PROOF; a
  facilitator/settle failure → 6002. `/verify`-without-`/settle` MUST NOT
  yield a Settlement (type-enforce: only `settle` returns one).

### FR-6 end-to-end flow test (always-on, canned)

One test driving the full MAP shape with this method: a PayQuote whose
`methods = [MethodOffer { uri: "pay:x402", params: X402Params{...} }]` →
PAY_AGREE → PAY_INVOICE → payer `build_proof` (canned signer) → payee
`verify_and_settle` (CannedFacilitator) → PAY_RECEIPT with
`proof_digest = pay_ref_digest(proof)`. Plus negative tests: wrong nonce →
6008; value ≠ invoice mantissa → 6008; tampered payload byte → 6008;
settle failure → 6002 and NO receipt. (Do NOT rewire Provider/
RequesterPayment or the metered pair to be method-generic — that
generalization waits for a third method, per the altitude rule; this test
drives the adapter functions directly against codec-level messages.)

### FR-7 map-cli demo (gated)

`map-cli x402-demo --facilitator URL --network base-sepolia ...` behind
`real-x402`: builds a real authorization with a throwaway key, runs
verify+settle against a live facilitator (testnet), prints the tx ref.
Failure modes print cleanly; no default-build impact (mirror the
`serve-ollama` wiring).

### FR-8 no vectors, no Go work

Settlement proofs are chain-dependent (signatures over EIP-712 domains,
live nonces) — out of deterministic-vector scope, like every other rail
method. Frozen vectors and map-go are untouched; the conformance counts do
not change.

## 3. Non-Functional Requirements

Default `cargo build --workspace` adds ZERO new external deps (verify with
`cargo tree`). Gated deps pinned with minimal features. Zero warnings both
with and without the feature. No panics on input; `X402Error` as values.

## 4. System Architecture

The adapter is a leaf: map-pay defines the opaque containers; this crate
defines what goes inside them for one method. Nothing in map-pay,
map-taskrun, or the specs' frozen surfaces changes. The ops §5 gateway
pattern (fronting external x402-priced HTTP APIs as MAP capabilities) is
deliberately OUT of scope here beyond the demo — it composes later from
this adapter + the TASK-016 GatewayProvider without new design.

## 5–9. Design / Data / Algorithms / Edge Cases

Edge cases the tests MUST pin: value overflow (invoice mantissa < 0 or >
u128 — reject); exponent ≠ negated token decimals (reject before signing);
validity window: `valid_before` in the past → payee 6008; params with an
unknown x402Version or scheme ≠ "exact" → payee 6008 (fail-closed, the
FR-2 constants are this revision's whole surface); CannedFacilitator
settle-twice → second settle is an error (the chain's nonce rule,
simulated) proving MAP's invoice-id dedup is layered, not load-bearing.

## 10. Error Handling

Adapter-internal `X402Error` (typed); the payee surface returns wire codes
(6008/6002) exactly as pay §5.1 assigns them.

## 11. Security Considerations

The FR-3 nonce binding is the section's substantive MUST — test it both
ways (right nonce accepted, wrong rejected). The `/verify`-is-not-
settlement rule is type-enforced (FR-5). The secp256k1 dep is the first
non-Ed25519 crypto in the tree: keep it feature-gated so the default
workspace's audit surface is unchanged, and say so in the README.

## 12–14. Performance / IO

Irrelevant; one HTTP round-trip per settle under the gate.

## 15. Acceptance Criteria

- ✓ Default build: workspace green, zero warnings, `cargo tree` shows no
  new external deps.
- ✓ `--features real-x402` build: compiles green, zero warnings (live
  facilitator tests may be `#[ignore]`d; canned tests always run).
- ✓ FR-6 flow + negatives green; nonce MUST tested both ways.
- ✓ Frozen vectors + map-go untouched (conformance counts unchanged).
- ✓ Crate README documents the gate, the signing-dep choice, and the
  pay §5.1 procedure mapping.

## 16. Comprehensive Test Plan

Codec round-trips (params/proof, cite the §5.1 CDDL); nonce derivation
fixed-output test; payer/payee flow + the §9 edge list; the FR-6
end-to-end with receipts. Live-facilitator test present but `#[ignore]`d
with a comment naming the env vars it needs.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-023 §FR-1..8 as `map-rs/adapters/x402-adapter`. Read
> FIRST: the `pay:x402` method definition in map-spec/draft-map-pay-01.md
> §5.1 (its MUSTs are the acceptance tests), pay §3, this package, and the
> public x402 spec (x402.org; github.com/coinbase/x402) for the
> PaymentPayload JSON + facilitator API — pin the exact JSON field names
> you implement in the crate README with a link/commit reference, since
> x402 is externally versioned. Mirror TASK-013's feature-gate wiring
> (`real-backend`) for `real-x402`; default build must add zero deps
> (verify with `cargo tree` and say so in your report). Do NOT modify
> map-pay, map-taskrun, the specs, the vectors, or map-go. Toolchain:
> PowerShell, `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"`,
> `cargo +stable-x86_64-pc-windows-gnu test --workspace` (then again with
> `--features real-x402` on this crate). If the x402 upstream spec is
> ambiguous or has moved, STOP and record a PENDING-ARCHITECT entry in
> map-spec/ERRATA.md naming the divergence rather than guessing. Update
> memory.md §5/§8 + tasks/README.md row 023 on completion; commit + push.

## 18. Validation Checklist (reviewer)

```
cargo +stable-x86_64-pc-windows-gnu test --workspace          # default: green
cargo tree -p x402-adapter                                     # no gated deps
cargo +stable-x86_64-pc-windows-gnu test -p x402-adapter --features real-x402
grep -n "6008\|6002\|MAP-x402-nonce" map-rs/adapters/x402-adapter/src/*.rs
```

## 19. Known Risks

R1: x402 is young and externally governed — its JSON/facilitator API may
drift; the README's pinned reference + the §17 STOP rule contain this.
R2: EIP-712 hashing is fiddly and easy to get silently wrong — the canned
signer must round-trip against at least one known-good vector from the
x402/EIP-3009 reference material, cited in the test. R3: scope creep
toward making the payment runtimes method-generic — resist (altitude:
wait for the third method). R4: k256 pulls a subtree — keep features
minimal and gated; if it leaks into default builds, that is a failure.

## 20. Handoff Notes

After this, MAP has one real-world rail end-to-end at the adapter level.
Future (unpackaged): method-generic payment runtimes (third method
trigger), the x402 gateway composition (TASK-016 GatewayProvider + this
adapter), x402 batch-settlement scheme support, and a testnet
live-interop demo.
