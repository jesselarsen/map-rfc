//! map-sim: the evaluation harness pre-registered in `eval/HYPOTHESES.md`.
//!
//! A deterministic discrete-event engine (`engine`), a seeded ChaCha8 stream
//! per run (`rng`), TOML configs expanded into sweep points (`config`), a
//! parallel runner that writes byte-identical CSVs whatever the thread
//! count (`runner`), and one module per experiment (`scenarios`). Protocol
//! decisions (receipt admissibility, checkpoints, resumption, shards,
//! metering) come from the real `map-*` crates, not from re-implementations.

pub mod config;
pub mod crypto;
pub mod engine;
pub mod rng;
pub mod runner;
pub mod scenarios;
