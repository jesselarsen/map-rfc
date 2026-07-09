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
//!
//! Delegation pass (TASK-010): TV-011 cert + TV-012 delegated OFFER consume,
//! NV-007 widened-scope reject. Spec-round-3 (TASK-011): TV-013 PAY_AGREE
//! (digest binds TV-010b's envelope), TV-014 RESERVE + TV-015 LEASE consume,
//! NV-008 out-of-window lease reject (pay §3.1 / tasks §6).

use crate::positive::{key_a, key_b, sha256};
use crate::vectors::{TV001_HEX, TV002_TASK_ID_HEX};
use crate::vectors2::*;
use crate::from_hex;
use map_core::delegation::DelegError;
use map_core::{open_delegated, validate_cert};
use map_pay::codec::{PayAgree, PayQuote};
use map_tasks::msg::{Lease, Reserve};
use sha2::{Digest, Sha256};
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

/// Delegation validity time inside every TV-011/012/NV-007 cert window.
const DELEG_NOW_MS: u64 = 1_780_000_100_000;

fn key_a_dk() -> Keypair {
    Keypair::from_seed(sha256(b"MAP-TEST-VECTOR-KEY-A-DK"))
}

/// TV-011 — a DelegationCert: RIK (Key A) delegates to DK. Validate it and
/// confirm issuer/subject; content round-trips through the codec (produce-guard).
pub fn case_tv011_cert() -> Result<(), String> {
    let body = validate_cert(&from_hex(TV011_ENV_HEX)?, DELEG_NOW_MS)
        .map_err(|e| format!("TV-011 validate_cert: {e}"))?;
    if body.issuer != key_a().agent_id() {
        return Err("TV-011 issuer != RIK (Key A)".into());
    }
    if body.subject != key_a_dk().agent_id() {
        return Err("TV-011 subject != DK".into());
    }
    // Produce-guard: re-encode the parsed body and compare to the wire content.
    let content = &open(&from_hex(TV011_ENV_HEX)?, b"")
        .map_err(|e| format!("TV-011 open: {e}"))?
        .body
        .content;
    let re = encode(&body.to_value()).map_err(|e| format!("encode: {e}"))?;
    let orig = encode(content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("TV-011 DelegateBody did not round-trip byte-for-byte".into());
    }
    Ok(())
}

/// TV-012 — a delegated OFFER signed by DK carrying [TV-011]. `open_delegated`
/// resolves the accountable root to RIK and attenuates scope to CAP.
pub fn case_tv012_delegated_offer() -> Result<(), String> {
    let od = open_delegated(&from_hex(TV012_ENV_HEX)?, b"", None, DELEG_NOW_MS)
        .map_err(|e| format!("TV-012 open_delegated: {e}"))?;
    if od.opened.sender != key_a_dk().agent_id() {
        return Err("TV-012 leaf signer != DK".into());
    }
    if od.scope.root != key_a().agent_id() {
        return Err("TV-012 accountable root != RIK (Key A)".into());
    }
    if od.scope.caps.as_deref() != Some(&["map:cap:inference/text-generation".to_string()][..]) {
        return Err(format!("TV-012 scope caps unexpected: {:?}", od.scope.caps));
    }
    if od.scope.depth_left != 1 {
        return Err(format!("TV-012 depth_left {} != 1", od.scope.depth_left));
    }
    // Plain open() still accepts it and ignores key 33 (byte-frozen behavior).
    if open(&from_hex(TV012_ENV_HEX)?, b"").map_err(|e| format!("open: {e}"))?.sender
        != key_a_dk().agent_id()
    {
        return Err("TV-012 plain open sender != DK".into());
    }
    Ok(())
}

/// NV-007 — a chain whose child widens cap scope. Opens as an envelope, but the
/// chain MUST be rejected (identity §4 tighten-only → WidenedScope, wire 2003).
pub fn case_nv007_widened_scope() -> Result<(), String> {
    match open_delegated(&from_hex(NV007_HEX)?, b"", None, DELEG_NOW_MS) {
        Ok(_) => Err("NV-007 chain accepted (must not)".into()),
        Err(EnvelopeError::Delegation(DelegError::WidenedScope { .. })) => Ok(()),
        Err(e) => Err(format!("NV-007 expected WidenedScope, got {e:?}")),
    }
}

/// TV-013 — PAY_AGREE binding the bound quote TV-010b. Key 2 MUST be the
/// multihash of TV-010b's received envelope bytes (pay §3.1, invariant I1).
pub fn case_tv013_pay_agree() -> Result<(), String> {
    let opened = expect_open(TV013_ENV_HEX, &key_a(), msg_type::PAY_AGREE)?;
    let a = PayAgree::from_value(&opened.body.content).map_err(|e| format!("PayAgree: {e:?}"))?;
    if a.quote_id != [0x10u8; 16] {
        return Err("TV-013 quote-id != TV-010b quote-id".into());
    }
    let mut mh = vec![0x12u8, 0x20];
    mh.extend_from_slice(&Sha256::digest(from_hex(TV010B_ENV_HEX)?));
    if a.quote_digest != mh {
        return Err("TV-013 key 2 != multihash(TV-010b envelope)".into());
    }
    Ok(())
}

/// The RESERVE window that TV-015 / NV-008 leases are checked against.
fn reserve_window() -> Result<(u64, u64), String> {
    let r = Reserve::from_value(&expect_open(TV014_ENV_HEX, &key_a(), msg_type::RESERVE)?.body.content)
        .map_err(|e| format!("Reserve: {e:?}"))?;
    Ok((r.from_ms, r.until_ms))
}

/// TV-014 — RESERVE round-trips and reserves CAP in a forward window.
pub fn case_tv014_reserve() -> Result<(), String> {
    let opened = expect_open(TV014_ENV_HEX, &key_a(), msg_type::RESERVE)?;
    let r = Reserve::from_value(&opened.body.content).map_err(|e| format!("Reserve: {e:?}"))?;
    if r.capability != "map:cap:inference/text-generation" {
        return Err(format!("TV-014 capability unexpected: {}", r.capability));
    }
    if r.until_ms <= r.from_ms {
        return Err("TV-014 window is not forward".into());
    }
    Ok(())
}

/// TV-015 — a granted LEASE whose window falls within TV-014's requested one.
pub fn case_tv015_lease_granted() -> Result<(), String> {
    let (from, until) = reserve_window()?;
    let opened = expect_open(TV015_ENV_HEX, &key_b(), msg_type::LEASE)?;
    let l = Lease::from_value(&opened.body.content).map_err(|e| format!("Lease: {e:?}"))?;
    if l.status != Lease::GRANTED {
        return Err(format!("TV-015 status {} != granted", l.status));
    }
    if !l.granted_window_within(from, until) {
        return Err("TV-015 granted window not within the request".into());
    }
    Ok(())
}

/// NV-008 — a granted LEASE outside the requested window. It opens (valid,
/// signed) but tasks §6 requires the requester to reject the out-of-window grant.
pub fn case_nv008_lease_out_of_window() -> Result<(), String> {
    let (from, until) = reserve_window()?;
    let opened = expect_open(NV008_HEX, &key_b(), msg_type::LEASE)
        .map_err(|e| format!("NV-008 must open (valid envelope): {e}"))?;
    let l = Lease::from_value(&opened.body.content).map_err(|e| format!("Lease: {e:?}"))?;
    if l.granted_window_within(from, until) {
        return Err("NV-008 window unexpectedly within the request".into());
    }
    Ok(())
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
