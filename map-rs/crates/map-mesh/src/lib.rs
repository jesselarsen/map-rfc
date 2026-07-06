//! MAP discovery message flows over `map-session` (draft-map-discovery-00
//! §2–§7). Implements descriptor publish/ingest, an in-process `MeshIndex`
//! standing in for the Kademlia DHT (the real DHT arrives with libp2p,
//! TASK-012), and the CAP_QUERY/STATUS responder + client calls.
//!
//! Dependency direction: `map-mesh` depends on session + discovery so those
//! two crates stay pure and independent of each other (the doc graph). Nothing
//! depends on `map-mesh`; TASK-012 replaces `MeshIndex` behind the same API.

pub mod flows;
pub mod index;
pub mod publish;

#[cfg(test)]
mod tests;

pub use flows::{
    query_caps, query_status, serve_one, serve_until_close, ServeOutcome, StatusReport,
    StatusSnapshot, DEFAULT_CAP_LIMIT,
};
pub use index::{IndexIngest, MeshIndex};
pub use publish::{
    ingest_descriptor_envelope, needs_republish, publish_descriptor,
    MAX_DESCRIPTOR_VALIDITY_MS, MAX_REPUBLISH_INTERVAL_MS,
};

use map_core::EnvelopeError;
use map_discovery::DiscoveryError;
use map_session::SessionError;
use std::fmt;

/// Errors from the mesh layer, carrying the underlying cause.
#[derive(Debug)]
pub enum MeshError {
    Session(SessionError),
    Envelope(EnvelopeError),
    Discovery(DiscoveryError),
    /// A descriptor failed a semantic check (self-signature, ttl, shape).
    InvalidDescriptor(String),
    /// A received message content map was the wrong shape.
    Malformed(String),
}

impl MeshError {
    /// The core §9 wire code a responder should report for this error.
    pub fn wire_code(&self) -> Option<u16> {
        use map_core::errors::codes;
        Some(match self {
            MeshError::InvalidDescriptor(_) | MeshError::Malformed(_) => codes::MALFORMED,
            MeshError::Session(e) => return e.wire_code(),
            MeshError::Envelope(_) | MeshError::Discovery(_) => return None,
        })
    }
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshError::Session(e) => write!(f, "session: {e:?}"),
            MeshError::Envelope(e) => write!(f, "envelope: {e}"),
            MeshError::Discovery(e) => write!(f, "discovery: {e}"),
            MeshError::InvalidDescriptor(m) => write!(f, "invalid descriptor: {m}"),
            MeshError::Malformed(m) => write!(f, "malformed message: {m}"),
        }
    }
}
impl std::error::Error for MeshError {}

impl From<SessionError> for MeshError {
    fn from(e: SessionError) -> Self {
        MeshError::Session(e)
    }
}
impl From<EnvelopeError> for MeshError {
    fn from(e: EnvelopeError) -> Self {
        MeshError::Envelope(e)
    }
}
impl From<DiscoveryError> for MeshError {
    fn from(e: DiscoveryError) -> Self {
        MeshError::Discovery(e)
    }
}
