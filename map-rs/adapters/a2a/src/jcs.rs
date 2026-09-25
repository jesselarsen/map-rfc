//! RFC 8785 JSON Canonicalization Scheme (JCS) over `serde_json::Value`.
//!
//! Used for A2A Agent Card signing (A2A §8.4) and for transcript leaves over
//! A2A Parts (binding A2A-14). Rules implemented:
//! - objects: members sorted by the UTF-16 code units of their names;
//! - strings: ECMAScript `JSON.stringify` escaping (only `"`, `\\` and
//!   control characters U+0000..U+001F are escaped; `\b \t \n \f \r` use
//!   their short forms, other controls `\u00xx` in lower-case hex);
//! - numbers: ECMAScript `Number.prototype.toString` of the IEEE-754 double
//!   (shortest round-trip digits; exponent form below 1e-6 and from 1e21);
//!   NaN and infinities are rejected;
//! - no insignificant whitespace.

use serde_json::{Number, Value};
use std::fmt::Write as _;

#[derive(Debug, PartialEq, Eq)]
pub enum JcsError {
    /// NaN or ±Infinity cannot be represented in JSON.
    NonFiniteNumber,
}

/// Canonical UTF-8 bytes of `v`.
pub fn canonicalize(v: &Value) -> Result<Vec<u8>, JcsError> {
    let mut out = String::new();
    write_value(v, &mut out)?;
    Ok(out.into_bytes())
}

fn write_value(v: &Value, out: &mut String) -> Result<(), JcsError> {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => write_number(n, out)?,
        Value::String(s) => write_string(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(x, out)?;
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(&m[k], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_number(n: &Number, out: &mut String) -> Result<(), JcsError> {
    // JCS numbers are IEEE-754 doubles; integers beyond 2^53 are converted
    // (as ECMAScript would) rather than printed with extra precision.
    let f = if let Some(i) = n.as_i64() {
        i as f64
    } else if let Some(u) = n.as_u64() {
        u as f64
    } else {
        n.as_f64().ok_or(JcsError::NonFiniteNumber)?
    };
    out.push_str(&es6_number(f)?);
    Ok(())
}

/// ECMAScript `Number::toString(x)` for a finite double (ECMA-262 §6.1.6.1.20).
pub fn es6_number(x: f64) -> Result<String, JcsError> {
    if !x.is_finite() {
        return Err(JcsError::NonFiniteNumber);
    }
    if x == 0.0 {
        return Ok("0".into()); // also -0
    }
    let neg = x < 0.0;
    // Rust's `{:e}` yields the shortest round-trip digits: "d.ddddde<exp>".
    let sci = format!("{:e}", x.abs());
    let (mant, exp) = sci.split_once('e').expect("{:e} always has an exponent");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let e: i32 = exp.parse().expect("integer exponent");
    let k = digits.len() as i32;
    let n = e + 1; // value = 0.d1..dk × 10^n
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        let (a, b) = digits.split_at(n as usize);
        format!("{a}.{b}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let sign = if n - 1 < 0 { '-' } else { '+' };
        let (d1, rest) = digits.split_at(1);
        if rest.is_empty() {
            format!("{d1}e{sign}{}", (n - 1).abs())
        } else {
            format!("{d1}.{rest}e{sign}{}", (n - 1).abs())
        }
    };
    Ok(if neg { format!("-{body}") } else { body })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 8785 §3.2.4 example (input with non-canonical numbers, escapes and
    /// member order) and its published canonical form.
    #[test]
    fn rfc8785_section_3_2_4_example() {
        // The RFC's JSON escapes are built at runtime (`esc`) so the source
        // file never contains them literally.
        let esc = |h: &str| format!("\\u{}", h);
        let input = format!(
            "{{\"numbers\": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001], \"string\": \"{}${}{}A'{}{}{}\\\\\\\"\\/\", \"literals\": [null, true, false]}}",
            esc("20ac"),
            esc("000F"),
            esc("000a"),
            esc("0042"),
            esc("0022"),
            esc("005c"),
        );
        let v: Value = serde_json::from_str(&input).unwrap();
        let got = String::from_utf8(canonicalize(&v).unwrap()).unwrap();
        let want = "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"\u{20ac}$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}";
        assert_eq!(got, want);
    }

    /// RFC 8785 §3.2.3 sorting example: keys sorted by UTF-16 code units
    /// (U+1F600 as a surrogate pair sorts before U+FB33).
    #[test]
    fn rfc8785_utf16_member_ordering() {
        let input = r#"{"€":"Euro Sign","\r":"Carriage Return","דּ":"Hebrew Letter Dalet With Dagesh","1":"One","😀":"Emoji: Grinning Face","\u0080":"Control","ö":"Latin Small Letter O With Diaeresis"}"#;
        let v: Value = serde_json::from_str(input).unwrap();
        let got = String::from_utf8(canonicalize(&v).unwrap()).unwrap();
        let order: Vec<&str> = ["\\r", "1", "\u{80}", "\u{f6}", "\u{20ac}", "\u{1f600}", "\u{fb33}"].to_vec();
        let mut last = 0;
        for k in order {
            let pos = got.find(&format!("\"{k}\":")).unwrap_or_else(|| panic!("key {k:?} missing in {got}"));
            assert!(pos >= last, "key {k:?} out of order in {got}");
            last = pos;
        }
    }

    /// ES6 number serialization samples (RFC 8785 Appendix B style).
    #[test]
    fn es6_numbers() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-1.5, "-1.5"),
            (100.0, "100"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (123456789012345680000.0, "123456789012345680000"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (0.1, "0.1"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (9007199254740992.0, "9007199254740992"),
            (295147905179352830000.0, "295147905179352830000"),
        ];
        for (x, want) in cases {
            assert_eq!(es6_number(*x).unwrap(), *want, "for {x:e}");
        }
        assert!(es6_number(f64::NAN).is_err());
        assert!(es6_number(f64::INFINITY).is_err());
    }
}
