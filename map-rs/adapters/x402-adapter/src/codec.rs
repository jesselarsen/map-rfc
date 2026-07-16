//! The `X402Params` (MethodOffer.2) and `X402Proof` (PayProof.3) containers,
//! deterministic-CBOR maps per the pay §5.1 CDDL. These travel as opaque byte
//! strings to the rest of MAP.

use crate::X402Error;
use map_core::cbor::{decode, encode, Value};

/// `X402Params` (pay §5.1): 1 version, 2 scheme, 3 network, 4 payTo,
/// ?5 facilitator URL, ?6 max validity seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X402Params {
    pub version: u64,
    pub scheme: String,
    pub network: String,
    pub pay_to: String,
    pub facilitator: Option<String>,
    pub max_validity_s: Option<u64>,
}

impl X402Params {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Uint(self.version)),
            (Value::Uint(2), Value::Text(self.scheme.clone())),
            (Value::Uint(3), Value::Text(self.network.clone())),
            (Value::Uint(4), Value::Text(self.pay_to.clone())),
        ];
        if let Some(f) = &self.facilitator {
            m.push((Value::Uint(5), Value::Text(f.clone())));
        }
        if let Some(v) = self.max_validity_s {
            m.push((Value::Uint(6), Value::Uint(v)));
        }
        Value::Map(m)
    }

    /// Encode to the opaque bytes carried in `MethodOffer.2`.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode(&self.to_value()).expect("x402 params encode")
    }

    pub fn from_value(v: &Value) -> Result<X402Params, X402Error> {
        let text = |k: u64| -> Result<String, X402Error> {
            v.map_get(k)
                .and_then(Value::as_text)
                .map(str::to_string)
                .ok_or_else(|| X402Error::Codec(format!("params missing text key {k}")))
        };
        let uint = |k: u64| -> Result<u64, X402Error> {
            v.map_get(k)
                .and_then(Value::as_uint)
                .ok_or_else(|| X402Error::Codec(format!("params missing uint key {k}")))
        };
        Ok(X402Params {
            version: uint(1)?,
            scheme: text(2)?,
            network: text(3)?,
            pay_to: text(4)?,
            facilitator: v.map_get(5).and_then(Value::as_text).map(str::to_string),
            max_validity_s: v.map_get(6).and_then(Value::as_uint),
        })
    }

    pub fn from_bytes(b: &[u8]) -> Result<X402Params, X402Error> {
        let v = decode(b).map_err(|e| X402Error::Codec(e.to_string()))?;
        Self::from_value(&v)
    }
}

/// `X402Proof` (pay §5.1): 1 => the x402 PaymentPayload JSON bytes, verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X402Proof {
    pub payload: Vec<u8>,
}

impl X402Proof {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![(Value::Uint(1), Value::Bytes(self.payload.clone()))])
    }

    /// Encode to the opaque bytes carried in `PayProof.3`.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode(&self.to_value()).expect("x402 proof encode")
    }

    pub fn from_bytes(b: &[u8]) -> Result<X402Proof, X402Error> {
        let v = decode(b).map_err(|e| X402Error::Codec(e.to_string()))?;
        let payload = v
            .map_get(1)
            .and_then(Value::as_bytes)
            .ok_or_else(|| X402Error::Codec("proof missing key 1 (payload)".into()))?
            .to_vec();
        Ok(X402Proof { payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_round_trip_with_and_without_optionals() {
        let full = X402Params {
            version: 1,
            scheme: "exact".into(),
            network: "base-sepolia".into(),
            pay_to: "0xabc".into(),
            facilitator: Some("https://x402.example/facilitator".into()),
            max_validity_s: Some(600),
        };
        assert_eq!(X402Params::from_bytes(&full.to_bytes()).unwrap(), full);
        let min = X402Params { facilitator: None, max_validity_s: None, ..full };
        assert_eq!(X402Params::from_bytes(&min.to_bytes()).unwrap(), min);
    }

    #[test]
    fn proof_round_trips_opaque_payload() {
        let p = X402Proof { payload: b"{\"x402Version\":1}".to_vec() };
        assert_eq!(X402Proof::from_bytes(&p.to_bytes()).unwrap(), p);
    }
}
