//! TASK-004 vectors consumed/rejected by the Rust implementation: TV-003..010
//! (envelope-level open + signer + msg-type, with typed round-trips where a
//! Rust body type exists), NV-001..005 (each rejected with the named class),
//! and the derived constants (TH, CB, transcript root) recomputed from the
//! primitives. Several message families (OFFER/ASSIGN/COMPLETE/PAY_QUOTE) have
//! no Rust body type yet, so their check is the uniform envelope-level baseline.
//!
//! Bound-quote pass (pay §3.1 key 9 TaskID binding): TV-010b is a bound
//! PAY_QUOTE whose key 9 equals the OFFER's TaskID (consumed + verified via the
//! real `PayQuote` codec); NV-006 carries a mismatching binding and MUST be
//! rejected by the §3.1 rule — the executable coverage of that normative MUST.

use crate::positive::{key_a, key_b};
use crate::vectors::{TV001_HEX, TV002_TASK_ID_HEX};
use crate::vectors2::*;
use crate::from_hex;
use map_pay::codec::PayQuote;
use map_core::cbor::{encode, CborError};
use map_core::envelope::{open, EnvelopeError, Opened};
use map_core::errors::ErrorMsg;
use map_core::hello::{channel_binding, transcript_hash, HelloAck};
use map_core::msg_type;
use map_discovery::{Descriptor, DescriptorCache, Ingest};
use map_identity::{IdentityError, Keypair};
use map_tasks::{Chunk, ChunkKind, TranscriptBuilder};
use map_trust::{Receipt, TrustError};

/// Open an envelope and assert its signer AgentID and message type.
fn expect_open(env_hex: &str, signer: &Keypair, msg_type: u16) -> Result<Opened, String> {
    let opened = open(&from_hex(env_hex)?, b"").map_err(|e| format!("open: {e}"))?;
    if opened.sender != signer.agent_id() {
        return Err("signer AgentID mismatch".into());
    }
    if opened.body.msg_type != msg_type {
        return Err(format!(
            "msg_type {:#06x} != {:#06x}",
            opened.body.msg_type, msg_type
        ));
    }
    Ok(opened)
}

pub fn case_tv003_hello_ack() -> Result<(), String> {
    let opened = expect_open(TV003_ENV_HEX, &key_b(), msg_type::HELLO_ACK)?;
    // Produce-guard (TASK-006 FR-1 / R1): the HelloAck codec must reproduce the
    // vector bytes, not merely parse them. Round-trip via from_value → to_value
    // and compare encodings — the same discipline as the DESCRIPTOR check.
    let re = encode(&HelloAck::from_value(&opened.body.content).map_err(|e| format!("HelloAck: {e:?}"))?.to_value())
        .map_err(|e| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("HelloAck did not round-trip byte-for-byte".into());
    }
    Ok(())
}

pub fn case_tv004_error() -> Result<(), String> {
    let opened = expect_open(TV004_ENV_HEX, &key_b(), msg_type::ERROR)?;
    let em = ErrorMsg::from_value(&opened.body.content).map_err(|e| format!("ErrorMsg: {e:?}"))?;
    if em.code != 1003 || em.retryable {
        return Err(format!("error fields: code {}, retryable {}", em.code, em.retryable));
    }
    Ok(())
}

pub fn case_tv005_descriptor() -> Result<(), String> {
    let opened = expect_open(TV005_ENV_HEX, &key_b(), msg_type::DESCRIPTOR)?;
    let desc = Descriptor::from_value(&opened.body.content).map_err(|e| format!("Descriptor: {e}"))?;
    if desc.seq != 1 || desc.agent != key_b().agent_id() {
        return Err("descriptor subject/seq mismatch".into());
    }
    // Round-trip byte-exactness cross-checks the Rust encoder's key ordering
    // against the cbor2-canonical vector.
    let re = encode(&desc.to_value().map_err(|e| format!("to_value: {e}"))?)
        .map_err(|e: CborError| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("descriptor did not round-trip byte-for-byte".into());
    }
    Ok(())
}

pub fn case_tv006_offer() -> Result<(), String> {
    expect_open(TV006_ENV_HEX, &key_a(), msg_type::OFFER).map(|_| ())
}
pub fn case_tv007_assign() -> Result<(), String> {
    expect_open(TV007_ENV_HEX, &key_a(), msg_type::ASSIGN).map(|_| ())
}
pub fn case_tv008_chunk() -> Result<(), String> {
    expect_open(TV008_ENV_HEX, &key_b(), msg_type::CHUNK).map(|_| ())
}
pub fn case_tv009_complete() -> Result<(), String> {
    expect_open(TV009_ENV_HEX, &key_b(), msg_type::COMPLETE).map(|_| ())
}
pub fn case_tv010_pay_quote() -> Result<(), String> {
    expect_open(TV010_ENV_HEX, &key_b(), msg_type::PAY_QUOTE).map(|_| ())
}

/// The OFFER's TaskID (= TV-002 core[1]); the bound quote MUST carry it in key 9.
fn offer_task_id() -> Result<[u8; 32], String> {
    <[u8; 32]>::try_from(from_hex(TV002_TASK_ID_HEX)?.as_slice())
        .map_err(|_| "TV-002 task-id not 32 bytes".into())
}

/// TV-010b — bound PAY_QUOTE: key 9 present and equal the OFFER's TaskID (pay
/// §3.1). Typed check via the real PayQuote codec, mirroring the payments runtime.
pub fn case_tv010b_pay_quote_bound() -> Result<(), String> {
    let opened = expect_open(TV010B_ENV_HEX, &key_b(), msg_type::PAY_QUOTE)?;
    let q = PayQuote::from_value(&opened.body.content).map_err(|e| format!("PayQuote: {e:?}"))?;
    match q.task_binding {
        Some(tb) if tb == offer_task_id()? => Ok(()),
        Some(_) => Err("TV-010b task binding present but != OFFER TaskID".into()),
        None => Err("TV-010b missing key 9 task binding (pay §3.1 MUST)".into()),
    }
}

/// NV-006 — mismatched task binding. The envelope is valid and OPENS; the pay
/// §3.1 binding rule (task_binding MUST equal the OFFER TaskID) is what rejects
/// it, exactly as `map-taskrun::payments` does before agreeing to a quote.
pub fn case_nv006_task_binding_mismatch() -> Result<(), String> {
    // It is a well-formed, correctly-signed PAY_QUOTE — opening must succeed.
    let opened = expect_open(NV006_HEX, &key_b(), msg_type::PAY_QUOTE)
        .map_err(|e| format!("NV-006 must open (valid envelope): {e}"))?;
    let q = PayQuote::from_value(&opened.body.content).map_err(|e| format!("PayQuote: {e:?}"))?;
    // pay §3.1: absent-or-mismatching binding => reject as a different-task quote.
    match q.task_binding {
        Some(tb) if tb == offer_task_id()? => {
            Err("NV-006 binding unexpectedly matched the OFFER TaskID".into())
        }
        _ => Ok(()), // mismatch (or absence) correctly triggers the §3.1 reject
    }
}

/// Recompute TH, CB, and the transcript root from primitives and match them.
pub fn case_derived_constants() -> Result<(), String> {
    let th = transcript_hash(&from_hex(TV001_HEX)?, &from_hex(TV003_ENV_HEX)?);
    if th.as_slice() != from_hex(TH_HEX)?.as_slice() {
        return Err("TH mismatch".into());
    }
    let cb = channel_binding(&th, &[0u8; 32]);
    if cb.as_slice() != from_hex(CB_HEX)?.as_slice() {
        return Err("CB mismatch".into());
    }
    let mut tb = TranscriptBuilder::new();
    tb.push(&Chunk {
        seq: 0,
        kind: ChunkKind::OutputText,
        content: b"Hello, MAP!".to_vec(),
    });
    if tb.root().as_slice() != from_hex(TRANSCRIPT_ROOT_HEX)?.as_slice() {
        return Err("transcript root mismatch".into());
    }
    Ok(())
}

pub fn case_nv001_tampered() -> Result<(), String> {
    match open(&from_hex(NV001_HEX)?, b"") {
        Ok(_) => Err("NV-001 opened (must not)".into()),
        Err(EnvelopeError::Identity(IdentityError::BadSignature)) => Ok(()),
        Err(e) => Err(format!("NV-001 expected bad-signature, got {e:?}")),
    }
}

pub fn case_nv002_wrong_tag() -> Result<(), String> {
    match open(&from_hex(NV002_HEX)?, b"") {
        Ok(_) => Err("NV-002 opened (must not)".into()),
        Err(EnvelopeError::NotCoseSign1) => Ok(()),
        Err(e) => Err(format!("NV-002 expected NotCoseSign1, got {e:?}")),
    }
}

pub fn case_nv003_non_canonical() -> Result<(), String> {
    // The signature is valid; only the payload's inner CBOR is non-canonical,
    // so open() fails at the payload decode with IndefiniteLength (MALFORMED).
    match open(&from_hex(NV003_HEX)?, b"") {
        Ok(_) => Err("NV-003 opened (must not)".into()),
        Err(EnvelopeError::Cbor(CborError::IndefiniteLength)) => Ok(()),
        Err(e) => Err(format!("NV-003 expected IndefiniteLength, got {e:?}")),
    }
}

pub fn case_nv004_stale_seq() -> Result<(), String> {
    let newer = Descriptor::from_value(
        &expect_open(TV005_ENV_HEX, &key_b(), msg_type::DESCRIPTOR)?.body.content,
    )
    .map_err(|e| format!("TV-005 descriptor: {e}"))?;
    let stale = Descriptor::from_value(
        &expect_open(NV004_HEX, &key_b(), msg_type::DESCRIPTOR)?.body.content,
    )
    .map_err(|e| format!("NV-004 descriptor: {e}"))?;
    let mut cache = DescriptorCache::new();
    if cache.ingest(&newer.agent, newer.seq, vec![1]) != Ingest::Accepted {
        return Err("NV-004 seq-1 ingest not Accepted".into());
    }
    match cache.ingest(&stale.agent, stale.seq, vec![2]) {
        Ingest::Stale => Ok(()),
        other => Err(format!("NV-004 expected Stale, got {other:?}")),
    }
}

pub fn case_nv005_swapped_sigs() -> Result<(), String> {
    let r = Receipt::decode_bytes(&from_hex(NV005_HEX)?).map_err(|e| format!("decode: {e}"))?;
    match r.verify() {
        Ok(_) => Err("NV-005 verified (must not)".into()),
        Err(TrustError::Identity(_)) => Ok(()),
        Err(e) => Err(format!("NV-005 expected identity error, got {e:?}")),
    }
}
