//! MAP over A2A v1.0 (spec/binding-a2a.md).
//!
//! MAP rides alongside A2A as an extension: an A2A client that does not
//! activate it gets plain A2A. This crate holds the binding's pure pieces
//! (canonical JSON, Agent Card signatures, the descriptor↔card rule, TaskID
//! derivation, the Part→transcript mapping, and the provider/requester
//! accountability flows). The `http` feature adds an A2A JSON-RPC
//! provider (`server`) and requester (`client`) over plain HTTP.

pub mod b64;
pub mod jcs;
pub mod binding;
pub mod card;
pub mod descriptor;
pub mod flow;
pub mod workload;

#[cfg(feature = "http")]
pub mod client;
#[cfg(feature = "http")]
pub mod server;
