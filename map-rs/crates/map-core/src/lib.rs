//! MAP core bounded context (draft-map-core-00).
//!
//! Aggregate root: the session. Durable fact: the negotiation transcript
//! hash. Derived, prunable state: replay caches and flow-credit counters
//! (ops §2). Everything signed or hashed goes through [`cbor`]'s
//! deterministic profile; every message is an [`envelope`] and signatures
//! verify over received bytes (invariant I1).

pub mod cbor;
pub mod envelope;
pub mod errors;
pub mod hello;
pub mod msg_type;
pub mod replay;

pub use envelope::{open, seal, Body, EnvelopeError, MsgId, Opened, PROTOCOL_VERSION_1_0};
pub use hello::{channel_binding, negotiate, transcript_hash, Hello, HelloAck, Negotiated};
