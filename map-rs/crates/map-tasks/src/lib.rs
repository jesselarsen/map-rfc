//! MAP tasks bounded context (draft-map-tasks-00).
//!
//! Data ownership (tasks §1): requester owns terms, provider owns execution
//! state, the countersigned transcript commitment is the only shared fact.
//! TaskID is the idempotency key; `chunk-seq` is the per-task ordering
//! scope. This crate implements the pure/deterministic pieces: transcript
//! commitment (§8.1 as patched), flow-credit accounting (core §8.1
//! semantics), and the requester state machine (§13).

use map_core::cbor::{encode, Value};
use sha2::{Digest, Sha256};
use std::fmt;

pub mod msg;

/// 32-byte requester-chosen idempotency key (tasks §1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TaskId(pub [u8; 32]);

impl TaskId {
    pub fn random() -> Self {
        let mut b = [0u8; 32];
        getrandom::getrandom(&mut b).expect("OS RNG unavailable");
        TaskId(b)
    }
    pub fn from_slice(s: &[u8]) -> Option<Self> {
        s.try_into().ok().map(TaskId)
    }
}

/// Chunk kinds (tasks §3). Kinds 0/1/4 are transcript-bearing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChunkKind {
    OutputText = 0,
    OutputBytes = 1,
    Log = 2,
    UsageUpdate = 3,
    ToolIo = 4,
}

impl ChunkKind {
    pub fn from_u64(v: u64) -> Option<Self> {
        Some(match v {
            0 => ChunkKind::OutputText,
            1 => ChunkKind::OutputBytes,
            2 => ChunkKind::Log,
            3 => ChunkKind::UsageUpdate,
            4 => ChunkKind::ToolIo,
            _ => return None,
        })
    }
    pub fn in_transcript(self) -> bool {
        matches!(
            self,
            ChunkKind::OutputText | ChunkKind::OutputBytes | ChunkKind::ToolIo
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub seq: u64,
    pub kind: ChunkKind,
    pub content: Vec<u8>,
}

// ---------------------------------------------------------------------
// Transcript commitment (tasks §8.1, as fully specified after the
// implementation pass):
//   leaf(c)      = H(0x00 || enc([seq, kind, content]))
//   parent(l, r) = H(0x01 || l || r)
//   odd node promoted unchanged; empty root = H(0x00 || enc([])).
// ---------------------------------------------------------------------

pub fn transcript_leaf(c: &Chunk) -> [u8; 32] {
    let enc = encode(&Value::Array(vec![
        Value::Uint(c.seq),
        Value::Uint(c.kind as u64),
        Value::Bytes(c.content.clone()),
    ]))
    .expect("array of primitives cannot fail to encode");
    let mut h = Sha256::new();
    h.update([0u8]);
    h.update(&enc);
    h.finalize().into()
}

fn parent(l: &[u8; 32], r: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([1u8]);
    h.update(l);
    h.update(r);
    h.finalize().into()
}

pub fn empty_transcript_root() -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0u8]);
    h.update(encode(&Value::Array(vec![])).expect("empty array encodes"));
    h.finalize().into()
}

/// Streaming-friendly builder shared by checkpoints, COMPLETE, receipts,
/// and disputes — one commitment structure everywhere.
#[derive(Default)]
pub struct TranscriptBuilder {
    leaves: Vec<[u8; 32]>,
}

impl TranscriptBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Non-transcript kinds (log, usage) are accepted and ignored, so
    /// callers can feed every chunk through.
    pub fn push(&mut self, c: &Chunk) {
        if c.kind.in_transcript() {
            self.leaves.push(transcript_leaf(c));
        }
    }

    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    pub fn root(&self) -> [u8; 32] {
        if self.leaves.is_empty() {
            return empty_transcript_root();
        }
        let mut level = self.leaves.clone();
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut it = level.chunks_exact(2);
            for pair in &mut it {
                next.push(parent(&pair[0], &pair[1]));
            }
            if let [odd] = it.remainder() {
                next.push(*odd); // promoted unchanged
            }
            level = next;
        }
        level[0]
    }
}

// ---------------------------------------------------------------------
// Flow credit (core §8.1): senders must not exceed outstanding credit in
// either unit; the receiver-side account rejects violations (error 1005).
// ---------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum FlowError {
    NoMessageCredit,
    NoByteCredit,
}

#[derive(Default, Debug)]
pub struct FlowAccount {
    msg_credits: u64,
    byte_credits: u64,
}

impl FlowAccount {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn grant(&mut self, msgs: u64, bytes: u64) {
        self.msg_credits = self.msg_credits.saturating_add(msgs);
        self.byte_credits = self.byte_credits.saturating_add(bytes);
    }
    pub fn try_consume(&mut self, content_len: usize) -> Result<(), FlowError> {
        let len = content_len as u64;
        if self.msg_credits == 0 {
            return Err(FlowError::NoMessageCredit);
        }
        if self.byte_credits < len {
            return Err(FlowError::NoByteCredit);
        }
        self.msg_credits -= 1;
        self.byte_credits -= len;
        Ok(())
    }
    pub fn remaining(&self) -> (u64, u64) {
        (self.msg_credits, self.byte_credits)
    }
}

// ---------------------------------------------------------------------
// Requester-side task state machine (tasks §13). One refinement over the
// spec diagram: the CREATED -OFFER→ QUOTED edge is split with an explicit
// OfferSent state so send and receive are distinct transitions.
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReqState {
    Created,
    OfferSent,
    Quoted,
    Pending,
    Active,
    Suspended,
    Delivered,
    Settling,
    Closed,
    Cancelled,
    Failed,
    Disputed,
}

impl ReqState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ReqState::Closed | ReqState::Cancelled | ReqState::Failed | ReqState::Disputed
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ReqEvent {
    SendOffer,
    RecvQuote,
    SendAssign,
    RecvAccept,
    RecvReject,
    RecvChunk,
    CheckpointAcked,
    ProviderLost,
    Resume,
    Cancel,
    RecvCompleteOk,
    TranscriptVerified,
    TranscriptMismatch,
    Settled,
    Fatal,
}

#[derive(Debug, PartialEq)]
pub struct InvalidTransition {
    pub from: ReqState,
}

impl fmt::Display for InvalidTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid transition from {:?}", self.from)
    }
}
impl std::error::Error for InvalidTransition {}

pub fn step(s: ReqState, e: ReqEvent) -> Result<ReqState, InvalidTransition> {
    use ReqEvent as E;
    use ReqState as S;
    if s.is_terminal() {
        return Err(InvalidTransition { from: s });
    }
    Ok(match (s, e) {
        (S::Created, E::SendOffer) => S::OfferSent,
        // Direct ASSIGN under a standing agreement (tasks §2).
        (S::Created, E::SendAssign) => S::Pending,
        (S::OfferSent, E::RecvQuote) => S::Quoted,
        (S::Quoted, E::SendAssign) => S::Pending,
        (S::Pending, E::RecvAccept) => S::Active,
        (S::Pending, E::RecvReject) => S::Failed,
        (S::Active, E::RecvChunk) => S::Active,
        (S::Active, E::CheckpointAcked) => S::Active,
        (S::Active, E::ProviderLost) => S::Suspended,
        (S::Suspended, E::Resume) => S::Active,
        (S::Active | S::Suspended, E::Cancel) => S::Cancelled,
        (S::Active, E::RecvCompleteOk) => S::Delivered,
        (S::Delivered, E::TranscriptVerified) => S::Settling,
        (S::Delivered, E::TranscriptMismatch) => S::Disputed,
        (S::Settling, E::Settled) => S::Closed,
        (_, E::Fatal) => S::Failed,
        _ => return Err(InvalidTransition { from: s }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(seq: u64, kind: ChunkKind, content: &[u8]) -> Chunk {
        Chunk {
            seq,
            kind,
            content: content.to_vec(),
        }
    }

    #[test]
    fn transcript_root_is_order_and_content_sensitive() {
        let mut a = TranscriptBuilder::new();
        a.push(&chunk(0, ChunkKind::OutputText, b"hello "));
        a.push(&chunk(1, ChunkKind::Log, b"ignored"));
        a.push(&chunk(2, ChunkKind::OutputText, b"world"));
        assert_eq!(a.leaf_count(), 2);

        let mut b = TranscriptBuilder::new();
        b.push(&chunk(0, ChunkKind::OutputText, b"hello "));
        b.push(&chunk(2, ChunkKind::OutputText, b"worlD"));
        assert_ne!(a.root(), b.root());

        // Ambiguity guard: the CBOR leaf encoding must separate fields that
        // bare concatenation would merge.
        let mut c1 = TranscriptBuilder::new();
        c1.push(&chunk(0x01, ChunkKind::OutputBytes, &[0x02, 0x03]));
        let mut c2 = TranscriptBuilder::new();
        c2.push(&chunk(0x0102, ChunkKind::OutputBytes, &[0x03]));
        assert_ne!(c1.root(), c2.root());
    }

    #[test]
    fn merkle_shapes() {
        // Empty and single-leaf roots are defined.
        assert_eq!(TranscriptBuilder::new().root(), empty_transcript_root());
        let l0 = chunk(0, ChunkKind::OutputText, b"a");
        let mut single = TranscriptBuilder::new();
        single.push(&l0);
        assert_eq!(single.root(), transcript_leaf(&l0));

        // Three leaves: root = parent(parent(l0,l1), l2-promoted).
        let l1 = chunk(1, ChunkKind::OutputText, b"b");
        let l2 = chunk(2, ChunkKind::OutputText, b"c");
        let mut t = TranscriptBuilder::new();
        t.push(&l0);
        t.push(&l1);
        t.push(&l2);
        let expect = parent(
            &parent(&transcript_leaf(&l0), &transcript_leaf(&l1)),
            &transcript_leaf(&l2),
        );
        assert_eq!(t.root(), expect);
    }

    #[test]
    fn flow_credit_enforced_in_both_units() {
        let mut acct = FlowAccount::new();
        assert_eq!(acct.try_consume(1), Err(FlowError::NoMessageCredit));
        acct.grant(2, 10);
        assert!(acct.try_consume(6).is_ok());
        assert_eq!(acct.try_consume(6), Err(FlowError::NoByteCredit));
        assert!(acct.try_consume(4).is_ok());
        assert_eq!(acct.try_consume(0), Err(FlowError::NoMessageCredit));
    }

    #[test]
    fn state_machine_golden_path_and_guards() {
        use ReqEvent as E;
        use ReqState as S;
        let mut s = S::Created;
        for e in [
            E::SendOffer,
            E::RecvQuote,
            E::SendAssign,
            E::RecvAccept,
            E::RecvChunk,
            E::ProviderLost,
            E::Resume,
            E::RecvCompleteOk,
            E::TranscriptVerified,
            E::Settled,
        ] {
            s = step(s, e).unwrap();
        }
        assert_eq!(s, S::Closed);
        // Terminal states admit nothing, including Fatal.
        assert!(step(S::Closed, E::Fatal).is_err());
        // Idempotent re-ASSIGN safety is a TaskID property, not a state
        // transition: sending ASSIGN twice from Pending is invalid here.
        assert!(step(S::Pending, E::SendAssign).is_err());
        // Root mismatch routes to dispute, never to settlement.
        assert_eq!(step(S::Delivered, E::TranscriptMismatch).unwrap(), S::Disputed);
    }
}
