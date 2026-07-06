//! HELLO / HELLO_ACK negotiation (core §6), transcript hash and channel
//! binding (core §6.2, §7.3).

use crate::cbor::Value;
use crate::envelope::EnvelopeError;
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

pub const MIN_MAX_MESSAGE_SIZE: u32 = 65_536;
pub const DEFAULT_MAX_MESSAGE_SIZE: u32 = 262_144;

/// HELLO content (core §6.1). HELLO_ACK reuses the shape with the chosen
/// singletons in place of lists (scaffold models both with this struct;
/// `negotiate` produces the ACK values).
#[derive(Clone, Debug, PartialEq)]
pub struct Hello {
    pub versions: Vec<u16>,
    pub suites: Vec<u16>,
    /// (extension key, max supported version) pairs.
    pub extensions: Vec<(u64, u64)>,
    pub max_message_size: u32,
    pub max_tasks: Option<u64>,
    pub zones: Vec<u8>,
}

impl Hello {
    pub fn to_value(&self) -> Value {
        let mut m: Vec<(Value, Value)> = vec![
            (
                Value::Uint(1),
                Value::Array(self.versions.iter().map(|v| Value::Uint(*v as u64)).collect()),
            ),
            (
                Value::Uint(2),
                Value::Array(self.suites.iter().map(|v| Value::Uint(*v as u64)).collect()),
            ),
            (
                Value::Uint(3),
                Value::Map(
                    self.extensions
                        .iter()
                        .map(|(k, v)| (Value::Uint(*k), Value::Uint(*v)))
                        .collect(),
                ),
            ),
            (Value::Uint(4), Value::Uint(self.max_message_size as u64)),
        ];
        if let Some(t) = self.max_tasks {
            m.push((Value::Uint(5), Value::Uint(t)));
        }
        if !self.zones.is_empty() {
            m.push((
                Value::Uint(7),
                Value::Array(self.zones.iter().map(|z| Value::Uint(*z as u64)).collect()),
            ));
        }
        Value::Map(m)
    }

    pub fn from_value(v: &Value) -> Result<Hello, EnvelopeError> {
        let u16s = |k: u64| -> Result<Vec<u16>, EnvelopeError> {
            v.map_get(k)
                .ok_or(EnvelopeError::MissingBodyField(k))?
                .as_array()
                .ok_or(EnvelopeError::BadBodyField(k))?
                .iter()
                .map(|x| {
                    x.as_uint()
                        .and_then(|n| u16::try_from(n).ok())
                        .ok_or(EnvelopeError::BadBodyField(k))
                })
                .collect()
        };
        let versions = u16s(1)?;
        let suites = u16s(2)?;
        let extensions = v
            .map_get(3)
            .ok_or(EnvelopeError::MissingBodyField(3))?
            .as_map()
            .ok_or(EnvelopeError::BadBodyField(3))?
            .iter()
            .map(|(k, val)| {
                Ok((
                    k.as_uint().ok_or(EnvelopeError::BadBodyField(3))?,
                    val.as_uint().ok_or(EnvelopeError::BadBodyField(3))?,
                ))
            })
            .collect::<Result<Vec<_>, EnvelopeError>>()?;
        let max_message_size = v
            .map_get(4)
            .ok_or(EnvelopeError::MissingBodyField(4))?
            .as_uint()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(EnvelopeError::BadBodyField(4))?;
        if max_message_size < MIN_MAX_MESSAGE_SIZE {
            return Err(EnvelopeError::BadBodyField(4));
        }
        let max_tasks = match v.map_get(5) {
            None => None,
            Some(x) => Some(x.as_uint().ok_or(EnvelopeError::BadBodyField(5))?),
        };
        let zones = match v.map_get(7) {
            None => vec![],
            Some(x) => x
                .as_array()
                .ok_or(EnvelopeError::BadBodyField(7))?
                .iter()
                .map(|z| {
                    z.as_uint()
                        .and_then(|n| u8::try_from(n).ok())
                        .filter(|n| *n <= 3)
                        .ok_or(EnvelopeError::BadBodyField(7))
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(Hello {
            versions,
            suites,
            extensions,
            max_message_size,
            max_tasks,
            zones,
        })
    }
}

/// HELLO_ACK content (core §6.1): the responder's chosen singletons — keys
/// 1,2,3,4 always present, ?5 max-tasks. Key 8 (transcript-hash) is not modeled
/// (TH is derived over envelope bytes, §6.2, not embedded). This is the wire
/// form TV-003 fixes; `Hello`'s array shape is for HELLO only.
#[derive(Clone, Debug, PartialEq)]
pub struct HelloAck {
    pub version: u16,
    pub suite: u16,
    pub extensions: Vec<(u64, u64)>,
    pub max_message_size: u32,
    pub max_tasks: Option<u64>,
}

impl HelloAck {
    /// Build the answer from a negotiation result. Per core §6.1, key 4 is the
    /// **responder's own** max-message-size (NOT the negotiated min) and key 5
    /// its own max-tasks — the initiator recomputes `min(local, ack)` itself.
    pub fn from_negotiated(n: &Negotiated, responder_max: u32, responder_max_tasks: Option<u64>) -> Self {
        HelloAck {
            version: n.version,
            suite: n.suite,
            extensions: n.extensions.clone(),
            max_message_size: responder_max,
            max_tasks: responder_max_tasks,
        }
    }

    pub fn to_value(&self) -> Value {
        let ext = Value::Map(
            self.extensions
                .iter()
                .map(|(k, v)| (Value::Uint(*k), Value::Uint(*v)))
                .collect(),
        );
        let mut m = vec![
            (Value::Uint(1), Value::Uint(self.version as u64)),
            (Value::Uint(2), Value::Uint(self.suite as u64)),
            (Value::Uint(3), ext), // always emitted, even when empty (TV-003: 3:{})
            (Value::Uint(4), Value::Uint(self.max_message_size as u64)),
        ];
        if let Some(t) = self.max_tasks {
            m.push((Value::Uint(5), Value::Uint(t)));
        }
        Value::Map(m)
    }

    pub fn from_value(v: &Value) -> Result<HelloAck, EnvelopeError> {
        let u16_at = |k: u64| -> Result<u16, EnvelopeError> {
            v.map_get(k)
                .ok_or(EnvelopeError::MissingBodyField(k))?
                .as_uint()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or(EnvelopeError::BadBodyField(k))
        };
        let version = u16_at(1)?;
        let suite = u16_at(2)?;
        let extensions = v
            .map_get(3)
            .ok_or(EnvelopeError::MissingBodyField(3))?
            .as_map()
            .ok_or(EnvelopeError::BadBodyField(3))?
            .iter()
            .map(|(k, val)| {
                Ok((
                    k.as_uint().ok_or(EnvelopeError::BadBodyField(3))?,
                    val.as_uint().ok_or(EnvelopeError::BadBodyField(3))?,
                ))
            })
            .collect::<Result<Vec<_>, EnvelopeError>>()?;
        let max_message_size = v
            .map_get(4)
            .ok_or(EnvelopeError::MissingBodyField(4))?
            .as_uint()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(EnvelopeError::BadBodyField(4))?;
        let max_tasks = match v.map_get(5) {
            None => None,
            Some(x) => Some(x.as_uint().ok_or(EnvelopeError::BadBodyField(5))?),
        };
        Ok(HelloAck { version, suite, extensions, max_message_size, max_tasks })
    }

    /// View the answer as a one-element `Hello` so the initiator can re-run
    /// [`negotiate`] to validate the responder's choice is within its offer.
    pub fn as_hello(&self) -> Hello {
        Hello {
            versions: vec![self.version],
            suites: vec![self.suite],
            extensions: self.extensions.clone(),
            max_message_size: self.max_message_size,
            max_tasks: self.max_tasks,
            zones: Vec::new(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum NegotiateError {
    NoCommonVersion,
    NoCommonSuite,
}

#[derive(Debug, PartialEq)]
pub struct Negotiated {
    pub version: u16,
    pub suite: u16,
    /// Intersection with min(offered, local) extension versions.
    pub extensions: Vec<(u64, u64)>,
    /// Effective size cap: min of both peers (each enforces its own too).
    pub max_message_size: u32,
}

impl Negotiated {
    /// The HELLO_ACK a responder sends for this result (core §6.1); pass the
    /// responder's own max-message-size and max-tasks (see [`HelloAck::from_negotiated`]).
    pub fn to_ack(&self, responder_max: u32, responder_max_tasks: Option<u64>) -> HelloAck {
        HelloAck::from_negotiated(self, responder_max, responder_max_tasks)
    }
}

/// Responder-side selection (core §6.1): highest mutual version; first
/// mutual suite in the *initiator's* preference order.
pub fn negotiate(initiator: &Hello, local: &Hello) -> Result<Negotiated, NegotiateError> {
    let version = initiator
        .versions
        .iter()
        .filter(|v| local.versions.contains(v))
        .max()
        .copied()
        .ok_or(NegotiateError::NoCommonVersion)?;
    let suite = initiator
        .suites
        .iter()
        .find(|s| local.suites.contains(s))
        .copied()
        .ok_or(NegotiateError::NoCommonSuite)?;
    let mut extensions = Vec::new();
    for (k, offered_v) in &initiator.extensions {
        if let Some((_, local_v)) = local.extensions.iter().find(|(lk, _)| lk == k) {
            extensions.push((*k, (*offered_v).min(*local_v)));
        }
    }
    Ok(Negotiated {
        version,
        suite,
        extensions,
        max_message_size: initiator.max_message_size.min(local.max_message_size),
    })
}

/// TH = H("MAPv1-transcript" || HELLO-envelope-bytes || HELLO_ACK-envelope-bytes)
/// (core §6.2). Downgrade resistance: stripping either offer changes TH and
/// thereby the channel binding of every subsequent envelope.
pub fn transcript_hash(hello_env: &[u8], hello_ack_env: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"MAPv1-transcript");
    h.update(hello_env);
    h.update(hello_ack_env);
    h.finalize().into()
}

/// CB = HKDF(salt = TH, ikm = transport-exporter, info = "map cb", L = 32)
/// (core §7.3). Used as COSE `external_aad` for every in-session envelope.
pub fn channel_binding(th: &[u8; 32], transport_exporter: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(th), transport_exporter);
    let mut okm = [0u8; 32];
    hk.expand(b"map cb", &mut okm)
        .expect("32 bytes is a valid HKDF output length");
    okm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation_prefers_initiator_suite_order_and_highest_version() {
        let init = Hello {
            versions: vec![0x0100, 0x0101],
            suites: vec![2, 1],
            extensions: vec![(100, 3), (200, 1)],
            max_message_size: 262_144,
            max_tasks: None,
            zones: vec![2],
        };
        let local = Hello {
            versions: vec![0x0100, 0x0101, 0x0200],
            suites: vec![1, 2],
            extensions: vec![(100, 2)],
            max_message_size: 131_072,
            max_tasks: None,
            zones: vec![],
        };
        let n = negotiate(&init, &local).unwrap();
        assert_eq!(n.version, 0x0101);
        assert_eq!(n.suite, 2); // initiator preference wins among mutual
        assert_eq!(n.extensions, vec![(100, 2)]); // min version
        assert_eq!(n.max_message_size, 131_072);

        let disjoint = Hello { suites: vec![9], ..init.clone() };
        assert_eq!(negotiate(&disjoint, &local).unwrap_err(), NegotiateError::NoCommonSuite);
    }

    #[test]
    fn hello_ack_round_trips_and_omits_empty_max_tasks() {
        let n = Negotiated { version: 0x0100, suite: 1, extensions: vec![(100, 2)], max_message_size: 131_072 };
        let ack = n.to_ack(262_144, None); // key 4 = responder's own max, not the min
        assert_eq!(ack.max_message_size, 262_144);
        let v = ack.to_value();
        // key 3 present (even if it had been empty); key 5 absent when None.
        assert!(v.map_get(3).is_some());
        assert!(v.map_get(5).is_none());
        assert_eq!(HelloAck::from_value(&v).unwrap(), ack);
        // as_hello round-trips through negotiate to the same chosen singletons.
        let back = negotiate(&ack.as_hello(), &ack.as_hello()).unwrap();
        assert_eq!((back.version, back.suite), (0x0100, 1));

        let with_tasks = n.to_ack(262_144, Some(4));
        assert!(with_tasks.to_value().map_get(5).is_some());
    }

    #[test]
    fn transcript_and_binding_are_input_sensitive() {
        let th1 = transcript_hash(b"hello-a", b"ack");
        let th2 = transcript_hash(b"hello-b", b"ack");
        assert_ne!(th1, th2);
        assert_ne!(channel_binding(&th1, b"exp"), channel_binding(&th2, b"exp"));
        assert_ne!(channel_binding(&th1, b"exp1"), channel_binding(&th1, b"exp2"));
    }
}
