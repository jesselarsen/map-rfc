//! MAP session layer over a pluggable transport (draft-map-core-01 §6/§7/§12).
//!
//! Proves the protocol logic end-to-end in-process: the HELLO/HELLO_ACK
//! handshake, transcript hash and channel binding, the session state machine,
//! and in-session `send`/`recv` with sequence, replay, and size enforcement —
//! all over a deterministic in-memory duplex, clock-free and testable. The
//! libp2p/QUIC binding (real transport exporter) is a later package.
//!
//! Depends only on `map-identity` + `map-core`: the session is a core-context
//! component and MUST NOT know about tasks/trust/pay — callers route content by
//! `msg_type` themselves.

pub mod clock;
pub mod session;
pub mod transport;

pub use clock::{Clock, CountingMsgIds, FixedClock, MsgIdSource, RandomMsgIds, SteppingClock, SystemClock};
pub use session::{Incoming, Session, SessionConfig, SessionError, SessionState};
pub use transport::{InMemoryDuplex, TcpTransport, Transport, TransportError};
