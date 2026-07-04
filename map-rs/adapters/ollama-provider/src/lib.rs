//! MAP local-inference provider adapter (draft-map-ops-00 §8).
//!
//! This crate is the **seam** a real Ollama backend will fill, not the backend
//! itself: it defines the [`InferenceBackend`] trait, a deterministic
//! [`MockBackend`] for tests, and the glue that turns a backend response into
//! transcript-eligible [`Chunk`]s the tasks-crate `TranscriptBuilder` can
//! commit. There is deliberately **no HTTP client, no async, no clock** — the
//! real localhost:11434 backend is future work (see TASK handoff notes), and
//! `InferenceResult::millis` is a *duration*, never a wall-clock read, so all
//! outputs here are reproducible.

use map_core::cbor::{encode, Value};
use map_tasks::{Chunk, ChunkKind};

/// A request handed to a local model server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InferenceRequest {
    pub model: String,
    pub prompt: String,
    pub max_tokens: u32,
}

/// A model server's response. `millis` is elapsed generation time, not a
/// timestamp (reproducibility rule, §7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InferenceResult {
    pub text: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub millis: u64,
}

/// Why an inference could not be produced (§10). `ModelNotFound` carries the
/// requested model name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    Unavailable,
    ModelNotFound(String),
    Failed(String),
}

/// Abstraction over a local model server. The only method a transport-free
/// scaffold needs; the real adapter implements this over HTTP later.
pub trait InferenceBackend {
    fn infer(&self, req: &InferenceRequest) -> Result<InferenceResult, BackendError>;
}

/// Deterministic in-memory backend for tests and demos: echoes the prompt and
/// reports reproducible token/time figures. No I/O.
#[derive(Clone, Debug)]
pub struct MockBackend {
    /// Models this mock serves; a request for anything else → `ModelNotFound`.
    pub models: Vec<String>,
    /// Fixed generation time reported per inference (ms) — a duration, not a
    /// clock read.
    pub millis: u64,
}

impl MockBackend {
    pub fn new() -> Self {
        MockBackend {
            models: vec!["mock".to_string()],
            millis: 42,
        }
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceBackend for MockBackend {
    fn infer(&self, req: &InferenceRequest) -> Result<InferenceResult, BackendError> {
        if !self.models.iter().any(|m| m == &req.model) {
            return Err(BackendError::ModelNotFound(req.model.clone()));
        }
        let text = format!("echo: {}", req.prompt);
        Ok(InferenceResult {
            prompt_tokens: whitespace_tokens(&req.prompt),
            completion_tokens: whitespace_tokens(&text),
            text,
            millis: self.millis,
        })
    }
}

/// Deterministic token estimate: whitespace-separated words.
fn whitespace_tokens(s: &str) -> u64 {
    s.split_whitespace().count() as u64
}

/// Turn a backend response into transcript-eligible chunks with `seq` starting
/// at 0 (FR-2):
/// * chunk 0 — kind 0 (output-text), content = `text` bytes;
/// * chunk 1 — kind 3 (usage), content = deterministic CBOR map
///   `{1: prompt_tokens, 2: completion_tokens, 4: millis}` (the same key
///   meanings as `ReceiptCore.metrics`).
pub fn result_to_chunks(r: &InferenceResult) -> Vec<Chunk> {
    let usage = encode(&Value::Map(vec![
        (Value::Uint(1), Value::Uint(r.prompt_tokens)),
        (Value::Uint(2), Value::Uint(r.completion_tokens)),
        (Value::Uint(4), Value::Uint(r.millis)),
    ]))
    .expect("small uint map encodes");
    vec![
        Chunk {
            seq: 0,
            kind: ChunkKind::OutputText,
            content: r.text.clone().into_bytes(),
        },
        Chunk {
            seq: 1,
            kind: ChunkKind::UsageUpdate,
            content: usage,
        },
    ]
}

/// FR-2 glue: run a backend and emit the resulting chunks. No ASSIGN message
/// type exists in the scaffold yet, so the request is passed directly; a future
/// session layer will decode it from a live ASSIGN. Returns `Err` (never
/// panics) if the backend fails.
pub fn run_task<B: InferenceBackend>(
    backend: &B,
    req: &InferenceRequest,
) -> Result<Vec<Chunk>, BackendError> {
    Ok(result_to_chunks(&backend.infer(req)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_core::cbor::decode;
    use map_tasks::TranscriptBuilder;

    fn req(model: &str, prompt: &str) -> InferenceRequest {
        InferenceRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            max_tokens: 128,
        }
    }

    #[test]
    fn mock_infer_roundtrip_and_model_not_found() {
        let b = MockBackend::new();
        let r = b.infer(&req("mock", "hello world")).unwrap();
        assert_eq!(r.text, "echo: hello world");
        assert_eq!(r.prompt_tokens, 2);
        assert_eq!(r.completion_tokens, 3); // "echo:" "hello" "world"
        assert_eq!(r.millis, 42);
        // Empty prompt is valid (§9), never an error.
        assert_eq!(b.infer(&req("mock", "")).unwrap().text, "echo: ");
        // Unknown model → ModelNotFound carrying the name.
        assert_eq!(
            b.infer(&req("absent", "x")).unwrap_err(),
            BackendError::ModelNotFound("absent".to_string())
        );
    }

    #[test]
    fn chunks_have_expected_seq_and_kinds() {
        let r = MockBackend::new().infer(&req("mock", "hi")).unwrap();
        let chunks = result_to_chunks(&r);
        assert_eq!(chunks.len(), 2);
        assert_eq!((chunks[0].seq, chunks[0].kind), (0, ChunkKind::OutputText));
        assert_eq!((chunks[1].seq, chunks[1].kind), (1, ChunkKind::UsageUpdate));
        assert_eq!(chunks[0].content, b"echo: hi");
    }

    #[test]
    fn usage_chunk_cbor_decodes_to_three_values() {
        let r = InferenceResult {
            text: "echo: hi".to_string(),
            prompt_tokens: 1,
            completion_tokens: 2,
            millis: 42,
        };
        let usage = &result_to_chunks(&r)[1].content;
        let v = decode(usage).unwrap();
        assert_eq!(v.map_get(1).and_then(|x| x.as_uint()), Some(1));
        assert_eq!(v.map_get(2).and_then(|x| x.as_uint()), Some(2));
        assert_eq!(v.map_get(4).and_then(|x| x.as_uint()), Some(42));
    }

    #[test]
    fn chunks_commit_to_a_stable_transcript_root() {
        let build_root = || {
            let chunks = run_task(&MockBackend::new(), &req("mock", "deterministic")).unwrap();
            let mut tb = TranscriptBuilder::new();
            for c in &chunks {
                tb.push(c);
            }
            tb.root()
        };
        // Only kind-0 output-text is transcript-bearing; kind-3 usage is ignored.
        assert_eq!(build_root(), build_root());
    }
}
