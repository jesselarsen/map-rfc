# gateway — MAP federation gateway (ops §5), plus the paid-API composition (ops §5.1)

Two providers:

- **`GatewayProvider`** (TASK-016): fronts a free upstream backend as a MAP
  capability, enforcing G-1..G-4 (sign-as-self provenance, accountable
  evidence, tighten-only zones, DNS-anchored namespaces) via `admit`.
- **`PaidGatewayProvider`** (TASK-025): fronts an EXTERNAL x402-priced HTTP
  service, composing the TASK-016 admit gate, the TASK-024 prepaid execution
  gate, and the x402-adapter's HTTP 402 payer loop.

## The two-leg model (ops §5.1)

A paid gateway participates in two INDEPENDENT payment legs:

| Leg | Role | Mechanism |
|---|---|---|
| Downstream (MAP) | ordinary MAP **payee** | pay §2–§5 with any method/terms; RECOMMENDED terms 1 prepaid (pay §5.2.2) |
| Upstream (rail) | x402 HTTP **payer** | 402/`X-PAYMENT` loop, gateway's own wallet |

Rule mapping (each is a named test in `src/paid.rs`):

- **G-5 — legs never launder.** No API converts an upstream settlement into a
  `PayProof` or vice versa. Upstream payment evidence enters the transcript
  ONLY as a kind-4 (tool-event) chunk of gateway-attested JSON
  (`PaymentEvidence::to_chunk_bytes`). The only cross-leg coupling is the
  boolean `set_settled` gate.
- **G-6 — spend order.** `serve` runs **admit → settled → pay → execute**;
  reject paths return before any upstream exchange or signature (tests count
  the canned upstream's calls and the signer's signatures — both must be 0).
  The settled gate is fed from
  `PrepaidProviderPayment::execution_permitted()` (TASK-024).
- **G-7 — bounded upstream spend.** `SpendPolicy { max_atomic_per_task,
  networks, assets }`; an over-cap 402 demand is refused
  (`X402Error::PriceCapExceeded`) BEFORE anything is signed. Policy selection
  is deterministic: the first `exact`-scheme entry on an allowed network and
  asset, in the order the upstream listed them.
- **Retry safety.** The upstream authorization nonce is
  `SHA-256("MAP-x402-upstream-nonce" || TaskID || be32(attempt))` — a
  retried `serve` at the same attempt re-presents the identical single-use
  authorization (the rail's nonce rule dedups; the canned upstream asserts
  single settlement). `next_attempt()` starts a genuinely new payment. The
  domain string is distinct from pay §5.1's `"MAP-x402-nonce"`, so an
  upstream challenge can never elicit an authorization valid for a MAP
  invoice (asserted by the domain-separation test).
- **Attributable loss.** If the upstream takes the payment and then fails,
  `serve` returns `PaidServeError::UpstreamPaid` CARRYING the evidence chunk,
  and the infallible `Executor` path degrades to that chunk alone (never to
  empty output) — the record pay §6 disputes consume.

## Feature wiring

The always-on build adds zero external dependencies (the x402-adapter dep is
its always-on seams: canned upstream/signer). Real HTTP + EIP-3009 signing
ride x402-adapter's existing `real-x402` feature; the live demo is
`map-cli x402-gateway-demo` (same feature).

Error values never carry upstream URLs, wallet material, or prompt content
(TASK-016 §10 discipline, extended to the paid path).
