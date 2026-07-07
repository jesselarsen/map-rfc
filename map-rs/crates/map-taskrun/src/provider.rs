//! Provider runtime: reply ACCEPT, stream CHUNKs under flow credit (parking as
//! `Starved` when out of credit rather than overrunning), build the transcript,
//! and send COMPLETE with the root + usage.

use crate::Outgoing;
use map_core::cbor::Value;
use map_core::msg_type;
use map_session::Incoming;
use map_tasks::msg::{Accept, Assign, ChunkMsg, Complete, FlowCredit, Offer, Quote, Usage};
use map_tasks::{ChunkKind, FlowAccount, TaskId, TranscriptBuilder};
use std::collections::VecDeque;

/// Default guaranteed TTFT this provider quotes (ms).
const DEFAULT_QUOTE_TTFT_MS: u64 = 1000;
/// Validity window a quote is offered for (ms).
const QUOTE_EXPIRY_WINDOW_MS: u64 = 60_000;

/// Produces a task's output chunks from its inline input (FR-2).
pub trait Executor {
    fn execute(&self, input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)>;
}

/// Deterministic echo (FR-6): one kind-0 output-text chunk echoing the input,
/// plus one kind-3 usage-update chunk (excluded from the transcript, kind 3 ∉
/// {0,1,4}). No clock, no randomness.
pub struct EchoExecutor;

impl Executor for EchoExecutor {
    fn execute(&self, input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)> {
        let usage = map_core::cbor::encode(&Value::Map(vec![
            (Value::Uint(1), Value::Uint(input.len() as u64)),
            (Value::Uint(2), Value::Uint(input.len() as u64)),
        ]))
        .expect("small usage map encodes");
        vec![
            (ChunkKind::OutputText, input.to_vec()),
            (ChunkKind::UsageUpdate, usage),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderState {
    AwaitAssign,
    Streaming,
    /// Out of credit mid-stream; parked until FLOW_CREDIT arrives.
    Starved,
    Completed,
    Cancelled,
    Failed,
}

pub struct ProviderTask<E: Executor> {
    executor: E,
    state: ProviderState,
    task: Option<TaskId>,
    flow: FlowAccount,
    transcript: TranscriptBuilder,
    pending: VecDeque<ChunkMsg>,
    quote_ttft_ms: u64,
    started_ms: u64,
    tokens_in: u64,
    tokens_out: u64,
    chunks_sent: u64,
}

impl<E: Executor> ProviderTask<E> {
    pub fn new(executor: E) -> Self {
        ProviderTask {
            executor,
            state: ProviderState::AwaitAssign,
            task: None,
            flow: FlowAccount::new(),
            transcript: TranscriptBuilder::new(),
            pending: VecDeque::new(),
            quote_ttft_ms: DEFAULT_QUOTE_TTFT_MS,
            started_ms: 0,
            tokens_in: 0,
            tokens_out: 0,
            chunks_sent: 0,
        }
    }

    pub fn state(&self) -> ProviderState {
        self.state
    }
    pub fn chunks_sent(&self) -> u64 {
        self.chunks_sent
    }

    /// The provider has no self-timed deadlines; present for API symmetry.
    pub fn poll_deadline(&mut self, _now_ms: u64) -> Vec<Outgoing> {
        Vec::new()
    }

    pub fn on_incoming(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        if self.is_terminal() {
            return Vec::new(); // idempotent terminals (§9/§10)
        }
        match inc.msg_type {
            t if t == msg_type::OFFER => self.on_offer(inc, now_ms),
            t if t == msg_type::ASSIGN => self.on_assign(inc, now_ms),
            t if t == msg_type::FLOW_CREDIT => self.on_flow_credit(inc, now_ms),
            t if t == msg_type::CANCEL => {
                // Terminal. COMPLETE(outcome=partial) is a settlement refinement
                // that only feeds pay §4 (TASK-009) — deferred here.
                self.state = ProviderState::Cancelled;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            ProviderState::Completed | ProviderState::Cancelled | ProviderState::Failed
        )
    }

    /// Answer an OFFER with a QUOTE (guaranteed TTFT + expiry). Stays
    /// `AwaitAssign` — the ASSIGN carries the full task.
    fn on_offer(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let offer = match Offer::from_value(&inc.content) {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };
        let quote = Quote {
            task: offer.task,
            pay_quote_ref: None,
            ttft_ms: self.quote_ttft_ms,
            expiry_ms: now_ms + QUOTE_EXPIRY_WINDOW_MS,
            ext: None,
        };
        vec![(msg_type::QUOTE, quote.to_value())]
    }

    fn on_assign(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        let assign = match Assign::from_value(&inc.content) {
            Ok(a) => a,
            Err(_) => {
                self.state = ProviderState::Failed;
                return Vec::new();
            }
        };
        self.task = Some(assign.task);
        self.started_ms = now_ms;
        let input = assign.input.inline.unwrap_or_default();
        self.tokens_in = input.len() as u64;
        self.flow.grant(assign.flow.msg_credits, assign.flow.byte_credits);

        // Queue the output chunks, dense seq from 0, fin on the last.
        let outputs = self.executor.execute(&input);
        let n = outputs.len();
        for (i, (kind, content)) in outputs.into_iter().enumerate() {
            self.pending.push_back(ChunkMsg {
                task: assign.task,
                seq: i as u64,
                kind,
                content,
                fin: i + 1 == n,
            });
        }
        self.state = ProviderState::Streaming;

        let mut out = vec![(msg_type::ACCEPT, Accept.to_value())];
        out.extend(self.drain(now_ms));
        out
    }

    fn on_flow_credit(&mut self, inc: &Incoming, now_ms: u64) -> Vec<Outgoing> {
        if let Ok(fc) = FlowCredit::from_value(&inc.content) {
            self.flow.grant(fc.msg_credits, fc.byte_credits);
        }
        self.drain(now_ms)
    }

    /// Send as many queued chunks as credit allows; park (`Starved`) when out,
    /// or emit COMPLETE once all are sent.
    fn drain(&mut self, now_ms: u64) -> Vec<Outgoing> {
        let mut out = Vec::new();
        while let Some(len) = self.pending.front().map(|c| c.content.len()) {
            // try_consume decrements credit only on success; MUST NOT overrun.
            if self.flow.try_consume(len).is_err() {
                self.state = ProviderState::Starved;
                return out;
            }
            let chunk = self.pending.pop_front().unwrap();
            if chunk.kind.in_transcript() {
                self.transcript.push(&chunk.transcript_chunk());
                self.tokens_out += chunk.content.len() as u64;
            }
            self.chunks_sent += 1;
            out.push((msg_type::CHUNK, chunk.to_value()));
        }
        let complete = Complete {
            task: self.task.expect("task set at ASSIGN"),
            outcome: 0,
            transcript_root: self.transcript.root(),
            usage: Usage {
                tokens_in: self.tokens_in,
                tokens_out: self.tokens_out,
                ttft_ms: 0,
                wall_ms: now_ms.saturating_sub(self.started_ms),
            },
            ext: None,
        };
        self.state = ProviderState::Completed;
        out.push((msg_type::COMPLETE, complete.to_value()));
        out
    }
}
