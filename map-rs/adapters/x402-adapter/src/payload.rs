//! The x402 "exact" EVM PaymentPayload — the JSON object an `X-PAYMENT` header
//! would carry — plus the `TransferAuthorization` the signer signs (EIP-3009
//! `transferWithAuthorization`). This JSON is produced by [`X402Payload::to_json_bytes`]
//! and consumed by [`X402Payload::from_json_bytes`] ONLY within this crate
//! (facilitators settle it verbatim), so it is serialised by hand — no
//! `serde_json` on the always-on path.

use crate::X402Error;

/// The EIP-3009 `transferWithAuthorization` message the payer signs. `token`
/// and `chain_id` form the EIP-712 domain (verifyingContract + chainId) and are
/// NOT serialised into the authorization JSON block — they come from the quote
/// Asset and the params network on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferAuthorization {
    pub from: String,        // 0x-address of the payer
    pub to: String,          // 0x-address of the payee (== params.pay_to)
    pub value_atomic: u128,  // token atomic units (mantissa at −decimals)
    pub valid_after: u64,    // unix seconds
    pub valid_before: u64,   // unix seconds
    pub nonce: [u8; 32],     // MUST be x402_nonce(invoice-id)
    pub token: String,       // 0x-address of the ERC-20 (EIP-712 verifyingContract)
    pub chain_id: u64,       // EIP-712 domain chainId
}

/// The x402 PaymentPayload for the "exact" EVM scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X402Payload {
    pub version: u64,
    pub scheme: String,
    pub network: String,
    pub signature: Vec<u8>,
    pub from: String,
    pub to: String,
    pub value_atomic: u128,
    pub valid_after: u64,
    pub valid_before: u64,
    pub nonce: [u8; 32],
}

impl X402Payload {
    /// Canonical JSON bytes (the x402 "exact" PaymentPayload shape). Numeric
    /// token amounts are string-encoded, per x402.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        format!(
            concat!(
                "{{\"x402Version\":{version},\"scheme\":\"{scheme}\",\"network\":\"{network}\",",
                "\"payload\":{{\"signature\":\"{sig}\",\"authorization\":{{",
                "\"from\":\"{from}\",\"to\":\"{to}\",\"value\":\"{value}\",",
                "\"validAfter\":\"{va}\",\"validBefore\":\"{vb}\",\"nonce\":\"{nonce}\"}}}}}}"
            ),
            version = self.version,
            scheme = self.scheme,
            network = self.network,
            sig = to_hex0x(&self.signature),
            from = self.from,
            to = self.to,
            value = self.value_atomic,
            va = self.valid_after,
            vb = self.valid_before,
            nonce = to_hex0x(&self.nonce),
        )
        .into_bytes()
    }

    /// Parse the x402 PaymentPayload JSON. Field names are globally unique in
    /// this fixed shape, so a targeted `"key":` extractor suffices (this only
    /// ever parses bytes this crate produced).
    pub fn from_json_bytes(b: &[u8]) -> Result<X402Payload, X402Error> {
        let s = std::str::from_utf8(b).map_err(|_| X402Error::Codec("payload not UTF-8".into()))?;
        let nonce = to_array32(&from_hex0x(&str_field(s, "nonce")?)?)?;
        Ok(X402Payload {
            version: uint_field(s, "x402Version")?,
            scheme: str_field(s, "scheme")?,
            network: str_field(s, "network")?,
            signature: from_hex0x(&str_field(s, "signature")?)?,
            from: str_field(s, "from")?,
            to: str_field(s, "to")?,
            value_atomic: str_field(s, "value")?
                .parse()
                .map_err(|_| X402Error::Codec("value not a u128".into()))?,
            valid_after: str_uint_field(s, "validAfter")?,
            valid_before: str_uint_field(s, "validBefore")?,
            nonce,
        })
    }
}

// ---- hex helpers ----------------------------------------------------------

pub(crate) fn to_hex0x(b: &[u8]) -> String {
    let mut s = String::with_capacity(2 + b.len() * 2);
    s.push_str("0x");
    for x in b {
        s.push(nibble(x >> 4));
        s.push(nibble(x & 0x0f));
    }
    s
}

fn nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'a' + (n - 10)) as char,
    }
}

pub(crate) fn from_hex0x(s: &str) -> Result<Vec<u8>, X402Error> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    if h.len() % 2 != 0 {
        return Err(X402Error::Codec("odd-length hex".into()));
    }
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| X402Error::Codec("bad hex".into())))
        .collect()
}

fn to_array32(b: &[u8]) -> Result<[u8; 32], X402Error> {
    <[u8; 32]>::try_from(b).map_err(|_| X402Error::Codec("nonce not 32 bytes".into()))
}

// ---- minimal JSON field extraction (fixed shape) --------------------------

fn value_start<'a>(s: &'a str, key: &str) -> Result<&'a str, X402Error> {
    let pat = format!("\"{key}\":");
    let i = s.find(&pat).ok_or_else(|| X402Error::Codec(format!("missing field {key}")))?;
    Ok(s[i + pat.len()..].trim_start())
}

pub(crate) fn str_field(s: &str, key: &str) -> Result<String, X402Error> {
    let rest = value_start(s, key)?
        .strip_prefix('"')
        .ok_or_else(|| X402Error::Codec(format!("field {key} not a string")))?;
    let end = rest.find('"').ok_or_else(|| X402Error::Codec(format!("field {key} unterminated")))?;
    Ok(rest[..end].to_string())
}

pub(crate) fn uint_field(s: &str, key: &str) -> Result<u64, X402Error> {
    let rest = value_start(s, key)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().map_err(|_| X402Error::Codec(format!("field {key} not a uint")))
}

/// A string-encoded unsigned integer (x402 quotes numeric amounts).
fn str_uint_field(s: &str, key: &str) -> Result<u64, X402Error> {
    str_field(s, key)?.parse().map_err(|_| X402Error::Codec(format!("field {key} not a uint")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> X402Payload {
        X402Payload {
            version: 1,
            scheme: "exact".into(),
            network: "base-sepolia".into(),
            signature: vec![0xAB; 65],
            from: "0x1111111111111111111111111111111111111111".into(),
            to: "0x2222222222222222222222222222222222222222".into(),
            value_atomic: 8_629_000,
            valid_after: 0,
            valid_before: 1_780_000_600,
            nonce: [0x33; 32],
        }
    }

    #[test]
    fn json_round_trips() {
        let p = sample();
        let json = p.to_json_bytes();
        assert_eq!(X402Payload::from_json_bytes(&json).unwrap(), p);
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(from_hex0x("0xabcd").unwrap(), vec![0xab, 0xcd]);
        assert_eq!(to_hex0x(&[0xab, 0xcd]), "0xabcd");
    }

    #[test]
    fn missing_field_is_a_codec_error() {
        let bad = b"{\"scheme\":\"exact\"}";
        assert!(X402Payload::from_json_bytes(bad).is_err());
    }
}
