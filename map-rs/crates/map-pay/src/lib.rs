//! MAP pay bounded context (draft-map-pay-01).
//!
//! Data semantics (pay §1): the settlement rail is the source of truth for
//! money movement; this crate holds the *statements about* it. Amounts are
//! integer decimal fractions — floats never touch wire structures (core
//! §4.2). Invoice settlement is idempotent: re-delivered proofs must not
//! double-settle (pay §4).

use map_core::cbor::{CborError, Value};
use std::collections::HashSet;
use std::fmt;

pub mod billing;
pub mod codec;
pub mod credit_ledger;

#[derive(Debug, PartialEq)]
pub enum PayError {
    Cbor(CborError),
    BadAmount,
    BadAsset(String),
    /// A pay message content map was missing a required field.
    MissingField(u64),
    /// A pay message content field had the wrong type/shape.
    BadField(u64),
}

impl fmt::Display for PayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PayError::Cbor(e) => write!(f, "cbor: {e}"),
            PayError::BadAmount => write!(f, "invalid amount encoding"),
            PayError::BadAsset(s) => write!(f, "invalid asset identifier: {s}"),
            PayError::MissingField(k) => write!(f, "pay message missing field {k}"),
            PayError::BadField(k) => write!(f, "pay message field {k} invalid"),
        }
    }
}
impl std::error::Error for PayError {}
impl From<CborError> for PayError {
    fn from(e: CborError) -> Self {
        PayError::Cbor(e)
    }
}

/// CBOR decimal fraction (tag 4): value = mantissa × 10^exponent.
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

    pub fn from_value(v: &Value) -> Result<Amount, PayError> {
        let Value::Tag(4, inner) = v else {
            return Err(PayError::BadAmount);
        };
        let arr = inner.as_array().ok_or(PayError::BadAmount)?;
        let [e, m] = arr else {
            return Err(PayError::BadAmount);
        };
        let exponent = e
            .as_i128()
            .and_then(|x| i32::try_from(x).ok())
            .ok_or(PayError::BadAmount)?;
        let mantissa = m
            .as_i128()
            .and_then(|x| i64::try_from(x).ok())
            .ok_or(PayError::BadAmount)?;
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

/// Asset identifier (pay §1): `iso4217:USD`, `asset:sat`,
/// `erc20:<chain>:<addr>`, `credit:<mesh>`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Asset(String);

impl Asset {
    pub fn new(s: &str) -> Result<Asset, PayError> {
        const PREFIXES: [&str; 4] = ["iso4217:", "asset:", "erc20:", "credit:"];
        if PREFIXES
            .iter()
            .any(|p| s.starts_with(p) && s.len() > p.len())
        {
            Ok(Asset(s.to_string()))
        } else {
            Err(PayError::BadAsset(s.to_string()))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct InvoiceId(pub [u8; 16]);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct QuoteId(pub [u8; 16]);

#[derive(Debug, PartialEq)]
pub enum Settle {
    FirstTime,
    /// Duplicate proof delivery — observable effect exactly once (pay §4).
    AlreadySettled,
}

/// Payee-side settlement dedup: invoices settle at most once no matter how
/// many times a proof is (re)delivered.
#[derive(Default)]
pub struct SettlementLedger {
    settled: HashSet<InvoiceId>,
}

impl SettlementLedger {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn settle_once(&mut self, invoice: InvoiceId) -> Settle {
        if self.settled.insert(invoice) {
            Settle::FirstTime
        } else {
            Settle::AlreadySettled
        }
    }
    pub fn is_settled(&self, invoice: &InvoiceId) -> bool {
        self.settled.contains(invoice)
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
    fn settlement_is_idempotent() {
        let mut ledger = SettlementLedger::new();
        let inv = InvoiceId([7u8; 16]);
        assert_eq!(ledger.settle_once(inv), Settle::FirstTime);
        assert_eq!(ledger.settle_once(inv), Settle::AlreadySettled);
        assert!(ledger.is_settled(&inv));
    }
}
