//! Dispute runtime hooks (pay §6, TASK-026) — deliberately THIN. The verdict
//! engine and codecs live in `map_pay::dispute`; this module only sequences
//! messages and plumbs storage: opening a dispute (idempotent by id),
//! responding with evidence drawn from what the runtime already stores (its
//! RECEIVED envelope bytes — the TASK-017 `Incoming.raw` seam — with the
//! session CB attached per §6.2), accumulating the peer's evidence (deduped,
//! bounded), and signalling ERROR 6003 for flows refused pending resolution.
//! The receipt path reads [`DisputeState::outcome_disputed`] to stamp trust
//! §3 outcome 4; receipt logic is NOT rebuilt here.

use crate::Outgoing;
use map_core::errors::{codes, ErrorMsg};
use map_core::msg_type;
use map_pay::dispute::{EvidenceItem, PayDispute, PayDisputeEvidence};
use map_session::Incoming;
use std::collections::{HashMap, HashSet};

fn err(code: u16, retryable: bool) -> Vec<Outgoing> {
    vec![(msg_type::ERROR, ErrorMsg::new(code, retryable).to_value())]
}

/// One party's view of one dispute: what it sent, what it holds, what the
/// peer supplied. Evidence is content-address-deduped and size-bounded
/// (§6.1/§6.4); the verdict is computed elsewhere, on demand, by
/// `map_pay::dispute::verify_dispute`.
pub struct DisputeState {
    /// Disputes this party OPENED, by id (idempotent re-send).
    opened: HashMap<[u8; 16], PayDispute>,
    /// Disputes received from the peer.
    received: HashMap<[u8; 16], PayDispute>,
    /// This party's own evidence store (what it will disclose).
    own_items: Vec<EvidenceItem>,
    /// Evidence received from the peer, per dispute (deduped, bounded).
    peer_items: HashMap<[u8; 16], Vec<EvidenceItem>>,
    seen_addrs: HashSet<[u8; 32]>,
    /// §6.4 local bound on stored peer evidence per dispute.
    pub max_peer_items: usize,
    /// Max items per outgoing PAY_DISPUTE_EVIDENCE message (chunk with
    /// `more`; volume rides FLOW_CREDIT).
    pub max_items_per_msg: usize,
    /// A flow-refusal flag: while set, billing traffic is answered 6003.
    paused_pending_dispute: bool,
}

impl Default for DisputeState {
    fn default() -> Self {
        DisputeState {
            opened: HashMap::new(),
            received: HashMap::new(),
            own_items: Vec::new(),
            peer_items: HashMap::new(),
            seen_addrs: HashSet::new(),
            max_peer_items: 256,
            max_items_per_msg: 16,
            paused_pending_dispute: false,
        }
    }
}

impl DisputeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stock this party's disclosure store. The runtime feeds artifacts as it
    /// holds them: received envelope frames (`Incoming.raw` — VERBATIM, I1)
    /// with the session CB for in-session ones (§6.2), inner-signed artifacts
    /// (receipts, countersigned checkpoints) as their own bytes. Deduped by
    /// content address.
    pub fn add_own_evidence(&mut self, item: EvidenceItem) {
        if self.seen_addrs.insert(item.content_address()) {
            self.own_items.push(item);
        }
    }

    /// Convenience for the common case: an artifact that arrived in-session
    /// (`inc.raw` + this session's CB).
    pub fn add_own_incoming(&mut self, kind: u64, inc: &Incoming, cb: [u8; 32]) {
        self.add_own_evidence(EvidenceItem { kind, bytes: inc.raw.clone(), cb: Some(cb) });
    }

    /// Open a dispute (0x0405). Idempotent by dispute-id: a re-open with the
    /// SAME content re-emits identically; a different dispute under a used id
    /// is refused locally (empty output).
    pub fn open_dispute(&mut self, dispute: PayDispute) -> Vec<Outgoing> {
        match self.opened.get(&dispute.dispute_id) {
            Some(existing) if *existing != dispute => Vec::new(),
            _ => {
                self.paused_pending_dispute = true;
                self.opened.insert(dispute.dispute_id, dispute.clone());
                vec![(msg_type::PAY_DISPUTE, dispute.to_value())]
            }
        }
    }

    /// Receive a PAY_DISPUTE: record it (idempotent by id AND bytes — a
    /// different dispute under a seen id is malformed) and respond with this
    /// party's evidence, chunked under `max_items_per_msg` with `more`.
    pub fn on_dispute(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        let d = match PayDispute::from_value(&inc.content) {
            Ok(d) => d,
            Err(_) => return err(codes::MALFORMED, false),
        };
        if let Some(existing) = self.received.get(&d.dispute_id) {
            if *existing != d {
                return err(codes::MALFORMED, false);
            }
        }
        self.received.insert(d.dispute_id, d.clone());
        self.evidence_messages(d.dispute_id)
    }

    /// This party's full disclosure for `dispute_id`, chunked. (Also used to
    /// push evidence for a dispute this party opened itself.)
    pub fn evidence_messages(&self, dispute_id: [u8; 16]) -> Vec<Outgoing> {
        if self.own_items.is_empty() {
            return Vec::new();
        }
        let chunks: Vec<&[EvidenceItem]> = self.own_items.chunks(self.max_items_per_msg).collect();
        let last = chunks.len() - 1;
        chunks
            .iter()
            .enumerate()
            .map(|(i, items)| {
                let msg = PayDisputeEvidence {
                    dispute_id,
                    items: items.to_vec(),
                    more: if i < last { Some(true) } else { None },
                };
                (msg_type::PAY_DISPUTE_EVIDENCE, msg.to_value())
            })
            .collect()
    }

    /// Receive PAY_DISPUTE_EVIDENCE: accumulate (deduped by content address,
    /// bounded by `max_peer_items` — beyond the bound items are DROPPED, and
    /// the verdict runs on what was kept: §6.4 availability doctrine).
    pub fn on_evidence(&mut self, inc: &Incoming) -> Vec<Outgoing> {
        let ev = match PayDisputeEvidence::from_value(&inc.content) {
            Ok(e) => e,
            Err(_) => return err(codes::MALFORMED, false),
        };
        let bucket = self.peer_items.entry(ev.dispute_id).or_default();
        for item in ev.items {
            if bucket.len() >= self.max_peer_items {
                break;
            }
            if self.seen_addrs.insert(item.content_address()) {
                bucket.push(item);
            }
        }
        Vec::new()
    }

    /// The peer's accumulated evidence for a dispute (verdict input).
    pub fn peer_evidence(&self, dispute_id: &[u8; 16]) -> &[EvidenceItem] {
        self.peer_items.get(dispute_id).map(Vec::as_slice).unwrap_or(&[])
    }
    /// This party's own disclosure set (verdict input).
    pub fn own_evidence(&self) -> &[EvidenceItem] {
        &self.own_items
    }
    /// True once any dispute is open in either direction — the receipt path
    /// stamps trust §3 outcome 4 (disputed) from this.
    pub fn outcome_disputed(&self) -> bool {
        !self.opened.is_empty() || !self.received.is_empty()
    }

    /// While a dispute is open and this party refuses to continue a payment
    /// flow, billing traffic is answered ERROR 6003 (pay §6.1). Local policy:
    /// the caller decides WHICH incoming types to route here.
    pub fn refuse_pending(&self) -> Vec<Outgoing> {
        if self.paused_pending_dispute || !self.received.is_empty() {
            err(codes::DISPUTE_OPENED, true)
        } else {
            Vec::new()
        }
    }
}
