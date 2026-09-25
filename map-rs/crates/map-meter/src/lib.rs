//! MAP metering bound to evidence (spec/layer.md §9).
//!
//! Money moves on an external rail (MPP sessions, x402 V2 batch-settlement)
//! via payer-signed cumulative vouchers. This crate holds only what MAP itself
//! defines: exact decimal amounts, asset identifiers, the cumulative `Usage`
//! vocabulary shared by checkpoints and receipts, and the exact price of a
//! usage figure under a rate table. No rounding rule exists anywhere: every
//! price is an exact `uint counter × decimal-fraction rate` product.
//!
//! The v1 settlement messages (quote/agree/invoice/proof/receipt), escrow,
//! credit ledger and disputes are retired (tag `map-v1-ietf`); the voucher
//! mapping (`voucher-digest`, spec/mapping-payment-rails.md) lands in P4.

use map_core::cbor::{CborError, Value};
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum MeterError {
    Cbor(CborError),
    BadAmount,
    BadAsset(String),
    /// A content map was missing a required field.
    MissingField(u64),
    /// A content field had the wrong type/shape.
    BadField(u64),
}

impl fmt::Display for MeterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeterError::Cbor(e) => write!(f, "cbor: {e}"),
            MeterError::BadAmount => write!(f, "invalid amount encoding"),
            MeterError::BadAsset(s) => write!(f, "invalid asset identifier: {s}"),
            MeterError::MissingField(k) => write!(f, "missing field {k}"),
            MeterError::BadField(k) => write!(f, "field {k} invalid"),
        }
    }
}
impl std::error::Error for MeterError {}
impl From<CborError> for MeterError {
    fn from(e: CborError) -> Self {
        MeterError::Cbor(e)
    }
}

/// CBOR decimal fraction (tag 4): value = mantissa × 10^exponent.
///
/// Layer rule M-4 makes amounts non-negative (reviewer finding R4-01); the
/// mantissa type and decoder enforce that in P3.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Amount {
    pub exponent: i32,
    pub mantissa: i64,
}

impl Amount {
    pub fn new(exponent: i32, mantissa: i64) -> Self {
        Amount { exponent, mantissa }
    }

    pub fn to_value(&self) -> Value {
        Value::Tag(
            4,
            Box::new(Value::Array(vec![
                Value::int(self.exponent as i64),
                Value::int(self.mantissa),
            ])),
        )
    }

    pub fn from_value(v: &Value) -> Result<Amount, MeterError> {
        let Value::Tag(4, inner) = v else {
            return Err(MeterError::BadAmount);
        };
        let arr = inner.as_array().ok_or(MeterError::BadAmount)?;
        let [e, m] = arr else {
            return Err(MeterError::BadAmount);
        };
        let exponent = e
            .as_i128()
            .and_then(|x| i32::try_from(x).ok())
            .ok_or(MeterError::BadAmount)?;
        let mantissa = m
            .as_i128()
            .and_then(|x| i64::try_from(x).ok())
            .ok_or(MeterError::BadAmount)?;
        Ok(Amount { exponent, mantissa })
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let neg = self.mantissa < 0;
        let digits = self.mantissa.unsigned_abs().to_string();
        let body = if self.exponent >= 0 {
            format!("{digits}{}", "0".repeat(self.exponent as usize))
        } else {
            let frac = (-self.exponent) as usize;
            if digits.len() > frac {
                let (int_part, frac_part) = digits.split_at(digits.len() - frac);
                format!("{int_part}.{frac_part}")
            } else {
                format!("0.{}{}", "0".repeat(frac - digits.len()), digits)
            }
        };
        write!(f, "{}{}", if neg { "-" } else { "" }, body)
    }
}

/// Asset identifier (layer §9.1): `iso4217:USD`, `asset:sat`,
/// `erc20:<chain>:<addr>`, `credit:<mesh>`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Asset(String);

impl Asset {
    pub fn new(s: &str) -> Result<Asset, MeterError> {
        const PREFIXES: [&str; 4] = ["iso4217:", "asset:", "erc20:", "credit:"];
        if PREFIXES
            .iter()
            .any(|p| s.starts_with(p) && s.len() > p.len())
        {
            Ok(Asset(s.to_string()))
        } else {
            Err(MeterError::BadAsset(s.to_string()))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `[Asset, Amount]`: the pairing used for rates and priced totals.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetAmount {
    pub asset: Asset,
    pub amount: Amount,
}

impl AssetAmount {
    pub fn to_value(&self) -> Value {
        Value::Array(vec![
            Value::Text(self.asset.as_str().to_string()),
            self.amount.to_value(),
        ])
    }
    pub fn from_value(v: &Value) -> Result<AssetAmount, MeterError> {
        let arr = v.as_array().ok_or(MeterError::BadAmount)?;
        let [a, m] = arr else {
            return Err(MeterError::BadAmount);
        };
        let asset = Asset::new(a.as_text().ok_or(MeterError::BadAmount)?)?;
        let amount = Amount::from_value(m)?;
        Ok(AssetAmount { asset, amount })
    }
}

/// Rate-table unit codes (layer §9.2). Each prices a SINGLE counted unit, so
/// a price is always an exact `uint counter × Amount` product.
pub const UNIT_FLAT: u64 = 0;
pub const UNIT_TOKEN_IN: u64 = 1;
pub const UNIT_TOKEN_OUT: u64 = 2;
pub const UNIT_MS: u64 = 3;
pub const UNIT_INVOCATION: u64 = 4;

/// Cumulative usage since task start (layer §6.1 `Usage`; CheckpointCore key
/// 6, ReceiptCore key 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Usage {
    pub tokens_in: u64,  // 1
    pub tokens_out: u64, // 2
    pub ttft_ms: u64,    // 3
    pub wall_ms: u64,    // 4
}

impl Usage {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Uint(self.tokens_in)),
            (Value::Uint(2), Value::Uint(self.tokens_out)),
            (Value::Uint(3), Value::Uint(self.ttft_ms)),
            (Value::Uint(4), Value::Uint(self.wall_ms)),
        ])
    }
    pub fn from_value(v: &Value) -> Result<Usage, MeterError> {
        let uint = |k: u64| {
            v.map_get(k)
                .ok_or(MeterError::MissingField(k))?
                .as_uint()
                .ok_or(MeterError::BadField(k))
        };
        Ok(Usage {
            tokens_in: uint(1)?,
            tokens_out: uint(2)?,
            ttft_ms: uint(3)?,
            wall_ms: uint(4)?,
        })
    }
}

/// Why a price could not be computed exactly.
#[derive(Debug, PartialEq, Eq)]
pub enum BillingError {
    /// The rate table lacks a unit the pricing model requires.
    MissingRate(u64),
    /// Rates the model combines disagree on decimal exponent (same-exponent rule, M-3).
    ExponentMismatch,
    /// Rates the model combines are in different assets (M-3).
    AssetMismatch,
    /// A `counter × mantissa` product or a sum overflowed the mantissa range.
    Overflow,
    /// The pricing model has no usage-based rule (0 flat, or unknown).
    UnsupportedModel(u64),
}

fn rate(table: &[(u64, AssetAmount)], unit: u64) -> Result<&AssetAmount, BillingError> {
    table
        .iter()
        .find(|(u, _)| *u == unit)
        .map(|(_, aa)| aa)
        .ok_or(BillingError::MissingRate(unit))
}

/// Exact `counter × mantissa` in i128 (the final mantissa is range-checked).
fn term(counter: u64, mantissa: i64) -> Result<i128, BillingError> {
    (counter as i128)
        .checked_mul(mantissa as i128)
        .ok_or(BillingError::Overflow)
}

fn make(exponent: i32, asset: &Asset, total: i128) -> Result<AssetAmount, BillingError> {
    let mantissa = i64::try_from(total).map_err(|_| BillingError::Overflow)?;
    Ok(AssetAmount { asset: asset.clone(), amount: Amount::new(exponent, mantissa) })
}

/// The exact price of cumulative `usage` under `model` and `rate_table`
/// (layer §9, M-3): model 1 per-token `in×rate[1] + out×rate[2]`; model 2
/// per-ms `wall_ms×rate[3]`; model 3 per-invocation `1×rate[4]`. Model 0
/// (flat) and unknown models are errors: a flat total is not a function of
/// usage.
pub fn billed(model: u64, rate_table: &[(u64, AssetAmount)], usage: &Usage) -> Result<AssetAmount, BillingError> {
    match model {
        1 => {
            let ri = rate(rate_table, UNIT_TOKEN_IN)?;
            let ro = rate(rate_table, UNIT_TOKEN_OUT)?;
            if ri.asset != ro.asset {
                return Err(BillingError::AssetMismatch);
            }
            if ri.amount.exponent != ro.amount.exponent {
                return Err(BillingError::ExponentMismatch);
            }
            let total = term(usage.tokens_in, ri.amount.mantissa)?
                .checked_add(term(usage.tokens_out, ro.amount.mantissa)?)
                .ok_or(BillingError::Overflow)?;
            make(ri.amount.exponent, &ri.asset, total)
        }
        2 => {
            let r = rate(rate_table, UNIT_MS)?;
            make(r.amount.exponent, &r.asset, term(usage.wall_ms, r.amount.mantissa)?)
        }
        3 => {
            let r = rate(rate_table, UNIT_INVOCATION)?;
            make(r.amount.exponent, &r.asset, r.amount.mantissa as i128) // one invocation
        }
        other => Err(BillingError::UnsupportedModel(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_display_and_roundtrip() {
        let cases = [
            (Amount::new(-2, 1995), "19.95"),
            (Amount::new(-2, -5), "-0.05"),
            (Amount::new(0, 42), "42"),
            (Amount::new(3, 5), "5000"),
            (Amount::new(-4, 12), "0.0012"),
        ];
        for (a, s) in cases {
            assert_eq!(a.to_string(), s);
            assert_eq!(Amount::from_value(&a.to_value()).unwrap(), a);
        }
        assert!(Amount::from_value(&Value::Uint(5)).is_err());
    }

    #[test]
    fn assets_validated() {
        assert!(Asset::new("iso4217:USD").is_ok());
        assert!(Asset::new("credit:example-mesh").is_ok());
        assert!(Asset::new("USD").is_err());
        assert!(Asset::new("erc20:").is_err());
    }

    #[test]
    fn usage_roundtrip_and_missing_field() {
        let u = Usage { tokens_in: 1, tokens_out: 2, ttft_ms: 3, wall_ms: 4 };
        assert_eq!(Usage::from_value(&u.to_value()).unwrap(), u);
        let partial = Value::Map(vec![(Value::Uint(1), Value::Uint(1))]);
        assert_eq!(Usage::from_value(&partial), Err(MeterError::MissingField(2)));
    }

    fn micro(mantissa: i64) -> AssetAmount {
        AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-6, mantissa) }
    }
    /// 3000 µUSD/token-in, 5000 µUSD/token-out (the v1 TV-016 rate table).
    fn rates() -> Vec<(u64, AssetAmount)> {
        vec![(UNIT_TOKEN_IN, micro(3000)), (UNIT_TOKEN_OUT, micro(5000))]
    }
    fn usage() -> Usage {
        Usage { tokens_in: 1523, tokens_out: 812, ttft_ms: 431, wall_ms: 42000 }
    }

    #[test]
    fn model1_exact_product() {
        // 1523×3000 + 812×5000 = 4_569_000 + 4_060_000 = 8_629_000.
        assert_eq!(billed(1, &rates(), &usage()).unwrap(), micro(8_629_000));
    }

    #[test]
    fn model2_per_millisecond_and_model3_per_invocation() {
        let r2 = vec![(UNIT_MS, micro(2))];
        assert_eq!(billed(2, &r2, &usage()).unwrap(), micro(84_000));
        let r3 = vec![(UNIT_INVOCATION, micro(500_000))];
        assert_eq!(billed(3, &r3, &usage()).unwrap(), micro(500_000));
    }

    #[test]
    fn zero_usage_bills_zero() {
        assert_eq!(billed(1, &rates(), &Usage::default()).unwrap(), micro(0));
    }

    #[test]
    fn missing_rate_and_bad_model_error() {
        assert_eq!(billed(1, &[(UNIT_TOKEN_IN, micro(3000))], &usage()), Err(BillingError::MissingRate(UNIT_TOKEN_OUT)));
        assert_eq!(billed(0, &rates(), &usage()), Err(BillingError::UnsupportedModel(0)));
    }

    #[test]
    fn mixed_exponent_rates_rejected() {
        let bad = vec![(UNIT_TOKEN_IN, micro(3000)), (UNIT_TOKEN_OUT, AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, 5) })];
        assert_eq!(billed(1, &bad, &usage()), Err(BillingError::ExponentMismatch));
    }

    #[test]
    fn overflow_is_checked_not_wrapped() {
        let huge = vec![(UNIT_MS, micro(i64::MAX))];
        let u = Usage { wall_ms: u64::MAX, ..Default::default() };
        assert_eq!(billed(2, &huge, &u), Err(BillingError::Overflow));
    }
}
