//! Deterministic CBOR per draft-map-core-01 §4.2 (RFC 8949 §4.2.1 profile).
//!
//! Two jobs, both normative:
//! * **Encode** canonically: shortest-form heads, definite lengths, map keys
//!   sorted bytewise on their encoded form, duplicates rejected, no floats.
//! * **Decode** strictly, *rejecting* profile violations (core §4.3 rule 1 —
//!   the malleability defense). Anything this decoder accepts re-encodes to
//!   the identical bytes.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum CborError {
    UnexpectedEof,
    TrailingBytes,
    /// Integer/length not in shortest form (profile violation).
    NonShortestForm,
    /// Indefinite-length item (profile violation).
    IndefiniteLength,
    /// Floating point encountered (profile violation, core §4.2).
    Float,
    /// Map keys out of bytewise order (profile violation).
    UnsortedMapKeys,
    DuplicateMapKey,
    DepthExceeded,
    InvalidUtf8,
    UnsupportedSimple(u8),
    Malformed(&'static str),
}

impl fmt::Display for CborError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CborError::UnexpectedEof => write!(f, "unexpected end of input"),
            CborError::TrailingBytes => write!(f, "trailing bytes after item"),
            CborError::NonShortestForm => write!(f, "non-shortest-form integer"),
            CborError::IndefiniteLength => write!(f, "indefinite length forbidden"),
            CborError::Float => write!(f, "floating point forbidden in signed structures"),
            CborError::UnsortedMapKeys => write!(f, "map keys not in bytewise order"),
            CborError::DuplicateMapKey => write!(f, "duplicate map key"),
            CborError::DepthExceeded => write!(f, "nesting depth exceeded"),
            CborError::InvalidUtf8 => write!(f, "text string is not valid UTF-8"),
            CborError::UnsupportedSimple(v) => write!(f, "unsupported simple value {v}"),
            CborError::Malformed(m) => write!(f, "malformed CBOR: {m}"),
        }
    }
}
impl std::error::Error for CborError {}

const MAX_DEPTH: usize = 32;

/// The value model of the MAP profile. Deliberately smaller than full CBOR:
/// no floats, no undefined, no arbitrary simple values (core §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Uint(u64),
    /// Negative integer; `Nint(n)` encodes `-1 - n`.
    Nint(u64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    /// Entries are sorted at encode time; decode enforces received order.
    Map(Vec<(Value, Value)>),
    Tag(u64, Box<Value>),
    Bool(bool),
    Null,
}

impl Value {
    pub fn int(i: i64) -> Value {
        if i >= 0 {
            Value::Uint(i as u64)
        } else {
            Value::Nint((-1 - i) as u64)
        }
    }

    pub fn as_i128(&self) -> Option<i128> {
        match self {
            Value::Uint(n) => Some(*n as i128),
            Value::Nint(n) => Some(-1 - (*n as i128)),
            _ => None,
        }
    }

    pub fn as_uint(&self) -> Option<u64> {
        match self {
            Value::Uint(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&[(Value, Value)]> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Look up an unsigned-integer key in a map value.
    pub fn map_get(&self, key: u64) -> Option<&Value> {
        self.as_map()?
            .iter()
            .find(|(k, _)| matches!(k, Value::Uint(x) if *x == key))
            .map(|(_, v)| v)
    }
}

fn write_head(major: u8, val: u64, out: &mut Vec<u8>) {
    let mb = major << 5;
    if val < 24 {
        out.push(mb | val as u8);
    } else if val <= 0xff {
        out.push(mb | 24);
        out.push(val as u8);
    } else if val <= 0xffff {
        out.push(mb | 25);
        out.extend_from_slice(&(val as u16).to_be_bytes());
    } else if val <= 0xffff_ffff {
        out.push(mb | 26);
        out.extend_from_slice(&(val as u32).to_be_bytes());
    } else {
        out.push(mb | 27);
        out.extend_from_slice(&val.to_be_bytes());
    }
}

/// Encode a value under the deterministic profile.
pub fn encode(v: &Value) -> Result<Vec<u8>, CborError> {
    let mut out = Vec::new();
    encode_into(v, &mut out)?;
    Ok(out)
}

fn encode_into(v: &Value, out: &mut Vec<u8>) -> Result<(), CborError> {
    match v {
        Value::Uint(n) => write_head(0, *n, out),
        Value::Nint(n) => write_head(1, *n, out),
        Value::Bytes(b) => {
            write_head(2, b.len() as u64, out);
            out.extend_from_slice(b);
        }
        Value::Text(s) => {
            write_head(3, s.len() as u64, out);
            out.extend_from_slice(s.as_bytes());
        }
        Value::Array(a) => {
            write_head(4, a.len() as u64, out);
            for item in a {
                encode_into(item, out)?;
            }
        }
        Value::Map(m) => {
            // Sort entries bytewise on the encoded key (RFC 8949 §4.2.1).
            let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(m.len());
            for (k, val) in m {
                pairs.push((encode(k)?, encode(val)?));
            }
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            for w in pairs.windows(2) {
                if w[0].0 == w[1].0 {
                    return Err(CborError::DuplicateMapKey);
                }
            }
            write_head(5, pairs.len() as u64, out);
            for (k, val) in pairs {
                out.extend_from_slice(&k);
                out.extend_from_slice(&val);
            }
        }
        Value::Tag(t, inner) => {
            write_head(6, *t, out);
            encode_into(inner, out)?;
        }
        Value::Bool(b) => out.push(if *b { 0xf5 } else { 0xf4 }),
        Value::Null => out.push(0xf6),
    }
    Ok(())
}

/// Decode exactly one item covering the whole input, enforcing the profile.
pub fn decode(bytes: &[u8]) -> Result<Value, CborError> {
    let mut d = Decoder { b: bytes, pos: 0 };
    let v = d.item(0)?;
    if d.pos != bytes.len() {
        return Err(CborError::TrailingBytes);
    }
    Ok(v)
}

struct Decoder<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    fn byte(&mut self) -> Result<u8, CborError> {
        let v = *self.b.get(self.pos).ok_or(CborError::UnexpectedEof)?;
        self.pos += 1;
        Ok(v)
    }

    fn take(&mut self, n: u64) -> Result<&'a [u8], CborError> {
        let n = usize::try_from(n).map_err(|_| CborError::Malformed("length overflow"))?;
        let end = self
            .pos
            .checked_add(n)
            .ok_or(CborError::Malformed("length overflow"))?;
        if end > self.b.len() {
            return Err(CborError::UnexpectedEof);
        }
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    /// Read the head argument for additional-info `ai`, enforcing
    /// shortest-form encoding.
    fn arg(&mut self, ai: u8) -> Result<u64, CborError> {
        match ai {
            0..=23 => Ok(ai as u64),
            24 => {
                let v = self.byte()? as u64;
                if v < 24 {
                    return Err(CborError::NonShortestForm);
                }
                Ok(v)
            }
            25 => {
                let v = u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as u64;
                if v <= 0xff {
                    return Err(CborError::NonShortestForm);
                }
                Ok(v)
            }
            26 => {
                let v = u32::from_be_bytes(self.take(4)?.try_into().unwrap()) as u64;
                if v <= 0xffff {
                    return Err(CborError::NonShortestForm);
                }
                Ok(v)
            }
            27 => {
                let v = u64::from_be_bytes(self.take(8)?.try_into().unwrap());
                if v <= 0xffff_ffff {
                    return Err(CborError::NonShortestForm);
                }
                Ok(v)
            }
            31 => Err(CborError::IndefiniteLength),
            _ => Err(CborError::Malformed("reserved additional info")),
        }
    }

    fn item(&mut self, depth: usize) -> Result<Value, CborError> {
        if depth > MAX_DEPTH {
            return Err(CborError::DepthExceeded);
        }
        let ib = self.byte()?;
        let major = ib >> 5;
        let ai = ib & 0x1f;

        if major == 7 {
            return match ai {
                20 => Ok(Value::Bool(false)),
                21 => Ok(Value::Bool(true)),
                22 => Ok(Value::Null),
                25 | 26 | 27 => Err(CborError::Float),
                24 => Err(CborError::UnsupportedSimple(self.byte()?)),
                31 => Err(CborError::Malformed("break outside indefinite item")),
                v => Err(CborError::UnsupportedSimple(v)),
            };
        }

        let val = self.arg(ai)?;
        match major {
            0 => Ok(Value::Uint(val)),
            1 => Ok(Value::Nint(val)),
            2 => Ok(Value::Bytes(self.take(val)?.to_vec())),
            3 => {
                let s = self.take(val)?;
                let t = std::str::from_utf8(s).map_err(|_| CborError::InvalidUtf8)?;
                Ok(Value::Text(t.to_string()))
            }
            4 => {
                let mut items = Vec::new();
                for _ in 0..val {
                    items.push(self.item(depth + 1)?);
                }
                Ok(Value::Array(items))
            }
            5 => {
                let mut entries = Vec::new();
                let mut prev_key: Option<&'a [u8]> = None;
                for _ in 0..val {
                    let kstart = self.pos;
                    let k = self.item(depth + 1)?;
                    let kbytes = &self.b[kstart..self.pos];
                    if let Some(p) = prev_key {
                        match p.cmp(kbytes) {
                            std::cmp::Ordering::Less => {}
                            std::cmp::Ordering::Equal => return Err(CborError::DuplicateMapKey),
                            std::cmp::Ordering::Greater => return Err(CborError::UnsortedMapKeys),
                        }
                    }
                    prev_key = Some(kbytes);
                    let v = self.item(depth + 1)?;
                    entries.push((k, v));
                }
                Ok(Value::Map(entries))
            }
            6 => Ok(Value::Tag(val, Box::new(self.item(depth + 1)?))),
            _ => unreachable!("major covered"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hexd(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn shortest_form_heads() {
        assert_eq!(encode(&Value::Uint(23)).unwrap(), vec![0x17]);
        assert_eq!(encode(&Value::Uint(24)).unwrap(), vec![0x18, 24]);
        assert_eq!(encode(&Value::int(-8)).unwrap(), vec![0x27]);
        assert_eq!(
            encode(&Value::Uint(262144)).unwrap(),
            hexd("1a00040000")
        );
    }

    #[test]
    fn map_keys_sorted_and_deduped() {
        let m = Value::Map(vec![
            (Value::Uint(10), Value::Uint(0)),
            (Value::Uint(1), Value::Uint(0)),
        ]);
        let enc = encode(&m).unwrap();
        // key 1 must precede key 10 (0x01 < 0x0a)
        assert_eq!(enc, hexd("a201000a00"));

        let dup = Value::Map(vec![
            (Value::Uint(1), Value::Uint(0)),
            (Value::Uint(1), Value::Uint(2)),
        ]);
        assert_eq!(encode(&dup).unwrap_err(), CborError::DuplicateMapKey);
    }

    #[test]
    fn decoder_enforces_profile() {
        // non-shortest: 24 encoded with a following byte < 24
        assert_eq!(decode(&[0x18, 0x05]).unwrap_err(), CborError::NonShortestForm);
        // indefinite bstr
        assert_eq!(decode(&[0x5f, 0xff]).unwrap_err(), CborError::IndefiniteLength);
        // half-precision float
        assert_eq!(
            decode(&hexd("f93c00")).unwrap_err(),
            CborError::Float
        );
        // unsorted map keys {2:0, 1:0}
        assert_eq!(
            decode(&hexd("a202000100")).unwrap_err(),
            CborError::UnsortedMapKeys
        );
        // duplicate keys {1:0, 1:0}
        assert_eq!(
            decode(&hexd("a201000100")).unwrap_err(),
            CborError::DuplicateMapKey
        );
        // trailing bytes
        assert_eq!(decode(&hexd("0000")).unwrap_err(), CborError::TrailingBytes);
    }

    #[test]
    fn roundtrip_is_identity_for_accepted_input() {
        let v = Value::Map(vec![
            (Value::Uint(0), Value::Uint(0x0100)),
            (Value::Uint(10), Value::Array(vec![Value::Text("x".into()), Value::Bytes(vec![1, 2])])),
            (Value::Uint(3), Value::Tag(4, Box::new(Value::Array(vec![Value::int(-2), Value::Uint(1995)])))),
        ]);
        let enc = encode(&v).unwrap();
        let dec = decode(&enc).unwrap();
        assert_eq!(encode(&dec).unwrap(), enc);
    }
}
