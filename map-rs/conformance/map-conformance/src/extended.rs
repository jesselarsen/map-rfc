//! Extended vectors consumed/rejected by the Rust implementation: TV-003
//! HELLO_ACK, TV-004 ERROR, TV-005 DESCRIPTOR, NV-001..005 (each rejected with
//! the named class), and the derived constants (TH, CB, transcript root)
//! recomputed from the primitives.
//!
//! Task, pay and dispute vectors were RETIRED(v1) in P2; delegation vectors
//! (TV-011/012, NV-007/009) in P3, with delegation itself (layer §3.3). Their
//! cases live at tag map-v1-ietf.

use crate::positive::key_b;
use crate::vectors::TV001_HEX;
use crate::vectors2::*;
use crate::from_hex;
use map_core::cbor::{encode, CborError};
use map_core::envelope::{open, EnvelopeError, Opened};
use map_core::errors::ErrorMsg;
use map_core::hello::{channel_binding, transcript_hash, HelloAck};
use map_core::msg_type;
use map_discovery::{Descriptor, DescriptorCache, Ingest};
use map_identity::{IdentityError, Keypair};
use map_transcript::{Chunk, ChunkKind, TranscriptBuilder};
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
