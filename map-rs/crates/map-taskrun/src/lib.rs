//! MAP task runtimes (draft-map-tasks-00 §2–§4, memory.md §8 step 3): the
//! provider and requester state machines that execute a task over a session —
//! OFFER → QUOTE → ASSIGN → ACCEPT → CHUNK stream under flow credit → COMPLETE
//! with a verified transcript root — proven with an echo capability.
//!
//! The runtimes are **pump-driven**: `on_incoming(&mut self, &Incoming, now_ms)`
//! and `poll_deadline(now_ms)` return the messages to send as `(msg_type,
//! Value)` pairs; the caller owns all session/transport I/O and the clock. No
//! threads, no wall-clock reads, no transport calls inside the runtimes — so
//! every transition is unit-testable in isolation.
//!
//! Depends only on identity/core/session/tasks: receipts (TASK-008) and
//! payment (TASK-009) hook in via the exposed [`ReceiptInputs`] artifact and
//! the already-present (but omitted) pay-ref codec fields — NOT via a
//! dependency on trust/pay/mesh.

pub mod provider;
pub mod requester;

#[cfg(test)]
mod tests;

use map_core::cbor::Value;
use map_tasks::msg::Usage;
use map_tasks::TaskId;

pub use provider::{EchoExecutor, Executor, ProviderState, ProviderTask};
pub use requester::RequesterTask;

/// A message the caller should seal + send over its session.
pub type Outgoing = (u16, Value);

/// Everything a receipt (trust §3 ReceiptCore) needs from a verified task run
/// EXCEPT the identities — the caller (TASK-008) supplies requester/provider.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceiptInputs {
    pub task: TaskId,
    pub capability: String,
    pub started_ms: u64,
    pub ended_ms: u64,
    pub usage: Usage,
    pub outcome: u8,
}
