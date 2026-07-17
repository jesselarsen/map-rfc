//! `pay:x402` settlement-method adapter (draft-map-pay-01 §5.1).
//!
//! MAP defines opaque containers — `MethodOffer.2` params and `PayProof.3`
//! proof payload — and this crate defines what goes inside them for the x402
//! method: the CBOR param/proof codecs, the invoice-binding nonce, and the
//! payer/payee flows over `EvmSigner` / `Facilitator` trait seams. The x402
//! PaymentPayload is a small fixed JSON object produced and consumed ONLY by
//! this crate (facilitators settle it verbatim, never produce it), so it is
//! serialised by hand — consistent with the project's hand-rolled CBOR — and
//! the always-on path (codec, nonce, flows, canned impls) carries ZERO
//! external dependencies. secp256k1/EIP-712 signing and facilitator HTTP live
//! behind the `real-x402` feature (see `real.rs`); they are the workspace's
//! first non-Ed25519 crypto, gated hard.

pub mod codec;
pub mod flow;
pub mod http402;
pub mod payload;

#[cfg(feature = "real-x402")]
pub mod real;

#[cfg(test)]
mod tests;

pub use codec::{X402Params, X402Proof};
pub use flow::{
    build_proof, verify_and_settle, CannedFacilitator, CannedSigner, EvmSigner, ExpectedPayment,
    Facilitator, Settlement,
};
pub use http402::{
    pay_402_loop, parse_payment_requirements, upstream_nonce, CannedMode, CannedUpstream,
    HttpUpstream, LoopFailure, PaidResponse, PaymentEvidence, PaymentRequirements, SpendPolicy,
    UpstreamError, UpstreamResponse,
};
pub use payload::{TransferAuthorization, X402Payload};

use sha2::{Digest, Sha256};

/// Method URI (pay §5.1 registry).
pub const X402_METHOD: &str = "pay:x402";
/// x402Version this revision implements.
pub const X402_VERSION: u64 = 1;
/// The only scheme this revision implements (pay §5.1).
pub const SCHEME_EXACT: &str = "exact";

/// The invoice-binding nonce (pay §5.1 MUST): `SHA-256("MAP-x402-nonce" ||
/// invoice-id)`. Binds an EIP-3009 authorization to exactly one MAP invoice;
/// the chain's single-use-nonce rule then gives exactly-once settlement,
/// layered under MAP's own invoice-id idempotency.
pub fn x402_nonce(invoice_id: &[u8; 16]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"MAP-x402-nonce");
    h.update(invoice_id);
    h.finalize().into()
}

/// Everything that can go wrong producing or checking an x402 proof. The payee
/// surface ([`verify_and_settle`]) maps these to the pay §5.1 wire codes:
/// structural failures → 6008 INVALID_PAYMENT_PROOF, a settle failure → 6002
/// SETTLEMENT_FAILED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402Error {
    /// CBOR/JSON structural failure (container or payload malformed).
    Codec(String),
    /// A field disagrees with the offered params (version/scheme/network/payTo)
    /// or the quote Asset (token/chain).
    BadParams(String),
    /// The invoice mantissa is negative or exceeds the u128 atomic-unit range.
    ValueOverflow,
    /// The Amount exponent is not the token's decimals negated (pay §5.1).
    ExponentMismatch,
    /// The authorization nonce is not `x402_nonce(invoice-id)` (the §5.1 MUST).
    NonceMismatch,
    /// The authorization validity window does not cover the verification time.
    Expired,
    /// The signer could not produce an authorization (gated real signer).
    Signing(String),
    /// The facilitator declined to verify or settle.
    Facilitator(String),
    /// An upstream 402 demanded more than the gateway's per-task ceiling
    /// (ops §5.1 G-7): refused fail-closed BEFORE signing. Added TASK-025.
    PriceCapExceeded,
}

impl std::fmt::Display for X402Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            X402Error::Codec(m) => write!(f, "x402 codec: {m}"),
            X402Error::BadParams(m) => write!(f, "x402 params: {m}"),
            X402Error::ValueOverflow => write!(f, "x402 value out of atomic-unit range"),
            X402Error::ExponentMismatch => write!(f, "x402 amount exponent != token decimals"),
            X402Error::NonceMismatch => write!(f, "x402 authorization nonce != MAP-x402-nonce(invoice)"),
            X402Error::Expired => write!(f, "x402 authorization validity window does not cover now"),
            X402Error::Signing(m) => write!(f, "x402 signing: {m}"),
            X402Error::Facilitator(m) => write!(f, "x402 facilitator: {m}"),
            X402Error::PriceCapExceeded => write!(f, "x402 upstream demand exceeds the per-task spend ceiling"),
        }
    }
}
impl std::error::Error for X402Error {}

/// Payee-side mapping of an adapter error to its pay §5.1 wire code.
pub fn wire_code(e: &X402Error) -> u16 {
    match e {
        X402Error::Facilitator(_) => map_core::errors::codes::SETTLEMENT_FAILED, // 6002
        _ => map_core::errors::codes::INVALID_PAYMENT_PROOF,                     // 6008
    }
}
