//! Pull-based evidence exchange (trust §4): EVIDENCE_QUERY / EVIDENCE_SET over a
//! session. The holder is UNTRUSTED transport — the consumer verifies every
//! returned receipt independently and applies to its projection only what is
//! dual-signed AND names the queried subject as provider (decision 2 + the
//! wrong-provider-attribution defense).

use crate::Outgoing;
use map_core::cbor::Value;
use map_core::errors::{codes, ErrorMsg};
use map_core::msg_type;
use map_identity::AgentId;
use map_session::{Clock, Incoming, MsgIdSource, Session, SessionError, Transport};
use map_trust::{Receipt, ReceiptCore, ReceiptStore, TrustProjection, Verified};

/// Max receipts per EVIDENCE_SET batch (NFR: bound the reply well under
/// max_message_size; receipts are ~330 B).
pub const EVIDENCE_BATCH_MAX: usize = 8;
const DEFAULT_LIMIT: u64 = 64;

#[derive(Debug)]
pub enum EvidenceError {
    Session(SessionError),
    Malformed(String),
}

impl From<SessionError> for EvidenceError {
    fn from(e: SessionError) -> Self {
        EvidenceError::Session(e)
    }
}

struct Query {
    subject: AgentId,
    cap: Option<String>,
    window: Option<(u64, u64)>,
    limit: u64,
    role: Option<u8>,
}

// ---- responder ------------------------------------------------------------

/// FR-3 holder side: filter the store for the query and reply EVIDENCE_SET
/// (batch ≤ 8, `more` when truncated). A malformed query → ERROR (kept alive).
pub fn serve_evidence(inc: &Incoming, store: &ReceiptStore) -> Vec<Outgoing> {
    let q = match parse_query(&inc.content) {
        Ok(q) => q,
        Err(_) => return vec![(msg_type::ERROR, ErrorMsg::new(codes::MALFORMED, false).to_value())],
    };
    let cap = (q.limit as usize).min(EVIDENCE_BATCH_MAX);
    let mut receipts = Vec::new();
    let mut more = false;
    for r in store.iter() {
        let core = match r.core() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if query_matches(&core, &q) {
            if receipts.len() >= cap {
                more = true;
                break;
            }
            receipts.push(r.to_value());
        }
    }
    let mut m = vec![(Value::Uint(1), Value::Array(receipts))];
    if more {
        m.push((Value::Uint(2), Value::Bool(true)));
    }
    vec![(msg_type::EVIDENCE_SET, Value::Map(m))]
}

fn query_matches(core: &ReceiptCore, q: &Query) -> bool {
    let subject_ok = match q.role {
        Some(1) => core.provider == q.subject,
        Some(2) => core.requester == q.subject,
        _ => core.provider == q.subject || core.requester == q.subject,
    };
    if !subject_ok {
        return false;
    }
    if let Some(cap) = &q.cap {
        if &core.capability != cap {
            return false;
        }
    }
    if let Some((from, to)) = q.window {
        if core.started_ms < from || core.started_ms > to {
            return false;
        }
    }
    true
}

fn parse_query(v: &Value) -> Result<Query, EvidenceError> {
    let subject = v
        .map_get(1)
        .and_then(|x| x.as_bytes())
        .and_then(|b| AgentId::from_bytes(b).ok())
        .ok_or_else(|| EvidenceError::Malformed("EVIDENCE_QUERY missing subject".into()))?;
    let cap = v.map_get(2).and_then(|x| x.as_text()).map(|s| s.to_string());
    let window = match v.map_get(3).and_then(|x| x.as_array()) {
        Some(a) if a.len() == 2 => match (a[0].as_uint(), a[1].as_uint()) {
            (Some(f), Some(t)) => Some((f, t)),
            _ => None,
        },
        _ => None,
    };
    let limit = v.map_get(4).and_then(|x| x.as_uint()).unwrap_or(DEFAULT_LIMIT);
    let role = v
        .map_get(5)
        .and_then(|x| x.as_uint())
        .and_then(|n| u8::try_from(n).ok());
    Ok(Query { subject, cap, window, limit, role })
}

// ---- consumer -------------------------------------------------------------

/// FR-3 consumer side: send EVIDENCE_QUERY, receive EVIDENCE_SET, and return the
/// receipts that VERIFY (untrusted transport → every one is checked). The caller
/// applies them via [`apply_verified_evidence`], which enforces the trust gates.
pub fn query_evidence<T: Transport, C: Clock, M: MsgIdSource>(
    session: &mut Session<T, C, M>,
    subject: &AgentId,
    cap: Option<&str>,
    role: Option<u8>,
) -> Result<Vec<Verified>, EvidenceError> {
    session.send(msg_type::EVIDENCE_QUERY, build_query(subject, cap, role))?;
    let inc = session.recv()?;
    if inc.msg_type != msg_type::EVIDENCE_SET {
        return Err(EvidenceError::Malformed("expected EVIDENCE_SET".into()));
    }
    let arr = inc
        .content
        .map_get(1)
        .and_then(|x| x.as_array())
        .ok_or_else(|| EvidenceError::Malformed("EVIDENCE_SET missing list".into()))?;
    let mut verified = Vec::new();
    for item in arr {
        if let Ok(r) = Receipt::from_value(item) {
            if let Ok(v) = r.verify() {
                verified.push(v);
            }
        }
    }
    Ok(verified)
}

/// Apply verified evidence to a projection, enforcing BOTH gates: a receipt is
/// applied ONLY if it is dual-signed AND names `subject` as its provider. The
/// dual gate is decision 2 (no self-reported trust); the provider gate stops a
/// holder filing a dual receipt (where it is merely the requester, or about a
/// third party) under the wrong `(provider, capability)` key. Returns the count
/// applied.
pub fn apply_verified_evidence(
    evidence: &[Verified],
    subject: &AgentId,
    proj: &mut TrustProjection,
    now_ms: u64,
    trust_in_issuer: f64,
) -> usize {
    let mut applied = 0;
    for v in evidence {
        if v.dual && &v.core.provider == subject {
            proj.apply(v, now_ms, trust_in_issuer);
            applied += 1;
        }
    }
    applied
}

fn build_query(subject: &AgentId, cap: Option<&str>, role: Option<u8>) -> Value {
    let mut m = vec![(Value::Uint(1), Value::Bytes(subject.as_bytes().to_vec()))];
    if let Some(c) = cap {
        m.push((Value::Uint(2), Value::Text(c.to_string())));
    }
    if let Some(r) = role {
        m.push((Value::Uint(5), Value::Uint(r as u64)));
    }
    Value::Map(m)
}
