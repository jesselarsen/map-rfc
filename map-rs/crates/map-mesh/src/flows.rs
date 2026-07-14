//! CAP_QUERY/CAP_RESPONSE (0x0103/0x0104) and STATUS_QUERY/STATUS_REPORT
//! (0x0101/0x0102) over a live session.
//!
//! Content-map field numbers (registry assignments recorded in
//! draft-map-discovery-01 §3/§7 as part of this change):
//!   CAP_QUERY    {1: CapURI, ?2: shard, ?3: limit}
//!   CAP_RESPONSE {1: [* descriptor-envelope bstr], ?2: more(bool)}
//!   STATUS_QUERY {?1: [+ CapURI], ?2: bstr .size 8 echo}   (§3 array form)
//!   STATUS_REPORT{1: ts, ?2: queue, ?3: est-ttft, ?4: accepting, ?6: echo}
//!
//! The index is UNTRUSTED transport: `query_caps` verifies EVERY returned
//! envelope independently (via `ingest_descriptor_envelope`) before caching.
//! STATUS is volatile (decision 3) and MUST NOT enter any cache/index.

use crate::index::MeshIndex;
use crate::publish::ingest_descriptor_envelope;
use crate::MeshError;
use map_core::cbor::Value;
use map_core::errors::{codes, ErrorMsg};
use map_core::msg_type;
use map_discovery::{Descriptor, DescriptorCache};
use map_identity::AgentId;
use map_session::{Clock, MsgIdSource, Session, Transport};
use std::collections::BTreeSet;

/// Default (and max) descriptors per CAP_RESPONSE batch (R2 — bound the reply).
pub const DEFAULT_CAP_LIMIT: usize = 8;

/// Volatile provider status (FR-5). NEVER persisted into cache/index.
#[derive(Clone, Debug, Default)]
pub struct StatusSnapshot {
    pub queue_length: Option<u64>,
    pub est_ttft_ms: Option<u64>,
    /// 0 = no, 1 = yes, 2 = yes-if-reserved (discovery §3).
    pub accepting: Option<u8>,
}

/// A parsed STATUS_REPORT (client side).
#[derive(Clone, Debug, PartialEq)]
pub struct StatusReport {
    pub timestamp_ms: u64,
    pub queue_length: Option<u64>,
    pub est_ttft_ms: Option<u64>,
    pub accepting: Option<u8>,
    pub echo: Option<[u8; 8]>,
}

/// Result of serving one message.
#[derive(Debug, PartialEq, Eq)]
pub enum ServeOutcome {
    Served,
    Closed,
}

// ---- responder ------------------------------------------------------------

/// Serve exactly one incoming query against `index`/`status` and reply. A
/// malformed or unsupported query is answered with an ERROR (mapped core §9
/// code) and still returns `Served` — a bad query MUST NOT kill the loop (§10).
/// Only transport/session failures return `Err`.
pub fn serve_one<T: Transport, C: Clock, M: MsgIdSource>(
    session: &mut Session<T, C, M>,
    index: &MeshIndex,
    status: &StatusSnapshot,
) -> Result<ServeOutcome, MeshError> {
    let inc = match session.recv() {
        Ok(i) => i,
        Err(map_session::SessionError::PeerClosed { .. }) => return Ok(ServeOutcome::Closed),
        Err(e) => return Err(MeshError::Session(e)),
    };
    let now = session.now_ms();
    match inc.msg_type {
        t if t == msg_type::CAP_QUERY => match parse_cap_query(&inc.content) {
            Ok((cap, shard, limit)) => {
                let resp = build_cap_response(index, &cap, shard, limit);
                session.send(msg_type::CAP_RESPONSE, resp)?;
            }
            Err(e) => reply_error(session, e)?,
        },
        t if t == msg_type::STATUS_QUERY => {
            let echo = parse_status_query_echo(&inc.content);
            session.send(msg_type::STATUS_REPORT, build_status_report(status, now, echo))?;
        }
        _ => reply_error(session, MeshError::Malformed("unsupported query type".into()))?,
    }
    Ok(ServeOutcome::Served)
}

/// Serve queries until the peer sends CLOSE.
pub fn serve_until_close<T: Transport, C: Clock, M: MsgIdSource>(
    session: &mut Session<T, C, M>,
    index: &MeshIndex,
    status: &StatusSnapshot,
) -> Result<(), MeshError> {
    while serve_one(session, index, status)? == ServeOutcome::Served {}
    Ok(())
}

fn reply_error<T: Transport, C: Clock, M: MsgIdSource>(
    session: &mut Session<T, C, M>,
    err: MeshError,
) -> Result<(), MeshError> {
    let code = err.wire_code().unwrap_or(codes::MALFORMED);
    session.send(msg_type::ERROR, ErrorMsg::new(code, false).to_value())?;
    Ok(())
}

fn build_cap_response(index: &MeshIndex, cap: &str, shard: Option<u8>, limit: Option<u64>) -> Value {
    let cap_limit = limit
        .map(|l| l as usize)
        .unwrap_or(DEFAULT_CAP_LIMIT)
        .min(DEFAULT_CAP_LIMIT);
    // With a shard given, serve that shard; else aggregate across all 256 (the
    // in-process stand-in for shard sampling — cheap, N small).
    let agents: Vec<AgentId> = match shard {
        Some(s) => index.providers(cap, s),
        None => {
            let mut set = BTreeSet::new();
            for s in 0u8..=255 {
                set.extend(index.providers(cap, s));
            }
            set.into_iter().collect()
        }
    };
    let mut envs = Vec::new();
    let mut more = false;
    for a in agents {
        if envs.len() >= cap_limit {
            more = true;
            break;
        }
        if let Some(bytes) = index.latest_desc(&a) {
            envs.push(Value::Bytes(bytes.clone()));
        }
    }
    let mut m = vec![(Value::Uint(1), Value::Array(envs))];
    if more {
        m.push((Value::Uint(2), Value::Bool(true)));
    }
    Value::Map(m)
}

fn build_status_report(status: &StatusSnapshot, now_ms: u64, echo: Option<[u8; 8]>) -> Value {
    let mut m = vec![(Value::Uint(1), Value::Uint(now_ms))];
    if let Some(q) = status.queue_length {
        m.push((Value::Uint(2), Value::Uint(q)));
    }
    if let Some(t) = status.est_ttft_ms {
        m.push((Value::Uint(3), Value::Uint(t)));
    }
    if let Some(a) = status.accepting {
        m.push((Value::Uint(4), Value::Uint(a as u64)));
    }
    if let Some(e) = echo {
        m.push((Value::Uint(6), Value::Bytes(e.to_vec())));
    }
    Value::Map(m)
}

// ---- client ---------------------------------------------------------------

/// FR-4 client: CAP_QUERY, then verify+ingest EVERY returned envelope. Bad
/// envelopes are reported, not fatal (§9): returns (verified descriptors,
/// per-envelope rejection reasons).
pub fn query_caps<T: Transport, C: Clock, M: MsgIdSource>(
    session: &mut Session<T, C, M>,
    cap: &str,
    shard: Option<u8>,
    limit: Option<u64>,
    cache: &mut DescriptorCache,
) -> Result<(Vec<Descriptor>, Vec<String>), MeshError> {
    session.send(msg_type::CAP_QUERY, build_cap_query(cap, shard, limit))?;
    let inc = session.recv()?;
    if inc.msg_type != msg_type::CAP_RESPONSE {
        return Err(MeshError::Malformed("expected CAP_RESPONSE".into()));
    }
    let (envelopes, _more) = parse_cap_response(&inc.content)?;
    let now = session.now_ms();
    let mut good = Vec::new();
    let mut bad = Vec::new();
    for env in envelopes {
        match ingest_descriptor_envelope(&env, cache, now) {
            Ok((desc, _)) => good.push(desc),
            Err(e) => bad.push(e.to_string()),
        }
    }
    Ok((good, bad))
}

/// FR-5 client: STATUS_QUERY, then parse the STATUS_REPORT. Nothing is cached.
pub fn query_status<T: Transport, C: Clock, M: MsgIdSource>(
    session: &mut Session<T, C, M>,
    caps: &[&str],
    echo: Option<[u8; 8]>,
) -> Result<StatusReport, MeshError> {
    session.send(msg_type::STATUS_QUERY, build_status_query(caps, echo))?;
    let inc = session.recv()?;
    if inc.msg_type != msg_type::STATUS_REPORT {
        return Err(MeshError::Malformed("expected STATUS_REPORT".into()));
    }
    parse_status_report(&inc.content)
}

// ---- content codecs -------------------------------------------------------

fn build_cap_query(cap: &str, shard: Option<u8>, limit: Option<u64>) -> Value {
    let mut m = vec![(Value::Uint(1), Value::Text(cap.to_string()))];
    if let Some(s) = shard {
        m.push((Value::Uint(2), Value::Uint(s as u64)));
    }
    if let Some(l) = limit {
        m.push((Value::Uint(3), Value::Uint(l)));
    }
    Value::Map(m)
}

fn parse_cap_query(v: &Value) -> Result<(String, Option<u8>, Option<u64>), MeshError> {
    let cap = v
        .map_get(1)
        .and_then(|x| x.as_text())
        .ok_or_else(|| MeshError::Malformed("CAP_QUERY missing cap (key 1)".into()))?
        .to_string();
    let shard = v.map_get(2).and_then(|x| x.as_uint()).and_then(|n| u8::try_from(n).ok());
    let limit = v.map_get(3).and_then(|x| x.as_uint());
    Ok((cap, shard, limit))
}

fn parse_cap_response(v: &Value) -> Result<(Vec<Vec<u8>>, bool), MeshError> {
    let arr = v
        .map_get(1)
        .and_then(|x| x.as_array())
        .ok_or_else(|| MeshError::Malformed("CAP_RESPONSE missing list (key 1)".into()))?;
    let envelopes = arr
        .iter()
        .map(|x| {
            x.as_bytes()
                .map(|b| b.to_vec())
                .ok_or_else(|| MeshError::Malformed("CAP_RESPONSE entry not bytes".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let more = matches!(v.map_get(2), Some(Value::Bool(true)));
    Ok((envelopes, more))
}

fn build_status_query(caps: &[&str], echo: Option<[u8; 8]>) -> Value {
    let mut m = Vec::new();
    if !caps.is_empty() {
        m.push((
            Value::Uint(1),
            Value::Array(caps.iter().map(|c| Value::Text(c.to_string())).collect()),
        ));
    }
    if let Some(e) = echo {
        m.push((Value::Uint(2), Value::Bytes(e.to_vec())));
    }
    Value::Map(m)
}

fn parse_status_query_echo(v: &Value) -> Option<[u8; 8]> {
    v.map_get(2)
        .and_then(|x| x.as_bytes())
        .and_then(|b| <[u8; 8]>::try_from(b).ok())
}

fn parse_status_report(v: &Value) -> Result<StatusReport, MeshError> {
    let timestamp_ms = v
        .map_get(1)
        .and_then(|x| x.as_uint())
        .ok_or_else(|| MeshError::Malformed("STATUS_REPORT missing timestamp (key 1)".into()))?;
    Ok(StatusReport {
        timestamp_ms,
        queue_length: v.map_get(2).and_then(|x| x.as_uint()),
        est_ttft_ms: v.map_get(3).and_then(|x| x.as_uint()),
        accepting: v.map_get(4).and_then(|x| x.as_uint()).and_then(|n| u8::try_from(n).ok()),
        echo: v.map_get(6).and_then(|x| x.as_bytes()).and_then(|b| <[u8; 8]>::try_from(b).ok()),
    })
}
