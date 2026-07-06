//! `MeshIndex` — an in-process stand-in for the sharded Kademlia DHT
//! (discovery §5/§6). Stores capability→providers and cid→descriptor bytes,
//! plus an agent→latest-cid side table so a responder can serve an agent's
//! current descriptor.
//!
//! DERIVED-data property (discovery §5 data-ownership): [`MeshIndex::rebuild`]
//! from a set of descriptor envelopes reproduces any incrementally built index
//! EXACTLY. This holds for ANY arrival order because caps and descriptor bytes
//! *accumulate* (order-independent set/map unions) and only the agent→cid
//! pointer is seq-LWW (argmax over a strictly-increasing per-subject seq).

use crate::MeshError;
use map_core::open;
use map_discovery::{multihash_sha256, shard, sharded_capability_key, Descriptor};
use map_identity::AgentId;
use std::collections::{BTreeSet, HashMap};

/// Outcome of an index ingest, reflecting the agent→cid pointer LWW.
#[derive(Debug, PartialEq, Eq)]
pub enum IndexIngest {
    /// This descriptor became the agent's latest.
    Accepted,
    /// An equal-or-newer seq was already the latest (caps/bytes still stored).
    Stale,
}

#[derive(Debug, Default, PartialEq)]
pub struct MeshIndex {
    /// K'(cap, shard) → set of provider AgentIDs.
    providers: HashMap<Vec<u8>, BTreeSet<AgentId>>,
    /// descriptor-cid → descriptor envelope bytes.
    descs: HashMap<Vec<u8>, Vec<u8>>,
    /// agent → (seq, cid) for its latest descriptor (seq-LWW).
    latest: HashMap<AgentId, (u64, Vec<u8>)>,
}

impl MeshIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// FR-3: record `agent` as a provider under `K'(cap, shard)`.
    pub fn provide(&mut self, cap: &str, shard: u8, agent: AgentId) {
        self.providers
            .entry(sharded_capability_key(cap, shard))
            .or_default()
            .insert(agent);
    }

    /// FR-3: providers published under `K'(cap, shard)`, deterministically sorted.
    pub fn providers(&self, cap: &str, shard: u8) -> Vec<AgentId> {
        self.providers
            .get(&sharded_capability_key(cap, shard))
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn put_desc(&mut self, cid: Vec<u8>, bytes: Vec<u8>) {
        self.descs.insert(cid, bytes);
    }

    pub fn get_desc(&self, cid: &[u8]) -> Option<&Vec<u8>> {
        self.descs.get(cid)
    }

    /// The stored descriptor envelope bytes for an agent's latest seq.
    pub fn latest_desc(&self, agent: &AgentId) -> Option<&Vec<u8>> {
        let (_, cid) = self.latest.get(agent)?;
        self.descs.get(cid)
    }

    /// Ingest a verified descriptor: index its caps under the agent's shard and
    /// store the envelope bytes by cid (both ACCUMULATE — order-independent).
    /// Only the agent→cid pointer is seq-LWW.
    pub fn ingest(&mut self, desc: &Descriptor, envelope_bytes: &[u8]) -> IndexIngest {
        let cid = multihash_sha256(envelope_bytes);
        let sh = shard(&desc.agent);
        for cap in &desc.capabilities {
            self.provide(cap, sh, desc.agent.clone());
        }
        self.put_desc(cid.clone(), envelope_bytes.to_vec());
        match self.latest.get(&desc.agent) {
            Some((cached, _)) if *cached >= desc.seq => IndexIngest::Stale,
            _ => {
                self.latest.insert(desc.agent.clone(), (desc.seq, cid));
                IndexIngest::Accepted
            }
        }
    }

    /// Rebuild the index from a set of descriptor envelopes (each verified via
    /// `open`), folding `ingest` sorted by (agent, seq). Equal to any
    /// incrementally built index over the same envelope multiset.
    pub fn rebuild(envelopes: &[Vec<u8>]) -> Result<MeshIndex, MeshError> {
        let mut parsed: Vec<(Descriptor, &Vec<u8>)> = Vec::with_capacity(envelopes.len());
        for e in envelopes {
            let opened = open(e, b"").map_err(MeshError::Envelope)?;
            let desc = Descriptor::from_value(&opened.body.content).map_err(MeshError::Discovery)?;
            parsed.push((desc, e));
        }
        parsed.sort_by(|a, b| a.0.agent.cmp(&b.0.agent).then(a.0.seq.cmp(&b.0.seq)));
        let mut idx = MeshIndex::new();
        for (d, bytes) in &parsed {
            idx.ingest(d, bytes);
        }
        Ok(idx)
    }
}
