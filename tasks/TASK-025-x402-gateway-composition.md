# TASK-025 — Paid-API gateway: x402 upstream loop + prepaid-gated GatewayProvider composition

Status: READY (blocked by TASK-024) · Priority: P4 · Target: `map-rs`
(`adapters/x402-adapter` grows the HTTP 402 payer loop;
`adapters/gateway` grows `PaidGatewayProvider`; map-cli demo; nothing in
crates/ changes)
Depends on: TASK-016 (GatewayProvider), TASK-023 (x402 adapter seams),
TASK-024 (prepaid terms-1 gate — REQUIRED, implement 024 first) ·
Blocks: nothing (periphery composition).

Authored by the 2026-07-16 architect session (second pass). Normative
ground truth: **ops §5.1 (added same session — READ IT FIRST: G-5/G-6/G-7
are the acceptance tests)**, pay §5.1 `pay:x402` method definition, pay
§5.2.2 (terms-1 execution gate), and the public x402 spec (x402.org) for
the HTTP 402 response body (`accepts` payment-requirements array) and
`X-PAYMENT` / `X-PAYMENT-RESPONSE` header shapes.

---

## 1. Problem Statement

TASK-016's gateway fronts a free upstream; TASK-023's adapter settles
MAP-peer invoices over x402 but has NO HTTP 402 client loop. The pay §5.1
note ("the 402 loop remains appropriate at a gateway") and the new ops
§5.1 rules have no implementation. Compose the three finished pieces into
a gateway that fronts an external x402-priced HTTP API as a MAP
capability: MAP-side prepaid settlement gates execution; the gateway then
pays upstream with its own wallet, bounded and retry-safe.

## 2. Functional Requirements

### FR-1 x402 HTTP payer loop (`x402-adapter`, new module `http402.rs`)

Always-on trait seam + canned impl; real HTTP behind the existing
`real-x402` feature (no new features, no new always-on deps):

```rust
pub struct PaymentRequirements { // parsed from the 402 body `accepts` entry
    pub scheme: String, pub network: String, pub max_amount_atomic: u128,
    pub pay_to: String, pub asset: String, /* token contract */
    pub max_timeout_s: u64, pub extra: Vec<u8> /* raw entry JSON */ }
pub trait HttpUpstream {
    /// GET/POST `url`; returns Ok(body) on 200, Err(Challenge(reqs)) on 402,
    /// Err(Http(status)) otherwise. With `payment: Some(header)` sends
    /// X-PAYMENT and returns the X-PAYMENT-RESPONSE header alongside.
    fn call(&self, url: &str, body: &[u8], payment: Option<&str>)
        -> Result<UpstreamResponse, UpstreamError>;
}
pub fn pay_402_loop(upstream: &impl HttpUpstream, signer: &impl EvmSigner,
    url: &str, body: &[u8], policy: &SpendPolicy, task: &[u8; 32],
    attempt: u32) -> Result<UpstreamResponse, X402Error>
```

Loop: call → on 402 parse `accepts`, select the first entry matching
`policy` (scheme "exact", allowed network list, asset allowlist) →
**G-7**: `max_amount_atomic <= policy.max_atomic_per_task` else
`X402Error::PriceCapExceeded` (fail-closed, NO signing) → build the
EIP-3009 authorization with nonce =
`SHA-256("MAP-x402-upstream-nonce" || task || be32(attempt))` (ops §5.1
retry-safety; distinct domain from the pay §5.1 invoice nonce — pin both
constants side by side in lib.rs) → re-call with the X-PAYMENT header →
200 or error. Exactly ONE payment attempt per (task, attempt); a second
402 after paying is `UpstreamError` (no auto-escalation).

`CannedUpstream` (always-on): configurable to (a) 200 free, (b) 402 then
200 on a valid payment whose amount/nonce it checks, (c) 402 demanding
over-cap, (d) 402 twice (greedy upstream). Real `ureq` impl behind
`real-x402`, mirroring TASK-023's RealFacilitator wiring.

### FR-2 SpendPolicy (G-7)

`SpendPolicy { max_atomic_per_task: u128, networks: Vec<String>,
assets: Vec<String> }` — construction-time config of the paid gateway.
Everything outside it is refusal, never negotiation.

### FR-3 `PaidGatewayProvider` (`adapters/gateway`, new module `paid.rs`)

Wraps the TASK-016 `GatewayProvider` composition-style (no changes to
provider.rs):

```rust
pub struct PaidGatewayProvider<U: HttpUpstream, S: EvmSigner> { ... }
impl Executor for PaidGatewayProvider<U, S> // same infallible contract
```

- Holds: the inner gateway fields (namespace/zone/scope via delegation —
  reuse `GatewayProvider::admit` by embedding a `GatewayProvider` or the
  admit inputs), upstream URL, `SpendPolicy`, signer, upstream seam, and
  a `settled: Fn/bool` gate fed by the TASK-024
  `PrepaidProviderPayment::execution_permitted()`.
- **G-6 order, enforced structurally**: `serve(task, cap, zone, input,
  now_ms)` runs admit → settled-check → `pay_402_loop` → chunks. The
  admit/settled failures return BEFORE any upstream call is made (test
  observes the canned upstream's call counter == 0).
- **G-5**: the upstream `X-PAYMENT-RESPONSE` (settlement evidence) is
  emitted as an additional transcript chunk (ChunkKind::ToolEvent, the
  existing kind-4) — gateway-attested evidence, never any PayProof type.
  There is deliberately NO API that converts an upstream settlement into
  a `PayProof` or vice versa.
- Attempt counter per task for the FR-1 nonce; a retried `serve` for the
  same (task, attempt) re-presents the identical authorization (canned
  upstream asserts single settle).

### FR-4 downstream leg wiring (test-level, no new abstractions)

The MAP leg in tests is TASK-024's `Prepaid{Provider,Requester}Payment`
over `pay:credit-ledger` (matching the TASK-022/024 house pattern: the
runtimes stay method-hardwired; do NOT generalize). The demo/README
documents that any method/terms works downstream — the gate is the only
coupling point.

### FR-5 map-cli demo (gated)

`map-cli x402-gateway-demo --upstream URL --network base-sepolia
--max-spend N ...` behind `real-x402`: fronts a real x402-priced endpoint
(or the x402.org demo endpoint), runs the full loop with a throwaway key,
prints the upstream tx evidence and the transcript chunks. Mirror the
TASK-023 demo wiring; zero default-build impact.

### FR-6 no spec/vector/Go work

ops §5.1 is composition guidance over existing wire; no new messages,
codes, or vectors. map-go untouched.

## 3. Non-Functional Requirements

Default build: ZERO new external deps (`cargo tree` unchanged); zero
warnings both builds; no panics on upstream input (the 402 body is
hostile JSON — parse fail-closed); `X402Error`/`UpstreamError` as values.
Upstream URLs, prompt content, and wallet material never appear in error
Display/Debug output (TASK-016 §10 discipline — extend the existing
no-content-leak test).

## 4. System Architecture

Pure composition at the adapter layer: x402-adapter gains the HTTP payer
role (it already has the signer seam), gateway gains a paid wrapper that
sequences admit → settle-gate → pay → execute. crates/ (map-pay,
map-taskrun, map-core) are UNTOUCHED. The two payment legs never meet in
a type: the only cross-leg artifact is the kind-4 evidence chunk (G-5),
and the only coupling is the boolean execution gate (G-6).

## 5–9. Design / Data / Algorithms / Edge Cases

Edge cases the tests MUST pin: 402 with empty/malformed `accepts` →
refuse, no spend; multiple `accepts` entries → first policy-matching one
selected deterministically (document the rule); over-cap demand → refuse
BEFORE signing (assert signer call count 0); upstream 402 → pay → 402
again → error, exactly one payment; upstream 500 after payment →
UpstreamError, evidence chunk still records the settlement (the money is
spent; the transcript must say so — G-5 evidence is what makes the loss
attributable); admit-reject and unsettled-gate paths spend nothing;
attempt increment produces a DIFFERENT nonce, same (task, attempt)
produces the SAME nonce (fixed-output test on the derivation, both
constants distinguished from the invoice-nonce domain).

## 10. Error Handling

`UpstreamError { Challenge(PaymentRequirements), Http(u16), Transport,
Malformed }`; `X402Error::PriceCapExceeded` new variant. The Executor
contract stays infallible: `serve` failures degrade to empty output +
(where a session exists) the standard task-failure path — mirror how
OllamaExecutor degrades. Wire codes are unchanged (2003/2004/2005 from
admit; task-level failure is the existing 5000 family).

## 11. Security Considerations

G-6 ordering is THE property: money moves only for admitted, settled
tasks — test the order by call-counting the canned upstream and signer in
every reject path. G-7 cap checked against the hostile 402 body before
any cryptographic operation. The upstream nonce domain separation
("MAP-x402-upstream-nonce" vs "MAP-x402-nonce") prevents an upstream
challenge from tricking the gateway into minting an authorization valid
for a MAP invoice; state both constants in one place with a test that the
derivations differ on identical 16/32-byte inputs. Wallet key lives only
behind the `EvmSigner` seam (never serialized, never logged).

## 12–14. Performance / IO

Two HTTP round-trips per paid call under the gate; irrelevant otherwise.

## 15. Acceptance Criteria

- ✓ Default build green, zero warnings, `cargo tree` unchanged; gated
  build green.
- ✓ E2E canned test: prepaid settle → admit → 402 → pay → 200 → output +
  usage + kind-4 evidence chunk; receipts on the MAP leg unchanged in
  shape.
- ✓ Every §5–9 edge pinned; G-5/G-6/G-7 each named in a test.
- ✓ No changes under crates/, map-go, map-spec vectors; TASK-016/023/024
  test suites untouched and green.
- ✓ README (gateway crate) documents the two-leg model + the ops §5.1
  rule mapping.

## 16. Comprehensive Test Plan

FR-1 loop unit tests against all four CannedUpstream modes; nonce
derivation fixed-output + domain-separation tests; SpendPolicy
selection/refusal matrix; FR-3 order-enforcement tests (call counters);
E2E happy path with balance assertions on the credit-ledger (requester
prepaid the gateway) AND the canned upstream (gateway paid exactly once);
`#[ignore]`d live test naming its env vars, as TASK-023.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-025 §FR-1..5 in `map-rs/adapters/x402-adapter` (module
> `http402.rs`) and `map-rs/adapters/gateway` (module `paid.rs`), plus the
> gated map-cli demo. PREREQUISITE: TASK-024 must be DONE (the
> `PrepaidProviderPayment::execution_permitted` gate is consumed here) —
> if it is not, STOP. Read FIRST: map-spec/draft-map-ops-01.md §5.1
> (G-5/G-6/G-7 are the acceptance tests), the pay §5.1 `pay:x402`
> definition, this package, and the x402 HTTP spec (x402.org — pin the 402
> `accepts` body and X-PAYMENT/X-PAYMENT-RESPONSE field names you
> implement in the crate README with a reference, as TASK-023 did). Reuse
> the existing `real-x402` feature for anything needing ureq/k256; the
> default build must add ZERO deps (cargo-tree-verify and say so). Do NOT
> modify anything under crates/, the specs, vectors, or map-go; do NOT
> generalize the payment runtimes. Toolchain: PowerShell,
> `$env:PATH="C:\msys64\mingw64\bin;$env:PATH"`,
> `cargo +stable-x86_64-pc-windows-gnu test --workspace` (then the gated
> crates with `--features real-x402`). If the x402 402-body shape is
> ambiguous or has drifted upstream, STOP and file PENDING-ARCHITECT in
> map-spec/ERRATA.md rather than guessing. Update memory.md §5/§8 +
> tasks/README.md row 025 on completion; commit + push.

## 18. Validation Checklist (reviewer)

```
cargo +stable-x86_64-pc-windows-gnu test --workspace        # green, 0 warn
cargo tree -p x402-adapter -p gateway                        # no gated deps
cargo +stable-x86_64-pc-windows-gnu test -p x402-adapter -p gateway --features real-x402
grep -n "MAP-x402-upstream-nonce" map-rs/adapters/x402-adapter/src/*.rs   # domain separation
grep -n "G-5\|G-6\|G-7" map-rs/adapters/gateway/src/paid.rs               # rules named in tests
```

## 19. Known Risks

R1: TASK-024 sequencing — this package consumes its gate; do not start
before 024 lands. R2: the x402 402-body JSON is externally governed and
younger than the payload format TASK-023 pinned — the README pin + STOP
rule contain drift. R3: "evidence chunk after paid-but-failed upstream"
is easy to skip and is the one path where money is lost — it is a named
acceptance test, not optional. R4: temptation to add a refund/dispute
path when the upstream takes money and fails — OUT of scope (that is pay
§6 material; the evidence chunk is the hook it will consume).

## 20. Handoff Notes

After this, the OpenRouter-style "paid API as a MAP peer" pattern from
the original design sketches is real end-to-end at the adapter level:
external x402 rail upstream, any MAP method downstream, accountability
and spend bounded by G-1..G-7. Remaining unpackaged frontier: disputes
(pay §6 — consumes TASK-024 escrow artifacts AND this task's paid-but-
failed evidence chunks), method-generic payment runtimes (third
structurally-different rail), Go task runtime + metered/escrow interop.
