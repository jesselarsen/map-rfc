# x402-adapter — the `pay:x402` MAP settlement method

Implements the `pay:x402` settlement method registered in
[`draft-map-pay-01.md` §5.1](../../../map-spec/draft-map-pay-01.md).
x402 ([x402.org](https://x402.org), [coinbase/x402](https://github.com/coinbase/x402))
is an HTTP-native stablecoin payment protocol; MAP adopts **only** its
signed-authorization payload and payee-chosen facilitator model. x402's own
HTTP `402`/`X-PAYMENT` request loop is **not** used between MAP peers —
PAY_QUOTE / PAY_AGREE / PAY_INVOICE already carry the negotiation with
stronger binding.

## What lives where

MAP defines two opaque containers — `MethodOffer.2` (params) and
`PayProof.3` (proof) — and this crate defines what goes inside them:

- **`codec`** — `X402Params` / `X402Proof` as deterministic-CBOR maps (the
  §5.1 CDDL).
- **`x402_nonce(invoice_id)`** — the §5.1 MUST:
  `SHA-256("MAP-x402-nonce" || invoice-id)`. This binds an EIP-3009
  authorization to exactly one MAP invoice; the chain's single-use-nonce
  rule then delivers exactly-once settlement, layered under MAP's own
  invoice-id idempotency.
- **`payload`** — the x402 "exact" EVM PaymentPayload (a small fixed JSON
  object) and the `TransferAuthorization` the payer signs. This JSON is
  produced by `build_proof` and consumed by `verify_and_settle` **only
  within this crate** (facilitators settle it verbatim, never produce it),
  so it is serialised by hand — consistent with the project's hand-rolled
  CBOR, and keeping the always-on path dependency-free.
- **`flow`** — `build_proof` (payer) and `verify_and_settle` (payee) over
  the `EvmSigner` / `Facilitator` seams, plus `CannedSigner` /
  `CannedFacilitator` for tests and the offline demo.

The payee runs the §5.1 verification procedure in order (version / scheme /
network / payTo → token vs Asset → value vs invoice mantissa → the nonce
MUST → validity window → facilitator verify → settle) and returns the pay
§5.1 wire code on rejection: **6008** (`INVALID_PAYMENT_PROOF`) for any
structural failure, **6002** (`SETTLEMENT_FAILED`) for a facilitator/settle
failure. Only `settle` returns a `Settlement` — the type enforces the §5.1
"`/verify` alone is NOT settlement" rule.

Asset/amount: the quote `Asset` MUST be the `erc20:<chain>:<addr>` form
whose `<chain>` equals `params.network`, and the Amount exponent MUST be the
token's decimals negated (USDC ⇒ −6), so the mantissa **is** the atomic-unit
count.

## The `real-x402` feature gate

**Default build: zero external dependencies.** The always-on path (codec,
nonce, flows, canned impls) uses only `map-core`, `map-pay`, and `sha2` —
all already in the workspace. `cargo tree -p x402-adapter` shows no gated
crates.

The `real-x402` feature adds the real settlement path and its dependencies —
the workspace's **first non-Ed25519 crypto** — gated hard so the default
audit surface is unchanged:

| dep | why | choice notes |
|---|---|---|
| `k256` (`ecdsa`, `std`) | secp256k1 ECDSA signing for the EIP-712 digest | RustCrypto; `default-features = false` to drop unused curve arithmetic |
| `sha3` | keccak-256 for EIP-712 hashing (Ethereum uses keccak, not SHA-2) | RustCrypto |
| `ureq` (`json`) | facilitator `/verify` + `/settle` HTTP | default TLS retained for HTTPS facilitators |
| `serde_json` | facilitator request/response bodies (JSON, an external format) | only the facilitator API — never MAP wire data |

`real::RealSigner` computes the EIP-3009 `TransferWithAuthorization` EIP-712
digest under the token's domain (the domain `name`/`version` vary per token
and are supplied to the constructor — e.g. USDC: `"USD Coin"` / `"2"`) and
produces a 65-byte recoverable signature. A gated unit test anchors the
digest code against the published EIP-3009 `TransferWithAuthorization`
typehash (`0x7c7c6cdb…2267`). `real::RealFacilitator` posts the x402
facilitator API bodies.

## Build and test

```
# default (always-on): zero new deps
cargo +stable-x86_64-pc-windows-gnu test -p x402-adapter

# with the real settlement path
cargo +stable-x86_64-pc-windows-gnu test -p x402-adapter --features real-x402
```

A live end-to-end test (`live_facilitator_settles`) is `#[ignore]`d; run it
with `--features real-x402 -- --ignored` after setting a reachable
facilitator, a funded testnet key, and the token/network. The `map-cli
x402-demo` subcommand (also behind `real-x402`) drives one real
authorization + verify/settle from the CLI.

## Scope

This crate is a leaf: it does not touch `map-pay`, `map-taskrun`, the specs'
frozen surfaces, the test vectors, or `map-go`. Settlement proofs are
chain-dependent, so there are no golden vectors (like every other rail
method). The payment runtimes stay method-specific until a third settlement
method lands; the ops §5 x402 **gateway** composition (fronting external
x402-priced HTTP services as MAP capabilities) reuses this adapter + the
`gateway` adapter without new design.
