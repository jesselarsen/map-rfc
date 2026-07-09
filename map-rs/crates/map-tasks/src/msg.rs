//! Task-negotiation and streaming message codecs (draft-map-tasks-00 §2/§2.1/
//! §3/§4, core §8.1). Pure `to_value`/`from_value` over the deterministic CBOR
//! `Value` model — the tasks context owns these wire structures; the runtimes
//! (map-taskrun) build on them.
//!
//! Payment references (Offer key 4, Quote key 2, Assign key 6) are modeled as
//! omittable `Option<Value>` and are NOT populated in this revision — TASK-009
//! wires them. Note tasks §2 currently makes Quote key 2 REQUIRED; see
//! `map-spec/ERRATA.md` (2026-07-05). `InputSpec` models inline bytes (key 1)
//! only; content-addressed fetch (keys 2/3) is out of scope (ERRATA).

use crate::{ChunkKind, TaskId};
use map_core::cbor::Value;
use std::fmt;

/// sha2-256 multihash prefix (0x12 code, 0x20 length) — Complete key 3 wraps
/// the 32-byte transcript root as `Multihash` (core §3, tasks §4).
const SHA256_MULTIHASH_PREFIX: [u8; 2] = [0x12, 0x20];

#[derive(Debug, PartialEq, Eq)]
pub enum MsgError {
    MissingField(u64),
    BadField(u64),
}

impl fmt::Display for MsgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MsgError::MissingField(k) => write!(f, "message missing field {k}"),
            MsgError::BadField(k) => write!(f, "message field {k} invalid"),
        }
    }
}
impl std::error::Error for MsgError {}

// --- small readers ---------------------------------------------------------

fn uint(v: &Value, k: u64) -> Result<u64, MsgError> {
    v.map_get(k).ok_or(MsgError::MissingField(k))?.as_uint().ok_or(MsgError::BadField(k))
}
fn opt_uint(v: &Value, k: u64) -> Result<Option<u64>, MsgError> {
    match v.map_get(k) {
        None => Ok(None),
        Some(x) => Ok(Some(x.as_uint().ok_or(MsgError::BadField(k))?)),
    }
}
fn bytes(v: &Value, k: u64) -> Result<&[u8], MsgError> {
    v.map_get(k).ok_or(MsgError::MissingField(k))?.as_bytes().ok_or(MsgError::BadField(k))
}
fn text(v: &Value, k: u64) -> Result<String, MsgError> {
    Ok(v.map_get(k).ok_or(MsgError::MissingField(k))?.as_text().ok_or(MsgError::BadField(k))?.to_string())
}
fn task_at(v: &Value, k: u64) -> Result<TaskId, MsgError> {
    TaskId::from_slice(bytes(v, k)?).ok_or(MsgError::BadField(k))
}
fn u16_at(v: &Value, k: u64) -> Result<u16, MsgError> {
    u16::try_from(uint(v, k)?).map_err(|_| MsgError::BadField(k))
}

// --- shared sub-structures -------------------------------------------------

/// Task constraints (tasks §2). All fields optional; key 8 is an opaque ext map
/// preserved for roundtrip. Hard-constraint enforcement is TASK-010+ territory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskConstraints {
    pub max_context_tokens: Option<u64>, // 1
    pub max_output_tokens: Option<u64>,  // 2
    pub deadline_ms: Option<u64>,        // 3
    pub ttft_ms: Option<u64>,            // 4
    pub zone: Option<u8>,                // 5
    pub min_sandbox: Option<u64>,        // 6
    pub model_digest: Option<Vec<u8>>,   // 7 (Multihash)
    pub ext: Option<Value>,              // 8
}

impl TaskConstraints {
    pub fn to_value(&self) -> Value {
        let mut m = Vec::new();
        if let Some(x) = self.max_context_tokens { m.push((Value::Uint(1), Value::Uint(x))); }
        if let Some(x) = self.max_output_tokens { m.push((Value::Uint(2), Value::Uint(x))); }
        if let Some(x) = self.deadline_ms { m.push((Value::Uint(3), Value::Uint(x))); }
        if let Some(x) = self.ttft_ms { m.push((Value::Uint(4), Value::Uint(x))); }
        if let Some(x) = self.zone { m.push((Value::Uint(5), Value::Uint(x as u64))); }
        if let Some(x) = self.min_sandbox { m.push((Value::Uint(6), Value::Uint(x))); }
        if let Some(d) = &self.model_digest { m.push((Value::Uint(7), Value::Bytes(d.clone()))); }
        if let Some(e) = &self.ext { m.push((Value::Uint(8), e.clone())); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<TaskConstraints, MsgError> {
        Ok(TaskConstraints {
            max_context_tokens: opt_uint(v, 1)?,
            max_output_tokens: opt_uint(v, 2)?,
            deadline_ms: opt_uint(v, 3)?,
            ttft_ms: opt_uint(v, 4)?,
            zone: match opt_uint(v, 5)? {
                None => None,
                Some(n) => Some(u8::try_from(n).map_err(|_| MsgError::BadField(5))?),
            },
            min_sandbox: opt_uint(v, 6)?,
            model_digest: v.map_get(7).map(|x| x.as_bytes().map(|b| b.to_vec()).ok_or(MsgError::BadField(7))).transpose()?,
            ext: v.map_get(8).cloned(),
        })
    }
}

/// Input specification (tasks §2.1). Inline bytes (key 1) only in this revision.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputSpec {
    pub inline: Option<Vec<u8>>, // 1
}
impl InputSpec {
    pub fn inline(bytes: Vec<u8>) -> Self {
        InputSpec { inline: Some(bytes) }
    }
    pub fn to_value(&self) -> Value {
        let mut m = Vec::new();
        if let Some(b) = &self.inline { m.push((Value::Uint(1), Value::Bytes(b.clone()))); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<InputSpec, MsgError> {
        Ok(InputSpec {
            inline: v.map_get(1).map(|x| x.as_bytes().map(|b| b.to_vec()).ok_or(MsgError::BadField(1))).transpose()?,
        })
    }
}

/// Initial flow credit (tasks §2.1 FlowInit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowInit {
    pub msg_credits: u64,  // 1
    pub byte_credits: u64, // 2
}
impl FlowInit {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Uint(self.msg_credits)),
            (Value::Uint(2), Value::Uint(self.byte_credits)),
        ])
    }
    pub fn from_value(v: &Value) -> Result<FlowInit, MsgError> {
        Ok(FlowInit { msg_credits: uint(v, 1)?, byte_credits: uint(v, 2)? })
    }
}

/// Metered usage (tasks §4 Complete key 4; same shape as trust ReceiptCore.metrics).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Usage {
    pub tokens_in: u64,  // 1
    pub tokens_out: u64, // 2
    pub ttft_ms: u64,    // 3
    pub wall_ms: u64,    // 4
}
impl Usage {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Uint(self.tokens_in)),
            (Value::Uint(2), Value::Uint(self.tokens_out)),
            (Value::Uint(3), Value::Uint(self.ttft_ms)),
            (Value::Uint(4), Value::Uint(self.wall_ms)),
        ])
    }
    pub fn from_value(v: &Value) -> Result<Usage, MsgError> {
        Ok(Usage {
            tokens_in: uint(v, 1)?,
            tokens_out: uint(v, 2)?,
            ttft_ms: uint(v, 3)?,
            wall_ms: uint(v, 4)?,
        })
    }
}

// --- messages --------------------------------------------------------------

/// OFFER (0x0200) — tasks §2.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub task: TaskId,
    pub capability: String,
    pub constraints: TaskConstraints,
    pub pay_ref: Option<Value>,  // 4 (omitted this revision)
    pub expiry_ms: Option<u64>,  // 5
}
impl Offer {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.task.0.to_vec())),
            (Value::Uint(2), Value::Text(self.capability.clone())),
            (Value::Uint(3), self.constraints.to_value()),
        ];
        if let Some(p) = &self.pay_ref { m.push((Value::Uint(4), p.clone())); }
        if let Some(e) = self.expiry_ms { m.push((Value::Uint(5), Value::Uint(e))); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<Offer, MsgError> {
        Ok(Offer {
            task: task_at(v, 1)?,
            capability: text(v, 2)?,
            constraints: TaskConstraints::from_value(v.map_get(3).ok_or(MsgError::MissingField(3))?)?,
            pay_ref: v.map_get(4).cloned(),
            expiry_ms: opt_uint(v, 5)?,
        })
    }
}

/// QUOTE (0x0201) — tasks §2. Key 2 (PayQuoteRef) is omitted here (ERRATA).
#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub task: TaskId,
    pub pay_quote_ref: Option<Value>, // 2 (draft-required; omitted — ERRATA)
    pub ttft_ms: u64,                 // 3 guaranteed ttft
    pub expiry_ms: u64,               // 4
    pub ext: Option<Value>,           // 5
}
impl Quote {
    pub fn to_value(&self) -> Value {
        let mut m = vec![(Value::Uint(1), Value::Bytes(self.task.0.to_vec()))];
        if let Some(p) = &self.pay_quote_ref { m.push((Value::Uint(2), p.clone())); }
        m.push((Value::Uint(3), Value::Uint(self.ttft_ms)));
        m.push((Value::Uint(4), Value::Uint(self.expiry_ms)));
        if let Some(e) = &self.ext { m.push((Value::Uint(5), e.clone())); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<Quote, MsgError> {
        Ok(Quote {
            task: task_at(v, 1)?,
            pay_quote_ref: v.map_get(2).cloned(),
            ttft_ms: uint(v, 3)?,
            expiry_ms: uint(v, 4)?,
            ext: v.map_get(5).cloned(),
        })
    }
}

/// ASSIGN (0x0202) — tasks §2.1.
#[derive(Clone, Debug, PartialEq)]
pub struct Assign {
    pub task: TaskId,
    pub capability: String,
    pub constraints: TaskConstraints,
    pub input: InputSpec,
    pub flow: FlowInit,
    pub pay_agree_ref: Option<Value>, // 6 (omitted this revision)
    pub ext: Option<Value>,           // 7
}
impl Assign {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.task.0.to_vec())),
            (Value::Uint(2), Value::Text(self.capability.clone())),
            (Value::Uint(3), self.constraints.to_value()),
            (Value::Uint(4), self.input.to_value()),
            (Value::Uint(5), self.flow.to_value()),
        ];
        if let Some(p) = &self.pay_agree_ref { m.push((Value::Uint(6), p.clone())); }
        if let Some(e) = &self.ext { m.push((Value::Uint(7), e.clone())); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<Assign, MsgError> {
        Ok(Assign {
            task: task_at(v, 1)?,
            capability: text(v, 2)?,
            constraints: TaskConstraints::from_value(v.map_get(3).ok_or(MsgError::MissingField(3))?)?,
            input: InputSpec::from_value(v.map_get(4).ok_or(MsgError::MissingField(4))?)?,
            flow: FlowInit::from_value(v.map_get(5).ok_or(MsgError::MissingField(5))?)?,
            pay_agree_ref: v.map_get(6).cloned(),
            ext: v.map_get(7).cloned(),
        })
    }
}

/// ACCEPT (0x0203) — empty content map (tasks §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accept;
impl Accept {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![])
    }
    pub fn from_value(_v: &Value) -> Result<Accept, MsgError> {
        Ok(Accept)
    }
}

/// REJECT (0x0204) — `{1 => reason-code}` (tasks §2; field number assigned here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reject {
    pub code: u16,
}
impl Reject {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![(Value::Uint(1), Value::Uint(self.code as u64))])
    }
    pub fn from_value(v: &Value) -> Result<Reject, MsgError> {
        Ok(Reject { code: u16_at(v, 1)? })
    }
}

/// CHUNK (0x0205) — tasks §3. Distinct from the transcript `Chunk`: this wire
/// message carries the TaskID and `fin`; the transcript leaf uses only
/// (seq, kind, content).
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkMsg {
    pub task: TaskId,
    pub seq: u64,
    pub kind: ChunkKind,
    pub content: Vec<u8>,
    pub fin: bool,
}
impl ChunkMsg {
    /// The transcript leaf view (seq, kind, content) for `TranscriptBuilder`.
    pub fn transcript_chunk(&self) -> crate::Chunk {
        crate::Chunk { seq: self.seq, kind: self.kind, content: self.content.clone() }
    }
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.task.0.to_vec())),
            (Value::Uint(2), Value::Uint(self.seq)),
            (Value::Uint(3), Value::Uint(self.kind as u64)),
            (Value::Uint(4), Value::Bytes(self.content.clone())),
        ];
        if self.fin { m.push((Value::Uint(5), Value::Bool(true))); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<ChunkMsg, MsgError> {
        let kind = ChunkKind::from_u64(uint(v, 3)?).ok_or(MsgError::BadField(3))?;
        Ok(ChunkMsg {
            task: task_at(v, 1)?,
            seq: uint(v, 2)?,
            kind,
            content: bytes(v, 4)?.to_vec(),
            fin: matches!(v.map_get(5), Some(Value::Bool(true))),
        })
    }
}

/// COMPLETE (0x020D) — tasks §4. Key 3 is `multihash-sha256(transcript-root)`;
/// the struct holds the raw 32-byte root (prefix stripped on decode).
#[derive(Clone, Debug, PartialEq)]
pub struct Complete {
    pub task: TaskId,
    pub outcome: u8, // 0 ok, 1 partial, 2 failed
    pub transcript_root: [u8; 32],
    pub usage: Usage,
    pub ext: Option<Value>,
}
impl Complete {
    pub fn to_value(&self) -> Value {
        let mut mh = Vec::with_capacity(34);
        mh.extend_from_slice(&SHA256_MULTIHASH_PREFIX);
        mh.extend_from_slice(&self.transcript_root);
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.task.0.to_vec())),
            (Value::Uint(2), Value::Uint(self.outcome as u64)),
            (Value::Uint(3), Value::Bytes(mh)),
            (Value::Uint(4), self.usage.to_value()),
        ];
        if let Some(e) = &self.ext { m.push((Value::Uint(5), e.clone())); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<Complete, MsgError> {
        let mh = bytes(v, 3)?;
        if mh.len() != 34 || mh[..2] != SHA256_MULTIHASH_PREFIX {
            return Err(MsgError::BadField(3));
        }
        let mut root = [0u8; 32];
        root.copy_from_slice(&mh[2..]);
        Ok(Complete {
            task: task_at(v, 1)?,
            outcome: u8::try_from(uint(v, 2)?).map_err(|_| MsgError::BadField(2))?,
            transcript_root: root,
            usage: Usage::from_value(v.map_get(4).ok_or(MsgError::MissingField(4))?)?,
            ext: v.map_get(5).cloned(),
        })
    }
}

/// CANCEL (0x020C) — `{1 => TaskID, 2 => reason-code}` (tasks §11; numbers here).
#[derive(Clone, Debug, PartialEq)]
pub struct Cancel {
    pub task: TaskId,
    pub code: u16,
}
impl Cancel {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Bytes(self.task.0.to_vec())),
            (Value::Uint(2), Value::Uint(self.code as u64)),
        ])
    }
    pub fn from_value(v: &Value) -> Result<Cancel, MsgError> {
        Ok(Cancel { task: task_at(v, 1)?, code: u16_at(v, 2)? })
    }
}

/// FLOW_CREDIT (0x0007) — core §8.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowCredit {
    pub scope: [u8; 32], // 1 (TaskID/SessionID)
    pub msg_credits: u64,  // 2
    pub byte_credits: u64, // 3
}
impl FlowCredit {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Bytes(self.scope.to_vec())),
            (Value::Uint(2), Value::Uint(self.msg_credits)),
            (Value::Uint(3), Value::Uint(self.byte_credits)),
        ])
    }
    pub fn from_value(v: &Value) -> Result<FlowCredit, MsgError> {
        let s = bytes(v, 1)?;
        let scope = <[u8; 32]>::try_from(s).map_err(|_| MsgError::BadField(1))?;
        Ok(FlowCredit { scope, msg_credits: uint(v, 2)?, byte_credits: uint(v, 3)? })
    }
}

// --- reservations (tasks §6, TASK-011) -------------------------------------

fn id16(v: &Value, k: u64) -> Result<[u8; 16], MsgError> {
    <[u8; 16]>::try_from(bytes(v, k)?).map_err(|_| MsgError::BadField(k))
}

/// RESERVE (0x020F) — tasks §6. A lease request for a scarce capability during
/// a time window. `pay_ref` (key 6) is a PayRef modeled opaquely (pay §3.1
/// owns it); `constraints` reuses the task-negotiation shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Reserve {
    pub rsv_id: [u8; 16],                     // 1
    pub capability: String,                   // 2
    pub from_ms: u64,                         // 3
    pub until_ms: u64,                        // 4
    pub constraints: Option<TaskConstraints>, // 5
    pub pay_ref: Option<Value>,               // 6 (PayRef, opaque)
    pub ext: Option<Value>,                   // 7
}
impl Reserve {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.rsv_id.to_vec())),
            (Value::Uint(2), Value::Text(self.capability.clone())),
            (Value::Uint(3), Value::Uint(self.from_ms)),
            (Value::Uint(4), Value::Uint(self.until_ms)),
        ];
        if let Some(c) = &self.constraints { m.push((Value::Uint(5), c.to_value())); }
        if let Some(p) = &self.pay_ref { m.push((Value::Uint(6), p.clone())); }
        if let Some(e) = &self.ext { m.push((Value::Uint(7), e.clone())); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<Reserve, MsgError> {
        Ok(Reserve {
            rsv_id: id16(v, 1)?,
            capability: text(v, 2)?,
            from_ms: uint(v, 3)?,
            until_ms: uint(v, 4)?,
            constraints: v.map_get(5).map(TaskConstraints::from_value).transpose()?,
            pay_ref: v.map_get(6).cloned(),
            ext: v.map_get(7).cloned(),
        })
    }
}

/// LEASE (0x0210) — tasks §6. `status`: 0 granted / 1 rejected / 2 revoked.
/// `window` (key 3) is the actually-granted `[from, until]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lease {
    pub rsv_id: [u8; 16],             // 1
    pub status: u8,                   // 2
    pub window: Option<(u64, u64)>,   // 3 [from, until]
    pub pay_quote_ref: Option<Value>, // 4 (PayQuoteRef, opaque)
    pub ext: Option<Value>,           // 5
}
impl Lease {
    pub const GRANTED: u8 = 0;
    pub const REJECTED: u8 = 1;
    pub const REVOKED: u8 = 2;

    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.rsv_id.to_vec())),
            (Value::Uint(2), Value::Uint(self.status as u64)),
        ];
        if let Some((a, b)) = self.window {
            m.push((Value::Uint(3), Value::Array(vec![Value::Uint(a), Value::Uint(b)])));
        }
        if let Some(p) = &self.pay_quote_ref { m.push((Value::Uint(4), p.clone())); }
        if let Some(e) = &self.ext { m.push((Value::Uint(5), e.clone())); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<Lease, MsgError> {
        let window = match v.map_get(3) {
            None => None,
            Some(x) => {
                let a = x.as_array().ok_or(MsgError::BadField(3))?;
                if a.len() != 2 {
                    return Err(MsgError::BadField(3));
                }
                Some((
                    a[0].as_uint().ok_or(MsgError::BadField(3))?,
                    a[1].as_uint().ok_or(MsgError::BadField(3))?,
                ))
            }
        };
        Ok(Lease {
            rsv_id: id16(v, 1)?,
            status: u8::try_from(uint(v, 2)?).map_err(|_| MsgError::BadField(2))?,
            window,
            pay_quote_ref: v.map_get(4).cloned(),
            ext: v.map_get(5).cloned(),
        })
    }
    /// tasks §6: a GRANTED lease's window MUST fall within the reservation's
    /// requested `[req_from, req_until]`; a grant outside it is invalid.
    pub fn granted_window_within(&self, req_from: u64, req_until: u64) -> bool {
        match self.window {
            None => true,
            Some((a, b)) => a <= b && a >= req_from && b <= req_until,
        }
    }
}

/// LEASE_RELEASE (0x0211) — tasks §6. Voluntary early release of a lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaseRelease {
    pub rsv_id: [u8; 16],    // 1
    pub reason: Option<u64>, // 2
}
impl LeaseRelease {
    pub fn to_value(&self) -> Value {
        let mut m = vec![(Value::Uint(1), Value::Bytes(self.rsv_id.to_vec()))];
        if let Some(r) = self.reason { m.push((Value::Uint(2), Value::Uint(r))); }
        Value::Map(m)
    }
    pub fn from_value(v: &Value) -> Result<LeaseRelease, MsgError> {
        Ok(LeaseRelease { rsv_id: id16(v, 1)?, reason: opt_uint(v, 2)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> TaskId {
        TaskId([7u8; 32])
    }

    #[test]
    fn roundtrip_all_messages_including_optional_absence() {
        let constraints = TaskConstraints { ttft_ms: Some(500), ..Default::default() };
        let offer = Offer { task: task(), capability: "map:cap:x".into(), constraints: constraints.clone(), pay_ref: None, expiry_ms: Some(1000) };
        assert_eq!(Offer::from_value(&offer.to_value()).unwrap(), offer);

        let quote = Quote { task: task(), pay_quote_ref: None, ttft_ms: 400, expiry_ms: 2000, ext: None };
        assert_eq!(Quote::from_value(&quote.to_value()).unwrap(), quote);

        let assign = Assign {
            task: task(), capability: "map:cap:x".into(), constraints,
            input: InputSpec::inline(b"hi".to_vec()),
            flow: FlowInit { msg_credits: 64, byte_credits: 1 << 20 },
            pay_agree_ref: None, ext: None,
        };
        assert_eq!(Assign::from_value(&assign.to_value()).unwrap(), assign);

        assert_eq!(Accept::from_value(&Accept.to_value()).unwrap(), Accept);
        assert_eq!(Reject::from_value(&Reject { code: 3000 }.to_value()).unwrap(), Reject { code: 3000 });

        let chunk = ChunkMsg { task: task(), seq: 3, kind: ChunkKind::OutputText, content: b"out".to_vec(), fin: true };
        assert_eq!(ChunkMsg::from_value(&chunk.to_value()).unwrap(), chunk);
        let chunk0 = ChunkMsg { task: task(), seq: 0, kind: ChunkKind::UsageUpdate, content: vec![], fin: false };
        assert_eq!(ChunkMsg::from_value(&chunk0.to_value()).unwrap(), chunk0);

        let complete = Complete { task: task(), outcome: 0, transcript_root: [9u8; 32], usage: Usage { tokens_in: 1, tokens_out: 2, ttft_ms: 3, wall_ms: 4 }, ext: None };
        assert_eq!(Complete::from_value(&complete.to_value()).unwrap(), complete);

        assert_eq!(Cancel::from_value(&Cancel { task: task(), code: 5001 }.to_value()).unwrap(), Cancel { task: task(), code: 5001 });
        let fc = FlowCredit { scope: [7u8; 32], msg_credits: 64, byte_credits: 1024 };
        assert_eq!(FlowCredit::from_value(&fc.to_value()).unwrap(), fc);
    }

    #[test]
    fn reservation_codecs_roundtrip_and_window_check() {
        let reserve = Reserve {
            rsv_id: [0x20; 16],
            capability: "map:cap:schedule/reservation".into(),
            from_ms: 1_780_000_000_000,
            until_ms: 1_780_000_600_000,
            constraints: Some(TaskConstraints { zone: Some(1), ..Default::default() }),
            pay_ref: None,
            ext: None,
        };
        assert_eq!(Reserve::from_value(&reserve.to_value()).unwrap(), reserve);

        let lease = Lease {
            rsv_id: [0x20; 16],
            status: Lease::GRANTED,
            window: Some((1_780_000_100_000, 1_780_000_500_000)),
            pay_quote_ref: None,
            ext: None,
        };
        assert_eq!(Lease::from_value(&lease.to_value()).unwrap(), lease);
        // Granted window inside the requested [from, until] is valid; outside is not.
        assert!(lease.granted_window_within(reserve.from_ms, reserve.until_ms));
        let bad = Lease { window: Some((reserve.until_ms + 1, reserve.until_ms + 10)), ..lease.clone() };
        assert!(!bad.granted_window_within(reserve.from_ms, reserve.until_ms));

        let rel = LeaseRelease { rsv_id: [0x20; 16], reason: Some(2) };
        assert_eq!(LeaseRelease::from_value(&rel.to_value()).unwrap(), rel);
    }

    #[test]
    fn complete_rejects_non_multihash_root() {
        let mut v = Complete { task: task(), outcome: 0, transcript_root: [0u8; 32], usage: Usage::default(), ext: None }.to_value();
        // Corrupt key 3 to a bare 32-byte string (no multihash prefix).
        if let Value::Map(m) = &mut v {
            for (k, val) in m.iter_mut() {
                if matches!(k, Value::Uint(3)) { *val = Value::Bytes(vec![0u8; 32]); }
            }
        }
        assert_eq!(Complete::from_value(&v), Err(MsgError::BadField(3)));
    }
}
