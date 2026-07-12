//! Error taxonomy (draft-map-core-00 §9): numeric code space with
//! retryability semantics that make orchestrator fallback deterministic
//! (tasks §14).

use crate::cbor::Value;
use crate::envelope::{EnvelopeError, MsgId};

pub mod codes {
    // 1xxx protocol
    pub const MALFORMED: u16 = 1000;
    pub const UNSUPPORTED_VERSION: u16 = 1001;
    pub const UNKNOWN_CRITICAL_EXT: u16 = 1002;
    pub const REPLAY: u16 = 1003;
    pub const TOO_LARGE: u16 = 1004;
    pub const RATE_LIMITED: u16 = 1005;
    pub const STATE_VIOLATION: u16 = 1006;
    // 2xxx auth/authz
    pub const BAD_SIGNATURE: u16 = 2000;
    pub const EXPIRED: u16 = 2001;
    pub const REVOKED: u16 = 2002;
    pub const BAD_DELEGATION: u16 = 2003;
    pub const ZONE_VIOLATION: u16 = 2004;
    pub const NOT_AUTHORIZED: u16 = 2005;
    /// The receiver declines to attest the sender's claimed facts (e.g. a
    /// receipt whose usage/timestamps fall outside the countersignature
    /// tolerance, trust §3). Added 2026-07-12 (ERRATA ruling); retryable
    /// is false — the requester must change its claim, not retry it.
    pub const COUNTERSIGN_REFUSED: u16 = 2006;
    // 3xxx capability
    pub const UNSUPPORTED: u16 = 3000;
    pub const MODEL_UNAVAILABLE: u16 = 3001;
    pub const CONTEXT_TOO_LARGE: u16 = 3002;
    pub const ONTOLOGY_UNKNOWN: u16 = 3003;
    // 4xxx resource
    pub const OVERLOADED: u16 = 4000;
    pub const QUEUE_FULL: u16 = 4001;
    pub const INSUFFICIENT_MEMORY: u16 = 4002;
    pub const RESERVATION_EXPIRED: u16 = 4003;
    pub const STORAGE_EXHAUSTED: u16 = 4004;
    // 5xxx execution
    pub const TASK_FAILED: u16 = 5000;
    pub const DEADLINE_EXCEEDED: u16 = 5001;
    pub const CANCELLED: u16 = 5002;
    pub const CHECKPOINT_INVALID: u16 = 5003;
    pub const TOOL_DENIED: u16 = 5004;
    pub const OUTPUT_POLICY: u16 = 5005;
    pub const RESUMPTION_IMPOSSIBLE: u16 = 5006;
    // 6xxx payment
    pub const QUOTE_EXPIRED: u16 = 6000;
    pub const PAYMENT_REQUIRED: u16 = 6001;
    pub const SETTLEMENT_FAILED: u16 = 6002;
    pub const DISPUTE_OPENED: u16 = 6003;
    pub const METHOD_UNSUPPORTED: u16 = 6004;
    // 6005-6008 added 2026-07-12 (ERRATA ruling): the specific pay
    // rejections that previously all collapsed to SETTLEMENT_FAILED.
    /// PayAgree.4 authorized cap exceeded (pay §3.1).
    pub const CAP_EXCEEDED: u16 = 6005;
    /// Quote/agreement envelope digest does not match the referenced
    /// bytes — the price-switch defense (pay §3.1).
    pub const AGREEMENT_DIGEST_MISMATCH: u16 = 6006;
    /// PayQuote.9 task binding present but != the OFFER's TaskID (pay §3).
    pub const TASK_BINDING_MISMATCH: u16 = 6007;
    /// Forged or mismatched settlement proof / ledger entry (pay §5).
    pub const INVALID_PAYMENT_PROOF: u16 = 6008;
}

pub fn family(code: u16) -> &'static str {
    match code / 1000 {
        1 => "protocol",
        2 => "auth",
        3 => "capability",
        4 => "resource",
        5 => "execution",
        6 => "payment",
        _ => "unknown",
    }
}

/// ERROR content (core §9). `retryable = true`: the identical request MAY
/// succeed later at the same peer; `false`: change something (peer, params,
/// payment) first. The `detail` map (key 5) is carried opaquely if present.
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorMsg {
    pub code: u16,
    pub retryable: bool,
    pub message: Option<String>,
    pub in_reply_to: Option<MsgId>,
    pub detail: Option<Value>,
}

impl ErrorMsg {
    pub fn new(code: u16, retryable: bool) -> Self {
        ErrorMsg {
            code,
            retryable,
            message: None,
            in_reply_to: None,
            detail: None,
        }
    }

    pub fn to_value(&self) -> Value {
        let mut m: Vec<(Value, Value)> = vec![
            (Value::Uint(1), Value::Uint(self.code as u64)),
            (Value::Uint(2), Value::Bool(self.retryable)),
        ];
        if let Some(s) = &self.message {
            m.push((Value::Uint(3), Value::Text(s.clone())));
        }
        if let Some(id) = &self.in_reply_to {
            m.push((Value::Uint(4), Value::Bytes(id.0.to_vec())));
        }
        if let Some(d) = &self.detail {
            m.push((Value::Uint(5), d.clone()));
        }
        Value::Map(m)
    }

    pub fn from_value(v: &Value) -> Result<Self, EnvelopeError> {
        let code = v
            .map_get(1)
            .ok_or(EnvelopeError::MissingBodyField(1))?
            .as_uint()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or(EnvelopeError::BadBodyField(1))?;
        let retryable = match v.map_get(2) {
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(EnvelopeError::BadBodyField(2)),
            None => return Err(EnvelopeError::MissingBodyField(2)),
        };
        let message = match v.map_get(3) {
            None => None,
            Some(x) => Some(
                x.as_text()
                    .ok_or(EnvelopeError::BadBodyField(3))?
                    .to_string(),
            ),
        };
        let in_reply_to = match v.map_get(4) {
            None => None,
            Some(x) => Some(
                x.as_bytes()
                    .and_then(MsgId::from_slice)
                    .ok_or(EnvelopeError::BadBodyField(4))?,
            ),
        };
        let detail = v.map_get(5).cloned();
        Ok(ErrorMsg {
            code,
            retryable,
            message,
            in_reply_to,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_and_roundtrip() {
        assert_eq!(family(codes::REPLAY), "protocol");
        assert_eq!(family(codes::ZONE_VIOLATION), "auth");
        assert_eq!(family(codes::PAYMENT_REQUIRED), "payment");

        let e = ErrorMsg {
            code: codes::RATE_LIMITED,
            retryable: true,
            message: Some("slow down".into()),
            in_reply_to: Some(MsgId([1u8; 16])),
            detail: Some(Value::Map(vec![(
                Value::Uint(1),
                Value::Uint(30_000), // retry-after-ms
            )])),
        };
        assert_eq!(ErrorMsg::from_value(&e.to_value()).unwrap(), e);
    }
}
