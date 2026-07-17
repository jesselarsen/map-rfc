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
use map_pay::billing::{billed, verify_watermark, AgreedTerms};
use map_pay::codec::{
    pay_ref_digest, AssetAmount, PayAgree, PayInvoice, PayQuote, PayReceipt, KIND_HELD, PHASE_HOLD,
    TERMS_ESCROW, UNIT_TOKEN_IN, UNIT_TOKEN_OUT,
};
use map_pay::dispute::{authenticate_item, Parties, PayDispute, PayDisputeEvidence};
use map_pay::escrow::{verify_escrow_invoice, EscrowContext};
use map_pay::{Amount, Asset};
use map_tasks::msg::{Assign, Lease, Reserve};
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
    let opened = expect_open(TV007_ENV_HEX, &key_a(), msg_type::ASSIGN)?;
    // Produce+consume guard (TASK-018): the Assign codec — including
    // InputSpec key 2 (content-addressed [+ Multihash]) — must round-trip the
    // frozen ASSIGN byte-for-byte, not merely open the envelope. This closes
    // the ERRATA 2026-07-05 hole where TV-007 was consume-only for Rust.
    let assign = Assign::from_value(&opened.body.content).map_err(|e| format!("Assign: {e}"))?;
    // The vector's InputSpec is content-addressed (key 2), not inline.
    let blocks = assign.input.blocks.as_ref().ok_or("TV-007 InputSpec missing key 2 (content-addressed blocks)")?;
    if blocks.len() != 1 || blocks[0].len() != 34 || blocks[0][0..2] != [0x12, 0x20] {
        return Err("TV-007 InputSpec block is not a single sha2-256 multihash".into());
    }
    if assign.input.inline.is_some() {
        return Err("TV-007 InputSpec must not carry inline key 1".into());
    }
    let re = encode(&assign.to_value()).map_err(|e| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("Assign did not round-trip byte-for-byte".into());
    }
    Ok(())
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

/// NV-009 — a 2-link chain whose PARENT declares max-depth 0 while the child
/// delegates one hop further; scopes are identical (no widening), isolating
/// the identity §4 depth bound. MUST reject as DepthExceeded (wire 2003).
/// (TASK-018; max-depth counting per the 2026-07-12 ARCHITECT RULING.)
pub fn case_nv009_depth_exceeded() -> Result<(), String> {
    match open_delegated(&from_hex(NV009_HEX)?, b"", None, DELEG_NOW_MS) {
        Ok(_) => Err("NV-009 chain accepted (must not)".into()),
        Err(EnvelopeError::Delegation(DelegError::DepthExceeded)) => Ok(()),
        Err(e) => Err(format!("NV-009 expected DepthExceeded, got {e:?}")),
    }
}

/// The model-1 rate table TV-016/NV-010 are billed under (pay §4.1): 3000
/// µUSD/token-in, 5000 µUSD/token-out. Not on the wire — the agreed quote's.
fn watermark_rates() -> Vec<(u64, AssetAmount)> {
    let micro = |m| AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-6, m) };
    vec![(UNIT_TOKEN_IN, micro(3000)), (UNIT_TOKEN_OUT, micro(5000))]
}

/// TV-016 — a metered PAY_INVOICE: the codec round-trips it byte-exact
/// (produce guard), the model-1 arithmetic reproduces cumulative-billed, and
/// `verify_watermark` accepts (pay §4/§4.1).
pub fn case_tv016_metered_invoice() -> Result<(), String> {
    let opened = expect_open(TV016_ENV_HEX, &key_b(), msg_type::PAY_INVOICE)?;
    let inv = PayInvoice::from_value(&opened.body.content).map_err(|e| format!("PayInvoice: {e:?}"))?;
    // produce guard: re-encode the parsed invoice and compare to the wire content.
    let re = encode(&inv.to_value()).map_err(|e| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("TV-016 PayInvoice did not round-trip byte-for-byte".into());
    }
    let w = inv.watermark.as_ref().ok_or("TV-016 missing UsageWatermark (key 4)")?;
    // arithmetic: rate × usage == the claimed cumulative-billed (= 8_629_000).
    let computed = billed(1, &watermark_rates(), &w.usage).map_err(|e| format!("billed: {e:?}"))?;
    if computed != w.cumulative_billed {
        return Err("TV-016 billing arithmetic != cumulative-billed".into());
    }
    let agreed = AgreedTerms { task: w.task, pricing_model: 1, rate_table: watermark_rates(), terms: 3, authorized_cap: None, quoted_total: None };
    verify_watermark(&agreed, None, &inv).map_err(|c| format!("TV-016 verify_watermark rejected with {c}"))
}

/// NV-010 — the TV-016 invoice with cumulative-billed off by one. It opens and
/// its signature verifies, but pay §4.1 arithmetic MUST reject with 6009.
pub fn case_nv010_watermark_billing() -> Result<(), String> {
    let opened = expect_open(NV010_HEX, &key_b(), msg_type::PAY_INVOICE)
        .map_err(|e| format!("NV-010 must open (valid envelope): {e}"))?;
    let inv = PayInvoice::from_value(&opened.body.content).map_err(|e| format!("PayInvoice: {e:?}"))?;
    let w = inv.watermark.as_ref().ok_or("NV-010 missing UsageWatermark")?;
    let agreed = AgreedTerms { task: w.task, pricing_model: 1, rate_table: watermark_rates(), terms: 3, authorized_cap: None, quoted_total: None };
    match verify_watermark(&agreed, None, &inv) {
        Ok(()) => Err("NV-010 watermark accepted (must reject 6009)".into()),
        Err(map_core::errors::codes::INVOICE_WATERMARK_VIOLATION) => Ok(()),
        Err(c) => Err(format!("NV-010 expected 6009, got {c}")),
    }
}

/// TV-017 — an escrow hold PAY_INVOICE (terms 2, phase 1): the codec
/// round-trips it byte-exact, and `verify_escrow_invoice` accepts it under a
/// terms-2 agreement whose quoted total matches (pay §5.2.1/§5.2.3, TASK-024).
pub fn case_tv017_escrow_hold_invoice() -> Result<(), String> {
    let opened = expect_open(TV017_ENV_HEX, &key_b(), msg_type::PAY_INVOICE)?;
    let inv = PayInvoice::from_value(&opened.body.content).map_err(|e| format!("PayInvoice: {e:?}"))?;
    let re = encode(&inv.to_value()).map_err(|e| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("TV-017 PayInvoice did not round-trip byte-for-byte".into());
    }
    if inv.phase != Some(PHASE_HOLD) {
        return Err("TV-017 phase (key 7) != 1 (hold)".into());
    }
    if inv.watermark.is_some() {
        return Err("TV-017 must not carry a watermark (presence iff terms 3)".into());
    }
    let total = AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, 500) };
    if inv.amount != total {
        return Err("TV-017 amount != the quoted total [-2, 500]".into());
    }
    let agreed = AgreedTerms {
        task: [0; 32],
        pricing_model: 1,
        rate_table: watermark_rates(),
        terms: TERMS_ESCROW,
        authorized_cap: None,
        quoted_total: Some(total),
    };
    verify_escrow_invoice(&agreed, &EscrowContext::default(), &inv)
        .map_err(|c| format!("TV-017 verify_escrow_invoice rejected with {c}"))
}

/// TV-018 — a held PAY_RECEIPT (kind 1): round-trips byte-exact with the kind
/// key decoded (pay §5.2.1, TASK-024).
pub fn case_tv018_held_receipt() -> Result<(), String> {
    let opened = expect_open(TV018_ENV_HEX, &key_b(), msg_type::PAY_RECEIPT)?;
    let r = PayReceipt::from_value(&opened.body.content).map_err(|e| format!("PayReceipt: {e:?}"))?;
    let re = encode(&r.to_value()).map_err(|e| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("TV-018 PayReceipt did not round-trip byte-for-byte".into());
    }
    if r.kind != Some(KIND_HELD) {
        return Err("TV-018 kind (key 5) != 1 (held)".into());
    }
    Ok(())
}

/// NV-011 — the TV-018 receipt with kind = 9. It opens and its signature
/// verifies, but pay §5.2.1 admits only kinds 0..3 — the codec MUST reject
/// the unrecognized kind fail-closed (6010 class).
pub fn case_nv011_unknown_receipt_kind() -> Result<(), String> {
    let opened = expect_open(NV011_HEX, &key_b(), msg_type::PAY_RECEIPT)
        .map_err(|e| format!("NV-011 must open (valid envelope): {e}"))?;
    match PayReceipt::from_value(&opened.body.content) {
        Ok(_) => Err("NV-011 unknown receipt kind accepted (must reject)".into()),
        Err(map_pay::PayError::BadField(5)) => Ok(()),
        Err(e) => Err(format!("NV-011 expected BadField(5), got {e:?}")),
    }
}

/// TV-019 — a PAY_DISPUTE (claim 3): round-trips byte-exact; the anchor ref
/// digest equals the TV-013 envelope multihash (pay §6.1, I1); the claim and
/// task binding decode as pinned (TASK-026).
pub fn case_tv019_pay_dispute() -> Result<(), String> {
    let opened = expect_open(TV019_ENV_HEX, &key_a(), msg_type::PAY_DISPUTE)?;
    let d = PayDispute::from_value(&opened.body.content).map_err(|e| format!("PayDispute: {e:?}"))?;
    let re = encode(&d.to_value()).map_err(|e| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("TV-019 PayDispute did not round-trip byte-for-byte".into());
    }
    if d.claim != 3 {
        return Err("TV-019 claim != 3 (overbilling)".into());
    }
    let tv013 = from_hex(TV013_ENV_HEX)?;
    if d.agree_ref.digest != pay_ref_digest(&tv013) {
        return Err("TV-019 agree-ref digest != multihash(TV-013 envelope) (I1)".into());
    }
    if d.statement.is_some() {
        return Err("TV-019 must carry no statement".into());
    }
    Ok(())
}

/// TV-020 — a PAY_DISPUTE_EVIDENCE whose single kind-0 item carries the
/// frozen TV-010b envelope VERBATIM (I1 verbatim-carry), out-of-session
/// (no CB) — and it authenticates + parses back to the bound quote.
pub fn case_tv020_dispute_evidence() -> Result<(), String> {
    let opened = expect_open(TV020_ENV_HEX, &key_a(), msg_type::PAY_DISPUTE_EVIDENCE)?;
    let ev = PayDisputeEvidence::from_value(&opened.body.content).map_err(|e| format!("PayDisputeEvidence: {e:?}"))?;
    let re = encode(&ev.to_value()).map_err(|e| format!("encode: {e}"))?;
    let orig = encode(&opened.body.content).map_err(|e| format!("encode content: {e}"))?;
    if re != orig {
        return Err("TV-020 did not round-trip byte-for-byte".into());
    }
    let [item] = ev.items.as_slice() else {
        return Err("TV-020 must carry exactly one item".into());
    };
    if item.kind != 0 || item.cb.is_some() {
        return Err("TV-020 item must be kind 0 (quote envelope), no CB".into());
    }
    if item.bytes != from_hex(TV010B_ENV_HEX)? {
        return Err("TV-020 item bytes != the frozen TV-010b envelope (I1 verbatim-carry)".into());
    }
    // §6.2/§6.3: the out-of-session item authenticates and parses as the quote.
    let parties = Parties { payer: key_a().agent_id(), payee: key_b().agent_id(), operator: None };
    let auth = authenticate_item(item, &parties).map_err(|e| format!("TV-020 item did not authenticate: {e:?}"))?;
    let q = PayQuote::from_value(auth.content.as_ref().ok_or("no content")?)
        .map_err(|e| format!("PayQuote: {e:?}"))?;
    if q.quote_id != [0x10u8; 16] {
        return Err("TV-020 carried quote has the wrong quote-id".into());
    }
    Ok(())
}

/// NV-012 — the TV-019 dispute with claim = 9. It opens and verifies, but
/// pay §6.1 admits only claims 0..4 — the codec MUST reject fail-closed.
pub fn case_nv012_unknown_claim() -> Result<(), String> {
    let opened = expect_open(NV012_HEX, &key_a(), msg_type::PAY_DISPUTE)
        .map_err(|e| format!("NV-012 must open (valid envelope): {e}"))?;
    match PayDispute::from_value(&opened.body.content) {
        Ok(_) => Err("NV-012 unknown claim accepted (must reject)".into()),
        Err(map_pay::PayError::BadField(3)) => Ok(()),
        Err(e) => Err(format!("NV-012 expected BadField(3), got {e:?}")),
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
