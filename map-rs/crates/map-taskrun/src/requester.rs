//! Requester runtime: drive the `map_tasks::ReqState`/`step` FSM, send OFFER
//! (or direct ASSIGN), evaluate QUOTE, stream in CHUNKs (feeding its own
//! transcript, consuming credit, re-granting via FLOW_CREDIT), enforce the
//! TTFT ×1.1 rule, and verify `root == own transcript root` on COMPLETE.

use crate::{Outgoing, ReceiptInputs};
use map_core::msg_type;
use map_session::Incoming;
use map_tasks::msg::{
    Assign, Cancel, ChunkMsg, Complete, FlowCredit, FlowInit, InputSpec, Offer, Quote, Reject,
    TaskConstraints,
};
use map_tasks::{step, ReqEvent, ReqState, TaskId};
use map_tasks::TranscriptBuilder;

/// Credit re-granted per FLOW_CREDIT and as the initial FlowInit (§8).
const GRANT_MSGS: u64 = 64;
const GRANT_BYTES: u64 = 1 << 20; // 1 MiB
/// TTFT slack: cancel if the first chunk misses `quoted × 1.1` (tasks §2).
const TTFT_SLACK_NUM: u64 = 11;
const TTFT_SLACK_DEN: u64 = 10;

/// Transcript-root mismatch (tasks §4). Note: this package routes mismatch to
/// `Failed` (via `Fatal`) per FR-3; the purpose-built `TranscriptMismatch` →
/// `Disputed` FSM edge is intentionally NOT used yet — whoever builds the
/// dispute path (after TASK-008) should rewire this to `Disputed`.
const ERR_TRANSCRIPT_MISMATCH: u16 = 5000;
const ERR_TTFT_MISSED: u16 = 5001;
const ERR_SEQ_GAP: u16 = 1003;

pub struct RequesterTask {
    state: ReqState,
    task: TaskId,
    capability: String,
    constraints: TaskConstraints,
    input: Vec<u8>,
    transcript: TranscriptBuilder,
    expected_seq: u64,
    granted_msgs: u64,
    granted_bytes: u64,
    used_msgs: u64,
    used_bytes: u64,
    quoted_ttft_ms: Option<u64>,
    ttft_deadline_ms: Option<u64>,
    first_chunk_seen: bool,
    started_ms: u64,
    ended_ms: u64,
    verified: Option<(ReceiptInputs, [u8; 32])>,
    error_code: Option<u16>,
    chunks_received: u64,
    credits_granted: u64,
}

impl RequesterTask {
    pub fn new(task: TaskId, capability: impl Into<String>, constraints: TaskConstraints, input: Vec<u8>) -> Self {
        RequesterTask {
            state: ReqState::Created,
            task,
            capability: capability.into(),
            constraints,
            input,
            transcript: TranscriptBuilder::new(),
            expected_seq: 0,
            granted_msgs: 0,
            granted_bytes: 0,
            used_msgs: 0,
            used_bytes: 0,
            quoted_ttft_ms: None,
            ttft_deadline_ms: None,
            first_chunk_seen: false,
            started_ms: 0,
            ended_ms: 0,
            verified: None,
            error_code: None,
            chunks_received: 0,
            credits_granted: 0,
        }
    }

    // ---- accessors --------------------------------------------------------

    pub fn state(&self) -> ReqState {
        self.state
    }
    /// The verified receipt inputs + transcript root, once the task verifies.
    pub fn verified(&self) -> Option<&(ReceiptInputs, [u8; 32])> {
        self.verified.as_ref()
    }
    pub fn error_code(&self) -> Option<u16> {
        self.error_code
    }
    pub fn chunks_received(&self) -> u64 {
        self.chunks_received
    }
    pub fn credits_granted(&self) -> u64 {
        self.credits_granted
    }

    // ---- entry points -----------------------------------------------------

    /// Begin the negotiated flow: send OFFER (Created → OfferSent).
    pub fn start_offer(&mut self, now_ms: u64) -> Vec<Outgoing> {
        self.started_ms = now_ms;
        if !self.try_step(ReqEvent::SendOffer) {
            return Vec::new();
        }
        let offer = Offer {
            task: self.task,
            capability: self.capability.clone(),
            constraints: self.constraints.clone(),
            pay_ref: None,
            expiry_ms: None,
        };
        vec![(msg_type::OFFER, offer.to_value())]
    }

    /// Direct-ASSIGN under a standing agreement (tasks §2; FR-5): skip OFFER/
    /// QUOTE, go straight to ASSIGN (Created → Pending). No quoted TTFT, so the
    /// ×1.1 deadline is not armed on this path.
    pub fn start_direct(&mut self, now_ms: u64) -> Vec<Outgoing> {
        self.started_ms = now_ms;
        if !self.try_step(ReqEvent::SendAssign) {
            return Vec::new();
        }
        vec![(msg_type::ASSIGN, self.assign_msg().to_value())]
    }

    pub fn poll_deadline(&mut self, now_ms: u64) -> Vec<Outgoing> {
        if self.state == ReqState::Active && !self.first_chunk_seen {
            if let Some(dl) = self.ttft_deadline_ms {
                if now_ms > dl {
                    self.error_code = Some(ERR_TTFT_MISSED);
                    self.try_step(ReqEvent::Cancel); // Active → Cancelled
                    return vec![(msg_type::CANCEL, Cancel { task: self.task, code: ERR_TTFT_MISSED }.to_value())];
                }
            }
        }
        Vec::new()
    }

    pub fn on_incoming(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        if self.is_done() {
            return Vec::new(); // idempotent terminals + verified
        }
        match inc.msg_type {
            t if t == msg_type::QUOTE => self.on_quote(inc, now_ms),
            t if t == msg_type::ACCEPT => self.on_accept(now_ms),
            t if t == msg_type::REJECT => self.on_reject(inc),
            t if t == msg_type::CHUNK => self.on_chunk(inc),
            t if t == msg_type::COMPLETE => self.on_complete(inc, now_ms),
            t if t == msg_type::CANCEL => {
                self.try_step(ReqEvent::Cancel);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    // ---- handlers ---------------------------------------------------------

    fn on_quote(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let quote = match Quote::from_value(&inc.content) {
            Ok(q) => q,
            Err(_) => return self.fail(None),
        };
        // Expired quote → local reject, no ASSIGN (§9).
        if now_ms > quote.expiry_ms {
            return self.fail(Some(6000)); // QUOTE_EXPIRED
        }
        // Accept iff the quoted TTFT meets the constraint (if one was set).
        if let Some(max) = self.constraints.ttft_ms {
            if quote.ttft_ms > max {
                return self.fail(None); // local policy reject
            }
        }
        self.quoted_ttft_ms = Some(quote.ttft_ms);
        if !self.try_step(ReqEvent::RecvQuote) {
            return Vec::new(); // OfferSent → Quoted
        }
        if !self.try_step(ReqEvent::SendAssign) {
            return Vec::new(); // Quoted → Pending
        }
        vec![(msg_type::ASSIGN, self.assign_msg().to_value())]
    }

    fn on_accept(&mut self, now_ms: u64) -> Vec<Outgoing> {
        if !self.try_step(ReqEvent::RecvAccept) {
            return Vec::new(); // Pending → Active
        }
        if let Some(ttft) = self.quoted_ttft_ms {
            self.ttft_deadline_ms = Some(now_ms + ttft * TTFT_SLACK_NUM / TTFT_SLACK_DEN);
        }
        Vec::new()
    }

    fn on_reject(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        self.error_code = Reject::from_value(&inc.content).ok().map(|r| r.code);
        self.try_step(ReqEvent::RecvReject); // Pending → Failed
        Vec::new()
    }

    fn on_chunk(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        if self.state != ReqState::Active {
            return self.fail(None);
        }
        let chunk = match ChunkMsg::from_value(&inc.content) {
            Ok(c) => c,
            Err(_) => return self.fail(None),
        };
        // chunk-seq density is a CONTENT-level check (field 2), separate from
        // the envelope seq map-session already enforces (Body key 7).
        if chunk.seq != self.expected_seq {
            return self.fail(Some(ERR_SEQ_GAP));
        }
        self.expected_seq += 1;
        self.first_chunk_seen = true;
        self.chunks_received += 1;
        self.used_msgs += 1;
        self.used_bytes += chunk.content.len() as u64;
        if chunk.kind.in_transcript() {
            self.transcript.push(&chunk.transcript_chunk());
        }
        if !self.try_step(ReqEvent::RecvChunk) {
            return Vec::new(); // Active → Active
        }
        self.maybe_regrant()
    }

    fn on_complete(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let complete = match Complete::from_value(&inc.content) {
            Ok(c) => c,
            Err(_) => return self.fail(None),
        };
        self.ended_ms = now_ms;
        if !self.try_step(ReqEvent::RecvCompleteOk) {
            return Vec::new(); // Active → Delivered
        }
        // complete.transcript_root is the raw 32-byte root (multihash prefix
        // stripped by the codec); compare against the bare TranscriptBuilder root.
        if complete.transcript_root == self.transcript.root() {
            self.try_step(ReqEvent::TranscriptVerified); // Delivered → Settling
            self.verified = Some((
                ReceiptInputs {
                    task: self.task,
                    capability: self.capability.clone(),
                    started_ms: self.started_ms,
                    ended_ms: self.ended_ms,
                    usage: complete.usage,
                    outcome: complete.outcome,
                },
                complete.transcript_root,
            ));
        } else {
            self.fail(Some(ERR_TRANSCRIPT_MISMATCH)); // Delivered → Failed (see note)
        }
        Vec::new()
    }

    // ---- internals --------------------------------------------------------

    fn assign_msg(&mut self) -> Assign {
        self.granted_msgs = GRANT_MSGS;
        self.granted_bytes = GRANT_BYTES;
        Assign {
            task: self.task,
            capability: self.capability.clone(),
            constraints: self.constraints.clone(),
            input: InputSpec::inline(self.input.clone()),
            flow: FlowInit { msg_credits: GRANT_MSGS, byte_credits: GRANT_BYTES },
            pay_agree_ref: None,
            ext: None,
        }
    }

    /// Grant another FlowInit-worth when either unit drops below 25% remaining.
    fn maybe_regrant(&mut self) -> Vec<Outgoing> {
        let rem_msgs = self.granted_msgs.saturating_sub(self.used_msgs);
        let rem_bytes = self.granted_bytes.saturating_sub(self.used_bytes);
        if rem_msgs * 4 < GRANT_MSGS || rem_bytes * 4 < GRANT_BYTES {
            self.granted_msgs += GRANT_MSGS;
            self.granted_bytes += GRANT_BYTES;
            self.credits_granted += 1;
            let fc = FlowCredit { scope: self.task.0, msg_credits: GRANT_MSGS, byte_credits: GRANT_BYTES };
            return vec![(msg_type::FLOW_CREDIT, fc.to_value())];
        }
        Vec::new()
    }

    fn try_step(&mut self, e: ReqEvent) -> bool {
        match step(self.state, e) {
            Ok(s) => {
                self.state = s;
                true
            }
            Err(_) => {
                self.state = ReqState::Failed;
                false
            }
        }
    }

    /// Move to Failed, recording an optional wire code. Returns no messages.
    fn fail(&mut self, code: Option<u16>) -> Vec<Outgoing> {
        if code.is_some() {
            self.error_code = code;
        }
        self.state = ReqState::Failed;
        Vec::new()
    }

    fn is_done(&self) -> bool {
        self.state.is_terminal() || self.verified.is_some()
    }
}
