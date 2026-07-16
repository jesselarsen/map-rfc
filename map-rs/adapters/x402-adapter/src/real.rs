//! Real x402 settlement (behind `real-x402`): a secp256k1 EIP-712 signer for
//! EIP-3009 `transferWithAuthorization`, and an HTTP facilitator client.
//!
//! These carry the workspace's only non-Ed25519 crypto (`k256`) and its keccak
//! (`sha3`), plus `ureq`/`serde_json` for the facilitator API — all gated, so
//! the default workspace audit surface is unchanged.
//!
//! x402 reference pinned in the crate README: the PaymentPayload shape and the
//! facilitator `/verify` + `/settle` API follow x402.org / coinbase/x402. The
//! EIP-712 typed data is EIP-3009 (`TransferWithAuthorization`); the token's
//! domain `name`/`version` vary per token and are supplied to `RealSigner`.

use crate::flow::{EvmSigner, ExpectedPayment, Facilitator, Settlement};
use crate::payload::{to_hex0x, TransferAuthorization, X402Payload};
use crate::{X402Error, X402Params};
use k256::ecdsa::{RecoveryId, Signature, SigningKey};
use sha3::{Digest, Keccak256};

fn keccak(bytes: &[u8]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(bytes);
    h.finalize().into()
}

/// abi.encode of a 20-byte address as a 32-byte left-padded word.
fn word_addr(addr: &str) -> Result<[u8; 32], X402Error> {
    let raw = crate::payload::from_hex0x(addr)?;
    if raw.len() != 20 {
        return Err(X402Error::BadParams(format!("address {addr} is not 20 bytes")));
    }
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(&raw);
    Ok(w)
}

/// abi.encode of a u128 as a 32-byte big-endian word.
fn word_u128(v: u128) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&v.to_be_bytes());
    w
}

/// abi.encode of a u64 as a 32-byte big-endian word.
fn word_u64(v: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&v.to_be_bytes());
    w
}

/// The EIP-712 typed-data digest for an EIP-3009 `transferWithAuthorization`
/// under the token's domain. `keccak256(0x1901 || domainSeparator || structHash)`.
pub fn eip712_digest(auth: &TransferAuthorization, domain_name: &str, domain_version: &str) -> Result<[u8; 32], X402Error> {
    // domainSeparator
    let domain_typehash = keccak(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
    let mut dom = Vec::new();
    dom.extend_from_slice(&domain_typehash);
    dom.extend_from_slice(&keccak(domain_name.as_bytes()));
    dom.extend_from_slice(&keccak(domain_version.as_bytes()));
    dom.extend_from_slice(&word_u64(auth.chain_id));
    dom.extend_from_slice(&word_addr(&auth.token)?);
    let domain_sep = keccak(&dom);

    // structHash
    let struct_typehash = keccak(
        b"TransferWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)",
    );
    let mut st = Vec::new();
    st.extend_from_slice(&struct_typehash);
    st.extend_from_slice(&word_addr(&auth.from)?);
    st.extend_from_slice(&word_addr(&auth.to)?);
    st.extend_from_slice(&word_u128(auth.value_atomic));
    st.extend_from_slice(&word_u64(auth.valid_after));
    st.extend_from_slice(&word_u64(auth.valid_before));
    st.extend_from_slice(&auth.nonce);
    let struct_hash = keccak(&st);

    let mut pre = Vec::with_capacity(2 + 64);
    pre.extend_from_slice(&[0x19, 0x01]);
    pre.extend_from_slice(&domain_sep);
    pre.extend_from_slice(&struct_hash);
    Ok(keccak(&pre))
}

/// A secp256k1 EIP-712 signer bound to one ERC-20's domain.
pub struct RealSigner {
    key: SigningKey,
    address: String,
    domain_name: String,
    domain_version: String,
}

impl RealSigner {
    /// From a 32-byte private key and the token's EIP-712 domain (e.g. USDC:
    /// name "USD Coin", version "2").
    pub fn new(private_key: &[u8; 32], domain_name: &str, domain_version: &str) -> Result<Self, X402Error> {
        let key = SigningKey::from_slice(private_key).map_err(|e| X402Error::Signing(e.to_string()))?;
        let vk = key.verifying_key();
        let point = vk.to_encoded_point(false); // uncompressed: 0x04 || X || Y
        let addr = keccak(&point.as_bytes()[1..]);
        Ok(RealSigner {
            key,
            address: to_hex0x(&addr[12..]),
            domain_name: domain_name.to_string(),
            domain_version: domain_version.to_string(),
        })
    }
}

impl EvmSigner for RealSigner {
    fn address(&self) -> String {
        self.address.clone()
    }
    fn sign_authorization(&self, auth: &TransferAuthorization) -> Result<Vec<u8>, X402Error> {
        let digest = eip712_digest(auth, &self.domain_name, &self.domain_version)?;
        let (sig, recid): (Signature, RecoveryId) = self
            .key
            .sign_prehash_recoverable(&digest)
            .map_err(|e| X402Error::Signing(e.to_string()))?;
        let mut out = sig.to_bytes().to_vec(); // 64 bytes: r || s
        out.push(27 + recid.to_byte()); // EIP-155 style v (27/28)
        Ok(out)
    }
}

/// An HTTP x402 facilitator client (`/verify`, `/settle`).
pub struct RealFacilitator {
    base_url: String,
}

impl RealFacilitator {
    pub fn new(base_url: &str) -> Self {
        RealFacilitator { base_url: base_url.trim_end_matches('/').to_string() }
    }

    /// The facilitator request body: `{ x402Version, paymentPayload,
    /// paymentRequirements }` (x402 facilitator API).
    fn body(&self, payload: &[u8], params: &X402Params, expected: &ExpectedPayment) -> Result<serde_json::Value, X402Error> {
        let payload_obj: serde_json::Value =
            serde_json::from_slice(payload).map_err(|e| X402Error::Codec(e.to_string()))?;
        let requirements = serde_json::json!({
            "scheme": params.scheme,
            "network": params.network,
            "maxAmountRequired": expected.value_atomic.to_string(),
            "payTo": params.pay_to,
            "asset": expected.token,
        });
        Ok(serde_json::json!({
            "x402Version": params.version,
            "paymentPayload": payload_obj,
            "paymentRequirements": requirements,
        }))
    }

    fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, X402Error> {
        let resp = ureq::post(&format!("{}{}", self.base_url, path))
            .send_json(body.clone())
            .map_err(|e| X402Error::Facilitator(format!("{path}: {e}")))?;
        resp.into_json::<serde_json::Value>()
            .map_err(|e| X402Error::Facilitator(format!("{path} response: {e}")))
    }
}

impl Facilitator for RealFacilitator {
    fn verify(&self, payload: &[u8], params: &X402Params, expected: &ExpectedPayment) -> Result<(), X402Error> {
        let body = self.body(payload, params, expected)?;
        let resp = self.post("/verify", &body)?;
        if resp.get("isValid").and_then(serde_json::Value::as_bool) == Some(true) {
            Ok(())
        } else {
            let reason = resp.get("invalidReason").and_then(serde_json::Value::as_str).unwrap_or("invalid");
            Err(X402Error::Facilitator(format!("verify rejected: {reason}")))
        }
    }
    fn settle(&self, payload: &[u8], params: &X402Params) -> Result<Settlement, X402Error> {
        // settle re-derives the expected fields from the payload it is given.
        let p = X402Payload::from_json_bytes(payload)?;
        let expected = ExpectedPayment {
            pay_to: params.pay_to.clone(),
            value_atomic: p.value_atomic,
            nonce: p.nonce,
            token: String::new(),
            chain_id: 0,
        };
        let body = self.body(payload, params, &expected)?;
        let resp = self.post("/settle", &body)?;
        if resp.get("success").and_then(serde_json::Value::as_bool) == Some(true) {
            let tx = resp.get("transaction").and_then(serde_json::Value::as_str).unwrap_or_default();
            Ok(Settlement { tx_ref: tx.to_string() })
        } else {
            let reason = resp.get("errorReason").and_then(serde_json::Value::as_str).unwrap_or("settle failed");
            Err(X402Error::Facilitator(format!("settle failed: {reason}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The EIP-712 struct typehash for EIP-3009 TransferWithAuthorization is a
    /// published constant (EIP-3009 / OpenZeppelin EIP3009): a correct keccak
    /// over the exact type string reproduces it. This anchors the digest code.
    #[test]
    fn transfer_with_authorization_typehash_matches_eip3009() {
        let th = keccak(
            b"TransferWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)",
        );
        assert_eq!(
            to_hex0x(&th),
            "0x7c7c6cdb67a18743f49ec6fa9b35f50d52ed05cbed4cc592e13b44501c1a2267"
        );
    }

    #[test]
    fn signer_derives_address_and_signs_deterministically() {
        let sk = [0x11u8; 32];
        let signer = RealSigner::new(&sk, "USD Coin", "2").unwrap();
        assert!(signer.address().starts_with("0x") && signer.address().len() == 42);
        let auth = TransferAuthorization {
            from: signer.address(),
            to: "0x2222222222222222222222222222222222222222".into(),
            value_atomic: 1000,
            valid_after: 0,
            valid_before: 1_800_000_000,
            nonce: [0x33; 32],
            token: "0x036CbD53842c5426634e7929541eC2318f3dCF7e".into(),
            chain_id: 84532,
        };
        let s1 = signer.sign_authorization(&auth).unwrap();
        let s2 = signer.sign_authorization(&auth).unwrap();
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 65);
        assert!(s1[64] == 27 || s1[64] == 28);
    }
}
