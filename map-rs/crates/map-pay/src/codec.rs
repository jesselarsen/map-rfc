//! Payment message and reference codecs (pay §3/§3.1/§4). Pure to_value/
//! from_value; field numbers exactly per the drafts' CDDL. Pay refs are
//! (id, digest) pairs; the digest is `pay_ref_digest` over the referenced
//! message's canonical content (see that fn and map-spec/ERRATA.md).

use crate::{Amount, Asset, PayError};
use map_core::cbor::Value;
use sha2::{Digest, Sha256};

/// `[Asset, Amount]` — the array pairing used throughout pay (rate entries,
/// totals, invoice/receipt amounts, authorized caps).
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
    pub fn from_value(v: &Value) -> Result<AssetAmount, PayError> {
        let arr = v.as_array().ok_or(PayError::BadAmount)?;
        let [a, m] = arr else {
            return Err(PayError::BadAmount);
        };
        let asset = Asset::new(a.as_text().ok_or(PayError::BadAmount)?)?;
        let amount = Amount::from_value(m)?;
        Ok(AssetAmount { asset, amount })
    }
}

/// The payment-reference digest (pay §3.1): multihash-sha256 (`0x12 0x20 ||
/// sha256`) over a message's bytes.
///
/// pay §3.1 specifies "received ENVELOPE bytes (I1)", but the pump/session
/// architecture surfaces parsed message CONTENT, not the sealed envelope frame
/// (neither `Session::send` nor `recv` exposes it). This MVP therefore digests
/// the canonical re-encoding of the referenced message content, which is
/// deterministic and equivalent for the price-switch defense (a different quote
/// → different content → different digest). Recorded in map-spec/ERRATA.md.
pub fn pay_ref_digest(content_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(34);
    out.extend_from_slice(&[0x12, 0x20]);
    out.extend_from_slice(&Sha256::digest(content_bytes));
    out
}

// ---- field helpers --------------------------------------------------------

fn id16(v: &Value, k: u64) -> Result<[u8; 16], PayError> {
    v.map_get(k)
        .ok_or(PayError::MissingField(k))?
        .as_bytes()
        .and_then(|b| <[u8; 16]>::try_from(b).ok())
        .ok_or(PayError::BadField(k))
}
fn uint(v: &Value, k: u64) -> Result<u64, PayError> {
    v.map_get(k)
        .ok_or(PayError::MissingField(k))?
        .as_uint()
        .ok_or(PayError::BadField(k))
}
fn text(v: &Value, k: u64) -> Result<String, PayError> {
    Ok(v.map_get(k)
        .ok_or(PayError::MissingField(k))?
        .as_text()
        .ok_or(PayError::BadField(k))?
        .to_string())
}
fn bytes(v: &Value, k: u64) -> Result<Vec<u8>, PayError> {
    Ok(v.map_get(k)
        .ok_or(PayError::MissingField(k))?
        .as_bytes()
        .ok_or(PayError::BadField(k))?
        .to_vec())
}
fn aa_at(v: &Value, k: u64) -> Result<AssetAmount, PayError> {
    AssetAmount::from_value(v.map_get(k).ok_or(PayError::MissingField(k))?)
}

// ---- MethodOffer ----------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct MethodOffer {
    pub uri: String,
    pub params: Vec<u8>,
}

impl MethodOffer {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Text(self.uri.clone())),
            (Value::Uint(2), Value::Bytes(self.params.clone())),
        ])
    }
    pub fn from_value(v: &Value) -> Result<MethodOffer, PayError> {
        Ok(MethodOffer { uri: text(v, 1)?, params: bytes(v, 2)? })
    }
}

// ---- PayQuote (0x0400) ----------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct PayQuote {
    pub quote_id: [u8; 16],
    pub pricing_model: u64,
    pub rate_table: Vec<(u64, AssetAmount)>,
    pub total: Option<AssetAmount>,
    pub methods: Vec<MethodOffer>,
    pub expiry_ms: u64,
    pub terms: Option<u64>,
    pub ext: Option<Value>,
    pub task_binding: Option<[u8; 32]>,
}

impl PayQuote {
    pub fn to_value(&self) -> Value {
        let rate = Value::Map(
            self.rate_table
                .iter()
                .map(|(u, aa)| (Value::Uint(*u), aa.to_value()))
                .collect(),
        );
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.quote_id.to_vec())),
            (Value::Uint(2), Value::Uint(self.pricing_model)),
            (Value::Uint(3), rate),
        ];
        if let Some(t) = &self.total {
            m.push((Value::Uint(4), t.to_value()));
        }
        m.push((
            Value::Uint(5),
            Value::Array(self.methods.iter().map(MethodOffer::to_value).collect()),
        ));
        m.push((Value::Uint(6), Value::Uint(self.expiry_ms)));
        if let Some(t) = self.terms {
            m.push((Value::Uint(7), Value::Uint(t)));
        }
        if let Some(e) = &self.ext {
            m.push((Value::Uint(8), e.clone()));
        }
        if let Some(tb) = &self.task_binding {
            m.push((Value::Uint(9), Value::Bytes(tb.to_vec())));
        }
        Value::Map(m)
    }

    pub fn from_value(v: &Value) -> Result<PayQuote, PayError> {
        let rate_table = v
            .map_get(3)
            .ok_or(PayError::MissingField(3))?
            .as_map()
            .ok_or(PayError::BadField(3))?
            .iter()
            .map(|(k, val)| {
                Ok((k.as_uint().ok_or(PayError::BadField(3))?, AssetAmount::from_value(val)?))
            })
            .collect::<Result<Vec<_>, PayError>>()?;
        let methods = v
            .map_get(5)
            .ok_or(PayError::MissingField(5))?
            .as_array()
            .ok_or(PayError::BadField(5))?
            .iter()
            .map(MethodOffer::from_value)
            .collect::<Result<Vec<_>, PayError>>()?;
        Ok(PayQuote {
            quote_id: id16(v, 1)?,
            pricing_model: uint(v, 2)?,
            rate_table,
            total: v.map_get(4).map(AssetAmount::from_value).transpose()?,
            methods,
            expiry_ms: uint(v, 6)?,
            terms: v.map_get(7).and_then(|x| x.as_uint()),
            ext: v.map_get(8).cloned(),
            task_binding: match v.map_get(9) {
                None => None,
                Some(x) => Some(
                    x.as_bytes()
                        .and_then(|b| <[u8; 32]>::try_from(b).ok())
                        .ok_or(PayError::BadField(9))?,
                ),
            },
        })
    }
}

// ---- PayAgree (0x0401) ----------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct PayAgree {
    pub quote_id: [u8; 16],
    pub quote_digest: Vec<u8>,
    pub method: String,
    pub authorized_cap: Option<AssetAmount>,
}

impl PayAgree {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.quote_id.to_vec())),
            (Value::Uint(2), Value::Bytes(self.quote_digest.clone())),
            (Value::Uint(3), Value::Text(self.method.clone())),
        ];
        if let Some(c) = &self.authorized_cap {
            m.push((Value::Uint(4), c.to_value()));
        }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<PayAgree, PayError> {
        Ok(PayAgree {
            quote_id: id16(v, 1)?,
            quote_digest: bytes(v, 2)?,
            method: text(v, 3)?,
            authorized_cap: v.map_get(4).map(AssetAmount::from_value).transpose()?,
        })
    }
}

// ---- refs (pay §3.1) ------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct PayQuoteRef {
    pub quote_id: [u8; 16],
    pub digest: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PayAgreeRef {
    pub quote_id: [u8; 16],
    pub digest: Vec<u8>,
}

macro_rules! id_digest_ref {
    ($t:ty) => {
        impl $t {
            pub fn to_value(&self) -> Value {
                Value::Map(vec![
                    (Value::Uint(1), Value::Bytes(self.quote_id.to_vec())),
                    (Value::Uint(2), Value::Bytes(self.digest.clone())),
                ])
            }
            pub fn from_value(v: &Value) -> Result<Self, PayError> {
                Ok(Self { quote_id: id16(v, 1)?, digest: bytes(v, 2)? })
            }
        }
    };
}
id_digest_ref!(PayQuoteRef);
id_digest_ref!(PayAgreeRef);

/// Pre-quote pricing expectation carried in `Offer.4` (at least one member).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PayRef {
    pub model: Option<u64>,
    pub max_total: Option<AssetAmount>,
    pub methods: Option<Vec<String>>,
    pub agree_ref: Option<PayAgreeRef>,
}

impl PayRef {
    pub fn to_value(&self) -> Value {
        let mut m = Vec::new();
        if let Some(model) = self.model {
            m.push((Value::Uint(1), Value::Uint(model)));
        }
        if let Some(t) = &self.max_total {
            m.push((Value::Uint(2), t.to_value()));
        }
        if let Some(ms) = &self.methods {
            m.push((
                Value::Uint(3),
                Value::Array(ms.iter().map(|s| Value::Text(s.clone())).collect()),
            ));
        }
        if let Some(a) = &self.agree_ref {
            m.push((Value::Uint(4), a.to_value()));
        }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<PayRef, PayError> {
        let methods = match v.map_get(3) {
            None => None,
            Some(x) => Some(
                x.as_array()
                    .ok_or(PayError::BadField(3))?
                    .iter()
                    .map(|e| e.as_text().map(|s| s.to_string()).ok_or(PayError::BadField(3)))
                    .collect::<Result<Vec<_>, PayError>>()?,
            ),
        };
        Ok(PayRef {
            model: v.map_get(1).and_then(|x| x.as_uint()),
            max_total: v.map_get(2).map(AssetAmount::from_value).transpose()?,
            methods,
            agree_ref: v.map_get(4).map(PayAgreeRef::from_value).transpose()?,
        })
    }
}

// ---- PayInvoice / PayProof / PayReceipt (pay §4) ---------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct PayInvoice {
    pub invoice_id: [u8; 16],
    pub quote_id: [u8; 16],
    pub amount: AssetAmount,
    // key 4 (usage watermark) omitted — postpaid flat only (see ERRATA.md).
    pub method_payload: Vec<u8>,
    pub timestamp_ms: u64,
}

impl PayInvoice {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Bytes(self.invoice_id.to_vec())),
            (Value::Uint(2), Value::Bytes(self.quote_id.to_vec())),
            (Value::Uint(3), self.amount.to_value()),
            (Value::Uint(5), Value::Bytes(self.method_payload.clone())),
            (Value::Uint(6), Value::Uint(self.timestamp_ms)),
        ])
    }
    pub fn from_value(v: &Value) -> Result<PayInvoice, PayError> {
        Ok(PayInvoice {
            invoice_id: id16(v, 1)?,
            quote_id: id16(v, 2)?,
            amount: aa_at(v, 3)?,
            method_payload: bytes(v, 5)?,
            timestamp_ms: uint(v, 6)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PayProof {
    pub invoice_id: [u8; 16],
    pub method: String,
    pub proof_payload: Vec<u8>,
}

impl PayProof {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Bytes(self.invoice_id.to_vec())),
            (Value::Uint(2), Value::Text(self.method.clone())),
            (Value::Uint(3), Value::Bytes(self.proof_payload.clone())),
        ])
    }
    pub fn from_value(v: &Value) -> Result<PayProof, PayError> {
        Ok(PayProof {
            invoice_id: id16(v, 1)?,
            method: text(v, 2)?,
            proof_payload: bytes(v, 3)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PayReceipt {
    pub invoice_id: [u8; 16],
    pub amount: AssetAmount,
    pub timestamp_ms: u64,
    pub proof_digest: Option<Vec<u8>>,
}

impl PayReceipt {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.invoice_id.to_vec())),
            (Value::Uint(2), self.amount.to_value()),
            (Value::Uint(3), Value::Uint(self.timestamp_ms)),
        ];
        if let Some(d) = &self.proof_digest {
            m.push((Value::Uint(4), Value::Bytes(d.clone())));
        }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<PayReceipt, PayError> {
        Ok(PayReceipt {
            invoice_id: id16(v, 1)?,
            amount: aa_at(v, 2)?,
            timestamp_ms: uint(v, 3)?,
            proof_digest: match v.map_get(4) {
                None => None,
                Some(x) => Some(x.as_bytes().ok_or(PayError::BadField(4))?.to_vec()),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd(mantissa: i64) -> AssetAmount {
        AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, mantissa) }
    }

    #[test]
    fn quote_roundtrips_with_and_without_task_binding() {
        let base = PayQuote {
            quote_id: [1; 16],
            pricing_model: 0,
            rate_table: vec![(0, usd(500))],
            total: Some(usd(500)),
            methods: vec![MethodOffer { uri: "pay:credit-ledger".into(), params: vec![] }],
            expiry_ms: 1_780_000_600_000,
            terms: Some(0),
            ext: None,
            task_binding: None,
        };
        assert_eq!(PayQuote::from_value(&base.to_value()).unwrap(), base);
        let bound = PayQuote { task_binding: Some([9; 32]), ..base };
        assert_eq!(PayQuote::from_value(&bound.to_value()).unwrap(), bound);
    }

    #[test]
    fn all_pay_messages_roundtrip() {
        let agree = PayAgree { quote_id: [1; 16], quote_digest: pay_ref_digest(b"q"), method: "pay:credit-ledger".into(), authorized_cap: Some(usd(500)) };
        assert_eq!(PayAgree::from_value(&agree.to_value()).unwrap(), agree);
        let qr = PayQuoteRef { quote_id: [1; 16], digest: pay_ref_digest(b"q") };
        assert_eq!(PayQuoteRef::from_value(&qr.to_value()).unwrap(), qr);
        let ar = PayAgreeRef { quote_id: [1; 16], digest: pay_ref_digest(b"a") };
        assert_eq!(PayAgreeRef::from_value(&ar.to_value()).unwrap(), ar);
        let pref = PayRef { model: Some(0), max_total: Some(usd(600)), methods: Some(vec!["pay:credit-ledger".into()]), agree_ref: Some(ar) };
        assert_eq!(PayRef::from_value(&pref.to_value()).unwrap(), pref);
        let inv = PayInvoice { invoice_id: [2; 16], quote_id: [1; 16], amount: usd(500), method_payload: vec![], timestamp_ms: 1 };
        assert_eq!(PayInvoice::from_value(&inv.to_value()).unwrap(), inv);
        let proof = PayProof { invoice_id: [2; 16], method: "pay:credit-ledger".into(), proof_payload: vec![1, 2, 3] };
        assert_eq!(PayProof::from_value(&proof.to_value()).unwrap(), proof);
        let rcpt = PayReceipt { invoice_id: [2; 16], amount: usd(500), timestamp_ms: 2, proof_digest: Some(pay_ref_digest(b"p")) };
        assert_eq!(PayReceipt::from_value(&rcpt.to_value()).unwrap(), rcpt);
    }
}
