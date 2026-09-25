//! Provider and requester accountability flows over A2A (spec/binding-a2a.md
//! §5–§8; spec/layer.md §6–§7). Transport-agnostic: these types consume and
//! produce A2A Part JSON and MAP artifact bytes; an HTTP layer (or the
//! simulator) moves them.
//!
//! Signing order (layer §6.3 as bound by A2A-7): the provider proposes
//! checkpoints and the final receipt; the requester verifies against its own
//! observation and countersigns.

use crate::binding::{part_chunk, task_id};
use crate::jcs::JcsError;
use map_core::{TaskId, Usage};
use map_identity::{AgentId, Keypair};
use map_transcript::checkpoint::{multihash_sha256, sign_checkpoint, ROLE_REQUESTER};
use map_transcript::{
    check_successor, resume, verify_prefix, CheckpointArtifact, CheckpointCore, CheckpointError, Chunk,
    TranscriptBuilder,
};
use map_trust::lifecycle::Tolerance;
use map_trust::{sign_receipt, Metrics, Outcome, Receipt, ReceiptCore, TrustError};
use serde_json::Value;

#[derive(Debug)]
pub enum BindingError {
    Checkpoint(CheckpointError),
    Trust(TrustError),
    Jcs(JcsError),
    /// A field of a proposed artifact does not match this task's state.
    Mismatch(&'static str),
    /// Proposed usage or timestamps fall outside the countersigner's
    /// tolerance of its own observation (layer §6.5): countersignature
    /// refused (binding error code 1).
    OutsideTolerance,
    /// Z-3: a resumption may not widen the zone.
    ZoneWidened,
    /// No countersigned checkpoint to resume from.
    NoResumptionPoint,
}

impl From<CheckpointError> for BindingError {
    fn from(e: CheckpointError) -> Self {
        BindingError::Checkpoint(e)
    }
}
impl From<TrustError> for BindingError {
    fn from(e: TrustError) -> Self {
        BindingError::Trust(e)
    }
}
impl From<JcsError> for BindingError {
    fn from(e: JcsError) -> Self {
        BindingError::Jcs(e)
    }
}

fn counters(u: &Usage) -> [u64; 4] {
    [u.tokens_in, u.tokens_out, u.ttft_ms, u.wall_ms]
}

fn metrics(u: &Usage) -> Metrics {
    Metrics { tokens_in: u.tokens_in, tokens_out: u.tokens_out, ttft_ms: u.ttft_ms, wall_ms: u.wall_ms }
}

/// Provider-side policy for one task.
#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub capability: String,
    /// The zone label the task runs under (layer §10).
    pub zone: u8,
    /// Propose a checkpoint every this many delivered Parts (A2A-3 default).
    pub checkpoint_every: u64,
}

/// The provider's side of one MAP-over-A2A task.
pub struct ProviderTask {
    kp: Keypair,
    requester: AgentId,
    task: TaskId,
    cfg: ProviderConfig,
    builder: TranscriptBuilder,
    next_seq: u64,
    since_checkpoint: u64,
    resumed_from: Option<Vec<u8>>,
    pending: Option<CheckpointArtifact>,
    last_countersigned: Option<CheckpointArtifact>,
    voucher_digest: Option<Vec<u8>>,
}

impl ProviderTask {
    /// A fresh task (TaskID per A2A-8).
    pub fn start(kp: Keypair, requester: AgentId, nonce: &[u8; 16], a2a_task_id: &str, cfg: ProviderConfig) -> Self {
        let task = task_id(nonce, &kp.agent_id(), a2a_task_id);
        ProviderTask {
            kp,
            requester,
            task,
            cfg,
            builder: TranscriptBuilder::new(),
            next_seq: 0,
            since_checkpoint: 0,
            resumed_from: None,
            pending: None,
            last_countersigned: None,
            voucher_digest: None,
        }
    }

    /// Continue a task another provider started (layer §7.4, A2A-23..25).
    /// Verifies that the checkpoint is a valid resumption point for this
    /// requester, that its root matches the transcript prefix (given as the
    /// delivered A2A Parts), and that the zone is not widened (Z-3).
    pub fn resume(
        kp: Keypair,
        requester: AgentId,
        nonce: &[u8; 16],
        a2a_task_id: &str,
        cfg: ProviderConfig,
        checkpoint: &[u8],
        prefix_parts: &[Value],
    ) -> Result<Self, BindingError> {
        let art = CheckpointArtifact::decode_bytes(checkpoint)?;
        let prefix: Vec<Chunk> = prefix_parts
            .iter()
            .enumerate()
            .map(|(i, p)| part_chunk(i as u64, p))
            .collect::<Result<_, _>>()?;
        let r = resume(&art, &prefix)?;
        if r.core.requester != requester {
            return Err(BindingError::Mismatch("checkpoint requester"));
        }
        if cfg.zone > r.core.zone {
            return Err(BindingError::ZoneWidened);
        }
        let mut t = ProviderTask::start(kp, requester, nonce, a2a_task_id, cfg);
        t.builder = r.builder;
        t.next_seq = r.next_seq;
        t.resumed_from = Some(r.resumed_from);
        Ok(t)
    }

    pub fn task_id(&self) -> TaskId {
        self.task
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Bind the digest of the requester's latest cumulative voucher (layer
    /// §9.3); it is carried by subsequent checkpoints and the receipt.
    pub fn set_voucher_digest(&mut self, digest: Vec<u8>) {
        self.voucher_digest = Some(digest);
    }

    /// Record one delivered artifact Part. When the cadence is reached,
    /// returns a provider-signed checkpoint proposal (C-1) to attach to the
    /// event carrying this Part.
    pub fn deliver(&mut self, part: &Value, cumulative: Usage, now_ms: u64) -> Result<Option<Vec<u8>>, BindingError> {
        self.builder.push(&part_chunk(self.next_seq, part)?);
        self.next_seq += 1;
        self.since_checkpoint += 1;
        if self.since_checkpoint < self.cfg.checkpoint_every {
            return Ok(None);
        }
        self.since_checkpoint = 0;
        let core = CheckpointCore {
            task: self.task,
            requester: self.requester.clone(),
            provider: self.kp.agent_id(),
            through_seq: self.next_seq - 1,
            root: self.builder.root(),
            usage: cumulative,
            zone: self.cfg.zone,
            timestamp_ms: now_ms,
            voucher_digest: self.voucher_digest.clone(),
            resumed_from: self.resumed_from.clone(),
        };
        let art = CheckpointArtifact::propose(&self.kp, &core)?;
        let bytes = art.encode()?;
        self.pending = Some(art);
        Ok(Some(bytes))
    }

    /// Accept the requester's countersignature on the pending proposal.
    pub fn on_countersigned(&mut self, bytes: &[u8]) -> Result<(), BindingError> {
        let art = CheckpointArtifact::decode_bytes(bytes)?;
        let pending = self.pending.as_ref().ok_or(BindingError::Mismatch("no pending checkpoint"))?;
        if art.core_bytes != pending.core_bytes {
            return Err(BindingError::Mismatch("countersigned a different core"));
        }
        art.verify_resumption_point()?;
        self.last_countersigned = Some(art);
        self.pending = None;
        Ok(())
    }

    /// Layer C-5: what a failed provider is owed, namely the usage in its
    /// last countersigned checkpoint.
    pub fn owed_usage(&self) -> Option<Usage> {
        self.last_countersigned.as_ref().and_then(|a| a.core().ok()).map(|c| c.usage)
    }

    pub fn last_countersigned(&self) -> Option<&CheckpointArtifact> {
        self.last_countersigned.as_ref()
    }

    /// Propose the final receipt (provider signs first, A2A-7).
    pub fn propose_receipt(&self, outcome: Outcome, cumulative: Usage, started_ms: u64, ended_ms: u64) -> Result<Vec<u8>, BindingError> {
        let core = ReceiptCore {
            task: self.task,
            requester: self.requester.clone(),
            provider: self.kp.agent_id(),
            capability: self.cfg.capability.clone(),
            started_ms,
            ended_ms,
            outcome,
            metrics: metrics(&cumulative),
            rating: None,
            payment_ref: self.voucher_digest.clone(),
            transcript_root: Some(self.builder.root()),
            zone: Some(self.cfg.zone),
            resumed_from: self.resumed_from.clone(),
        };
        let core_bytes = core.encode()?;
        Ok(Receipt {
            sig_provider: Some(sign_receipt(&self.kp, map_trust::ROLE_PROVIDER, &core_bytes)),
            sig_requester: None,
            core_bytes,
        }
        .encode()?)
    }

    /// Accept the requester's countersigned receipt for `proposed`.
    pub fn on_countersigned_receipt(&self, proposed: &[u8], countersigned: &[u8]) -> Result<Receipt, BindingError> {
        let p = Receipt::decode_bytes(proposed)?;
        let r = Receipt::decode_bytes(countersigned)?;
        if r.core_bytes != p.core_bytes {
            return Err(BindingError::Mismatch("countersigned a different receipt core"));
        }
        if !r.verify()?.dual {
            return Err(BindingError::Mismatch("receipt not dual-signed"));
        }
        Ok(r)
    }
}

/// The requester's side of one MAP-over-A2A task.
pub struct RequesterTask {
    kp: Keypair,
    provider: AgentId,
    task: TaskId,
    capability: String,
    zone: u8,
    tolerance: Tolerance,
    parts: Vec<Value>,
    chunks: Vec<Chunk>,
    builder: TranscriptBuilder,
    resumed_from: Option<Vec<u8>>,
    last_checkpoint: Option<(Vec<u8>, CheckpointCore)>,
}

/// What the requester hands a successor provider (A2A-23).
#[derive(Clone, Debug)]
pub struct ResumePackage {
    pub checkpoint: Vec<u8>,
    pub prefix_parts: Vec<Value>,
}

impl RequesterTask {
    pub fn start(kp: Keypair, provider: AgentId, nonce: &[u8; 16], a2a_task_id: &str, capability: &str, zone: u8) -> Self {
        let task = task_id(nonce, &provider, a2a_task_id);
        RequesterTask {
            kp,
            provider,
            task,
            capability: capability.to_string(),
            zone,
            tolerance: Tolerance::default(),
            parts: Vec::new(),
            chunks: Vec::new(),
            builder: TranscriptBuilder::new(),
            resumed_from: None,
            last_checkpoint: None,
        }
    }

    pub fn with_tolerance(mut self, t: Tolerance) -> Self {
        self.tolerance = t;
        self
    }

    pub fn task_id(&self) -> TaskId {
        self.task
    }

    /// Everything needed to resume on a successor: the last countersigned
    /// checkpoint and the Parts it covers.
    pub fn resume_package(&self) -> Result<ResumePackage, BindingError> {
        let (bytes, core) = self.last_checkpoint.as_ref().ok_or(BindingError::NoResumptionPoint)?;
        let n = core.through_seq as usize + 1;
        Ok(ResumePackage { checkpoint: bytes.clone(), prefix_parts: self.parts[..n].to_vec() })
    }

    /// A requester for the successor task (new provider, new A2A task, new
    /// TaskID per A2A-25), continuing the same transcript sequence.
    pub fn resume_on(
        &self,
        new_provider: AgentId,
        nonce: &[u8; 16],
        a2a_task_id: &str,
    ) -> Result<(RequesterTask, ResumePackage), BindingError> {
        let pkg = self.resume_package()?;
        let mut next = RequesterTask::start(self.kp.clone(), new_provider, nonce, a2a_task_id, &self.capability, self.zone)
            .with_tolerance(self.tolerance);
        for p in &pkg.prefix_parts {
            next.on_part(p)?;
        }
        next.resumed_from = Some(multihash_sha256(&pkg.checkpoint));
        Ok((next, pkg))
    }

    /// Record one delivered artifact Part.
    pub fn on_part(&mut self, part: &Value) -> Result<(), BindingError> {
        let chunk = part_chunk(self.chunks.len() as u64, part)?;
        self.builder.push(&chunk);
        self.chunks.push(chunk);
        self.parts.push(part.clone());
        Ok(())
    }

    pub fn transcript_root(&self) -> [u8; 32] {
        self.builder.root()
    }

    /// Parts in this task's transcript, including a resumed prefix.
    pub fn parts_received(&self) -> usize {
        self.parts.len()
    }

    /// Verify a provider's checkpoint proposal against what this requester
    /// actually received and observed, then countersign it (C-2). Returns
    /// the dual-signed artifact bytes to send back.
    pub fn on_checkpoint(&mut self, proposal: &[u8], observed: &Usage) -> Result<Vec<u8>, BindingError> {
        let mut art = CheckpointArtifact::decode_bytes(proposal)?;
        let v = art.verify()?;
        if art.sig_provider.is_none() {
            return Err(BindingError::Mismatch("checkpoint not signed by the provider"));
        }
        let core = v.core;
        if core.task != self.task {
            return Err(BindingError::Mismatch("task"));
        }
        if core.requester != self.kp.agent_id() || core.provider != self.provider {
            return Err(BindingError::Mismatch("parties"));
        }
        if core.zone != self.zone {
            return Err(BindingError::Mismatch("zone"));
        }
        if core.resumed_from != self.resumed_from {
            return Err(BindingError::Mismatch("resumed-from"));
        }
        let n = core.through_seq as usize + 1;
        if n > self.chunks.len() {
            return Err(BindingError::Mismatch("checkpoint covers Parts not yet received"));
        }
        verify_prefix(&core, &self.chunks[..n])?;
        if let Some((_, prev)) = &self.last_checkpoint {
            check_successor(prev, &core)?;
        }
        if !self.tolerance.accepts_counters(counters(&core.usage), counters(observed)) {
            return Err(BindingError::OutsideTolerance);
        }
        art.sig_requester = Some(sign_checkpoint(&self.kp, ROLE_REQUESTER, &art.core_bytes));
        let bytes = art.encode()?;
        self.last_checkpoint = Some((bytes.clone(), core));
        Ok(bytes)
    }

    /// Verify the provider's receipt proposal and countersign it. `observed`
    /// is this requester's own view of usage and timing.
    pub fn on_receipt(&self, proposal: &[u8], observed_usage: &Usage, started_ms: u64, ended_ms: u64) -> Result<Vec<u8>, BindingError> {
        let mut r = Receipt::decode_bytes(proposal)?;
        if r.sig_provider.is_none() {
            return Err(BindingError::Mismatch("receipt not signed by the provider"));
        }
        let core = r.verify()?.core;
        if core.task != self.task || core.requester != self.kp.agent_id() || core.provider != self.provider {
            return Err(BindingError::Mismatch("task or parties"));
        }
        if core.capability != self.capability {
            return Err(BindingError::Mismatch("capability"));
        }
        if core.transcript_root != Some(self.builder.root()) {
            return Err(BindingError::Mismatch("transcript root"));
        }
        if core.zone != Some(self.zone) || core.resumed_from != self.resumed_from {
            return Err(BindingError::Mismatch("zone or resumed-from"));
        }
        let mut observed = core.clone();
        observed.metrics = metrics(observed_usage);
        observed.started_ms = started_ms;
        observed.ended_ms = ended_ms;
        if !self.tolerance.accepts(&core, &observed) {
            return Err(BindingError::OutsideTolerance);
        }
        r.sig_requester = Some(sign_receipt(&self.kp, map_trust::ROLE_REQUESTER, &r.core_bytes));
        Ok(r.encode()?)
    }
}
