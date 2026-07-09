//! MAP libp2p transport binding (`/map/1.0.0`) — TASK-012.
//!
//! Adds the first real network transport behind the existing
//! [`map_session::Transport`] trait, plus a Kademlia-backed descriptor index
//! replacing `map_mesh::MeshIndex` behind the same publish/lookup shape. All
//! async lives inside this crate; the public API ([`MapNode`]) is blocking, so
//! no other crate changes its programming model.
//!
//! Scope: LAN/loopback grade. NAT traversal (AutoNAT/relay/DCUtR), discovery
//! privacy, and anti-eclipse defenses are OUT of scope (a later package).
//!
//! Build note: on this windows-gnu box the mingw toolchain
//! (`C:\msys64\mingw64\bin`) must be on PATH for `ring` (quic) and `dlltool`
//! (windows-sys import libs). See `memory.md` §5.

pub mod framing;
pub mod node;
pub mod transport;

pub use node::{MapNode, IncomingSession, MAP_PROTOCOL};
pub use transport::Libp2pTransport;

use libp2p::{identity, PeerId};
use map_identity::AgentId;
use map_session::SessionError;
use std::fmt;

/// Errors from the network layer. Session-level failures stay [`SessionError`].
#[derive(Debug)]
pub enum NetError {
    /// PeerID ≠ AgentID, or a keypair/identity decode failure (FR-3).
    Identity(String),
    /// The tokio runtime could not be built.
    Runtime(String),
    /// A swarm/dial/listen/kad operation failed.
    Swarm(String),
    /// A MAP session handshake or in-session error.
    Session(SessionError),
    /// The node (its swarm task) has stopped.
    Closed,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Identity(m) => write!(f, "identity: {m}"),
            NetError::Runtime(m) => write!(f, "runtime: {m}"),
            NetError::Swarm(m) => write!(f, "swarm: {m}"),
            NetError::Session(e) => write!(f, "session: {e:?}"),
            NetError::Closed => write!(f, "node stopped"),
        }
    }
}
impl std::error::Error for NetError {}

/// The libp2p `PeerId` for a MAP `AgentId` (ed25519; identity §3.2). `None` if
/// the embedded key is not a valid ed25519 point.
pub(crate) fn peer_id_of(agent: &AgentId) -> Option<PeerId> {
    let bytes = agent.as_bytes();
    if bytes.len() != 34 {
        return None;
    }
    let ed = identity::ed25519::PublicKey::try_from_bytes(&bytes[2..]).ok()?;
    let pk: identity::PublicKey = ed.into();
    Some(pk.to_peer_id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;

    #[test]
    fn agent_id_peer_id_roundtrip() {
        // AgentId -> PeerId -> AgentId is the identity binding (FR-3).
        let agent = Keypair::from_seed([5u8; 32]).agent_id();
        let peer = peer_id_of(&agent).expect("valid ed25519 -> PeerId");
        let back = node::agent_id_from_peer(&peer).expect("PeerId embeds the key");
        assert_eq!(agent, back);
    }
}
