//! `Libp2pTransport` — a blocking [`map_session::Transport`] bridged onto an
//! async libp2p substream (TASK-012 FR-2). The MAP session layer stays fully
//! synchronous: two tokio tasks own the stream halves and shuttle frames
//! through channels the blocking API reads/writes.
//!
//! - outbound: `tokio::sync::mpsc::UnboundedSender` (its `send` is synchronous,
//!   safe to call from the blocking `Transport::send`);
//! - inbound: `std::sync::mpsc::Receiver` (blocking `recv` in `Transport::recv`).
//!
//! When either stream half ends, its task drops its channel end and the paired
//! blocking call observes `TransportError::Closed`.

use crate::framing::{read_frame, write_frame};
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use map_session::{Transport, TransportError};

/// Hard per-frame ceiling for the stream reader (a DoS bound, independent of
/// the session's negotiated size — the session re-checks the tighter limit).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

pub struct Libp2pTransport {
    outbound: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    inbound: std::sync::mpsc::Receiver<Vec<u8>>,
    exporter: Vec<u8>,
}

impl Libp2pTransport {
    /// Wire a negotiated substream to the blocking [`Transport`] API. Must be
    /// called inside the tokio runtime (it spawns the reader/writer tasks).
    /// `exporter` is the channel-binding material (FR-4): the initiator and
    /// responder AgentIDs in initiator-first order.
    pub(crate) fn spawn<S>(stream: S, exporter: Vec<u8>) -> Libp2pTransport
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let (mut read_half, mut write_half) = stream.split();
        let (in_tx, in_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        // Reader: frame → inbound channel. Ends on EOF/error/oversize.
        tokio::spawn(async move {
            loop {
                match read_frame(&mut read_half, MAX_FRAME).await {
                    Ok(frame) => {
                        if in_tx.send(frame).is_err() {
                            break; // receiver (session) dropped
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Writer: outbound channel → framed write. Ends when the sender drops.
        tokio::spawn(async move {
            while let Some(frame) = out_rx.recv().await {
                if write_frame(&mut write_half, &frame).await.is_err() {
                    break;
                }
            }
        });

        Libp2pTransport { outbound: out_tx, inbound: in_rx, exporter }
    }
}

impl Transport for Libp2pTransport {
    fn send(&self, frame: &[u8]) -> Result<(), TransportError> {
        self.outbound
            .send(frame.to_vec())
            .map_err(|_| TransportError::Closed)
    }
    fn recv(&self) -> Result<Vec<u8>, TransportError> {
        self.inbound.recv().map_err(|_| TransportError::Closed)
    }
    fn exporter(&self) -> &[u8] {
        &self.exporter
    }
}
