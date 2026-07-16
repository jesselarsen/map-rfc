//! Payer and payee flows over the `EvmSigner` / `Facilitator` seams, plus
//! canned implementations for tests and the demo's offline mode. These are
//! always-on and generic over the traits; the real (gated) impls live in
//! `real.rs`.

use crate::codec::{X402Params, X402Proof};
use crate::payload::{TransferAuthorization, X402Payload};
use crate::{x402_nonce, wire_code, X402Error, SCHEME_EXACT, X402_VERSION};
use map_pay::codec::PayInvoice;
use map_pay::Asset;
use std::cell::RefCell;
use std::collections::HashSet;

/// A settled x402 payment: the on-chain (or facilitator-reported) reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    pub tx_ref: String,
}

/// What the payee expects the authorization to move (cross-checked by the
/// facilitator on chain).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedPayment {
    pub pay_to: String,
    pub value_atomic: u128,
    pub nonce: [u8; 32],
    pub token: String,
    pub chain_id: u64,
}

/// Produces the signed EIP-3009 authorization (the payer's on-chain key).
pub trait EvmSigner {
    /// The payer's 0x-address (the authorization `from`).
    fn address(&self) -> String;
    /// Sign a `transferWithAuthorization` message, returning the signature bytes.
    fn sign_authorization(&self, auth: &TransferAuthorization) -> Result<Vec<u8>, X402Error>;
}

/// Verifies and settles an x402 authorization (the payee's facilitator or a
/// direct chain client). **`verify` never settles**: only `settle` returns a
/// [`Settlement`] — the type enforces the pay §5.1 "`/verify` alone is NOT
/// settlement" rule.
pub trait Facilitator {
    fn verify(&self, payload: &[u8], params: &X402Params, expected: &ExpectedPayment) -> Result<(), X402Error>;
    fn settle(&self, payload: &[u8], params: &X402Params) -> Result<Settlement, X402Error>;
}

/// Parse `erc20:<chain>:<addr>` into `(chain, token-address)` (pay §1).
fn parse_erc20(asset: &Asset) -> Result<(String, String), X402Error> {
    let s = asset.as_str();
    let rest = s
        .strip_prefix("erc20:")
        .ok_or_else(|| X402Error::BadParams(format!("asset {s} is not erc20:<chain>:<addr>")))?;
    let (chain, addr) = rest
        .split_once(':')
        .ok_or_else(|| X402Error::BadParams(format!("asset {s} missing chain or address")))?;
    if chain.is_empty() || addr.is_empty() {
        return Err(X402Error::BadParams(format!("asset {s} has an empty chain or address")));
    }
    Ok((chain.to_string(), addr.to_string()))
}

/// Best-effort EIP-712 domain chainId for a network id (extend as needed).
fn chain_id_for(network: &str) -> u64 {
    match network {
        "base" => 8453,
        "base-sepolia" => 84532,
        "ethereum" | "mainnet" => 1,
        _ => 0,
    }
}

/// Map an invoice `Amount` to atomic token units: the exponent MUST equal the
/// token's decimals negated (pay §5.1), and the mantissa MUST be a
/// representable non-negative atomic-unit count.
fn to_atomic(invoice: &PayInvoice, token_decimals: u32) -> Result<u128, X402Error> {
    if invoice.amount.amount.exponent != -(token_decimals as i32) {
        return Err(X402Error::ExponentMismatch);
    }
    if invoice.amount.amount.mantissa < 0 {
        return Err(X402Error::ValueOverflow);
    }
    Ok(invoice.amount.amount.mantissa as u128)
}

/// Payer: build an x402 `PayProof.3` payload settling `invoice` under `params`.
/// `token_decimals` fixes the atomic-unit scale (pay §5.1); `now_s` and
/// `params.max_validity_s` set the authorization validity window.
pub fn build_proof(
    signer: &dyn EvmSigner,
    params: &X402Params,
    quote_asset: &Asset,
    invoice: &PayInvoice,
    token_decimals: u32,
    now_s: u64,
) -> Result<Vec<u8>, X402Error> {
    let value_atomic = to_atomic(invoice, token_decimals)?;
    let (_chain, token) = parse_erc20(quote_asset)?;
    let nonce = x402_nonce(&invoice.invoice_id);
    let valid_before = now_s.saturating_add(params.max_validity_s.unwrap_or(600));
    let auth = TransferAuthorization {
        from: signer.address(),
        to: params.pay_to.clone(),
        value_atomic,
        valid_after: now_s,
        valid_before,
        nonce,
        token,
        chain_id: chain_id_for(&params.network),
    };
    let signature = signer.sign_authorization(&auth)?;
    let payload = X402Payload {
        version: params.version,
        scheme: params.scheme.clone(),
        network: params.network.clone(),
        signature,
        from: auth.from,
        to: auth.to,
        value_atomic,
        valid_after: auth.valid_after,
        valid_before: auth.valid_before,
        nonce,
    };
    Ok(X402Proof { payload: payload.to_json_bytes() }.to_bytes())
}

/// Payee: run the pay §5.1 verification procedure IN ORDER, then settle.
/// Returns the wire code (6008 structural / 6002 settle) on rejection.
pub fn verify_and_settle(
    facilitator: &dyn Facilitator,
    params: &X402Params,
    quote_asset: &Asset,
    invoice: &PayInvoice,
    token_decimals: u32,
    proof_bytes: &[u8],
    now_s: u64,
) -> Result<Settlement, u16> {
    verify_and_settle_inner(facilitator, params, quote_asset, invoice, token_decimals, proof_bytes, now_s)
        .map_err(|e| wire_code(&e))
}

fn verify_and_settle_inner(
    facilitator: &dyn Facilitator,
    params: &X402Params,
    quote_asset: &Asset,
    invoice: &PayInvoice,
    token_decimals: u32,
    proof_bytes: &[u8],
    now_s: u64,
) -> Result<Settlement, X402Error> {
    // Fail-closed on params surface: only x402Version 1 / scheme "exact".
    if params.version != X402_VERSION {
        return Err(X402Error::BadParams(format!("unsupported x402Version {}", params.version)));
    }
    if params.scheme != SCHEME_EXACT {
        return Err(X402Error::BadParams(format!("unsupported scheme {}", params.scheme)));
    }
    let proof = X402Proof::from_bytes(proof_bytes)?;
    let payload = X402Payload::from_json_bytes(&proof.payload)?;

    // version / scheme / network / payTo.
    if payload.version != params.version || payload.scheme != params.scheme || payload.network != params.network {
        return Err(X402Error::BadParams("payload version/scheme/network != params".into()));
    }
    if payload.to != params.pay_to {
        return Err(X402Error::BadParams("payload `to` != params payTo".into()));
    }
    // token vs Asset: the quote Asset MUST be erc20 on the params network.
    let (chain, token) = parse_erc20(quote_asset)?;
    if chain != params.network {
        return Err(X402Error::BadParams(format!("asset chain {chain} != network {}", params.network)));
    }
    // value vs invoice mantissa (atomic units).
    let expected_value = to_atomic(invoice, token_decimals)?;
    if payload.value_atomic != expected_value {
        return Err(X402Error::BadParams("payload value != invoice amount".into()));
    }
    // the invoice-binding nonce MUST (pay §5.1).
    if payload.nonce != x402_nonce(&invoice.invoice_id) {
        return Err(X402Error::NonceMismatch);
    }
    // validity window covers now.
    if payload.valid_after > now_s || payload.valid_before <= now_s {
        return Err(X402Error::Expired);
    }

    let expected = ExpectedPayment {
        pay_to: params.pay_to.clone(),
        value_atomic: expected_value,
        nonce: payload.nonce,
        token,
        chain_id: chain_id_for(&params.network),
    };
    // Facilitator verify (chain/signature) THEN settle — /verify never settles.
    facilitator.verify(&proof.payload, params, &expected)?;
    facilitator.settle(&proof.payload, params)
}

// ---- canned implementations (always-on; tests + offline demo) -------------

/// A deterministic signer for tests/offline: a fixed address and a
/// non-cryptographic but stable 65-byte signature. NOT a real EVM key.
pub struct CannedSigner {
    address: String,
}

impl CannedSigner {
    pub fn new() -> Self {
        CannedSigner { address: "0x00000000000000000000000000000000c0ffee01".into() }
    }
}

impl Default for CannedSigner {
    fn default() -> Self {
        Self::new()
    }
}

impl EvmSigner for CannedSigner {
    fn address(&self) -> String {
        self.address.clone()
    }
    fn sign_authorization(&self, auth: &TransferAuthorization) -> Result<Vec<u8>, X402Error> {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(auth.from.as_bytes());
        h.update(auth.to.as_bytes());
        h.update(auth.value_atomic.to_be_bytes());
        h.update(auth.valid_after.to_be_bytes());
        h.update(auth.valid_before.to_be_bytes());
        h.update(auth.nonce);
        h.update(auth.token.as_bytes());
        h.update(auth.chain_id.to_be_bytes());
        let r: [u8; 32] = h.finalize().into();
        let s: [u8; 32] = Sha256::digest(r).into();
        let mut sig = Vec::with_capacity(65);
        sig.extend_from_slice(&r);
        sig.extend_from_slice(&s);
        sig.push(27); // canonical EIP-155-style v placeholder
        Ok(sig)
    }
}

/// An offline facilitator: re-affirms the MAP-level bindings and "settles" by
/// recording the nonce. A second settle of the same nonce fails — simulating
/// the chain's single-use-nonce rule, proving MAP's invoice-id dedup is
/// layered on top, not load-bearing.
pub struct CannedFacilitator {
    settled: RefCell<HashSet<[u8; 32]>>,
}

impl CannedFacilitator {
    pub fn new() -> Self {
        CannedFacilitator { settled: RefCell::new(HashSet::new()) }
    }
}

impl Default for CannedFacilitator {
    fn default() -> Self {
        Self::new()
    }
}

impl Facilitator for CannedFacilitator {
    fn verify(&self, payload: &[u8], _params: &X402Params, expected: &ExpectedPayment) -> Result<(), X402Error> {
        let p = X402Payload::from_json_bytes(payload)?;
        if p.to != expected.pay_to || p.value_atomic != expected.value_atomic || p.nonce != expected.nonce {
            return Err(X402Error::Facilitator("payload does not match expected payment".into()));
        }
        Ok(())
    }
    fn settle(&self, payload: &[u8], _params: &X402Params) -> Result<Settlement, X402Error> {
        let p = X402Payload::from_json_bytes(payload)?;
        let mut settled = self.settled.borrow_mut();
        if !settled.insert(p.nonce) {
            return Err(X402Error::Facilitator("nonce already settled (single-use)".into()));
        }
        Ok(Settlement { tx_ref: crate::payload::to_hex0x(&p.nonce) })
    }
}
