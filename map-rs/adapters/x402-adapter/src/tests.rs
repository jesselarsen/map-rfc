//! Always-on adapter tests (FR-6 + the §9 edge list + the nonce MUST both
//! ways), driving the payer/payee flows against codec-level messages with the
//! canned signer/facilitator. No feature required.

use crate::*;
use map_pay::codec::{pay_ref_digest, AssetAmount, PayInvoice, PayReceipt, UsageWatermark, Usage};
use map_pay::{Amount, Asset};

const NOW: u64 = 1_780_000_000;
const USDC: &str = "erc20:base-sepolia:0x036CbD53842c5426634e7929541eC2318f3dCF7e";

fn params() -> X402Params {
    X402Params {
        version: 1,
        scheme: "exact".into(),
        network: "base-sepolia".into(),
        pay_to: "0x2222222222222222222222222222222222222222".into(),
        facilitator: Some("https://facilitator.example".into()),
        max_validity_s: Some(600),
    }
}

fn asset() -> Asset {
    Asset::new(USDC).unwrap()
}

/// A metered invoice for 8_629_000 atomic USDC (exponent −6 = 6 decimals).
fn invoice(id: u8, mantissa: i64) -> PayInvoice {
    PayInvoice {
        invoice_id: [id; 16],
        quote_id: [0x10; 16],
        amount: AssetAmount { asset: asset(), amount: Amount::new(-6, mantissa) },
        watermark: Some(UsageWatermark {
            task: [0xAB; 32],
            through_seq: 8,
            usage: Usage { tokens_in: 1523, tokens_out: 812, ttft_ms: 431, wall_ms: 42000 },
            cumulative_billed: AssetAmount { asset: asset(), amount: Amount::new(-6, mantissa) },
        }),
        method_payload: vec![],
        timestamp_ms: 0,
    }
}

// ---- FR-3: the invoice-binding nonce --------------------------------------

#[test]
fn nonce_is_sha256_of_domain_and_invoice_id() {
    // Fixed-output test (pay §5.1): SHA-256("MAP-x402-nonce" || invoice-id).
    use sha2::{Digest, Sha256};
    let id = [0x30u8; 16];
    let mut h = Sha256::new();
    h.update(b"MAP-x402-nonce");
    h.update(id);
    let want: [u8; 32] = h.finalize().into();
    assert_eq!(x402_nonce(&id), want);
    // Different invoice → different nonce.
    assert_ne!(x402_nonce(&[0x30; 16]), x402_nonce(&[0x31; 16]));
}

// ---- FR-6: the full MAP-shaped flow ---------------------------------------

#[test]
fn end_to_end_build_then_verify_and_settle_yields_receipt() {
    let signer = CannedSigner::new();
    let fac = CannedFacilitator::new();
    let inv = invoice(0x30, 8_629_000);

    let proof_bytes = build_proof(&signer, &params(), &asset(), &inv, 6, NOW).unwrap();
    let settlement = verify_and_settle(&fac, &params(), &asset(), &inv, 6, &proof_bytes, NOW).unwrap();

    // The payee only now forms the PAY_RECEIPT (settlement succeeded).
    let receipt = PayReceipt {
        invoice_id: inv.invoice_id,
        amount: inv.amount.clone(),
        timestamp_ms: 1,
        proof_digest: Some(pay_ref_digest(&proof_bytes)),
    };
    assert_eq!(receipt.invoice_id, [0x30; 16]);
    assert!(!settlement.tx_ref.is_empty());
    // tx_ref is the settled nonce (single-use anchor).
    assert_eq!(settlement.tx_ref, crate::payload::to_hex0x(&x402_nonce(&inv.invoice_id)));
}

// ---- FR-6 negatives + §9 edge list ----------------------------------------

/// Tamper one byte of the CBOR proof → the payee rejects (6008).
#[test]
fn tampered_proof_byte_rejected_6008() {
    let fac = CannedFacilitator::new();
    let inv = invoice(0x30, 8_629_000);
    let mut proof = build_proof(&CannedSigner::new(), &params(), &asset(), &inv, 6, NOW).unwrap();
    let n = proof.len();
    proof[n - 1] ^= 0xFF;
    assert_eq!(verify_and_settle(&fac, &params(), &asset(), &inv, 6, &proof, NOW), Err(6008));
}

/// A proof whose nonce is NOT x402_nonce(invoice) → 6008 (the §5.1 MUST).
#[test]
fn wrong_nonce_rejected_6008() {
    let fac = CannedFacilitator::new();
    let inv = invoice(0x30, 8_629_000);
    // Build the proof for a DIFFERENT invoice-id (so its nonce binds elsewhere)...
    let other = invoice(0x31, 8_629_000);
    let proof = build_proof(&CannedSigner::new(), &params(), &asset(), &other, 6, NOW).unwrap();
    // ...and present it to settle `inv`: nonce mismatch.
    assert_eq!(verify_and_settle(&fac, &params(), &asset(), &inv, 6, &proof, NOW), Err(6008));
}

/// A correctly-nonce'd proof whose value ≠ the invoice mantissa → 6008.
#[test]
fn value_mismatch_rejected_6008() {
    let fac = CannedFacilitator::new();
    let paid = invoice(0x30, 8_629_000);
    let proof = build_proof(&CannedSigner::new(), &params(), &asset(), &paid, 6, NOW).unwrap();
    // Same invoice-id (so nonce matches) but a larger claimed amount.
    let bigger = invoice(0x30, 9_000_000);
    assert_eq!(verify_and_settle(&fac, &params(), &asset(), &bigger, 6, &proof, NOW), Err(6008));
}

/// Exponent ≠ negated token decimals is rejected before signing (payer side).
#[test]
fn exponent_mismatch_rejected_on_build() {
    // Invoice amount at −2 while USDC is 6 decimals → ExponentMismatch.
    let mut inv = invoice(0x30, 8_629_000);
    inv.amount.amount = Amount::new(-2, 8_629_000);
    let err = build_proof(&CannedSigner::new(), &params(), &asset(), &inv, 6, NOW).unwrap_err();
    assert_eq!(err, X402Error::ExponentMismatch);
}

/// A negative mantissa cannot map to atomic units.
#[test]
fn negative_amount_rejected() {
    let mut inv = invoice(0x30, -1);
    inv.amount.amount = Amount::new(-6, -1);
    assert_eq!(build_proof(&CannedSigner::new(), &params(), &asset(), &inv, 6, NOW).unwrap_err(), X402Error::ValueOverflow);
}

/// An expired authorization (valid_before in the past) → 6008.
#[test]
fn expired_authorization_rejected_6008() {
    let fac = CannedFacilitator::new();
    let inv = invoice(0x30, 8_629_000);
    // Sign at NOW-100 with a 10s validity window → valid_before = NOW-90,
    // which does not cover the verification time NOW.
    let short = X402Params { max_validity_s: Some(10), ..params() };
    let proof = build_proof(&CannedSigner::new(), &short, &asset(), &inv, 6, NOW - 100).unwrap();
    assert_eq!(verify_and_settle(&fac, &params(), &asset(), &inv, 6, &proof, NOW), Err(6008));
}

/// Unknown x402Version / non-"exact" scheme are fail-closed (6008).
#[test]
fn unknown_version_or_scheme_rejected_6008() {
    let fac = CannedFacilitator::new();
    let inv = invoice(0x30, 8_629_000);
    let proof = build_proof(&CannedSigner::new(), &params(), &asset(), &inv, 6, NOW).unwrap();
    let bad_ver = X402Params { version: 2, ..params() };
    assert_eq!(verify_and_settle(&fac, &bad_ver, &asset(), &inv, 6, &proof, NOW), Err(6008));
    let bad_scheme = X402Params { scheme: "upto".into(), ..params() };
    assert_eq!(verify_and_settle(&fac, &bad_scheme, &asset(), &inv, 6, &proof, NOW), Err(6008));
}

/// The Asset chain must match the params network.
#[test]
fn asset_chain_mismatch_rejected_6008() {
    let fac = CannedFacilitator::new();
    let inv = invoice(0x30, 8_629_000);
    let proof = build_proof(&CannedSigner::new(), &params(), &asset(), &inv, 6, NOW).unwrap();
    let mainnet_asset = Asset::new("erc20:base:0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap();
    assert_eq!(verify_and_settle(&fac, &params(), &mainnet_asset, &inv, 6, &proof, NOW), Err(6008));
}

/// A settle failure (here: a re-settled single-use nonce) maps to 6002, and
/// no receipt is formed. Proves MAP's invoice-id dedup is layered, not
/// load-bearing.
#[test]
fn re_settle_same_nonce_is_6002() {
    let fac = CannedFacilitator::new();
    let inv = invoice(0x30, 8_629_000);
    let proof = build_proof(&CannedSigner::new(), &params(), &asset(), &inv, 6, NOW).unwrap();
    assert!(verify_and_settle(&fac, &params(), &asset(), &inv, 6, &proof, NOW).is_ok());
    // The facilitator has already consumed the nonce → second settle 6002.
    assert_eq!(verify_and_settle(&fac, &params(), &asset(), &inv, 6, &proof, NOW), Err(6002));
}

/// A live-facilitator test against a real testnet. Ignored by default; run with
/// `--features real-x402 -- --ignored` after setting X402_FACILITATOR (base
/// URL), X402_NETWORK, X402_TOKEN, X402_PAYTO, and X402_SIGNER_KEY (hex).
#[cfg(feature = "real-x402")]
#[test]
#[ignore = "needs a live x402 facilitator + funded testnet key (see env vars)"]
fn live_facilitator_settles() {
    // Intentionally minimal: exercised manually. The gated real impls are
    // unit-tested for EIP-712 digest correctness in real.rs.
}
