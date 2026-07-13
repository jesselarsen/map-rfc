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
    /// A framed transport read a length prefix above the negotiated maximum
    /// (rejected before the body — core §6.3). The session maps this to 1004.
    TooLarge,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Closed => f.write_str("transport closed"),
            TransportError::TooLarge => f.write_str("frame exceeds negotiated maximum"),
        }
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

/// The 4-byte big-endian frame length prefix (core §10 stream framing).
pub const LEN_PREFIX_BYTES: usize = 4;

/// A blocking, length-prefixed transport over a `std::net::TcpStream`
/// (TASK-020 FR-1). **Test-harness only**: raw TCP does not authenticate the
/// static keys, so the exporter (the two AgentIDs) is honest only because the
/// harness pins both peer identities — never a deployment transport (those are
/// core §10's TLS/QUIC/libp2p bindings). Reads are size-first: a length prefix
/// above `max` is rejected before the body is read (core §6.3).
pub struct TcpTransport {
    stream: std::net::TcpStream,
    exporter: Vec<u8>,
    max: u32,
}

impl TcpTransport {
    /// Wrap a connected stream. `initiator`/`responder` are the two
    /// authenticated AgentIDs (stream-initiator first); `max` is the read
    /// ceiling (use the local HELLO's max-message-size).
    pub fn new(stream: std::net::TcpStream, initiator: &AgentId, responder: &AgentId, max: u32) -> Self {
        let mut exporter = Vec::with_capacity(68);
        exporter.extend_from_slice(initiator.as_bytes());
        exporter.extend_from_slice(responder.as_bytes());
        TcpTransport { stream, exporter, max }
    }
}

impl Transport for TcpTransport {
    fn send(&self, frame: &[u8]) -> Result<(), TransportError> {
        use std::io::Write;
        let len = u32::try_from(frame.len()).map_err(|_| TransportError::Closed)?;
        // `&TcpStream` implements Write, so a shared &self suffices.
        let mut s = &self.stream;
        s.write_all(&len.to_be_bytes()).map_err(|_| TransportError::Closed)?;
        s.write_all(frame).map_err(|_| TransportError::Closed)?;
        s.flush().map_err(|_| TransportError::Closed)?;
        Ok(())
    }
    fn recv(&self) -> Result<Vec<u8>, TransportError> {
        use std::io::Read;
        let mut s = &self.stream;
        let mut lp = [0u8; LEN_PREFIX_BYTES];
        s.read_exact(&mut lp).map_err(|_| TransportError::Closed)?;
        let len = u32::from_be_bytes(lp);
        if len > self.max {
            return Err(TransportError::TooLarge);
        }
        let mut buf = vec![0u8; len as usize];
        s.read_exact(&mut buf).map_err(|_| TransportError::Closed)?;
        Ok(buf)
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
