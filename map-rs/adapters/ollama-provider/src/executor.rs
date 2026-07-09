//! `OllamaExecutor` — adapts any [`InferenceBackend`](crate::InferenceBackend)
//! to the `map-taskrun` [`Executor`] trait (TASK-013 FR-4), so a real model can
//! drive a MAP task's CHUNK stream.
//!
//! The task input (ASSIGN `InputSpec` inline bytes) is the prompt, interpreted
//! as UTF-8. `Executor::execute` is infallible by contract, so this adapter
//! degrades all failure modes to **empty output** — never a panic:
//! - invalid UTF-8 input → no chunks;
//! - a backend error (unavailable / model-not-found / failed) → no chunks.
//!
//! (Surfacing a backend failure as a COMPLETE `outcome = 1` would need a
//! fallible `Executor`; that is a noted map-taskrun follow-up. For the demo,
//! call the backend directly via [`crate::InferenceBackend::infer`] to see the
//! specific error.)

use crate::{result_to_chunks, InferenceBackend, InferenceRequest};
use map_tasks::ChunkKind;
use map_taskrun::Executor;

/// Runs task input through a model. `model`/`max_tokens` are fixed per provider.
pub struct OllamaExecutor<B: InferenceBackend> {
    pub backend: B,
    pub model: String,
    pub max_tokens: u32,
}

impl<B: InferenceBackend> OllamaExecutor<B> {
    pub fn new(backend: B, model: impl Into<String>, max_tokens: u32) -> Self {
        OllamaExecutor { backend, model: model.into(), max_tokens }
    }
}

impl<B: InferenceBackend> Executor for OllamaExecutor<B> {
    fn execute(&self, input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)> {
        // Prompt = inline input as UTF-8; invalid → empty output (FR-4, no panic).
        let prompt = match std::str::from_utf8(input) {
            Ok(s) => s.to_string(),
            Err(_) => return Vec::new(),
        };
        let req = InferenceRequest { model: self.model.clone(), prompt, max_tokens: self.max_tokens };
        match self.backend.infer(&req) {
            Ok(result) => result_to_chunks(&result)
                .into_iter()
                .map(|c| (c.kind, c.content))
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockBackend;

    #[test]
    fn executor_produces_output_and_usage_chunks() {
        let exec = OllamaExecutor::new(MockBackend::new(), "mock", 128);
        let chunks = exec.execute(b"hello world");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0, ChunkKind::OutputText);
        assert_eq!(chunks[0].1, b"echo: hello world");
        assert_eq!(chunks[1].0, ChunkKind::UsageUpdate);
    }

    #[test]
    fn invalid_utf8_yields_empty_output_never_panics() {
        let exec = OllamaExecutor::new(MockBackend::new(), "mock", 128);
        assert!(exec.execute(&[0xFF, 0xFE, 0x00]).is_empty());
    }

    #[test]
    fn unknown_model_error_yields_empty_output() {
        let exec = OllamaExecutor::new(MockBackend::new(), "absent-model", 128);
        assert!(exec.execute(b"hi").is_empty());
    }
}
