//! base64url without padding (RFC 4648 §5), as used by JWS (RFC 7515) and by
//! the binding's metadata carriage (binding-a2a §3).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3F) as usize] as char);
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError;

pub fn decode(s: &str) -> Result<Vec<u8>, DecodeError> {
    let val = |c: u8| -> Result<u32, DecodeError> {
        Ok(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err(DecodeError),
        } as u32)
    };
    let bytes = s.as_bytes();
    if bytes.len() % 4 == 1 {
        return Err(DecodeError);
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let produced = chunk.len() - 1;
        for i in 0..produced {
            out.push((n >> (16 - 8 * i)) as u8);
        }
        // Reject non-canonical encodings (non-zero trailing bits).
        let used_bits = produced * 8;
        let total_bits = chunk.len() * 6;
        if total_bits > used_bits && (n >> (24 - total_bits)) & ((1 << (total_bits - used_bits)) - 1) != 0 {
            return Err(DecodeError);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648 §10 test vectors (base64url alphabet, padding stripped).
    #[test]
    fn rfc4648_vectors() {
        for (raw, enc) in [("", ""), ("f", "Zg"), ("fo", "Zm8"), ("foo", "Zm9v"), ("foob", "Zm9vYg"), ("fooba", "Zm9vYmE"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(encode(raw.as_bytes()), enc);
            assert_eq!(decode(enc).unwrap(), raw.as_bytes());
        }
        assert_eq!(encode(&[0xfb, 0xff]), "-_8");
        assert_eq!(decode("-_8").unwrap(), vec![0xfb, 0xff]);
        assert!(decode("Zh").is_err()); // non-zero trailing bits
        assert!(decode("Z").is_err());
        assert!(decode("Zm9v=").is_err());
    }
}
