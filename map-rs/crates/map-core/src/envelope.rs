//! The MAP envelope (draft-map-core-00 §4): COSE_Sign1 over deterministic
//! CBOR, with invariant I1 — signatures are verified over the *received*
//! `protected`/`payload` byte strings, never a re-serialization.

use crate::cbor::{decode, encode, CborError, Value};
use map_identity::{verify as id_verify, AgentId, IdentityError, Keypair};
use std::fmt;

pub const COSE_SIGN1_TAG: u64 = 18;
pub const ALG_EDDSA: i64 = -8;
/// Current protocol version 1.0 (core §3).
pub const PROTOCOL_VERSION_1_0: u16 = 0x0100;

#[derive(Debug)]
pub enum EnvelopeError {
    Cbor(CborError),
    Identity(IdentityError),
    /// Not tag 18 / not a 4-element array / wrong element types.
    NotCoseSign1,
    /// `unprotected` was not the empty map (core §4.1).
    NonEmptyUnprotected,
    /// Protected header alg was not EdDSA (-8).
    UnsupportedAlg,
    MissingHeader(&'static str),
    MissingBodyField(u64),
    BadBodyField(u64),
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopeError::Cbor(e) => write!(f, "cbor: {e}"),
            EnvelopeError::Identity(e) => write!(f, "identity: {e}"),
            EnvelopeError::NotCoseSign1 => write!(f, "not a COSE_Sign1 envelope"),
            EnvelopeError::NonEmptyUnprotected => write!(f, "unprotected header must be empty"),
            EnvelopeError::UnsupportedAlg => write!(f, "unsupported COSE alg (want EdDSA/-8)"),
            EnvelopeError::MissingHeader(h) => write!(f, "missing protected header {h}"),
            EnvelopeError::MissingBodyField(k) => write!(f, "missing body field {k}"),
            EnvelopeError::BadBodyField(k) => write!(f, "invalid body field {k}"),
        }
    }
}
impl std::error::Error for EnvelopeError {}
impl From<CborError> for EnvelopeError {
    fn from(e: CborError) -> Self {
        EnvelopeError::Cbor(e)
    }
}
impl From<IdentityError> for EnvelopeError {
    fn from(e: IdentityError) -> Self {
        EnvelopeError::Identity(e)
    }
}

/// 128-bit random per-message id (core §7.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MsgId(pub [u8; 16]);

impl MsgId {
    pub fn random() -> Self {
        let mut b = [0u8; 16];
        getrandom::getrandom(&mut b).expect("OS RNG unavailable");
        MsgId(b)
    }
    pub fn from_slice(s: &[u8]) -> Option<Self> {
        s.try_into().ok().map(MsgId)
    }
}

/// Envelope body (core §4.1, keys 0–10). `crit`/`ext` (keys 8/9) are
/// carried opaquely as raw map entries so unknown extensions survive
/// store-and-forward byte-exactly (invariant I3) — the scaffold does not
/// yet interpret them.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub version: u16,
    pub msg_type: u16,
    pub msg_id: MsgId,
    pub timestamp_ms: u64,
    pub recipient: Option<AgentId>,
    pub ttl_ms: Option<u64>,
    pub session_id: Option<[u8; 16]>,
    pub seq: Option<u64>,
    /// Message-type-specific content (body key 10).
    pub content: Value,
}

impl Body {
    pub fn to_value(&self) -> Value {
        let mut m: Vec<(Value, Value)> = vec![
            (Value::Uint(0), Value::Uint(self.version as u64)),
            (Value::Uint(1), Value::Uint(self.msg_type as u64)),
            (Value::Uint(2), Value::Bytes(self.msg_id.0.to_vec())),
            (Value::Uint(3), Value::Uint(self.timestamp_ms)),
        ];
        if let Some(r) = &self.recipient {
            m.push((Value::Uint(4), Value::Bytes(r.as_bytes().to_vec())));
        }
        if let Some(t) = self.ttl_ms {
            m.push((Value::Uint(5), Value::Uint(t)));
        }
        if let Some(s) = &self.session_id {
            m.push((Value::Uint(6), Value::Bytes(s.to_vec())));
        }
        if let Some(q) = self.seq {
            m.push((Value::Uint(7), Value::Uint(q)));
        }
        m.push((Value::Uint(10), self.content.clone()));
        Value::Map(m)
    }

    pub fn from_value(v: &Value) -> Result<Body, EnvelopeError> {
        let need_uint = |k: u64| {
            v.map_get(k)
                .ok_or(EnvelopeError::MissingBodyField(k))?
                .as_uint()
                .ok_or(EnvelopeError::BadBodyField(k))
        };
        let version =
            u16::try_from(need_uint(0)?).map_err(|_| EnvelopeError::BadBodyField(0))?;
        let msg_type =
            u16::try_from(need_uint(1)?).map_err(|_| EnvelopeError::BadBodyField(1))?;
        let msg_id = MsgId::from_slice(
            v.map_get(2)
                .ok_or(EnvelopeError::MissingBodyField(2))?
                .as_bytes()
                .ok_or(EnvelopeError::BadBodyField(2))?,
        )
        .ok_or(EnvelopeError::BadBodyField(2))?;
        let timestamp_ms = need_uint(3)?;

        let recipient = match v.map_get(4) {
            None => None,
            Some(x) => Some(
                AgentId::from_bytes(x.as_bytes().ok_or(EnvelopeError::BadBodyField(4))?)
                    .map_err(|_| EnvelopeError::BadBodyField(4))?,
            ),
        };
        let ttl_ms = match v.map_get(5) {
            None => None,
            Some(x) => Some(x.as_uint().ok_or(EnvelopeError::BadBodyField(5))?),
        };
        let session_id = match v.map_get(6) {
            None => None,
            Some(x) => Some(
                x.as_bytes()
                    .and_then(|b| <[u8; 16]>::try_from(b).ok())
                    .ok_or(EnvelopeError::BadBodyField(6))?,
            ),
        };
        let seq = match v.map_get(7) {
            None => None,
            Some(x) => Some(x.as_uint().ok_or(EnvelopeError::BadBodyField(7))?),
        };
        let content = v
            .map_get(10)
            .ok_or(EnvelopeError::MissingBodyField(10))?
            .clone();

        Ok(Body {
            version,
            msg_type,
            msg_id,
            timestamp_ms,
            recipient,
            ttl_ms,
            session_id,
            seq,
            content,
        })
    }
}

/// A verified, opened envelope. `protected`/`payload` are the exact
/// received byte strings (I1); keep them if you re-forward or store.
pub struct Opened {
    pub sender: AgentId,
    pub body: Body,
    pub protected: Vec<u8>,
    pub payload: Vec<u8>,
}

fn sig_structure(protected: &[u8], external_aad: &[u8], payload: &[u8]) -> Result<Vec<u8>, CborError> {
    encode(&Value::Array(vec![
        Value::Text("Signature1".into()),
        Value::Bytes(protected.to_vec()),
        Value::Bytes(external_aad.to_vec()),
        Value::Bytes(payload.to_vec()),
    ]))
}

/// Build and sign an envelope. `external_aad` is the channel-binding token
/// for in-session messages, empty for store-and-forward objects (§7.3/7.4).
pub fn seal(kp: &Keypair, body: &Body, external_aad: &[u8]) -> Result<Vec<u8>, EnvelopeError> {
    let kid = kp.agent_id();
    let protected = encode(&Value::Map(vec![
        (Value::Uint(1), Value::int(ALG_EDDSA)),
        (Value::Uint(4), Value::Bytes(kid.as_bytes().to_vec())),
    ]))?;
    let payload = encode(&body.to_value())?;
    let sig = kp.sign(&sig_structure(&protected, external_aad, &payload)?);
    let env = Value::Tag(
        COSE_SIGN1_TAG,
        Box::new(Value::Array(vec![
            Value::Bytes(protected),
            Value::Map(vec![]),
            Value::Bytes(payload),
            Value::Bytes(sig.to_vec()),
        ])),
    );
    Ok(encode(&env)?)
}

/// Decode, profile-check, and verify an envelope (core §4.3 steps 1–7 minus
/// the stateful checks, which live in [`crate::replay`] and session logic).
pub fn open(env_bytes: &[u8], external_aad: &[u8]) -> Result<Opened, EnvelopeError> {
    // Strict decode enforces the deterministic profile (rule 1).
    let v = decode(env_bytes)?;
    let inner = match v {
        Value::Tag(COSE_SIGN1_TAG, inner) => *inner,
        _ => return Err(EnvelopeError::NotCoseSign1),
    };
    let arr = inner.as_array().ok_or(EnvelopeError::NotCoseSign1)?;
    if arr.len() != 4 {
        return Err(EnvelopeError::NotCoseSign1);
    }
    let protected = arr[0]
        .as_bytes()
        .ok_or(EnvelopeError::NotCoseSign1)?
        .to_vec();
    match arr[1].as_map() {
        Some(m) if m.is_empty() => {}
        _ => return Err(EnvelopeError::NonEmptyUnprotected),
    }
    let payload = arr[2]
        .as_bytes()
        .ok_or(EnvelopeError::NotCoseSign1)?
        .to_vec();
    let sig = arr[3].as_bytes().ok_or(EnvelopeError::NotCoseSign1)?;

    // Protected header: alg + kid (rule 2/7).
    let ph = decode(&protected)?;
    match ph.map_get(1).and_then(|x| x.as_i128()) {
        Some(a) if a == ALG_EDDSA as i128 => {}
        _ => return Err(EnvelopeError::UnsupportedAlg),
    }
    let kid_bytes = ph
        .map_get(4)
        .and_then(|x| x.as_bytes())
        .ok_or(EnvelopeError::MissingHeader("kid"))?;
    let sender = AgentId::from_bytes(kid_bytes)?;

    // I1: verify over the exact received byte strings.
    let ss = sig_structure(&protected, external_aad, &payload)?;
    id_verify(&sender, &ss, sig)?;

    let body = Body::from_value(&decode(&payload)?)?;
    Ok(Opened {
        sender,
        body,
        protected,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg_type;

    fn body() -> Body {
        Body {
            version: PROTOCOL_VERSION_1_0,
            msg_type: msg_type::PING,
            msg_id: MsgId([9u8; 16]),
            timestamp_ms: 1_780_000_000_000,
            recipient: None,
            ttl_ms: None,
            session_id: Some([1u8; 16]),
            seq: Some(3),
            content: Value::Bytes(vec![0xAB; 8]),
        }
    }

    #[test]
    fn seal_open_roundtrip_with_aad() {
        let kp = Keypair::from_seed([1u8; 32]);
        let cb = [0x42u8; 32];
        let env = seal(&kp, &body(), &cb).unwrap();
        let opened = open(&env, &cb).unwrap();
        assert_eq!(opened.sender, kp.agent_id());
        assert_eq!(opened.body, body());
        // Channel binding: same bytes on another "session" must not verify.
        assert!(open(&env, &[0x43u8; 32]).is_err());
    }

    #[test]
    fn tamper_detection_on_received_bytes() {
        let kp = Keypair::from_seed([2u8; 32]);
        let mut env = seal(&kp, &body(), b"").unwrap();
        let n = env.len();
        env[n - 70] ^= 1; // flip a payload byte
        assert!(open(&env, b"").is_err());
    }
}
