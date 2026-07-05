//! Byte-oriented, message-framed transport (FR-1/FR-2). The session layer is
//! transport-agnostic; `InMemoryDuplex` is deterministic test infrastructure,
//! **not** a deployable transport — a real binding (libp2p/QUIC) MUST supply a
//! genuine TLS-exporter/Noise-hash `exporter()` (core §7.3), whereas this pair
//! synthesizes one from the two authenticated AgentIDs.

use map_identity::AgentId;
use std::fmt;
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug, PartialEq, Eq)]
pub enum TransportError {
    /// The peer endpoint was dropped; no more frames can be sent or received.
    Closed,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("transport closed")
    }
}
impl std::error::Error for TransportError {}

/// One complete envelope per `send`/`recv` — the transport preserves message
/// boundaries. Stream transports will need length-framing (future package).
pub trait Transport {
    fn send(&self, frame: &[u8]) -> Result<(), TransportError>;
    fn recv(&self) -> Result<Vec<u8>, TransportError>;
    /// Transport keying material bound into the channel binding (core §7.3).
    fn exporter(&self) -> &[u8];
}

/// A bidirectional in-process pipe over two unbounded `mpsc` channels.
pub struct InMemoryDuplex {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
    exporter: Vec<u8>,
}

impl InMemoryDuplex {
    /// Build a connected pair. The returned `.0` is the **initiator** endpoint
    /// and `.1` the **responder**; drive `.0` with [`crate::Session::initiate`]
    /// and `.1` with [`crate::Session::respond`]. The shared exporter is
    /// `initiator-AgentID || responder-AgentID` (68 bytes) — identical on both
    /// ends, honest by construction for the in-memory case (FR-2).
    pub fn pair(initiator: &AgentId, responder: &AgentId) -> (InMemoryDuplex, InMemoryDuplex) {
        let (tx_ir, rx_ir) = channel(); // initiator -> responder
        let (tx_ri, rx_ri) = channel(); // responder -> initiator
        let mut exporter = Vec::with_capacity(68);
        exporter.extend_from_slice(initiator.as_bytes());
        exporter.extend_from_slice(responder.as_bytes());
        (
            InMemoryDuplex { tx: tx_ir, rx: rx_ri, exporter: exporter.clone() },
            InMemoryDuplex { tx: tx_ri, rx: rx_ir, exporter },
        )
    }
}

impl Transport for InMemoryDuplex {
    fn send(&self, frame: &[u8]) -> Result<(), TransportError> {
        self.tx.send(frame.to_vec()).map_err(|_| TransportError::Closed)
    }
    fn recv(&self) -> Result<Vec<u8>, TransportError> {
        self.rx.recv().map_err(|_| TransportError::Closed)
    }
    fn exporter(&self) -> &[u8] {
        &self.exporter
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;

    #[test]
    fn duplex_carries_frames_and_shares_exporter() {
        let a = Keypair::from_seed([1u8; 32]).agent_id();
        let b = Keypair::from_seed([2u8; 32]).agent_id();
        let (init, resp) = InMemoryDuplex::pair(&a, &b);
        // Exporter identical on both ends, initiator AID first.
        assert_eq!(init.exporter(), resp.exporter());
        assert_eq!(&init.exporter()[..34], a.as_bytes());
        assert_eq!(&init.exporter()[34..], b.as_bytes());
        // Frames cross in both directions.
        init.send(b"ping").unwrap();
        assert_eq!(resp.recv().unwrap(), b"ping");
        resp.send(b"pong").unwrap();
        assert_eq!(init.recv().unwrap(), b"pong");
    }
}
