//! MAP core bounded context (draft-map-core-01).
//!
//! Aggregate root: the session. Durable fact: the negotiation transcript
//! hash. Derived, prunable state: replay caches and flow-credit counters
//! (ops §2). Everything signed or hashed goes through [`cbor`]'s
//! deterministic profile; every message is an [`envelope`] and signatures
//! verify over received bytes (invariant I1).

pub mod cbor;
pub mod delegation;
pub mod envelope;
pub mod errors;
pub mod hello;
pub mod msg_type;
pub mod replay;
pub mod task_id;

pub use delegation::{
    issue, validate_cert, validate_chain, Caveat, DelegError, DelegateBody, EffectiveScope,
};
pub use envelope::{
    open, open_delegated, seal, seal_delegated, Body, EnvelopeError, MsgId, Opened,
    OpenedDelegated, PROTOCOL_VERSION_1_0,
};
pub use hello::{channel_binding, negotiate, transcript_hash, Hello, HelloAck, Negotiated};
pub use task_id::TaskId;
