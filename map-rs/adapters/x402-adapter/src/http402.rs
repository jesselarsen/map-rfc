//! The x402 HTTP payer loop (ops §5.1, TASK-025): the 402/`X-PAYMENT` request
//! cycle a paid-API gateway runs against an EXTERNAL x402-priced service. This
//! loop is appropriate exactly at the gateway boundary — never between MAP
//! peers (pay §5.1).
//!
//! Always-on: the [`HttpUpstream`] trait seam, the 402-body parser, the
//! [`SpendPolicy`] (G-7), the [`upstream_nonce`] derivation, [`pay_402_loop`],
//! and [`CannedUpstream`]. The real HTTP client rides the existing `real-x402`
//! feature (ureq), mirroring `real.rs`.
//!
//! Money rules enforced here (ops §5.1):
//! - **G-7**: the challenge amount is checked against the policy ceiling
//!   BEFORE any cryptographic operation — over-cap demands are refused with
//!   no signing and no spend.
//! - **Retry safety**: the authorization nonce is derived deterministically
//!   from (task, attempt) — a retried call re-presents the identical
//!   single-use authorization; the rail's nonce rule dedups. The domain
//!   string is distinct from the pay §5.1 invoice nonce so an upstream
//!   challenge can never elicit an authorization valid for a MAP invoice
//!   (see [`upstream_nonce`] and the domain-separation test).
//! - Exactly ONE payment attempt per (task, attempt): a second 402 after
//!   paying is an error, never an escalation.

use crate::payload::{str_field, to_hex0x, uint_field, TransferAuthorization, X402Payload};
use crate::flow::EvmSigner;
use crate::{X402Error, SCHEME_EXACT, X402_VERSION};
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;

/// Domain string for the upstream authorization nonce. MUST differ from the
/// pay §5.1 `"MAP-x402-nonce"` invoice domain (ops §5.1 domain separation).
const UPSTREAM_NONCE_DOMAIN: &[u8] = b"MAP-x402-upstream-nonce";

/// The deterministic per-(task, attempt) authorization nonce (ops §5.1):
/// `SHA-256("MAP-x402-upstream-nonce" || task || be32(attempt))`.
pub fn upstream_nonce(task: &[u8; 32], attempt: u32) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(UPSTREAM_NONCE_DOMAIN);
    h.update(task);
    h.update(attempt.to_be_bytes());
    h.finalize().into()
}

/// One parsed entry of a 402 response body's `accepts` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRequirements {
    pub scheme: String,
    pub network: String,
    /// `maxAmountRequired`, atomic token units (string-encoded in x402 JSON).
    pub max_amount_atomic: u128,
    pub pay_to: String,
    /// The token contract address (`asset` in x402 JSON).
    pub asset: String,
    /// `maxTimeoutSeconds` (0 if absent).
    pub max_timeout_s: u64,
    /// The raw entry JSON, verbatim (for evidence/debugging; never parsed
    /// beyond the fields above).
    pub extra: String,
}

/// A non-402 upstream response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamResponse {
    pub status: u16,
    pub body: Vec<u8>,
    /// The `X-PAYMENT-RESPONSE` header, if the upstream sent one (settlement
    /// evidence — G-5: gateway-attested transcript content, never a PayProof).
    pub payment_response: Option<String>,
}

/// Why an upstream call did not yield a 2xx response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamError {
    /// HTTP 402 with parsed payment requirements.
    Challenge(Vec<PaymentRequirements>),
    /// Any other non-2xx status.
    Http(u16),
    /// Connection/transport failure.
    Transport(String),
    /// The 402 body (or a header) was not parseable — fail-closed, no spend.
    Malformed(String),
}

/// One HTTP exchange with the external service. Implementations MUST parse a
/// 402 into `Err(Challenge(..))` (fail-closed `Malformed` on a bad body) and
/// surface the `X-PAYMENT-RESPONSE` header on success.
pub trait HttpUpstream {
    fn call(&self, url: &str, body: &[u8], payment: Option<&str>) -> Result<UpstreamResponse, UpstreamError>;
}

/// The gateway's upstream spend bounds (ops §5.1 G-7). Everything outside it
/// is refusal, never negotiation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendPolicy {
    /// Hard per-task ceiling, atomic token units.
    pub max_atomic_per_task: u128,
    /// Acceptable network ids (e.g. "base-sepolia").
    pub networks: Vec<String>,
    /// Acceptable token contract addresses.
    pub assets: Vec<String>,
}

impl SpendPolicy {
    /// The first policy-matching entry, in the order the upstream listed them
    /// (deterministic selection rule). G-7 is checked by the caller AFTER
    /// selection so an over-cap demand is a distinct, reportable refusal.
    pub fn select<'a>(&self, accepts: &'a [PaymentRequirements]) -> Option<&'a PaymentRequirements> {
        accepts.iter().find(|r| {
            r.scheme == SCHEME_EXACT
                && self.networks.iter().any(|n| n == &r.network)
                && self.assets.iter().any(|a| a.eq_ignore_ascii_case(&r.asset))
        })
    }
}

/// What the gateway attests about an upstream payment (the G-5 evidence: it
/// enters the transcript as gateway-signed content, never as any proof type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentEvidence {
    /// The X-PAYMENT header value sent (base64 of the PaymentPayload JSON).
    pub payment_header: String,
    /// The upstream's X-PAYMENT-RESPONSE, if any arrived.
    pub payment_response: Option<String>,
    pub amount_atomic: u128,
    pub network: String,
    pub pay_to: String,
}

impl PaymentEvidence {
    /// The transcript-chunk bytes (kind 4 tool-event content): a small JSON
    /// object, gateway-attested (ops §5.1 G-5).
    pub fn to_chunk_bytes(&self) -> Vec<u8> {
        format!(
            concat!(
                "{{\"x402-upstream-payment\":{{\"network\":\"{net}\",\"payTo\":\"{to}\",",
                "\"amountAtomic\":\"{amt}\",\"xPayment\":\"{hdr}\",\"xPaymentResponse\":{resp}}}}}"
            ),
            net = self.network,
            to = self.pay_to,
            amt = self.amount_atomic,
            hdr = self.payment_header,
            resp = match &self.payment_response {
                Some(r) => format!("\"{r}\""),
                None => "null".into(),
            },
        )
        .into_bytes()
    }
}

/// A successful loop outcome: the 2xx response, plus payment evidence if the
/// call was paid (None: the upstream served it free).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaidResponse {
    pub response: UpstreamResponse,
    pub evidence: Option<PaymentEvidence>,
}

/// A failed loop outcome. `evidence` is `Some` iff a payment was SENT before
/// the failure — the money may be spent, and the caller MUST record the
/// evidence (the attributable-loss rule, TASK-025 §5-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopFailure {
    pub error: X402Error,
    pub evidence: Option<PaymentEvidence>,
}

/// Parse an x402 402-response body: `{"x402Version":1,…,"accepts":[{…},…]}`.
/// Hostile input: any structural surprise is `Err` (fail-closed, no spend).
pub fn parse_payment_requirements(body: &[u8]) -> Result<Vec<PaymentRequirements>, UpstreamError> {
    let s = std::str::from_utf8(body).map_err(|_| UpstreamError::Malformed("402 body not UTF-8".into()))?;
    let accepts_at = s
        .find("\"accepts\"")
        .ok_or_else(|| UpstreamError::Malformed("402 body has no accepts array".into()))?;
    let rest = s[accepts_at + "\"accepts\"".len()..]
        .trim_start()
        .strip_prefix(':')
        .map(str::trim_start)
        .and_then(|r| r.strip_prefix('['))
        .ok_or_else(|| UpstreamError::Malformed("accepts is not an array".into()))?;
    // Split top-level objects by brace matching (strings may contain braces —
    // track the in-string state; x402 field values never contain escapes we
    // care about, but skip \" defensively).
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    let mut in_str = false;
    let mut prev_backslash = false;
    for (i, c) in rest.char_indices() {
        if in_str {
            match c {
                '"' if !prev_backslash => in_str = false,
                '\\' => {
                    prev_backslash = !prev_backslash;
                    continue;
                }
                _ => {}
            }
            prev_backslash = false;
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.checked_sub(1).ok_or_else(|| UpstreamError::Malformed("unbalanced braces".into()))?;
                if depth == 0 {
                    let entry = &rest[start.take().ok_or_else(|| UpstreamError::Malformed("brace state".into()))?..=i];
                    entries.push(parse_entry(entry)?);
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    if entries.is_empty() {
        return Err(UpstreamError::Malformed("accepts array is empty".into()));
    }
    Ok(entries)
}

fn parse_entry(entry: &str) -> Result<PaymentRequirements, UpstreamError> {
    let field = |k: &str| str_field(entry, k).map_err(|e| UpstreamError::Malformed(e.to_string()));
    Ok(PaymentRequirements {
        scheme: field("scheme")?,
        network: field("network")?,
        max_amount_atomic: field("maxAmountRequired")?
            .parse()
            .map_err(|_| UpstreamError::Malformed("maxAmountRequired not a u128".into()))?,
        pay_to: field("payTo")?,
        asset: field("asset")?,
        max_timeout_s: uint_field(entry, "maxTimeoutSeconds").unwrap_or(0),
        extra: entry.to_string(),
    })
}

/// Standard base64 (RFC 4648, with padding) — the X-PAYMENT header encoding.
pub fn base64_encode(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for chunk in b.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Decode standard base64 (with or without padding). Fail-closed on any
/// non-alphabet byte.
pub fn base64_decode(s: &str) -> Result<Vec<u8>, X402Error> {
    fn val(c: u8) -> Result<u32, X402Error> {
        Ok(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(X402Error::Codec("bad base64".into())),
        })
    }
    let stripped: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(stripped.len() * 3 / 4);
    for chunk in stripped.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

/// Run the x402 payer loop against `url` (ops §5.1): call; on a 402 select a
/// policy-matching requirement, enforce G-7 BEFORE signing, build the
/// deterministic (task, attempt) authorization, and re-call with X-PAYMENT.
/// Exactly one payment attempt; a second 402 after paying is an error.
#[allow(clippy::too_many_arguments)]
pub fn pay_402_loop(
    upstream: &impl HttpUpstream,
    signer: &dyn EvmSigner,
    url: &str,
    body: &[u8],
    policy: &SpendPolicy,
    task: &[u8; 32],
    attempt: u32,
    now_s: u64,
) -> Result<PaidResponse, LoopFailure> {
    let fail = |e: X402Error| LoopFailure { error: e, evidence: None };
    // 1. Unpaid call.
    let accepts = match upstream.call(url, body, None) {
        Ok(resp) => return Ok(PaidResponse { response: resp, evidence: None }),
        Err(UpstreamError::Challenge(a)) => a,
        Err(e) => return Err(fail(X402Error::Facilitator(format!("upstream: {e:?}")))),
    };
    // 2. Deterministic selection under the policy.
    let req = policy
        .select(&accepts)
        .ok_or_else(|| fail(X402Error::BadParams("no policy-acceptable payment requirement".into())))?;
    // 3. G-7: the ceiling, BEFORE any cryptographic operation.
    if req.max_amount_atomic > policy.max_atomic_per_task {
        return Err(fail(X402Error::PriceCapExceeded));
    }
    // 4. The deterministic authorization (retry-safe single-use nonce).
    let nonce = upstream_nonce(task, attempt);
    let valid_before = now_s.saturating_add(req.max_timeout_s.clamp(30, 600));
    let auth = TransferAuthorization {
        from: signer.address(),
        to: req.pay_to.clone(),
        value_atomic: req.max_amount_atomic,
        valid_after: 0,
        valid_before,
        nonce,
        token: req.asset.clone(),
        chain_id: 0, // upstream-published; the canned/real signer derives from network
    };
    let signature = signer.sign_authorization(&auth).map_err(fail)?;
    let payload = X402Payload {
        version: X402_VERSION,
        scheme: SCHEME_EXACT.into(),
        network: req.network.clone(),
        signature,
        from: auth.from,
        to: auth.to,
        value_atomic: req.max_amount_atomic,
        valid_after: 0,
        valid_before,
        nonce,
    };
    let header = base64_encode(&payload.to_json_bytes());
    let evidence = |resp: Option<String>| PaymentEvidence {
        payment_header: header.clone(),
        payment_response: resp,
        amount_atomic: req.max_amount_atomic,
        network: req.network.clone(),
        pay_to: req.pay_to.clone(),
    };
    // 5. Paid call — the money may move from here on: every failure path
    //    carries the evidence (attributable loss, TASK-025 §5-9).
    match upstream.call(url, body, Some(&header)) {
        Ok(resp) => {
            let ev = evidence(resp.payment_response.clone());
            Ok(PaidResponse { response: resp, evidence: Some(ev) })
        }
        Err(UpstreamError::Challenge(_)) => Err(LoopFailure {
            error: X402Error::Facilitator("second 402 after payment (greedy upstream)".into()),
            evidence: Some(evidence(None)),
        }),
        Err(e) => Err(LoopFailure {
            error: X402Error::Facilitator(format!("upstream after payment: {e:?}")),
            evidence: Some(evidence(None)),
        }),
    }
}

// ---- canned upstream (always-on; tests + offline demo) ---------------------

/// What the canned upstream should do (TASK-025 §16 modes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CannedMode {
    /// 200 with no payment demanded.
    Free,
    /// 402 once; a valid payment of `price_atomic` buys a 200.
    PayThenOk { price_atomic: u128 },
    /// 402 demanding `price_atomic` (intended to exceed the policy ceiling).
    OverCap { price_atomic: u128 },
    /// 402; accepts the payment (settles the nonce) and then 402s AGAIN.
    Greedy { price_atomic: u128 },
    /// 402; accepts the payment (settles the nonce) and then fails 500 —
    /// the money is spent, the service is not delivered.
    FailAfterPay { price_atomic: u128 },
}

/// An in-process x402-priced upstream. Counts calls and payments so tests can
/// assert the G-6 order ("no spend on reject paths") and single-settlement.
pub struct CannedUpstream {
    mode: CannedMode,
    pub network: String,
    pub asset: String,
    pub pay_to: String,
    /// Total HTTP exchanges (any kind).
    pub calls: Cell<u32>,
    /// Distinct settled payments (nonce-deduplicated).
    payments: RefCell<Vec<String>>,
    settled_nonces: RefCell<HashSet<[u8; 32]>>,
}

impl CannedUpstream {
    pub fn new(mode: CannedMode) -> Self {
        CannedUpstream {
            mode,
            network: "base-sepolia".into(),
            asset: "0x036CbD53842c5426634e7929541eC2318f3dCF7e".into(),
            pay_to: "0x00000000000000000000000000000000000fee01".into(),
            calls: Cell::new(0),
            payments: RefCell::new(Vec::new()),
            settled_nonces: RefCell::new(HashSet::new()),
        }
    }

    /// Distinct settled payments (a re-presented authorization settles once).
    pub fn payment_count(&self) -> usize {
        self.payments.borrow().len()
    }

    fn requirements(&self, price: u128) -> Vec<PaymentRequirements> {
        vec![PaymentRequirements {
            scheme: SCHEME_EXACT.into(),
            network: self.network.clone(),
            max_amount_atomic: price,
            pay_to: self.pay_to.clone(),
            asset: self.asset.clone(),
            max_timeout_s: 60,
            extra: String::new(),
        }]
    }

    /// Validate an X-PAYMENT header against this upstream's price; settle the
    /// nonce once (the chain's single-use rule: a re-presented authorization
    /// is already settled — served without a second charge).
    fn accept_payment(&self, header: &str, price: u128) -> Result<(), UpstreamError> {
        let json = base64_decode(header).map_err(|e| UpstreamError::Malformed(e.to_string()))?;
        let p = X402Payload::from_json_bytes(&json).map_err(|e| UpstreamError::Malformed(e.to_string()))?;
        if p.to != self.pay_to || p.network != self.network || p.value_atomic != price || p.scheme != SCHEME_EXACT {
            return Err(UpstreamError::Http(400));
        }
        if self.settled_nonces.borrow_mut().insert(p.nonce) {
            self.payments.borrow_mut().push(to_hex0x(&p.nonce));
        }
        Ok(())
    }
}

impl HttpUpstream for CannedUpstream {
    fn call(&self, _url: &str, body: &[u8], payment: Option<&str>) -> Result<UpstreamResponse, UpstreamError> {
        self.calls.set(self.calls.get() + 1);
        let ok = |payment_response: Option<String>| {
            Ok(UpstreamResponse { status: 200, body: [b"upstream:", body].concat(), payment_response })
        };
        match (&self.mode, payment) {
            (CannedMode::Free, _) => ok(None),
            (CannedMode::PayThenOk { price_atomic }
            | CannedMode::OverCap { price_atomic }
            | CannedMode::Greedy { price_atomic }
            | CannedMode::FailAfterPay { price_atomic }, None) => {
                Err(UpstreamError::Challenge(self.requirements(*price_atomic)))
            }
            (CannedMode::PayThenOk { price_atomic }, Some(h)) => {
                self.accept_payment(h, *price_atomic)?;
                ok(Some("{\"success\":true,\"txHash\":\"0xcanned\"}".into()))
            }
            (CannedMode::OverCap { price_atomic }, Some(h)) => {
                // Should be unreachable through the loop (G-7 refuses first).
                self.accept_payment(h, *price_atomic)?;
                ok(None)
            }
            (CannedMode::Greedy { price_atomic }, Some(h)) => {
                self.accept_payment(h, *price_atomic)?;
                Err(UpstreamError::Challenge(self.requirements(*price_atomic)))
            }
            (CannedMode::FailAfterPay { price_atomic }, Some(h)) => {
                self.accept_payment(h, *price_atomic)?;
                Err(UpstreamError::Http(500))
            }
        }
    }
}

// ---- real HTTP client (feature `real-x402`) --------------------------------

/// A ureq-backed [`HttpUpstream`] (POSTs `body`, sends/reads the x402
/// headers). Gated exactly like `real.rs`.
#[cfg(feature = "real-x402")]
pub struct RealUpstream;

#[cfg(feature = "real-x402")]
impl HttpUpstream for RealUpstream {
    fn call(&self, url: &str, body: &[u8], payment: Option<&str>) -> Result<UpstreamResponse, UpstreamError> {
        let mut req = ureq::post(url).set("Content-Type", "application/octet-stream");
        if let Some(h) = payment {
            req = req.set("X-PAYMENT", h);
        }
        match req.send_bytes(body) {
            Ok(resp) => {
                let payment_response = resp.header("X-PAYMENT-RESPONSE").map(str::to_string);
                let mut buf = Vec::new();
                use std::io::Read;
                resp.into_reader()
                    .take(1 << 20)
                    .read_to_end(&mut buf)
                    .map_err(|e| UpstreamError::Transport(e.to_string()))?;
                Ok(UpstreamResponse { status: 200, body: buf, payment_response })
            }
            Err(ureq::Error::Status(402, resp)) => {
                let body = resp.into_string().map_err(|e| UpstreamError::Transport(e.to_string()))?;
                Err(UpstreamError::Challenge(parse_payment_requirements(body.as_bytes())?))
            }
            Err(ureq::Error::Status(code, _)) => Err(UpstreamError::Http(code)),
            Err(e) => Err(UpstreamError::Transport(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::CannedSigner;

    const TASK: [u8; 32] = [0xE; 32];

    fn policy() -> SpendPolicy {
        SpendPolicy {
            max_atomic_per_task: 100_000,
            networks: vec!["base-sepolia".into()],
            assets: vec!["0x036CbD53842c5426634e7929541eC2318f3dCF7e".into()],
        }
    }

    #[test]
    fn base64_round_trips_and_fails_closed() {
        for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"foobar", &[0u8, 255, 7, 128]] {
            assert_eq!(base64_decode(&base64_encode(input)).unwrap(), input.to_vec());
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy"); // RFC 4648 vector
        assert!(base64_decode("not!base64").is_err());
    }

    #[test]
    fn parse_402_body_extracts_entries_in_order() {
        let body = br#"{"x402Version":1,"error":"payment required","accepts":[
            {"scheme":"exact","network":"base-sepolia","maxAmountRequired":"10000",
             "resource":"https://api.example/x","description":"d","mimeType":"text/plain",
             "payTo":"0xPAYEE","maxTimeoutSeconds":60,"asset":"0xTOKEN","extra":{"name":"USDC","version":"2"}},
            {"scheme":"exact","network":"base","maxAmountRequired":"9000",
             "payTo":"0xPAYEE2","asset":"0xTOKEN2"}]}"#;
        let reqs = parse_payment_requirements(body).unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].network, "base-sepolia");
        assert_eq!(reqs[0].max_amount_atomic, 10_000);
        assert_eq!(reqs[0].max_timeout_s, 60);
        assert_eq!(reqs[1].network, "base");
        assert_eq!(reqs[1].max_timeout_s, 0); // absent → 0
    }

    #[test]
    fn policy_selects_first_match_only() {
        let mk = |network: &str, asset: &str| PaymentRequirements {
            scheme: SCHEME_EXACT.into(),
            network: network.into(),
            max_amount_atomic: 1,
            pay_to: "0xP".into(),
            asset: asset.into(),
            max_timeout_s: 0,
            extra: String::new(),
        };
        let p = policy();
        let good = mk("base-sepolia", "0x036CbD53842c5426634e7929541eC2318f3dCF7e");
        // Wrong network, wrong asset, wrong scheme are all skipped; the asset
        // match is case-insensitive (EVM addresses).
        let wrong_net = mk("mainnet", &good.asset);
        let wrong_asset = mk("base-sepolia", "0xother");
        let mut wrong_scheme = good.clone();
        wrong_scheme.scheme = "upto".into();
        let list = [wrong_net, wrong_scheme, wrong_asset, good.clone()];
        assert_eq!(p.select(&list), Some(&list[3]));
        let lower = mk("base-sepolia", &good.asset.to_lowercase());
        assert_eq!(p.select(std::slice::from_ref(&lower)), Some(&lower));
        assert_eq!(p.select(&list[..3]), None);
    }

    #[test]
    fn loop_free_paid_overcap_and_evidence_on_late_failure() {
        let signer = CannedSigner::new();
        // Free: no evidence.
        let free = CannedUpstream::new(CannedMode::Free);
        let r = pay_402_loop(&free, &signer, "u", b"in", &policy(), &TASK, 0, 1_780_000_100).unwrap();
        assert!(r.evidence.is_none());
        // Paid: evidence with the upstream's X-PAYMENT-RESPONSE.
        let paid = CannedUpstream::new(CannedMode::PayThenOk { price_atomic: 10_000 });
        let r = pay_402_loop(&paid, &signer, "u", b"in", &policy(), &TASK, 0, 1_780_000_100).unwrap();
        let ev = r.evidence.unwrap();
        assert_eq!(ev.amount_atomic, 10_000);
        assert!(ev.payment_response.is_some());
        assert!(String::from_utf8(ev.to_chunk_bytes()).unwrap().contains("x402-upstream-payment"));
        // Over-cap: PriceCapExceeded with NO evidence (nothing was signed).
        let over = CannedUpstream::new(CannedMode::OverCap { price_atomic: 200_000 });
        let f = pay_402_loop(&over, &signer, "u", b"in", &policy(), &TASK, 0, 1_780_000_100).unwrap_err();
        assert_eq!(f.error, X402Error::PriceCapExceeded);
        assert!(f.evidence.is_none());
        assert_eq!(over.payment_count(), 0);
        // Fail-after-pay: the failure CARRIES the evidence (money spent).
        let dead = CannedUpstream::new(CannedMode::FailAfterPay { price_atomic: 10_000 });
        let f = pay_402_loop(&dead, &signer, "u", b"in", &policy(), &TASK, 0, 1_780_000_100).unwrap_err();
        assert!(f.evidence.is_some(), "paid-then-failed must keep the evidence");
        assert_eq!(dead.payment_count(), 1);
    }
}
