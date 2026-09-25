//! Binding metering evidence to payment-rail vouchers (spec/layer.md §9.3,
//! spec/mapping-payment-rails.md). MAP never mints vouchers. It computes the
//! `voucher-digest` a checkpoint (key 9) or receipt (key 10) carries, the
//! exact atomic-unit amount a rail expects, and the ceiling a requester
//! should not exceed when signing the next voucher (layer M-6).

use crate::{billed, Amount, Asset, AssetAmount, BillingError, Usage};
use map_core::cbor::{encode, Value};
use sha2::{Digest, Sha256};

/// `0x12 0x20 || SHA-256(data)`.
fn multihash_sha256(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x12, 0x20];
    out.extend_from_slice(&Sha256::digest(data));
    out
}

/// RAIL-1: a rail with its own canonical signed-authorization bytes (a
/// serialized transaction, an EIP-712 typed-data hash) is digested exactly
/// as-is, never re-encoded by MAP.
pub fn digest_rail_bytes(rail_bytes: &[u8]) -> Vec<u8> {
    multihash_sha256(rail_bytes)
}

/// RAIL-5: MPP Tempo session fallback digest over the named fields, in the
/// fixed order `[channelId, [exponent, mantissa], reference, method]`.
pub fn digest_mpp_tempo(channel_id: &[u8], cumulative: &Amount, reference: &str, method: &str) -> Vec<u8> {
    let fields = Value::Array(vec![
        Value::Bytes(channel_id.to_vec()),
        Value::Array(vec![Value::int(cumulative.exponent as i64), Value::Uint(cumulative.mantissa)]),
        Value::Text(reference.to_string()),
        Value::Text(method.to_string()),
    ]);
    multihash_sha256(&encode(&fields).expect("array of primitives encodes"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum VoucherError {
    /// RAIL-3: the amount's exponent is not exactly `-decimals`; converting
    /// would need rounding, which is never done.
    InexactUnits { exponent: i32, decimals: u32 },
    AssetMismatch,
    ExponentMismatch,
    Billing(BillingError),
    Overflow,
}

impl From<BillingError> for VoucherError {
    fn from(e: BillingError) -> Self {
        VoucherError::Billing(e)
    }
}

/// RAIL-2/RAIL-3: an amount in a rail's atomic units (e.g. USDC, 6
/// decimals). Exact by construction: the exponent must equal `-decimals`.
pub fn to_atomic_units(amount: &Amount, decimals: u32) -> Result<u64, VoucherError> {
    if amount.exponent != -(decimals as i32) {
        return Err(VoucherError::InexactUnits { exponent: amount.exponent, decimals });
    }
    Ok(amount.mantissa)
}

/// Layer M-6: the most a requester should authorize in its next cumulative
/// voucher, namely the price of the usage in its latest dual-signed checkpoint
/// plus a deliberate, bounded credit extension.
pub fn voucher_ceiling(
    model: u64,
    rate_table: &[(u64, AssetAmount)],
    acknowledged_usage: &Usage,
    credit: &AssetAmount,
) -> Result<AssetAmount, VoucherError> {
    let owed = billed(model, rate_table, acknowledged_usage)?;
    if owed.asset != credit.asset {
        return Err(VoucherError::AssetMismatch);
    }
    if owed.amount.exponent != credit.amount.exponent {
        return Err(VoucherError::ExponentMismatch);
    }
    let mantissa = owed.amount.mantissa.checked_add(credit.amount.mantissa).ok_or(VoucherError::Overflow)?;
    Ok(AssetAmount { asset: owed.asset, amount: Amount::new(owed.amount.exponent, mantissa) })
}

/// A voucher the requester is about to sign, checked against M-6.
pub fn within_ceiling(voucher_cumulative: &AssetAmount, ceiling: &AssetAmount) -> Result<bool, VoucherError> {
    if voucher_cumulative.asset != ceiling.asset {
        return Err(VoucherError::AssetMismatch);
    }
    if voucher_cumulative.amount.exponent != ceiling.amount.exponent {
        return Err(VoucherError::ExponentMismatch);
    }
    Ok(voucher_cumulative.amount.mantissa <= ceiling.amount.mantissa)
}

/// Convenience: a USDC-style asset identifier helper for tests and demos.
pub fn erc20(chain: &str, address: &str) -> Asset {
    Asset::new(&format!("erc20:{chain}:{address}")).expect("well-formed erc20 asset")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UNIT_TOKEN_IN, UNIT_TOKEN_OUT};

    fn usdc(m: u64) -> AssetAmount {
        AssetAmount { asset: erc20("base", "0xusdc"), amount: Amount::new(-6, m) }
    }

    #[test]
    fn exact_units_or_rejection() {
        assert_eq!(to_atomic_units(&Amount::new(-6, 5200), 6), Ok(5200));
        assert_eq!(
            to_atomic_units(&Amount::new(-2, 5), 6),
            Err(VoucherError::InexactUnits { exponent: -2, decimals: 6 })
        );
    }

    #[test]
    fn m6_ceiling_matches_the_layer_worked_example() {
        // Layer §9.5: 2 and 8 µunits per token in/out; checkpoint usage
        // 1000 in / 400 out = 5200; one more interval of credit = 8000.
        let rates = vec![(UNIT_TOKEN_IN, usdc(2)), (UNIT_TOKEN_OUT, usdc(8))];
        let usage = Usage { tokens_in: 1000, tokens_out: 400, ttft_ms: 0, wall_ms: 0 };
        let ceiling = voucher_ceiling(1, &rates, &usage, &usdc(8000)).unwrap();
        assert_eq!(ceiling, usdc(13_200));
        assert_eq!(within_ceiling(&usdc(13_200), &ceiling), Ok(true));
        assert_eq!(within_ceiling(&usdc(13_201), &ceiling), Ok(false));
    }

    #[test]
    fn digests_are_deterministic_and_field_sensitive() {
        let a = digest_mpp_tempo(&[1, 2, 3], &Amount::new(-6, 5200), "ref-1", "tempo");
        assert_eq!(a, digest_mpp_tempo(&[1, 2, 3], &Amount::new(-6, 5200), "ref-1", "tempo"));
        assert_ne!(a, digest_mpp_tempo(&[1, 2, 3], &Amount::new(-6, 5201), "ref-1", "tempo"));
        assert_eq!(a.len(), 34);
        assert_eq!(&a[..2], &[0x12, 0x20]);
        assert_eq!(digest_rail_bytes(b"tx").len(), 34);
    }
}
