//! Real Ollama HTTP backend (TASK-013 FR-2/FR-3). Behind the `real-backend`
//! feature. Talks plain HTTP to a local Ollama server's `/api/generate` with
//! `stream: false` (the simplest correct mapping; token streaming is future
//! work, §12).
//!
//! Zone hygiene (§11): error strings never include prompt or output content —
//! only transport/model diagnostics. A non-loopback `base_url` logs one loud
//! warning at construction (payloads would leave the machine, Zone ≥2).

use crate::{BackendError, InferenceBackend, InferenceRequest, InferenceResult};
use std::time::{Duration, Instant};

/// A client for a local Ollama server. The model is taken from each
/// [`InferenceRequest`] (which the provider fills from its configured model),
/// so it is not duplicated on the backend.
pub struct OllamaBackend {
    pub base_url: String,
    pub timeout: Duration,
}

impl OllamaBackend {
    /// Local Ollama on loopback, 120 s timeout (NFR default).
    pub fn local() -> Self {
        OllamaBackend {
            base_url: "http://127.0.0.1:11434".to_string(),
            timeout: Duration::from_secs(120),
        }
    }

    /// Custom endpoint. Warns once if `base_url` is not loopback (§11).
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Self {
        let base_url = base_url.into();
        if !is_loopback(&base_url) {
            eprintln!(
                "WARNING: OllamaBackend base_url is not loopback ({base_url}) — \
                 prompts and outputs leave this machine (treat as Zone >= 2)."
            );
        }
        OllamaBackend { base_url, timeout }
    }

    /// Build the `/api/generate` request body (FR-2). `num_predict` is omitted
    /// when `max_tokens == 0` (§9).
    pub(crate) fn request_body(req: &InferenceRequest) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": req.model,
            "prompt": req.prompt,
            "stream": false,
        });
        if req.max_tokens > 0 {
            body["options"] = serde_json::json!({ "num_predict": req.max_tokens });
        }
        body
    }
}

fn is_loopback(url: &str) -> bool {
    url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost") || url.starts_with("http://[::1]")
}

/// Map a decoded `/api/generate` JSON body to an [`InferenceResult`] (FR-2).
/// `done` MUST be true (a `stream:false` reply with `done:false` is a failure,
/// §9). `total_duration` is nanoseconds → milliseconds.
pub(crate) fn map_response(body: &str) -> Result<InferenceResult, BackendError> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|_| BackendError::Failed("invalid JSON response".into()))?;
    if v.get("error").and_then(|e| e.as_str()).is_some() {
        // Ollama returns {"error": "..."} for e.g. an unknown model on 200/404.
        return Err(BackendError::ModelNotFound(
            v.get("model").and_then(|m| m.as_str()).unwrap_or("unknown").to_string(),
        ));
    }
    if v.get("done").and_then(|d| d.as_bool()) != Some(true) {
        return Err(BackendError::Failed("response not done (stream:false)".into()));
    }
    let text = v.get("response").and_then(|r| r.as_str()).unwrap_or("").to_string();
    let prompt_tokens = v.get("prompt_eval_count").and_then(|n| n.as_u64()).unwrap_or(0);
    let completion_tokens = v.get("eval_count").and_then(|n| n.as_u64()).unwrap_or(0);
    let millis = v.get("total_duration").and_then(|n| n.as_u64()).unwrap_or(0) / 1_000_000;
    Ok(InferenceResult { text, prompt_tokens, completion_tokens, millis })
}

impl InferenceBackend for OllamaBackend {
    fn infer(&self, req: &InferenceRequest) -> Result<InferenceResult, BackendError> {
        let url = format!("{}/api/generate", self.base_url.trim_end_matches('/'));
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let started = Instant::now();
        // Serialize with serde_json directly (rather than ureq's `json` feature,
        // which we leave off to keep the tree minimal).
        let body = Self::request_body(req).to_string();
        match agent.post(&url).set("Content-Type", "application/json").send_string(&body) {
            Ok(resp) => {
                let body = resp
                    .into_string()
                    .map_err(|_| BackendError::Failed("could not read response body".into()))?;
                map_response(&body)
            }
            // 404 with a model error is the model-not-found signal (FR-3).
            Err(ureq::Error::Status(404, _)) => Err(BackendError::ModelNotFound(req.model.clone())),
            Err(ureq::Error::Status(code, _)) => Err(BackendError::Failed(format!("ollama HTTP {code}"))),
            // Connection refused / DNS / timeout → Unavailable, with elapsed ms
            // (FR-3/FR-10). The message is a transport diagnostic — no prompt.
            Err(ureq::Error::Transport(_)) => Err(BackendError::Unavailable(format!(
                "ollama unreachable at {} after {}ms",
                self.base_url,
                started.elapsed().as_millis()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> InferenceRequest {
        InferenceRequest { model: "llama3:8b".into(), prompt: "why is the sky blue?".into(), max_tokens: 64 }
    }

    #[test]
    fn request_body_shape_and_num_predict_omitted_at_zero() {
        let b = OllamaBackend::request_body(&req());
        assert_eq!(b["model"], "llama3:8b");
        assert_eq!(b["prompt"], "why is the sky blue?");
        assert_eq!(b["stream"], false);
        assert_eq!(b["options"]["num_predict"], 64);
        // max_tokens 0 → no options (§9).
        let mut r0 = req();
        r0.max_tokens = 0;
        assert!(OllamaBackend::request_body(&r0).get("options").is_none());
    }

    #[test]
    fn maps_a_successful_generate_response() {
        // Canned /api/generate body (FR-2 field mapping).
        let body = r#"{"model":"llama3:8b","response":"The sky is blue because of Rayleigh scattering.",
            "done":true,"prompt_eval_count":7,"eval_count":11,"total_duration":4200000000}"#;
        let r = map_response(body).unwrap();
        assert_eq!(r.text, "The sky is blue because of Rayleigh scattering.");
        assert_eq!(r.prompt_tokens, 7);
        assert_eq!(r.completion_tokens, 11);
        assert_eq!(r.millis, 4200); // 4.2e9 ns → 4200 ms
    }

    #[test]
    fn not_done_is_failed() {
        let body = r#"{"response":"partial","done":false}"#;
        assert_eq!(map_response(body), Err(BackendError::Failed("response not done (stream:false)".into())));
    }

    #[test]
    fn error_body_is_model_not_found() {
        let body = r#"{"error":"model 'nope' not found","model":"nope"}"#;
        assert_eq!(map_response(body), Err(BackendError::ModelNotFound("nope".into())));
    }

    #[test]
    fn garbage_body_is_failed_not_panic() {
        assert_eq!(map_response("not json at all"), Err(BackendError::Failed("invalid JSON response".into())));
    }

    #[test]
    fn no_error_string_contains_prompt() {
        // Every error variant a caller can produce here excludes prompt text.
        for e in [
            map_response("nope").unwrap_err(),
            map_response(r#"{"done":false}"#).unwrap_err(),
            BackendError::Unavailable("ollama unreachable at http://127.0.0.1:11434 after 5ms".into()),
        ] {
            let s = format!("{e:?}");
            assert!(!s.contains("why is the sky blue"), "error leaked prompt: {s}");
        }
    }

    /// Live smoke test — needs a running Ollama with the model. Ignored + gated.
    /// Run: `cargo test -p ollama-provider --features real-backend -- --ignored`
    #[test]
    #[ignore]
    fn live_generate_against_local_ollama() {
        let backend = OllamaBackend::local();
        let r = backend
            .infer(&InferenceRequest { model: "llama3.2".into(), prompt: "Say hi in one word.".into(), max_tokens: 16 })
            .expect("local ollama must be running with the model pulled");
        assert!(!r.text.is_empty());
    }
}
