//! The deterministic demo workload and the usage both parties derive from
//! it. A real deployment meters a real model (e.g. `ollama-provider`); the
//! demo needs a generator that a successor provider reproduces exactly, so
//! resumption-by-replay (layer §7.4) is observable, and usage that requester
//! and provider can each compute independently, so the tolerance check
//! (layer §6.5) compares like with like.

use map_core::Usage;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// `n` output words derived from the prompt: `w001-<6 hex>` and so on.
pub fn generate(prompt: &str, n: usize) -> Vec<String> {
    (1..=n)
        .map(|i| {
            let h = Sha256::digest(format!("{prompt}\u{0}{i}").as_bytes());
            format!("w{i:03}-{:02x}{:02x}{:02x}", h[0], h[1], h[2])
        })
        .collect()
}

/// One input token per prompt word, one output token per delivered Part.
pub fn observed_usage(prompt_words: u64, parts: u64) -> Usage {
    Usage { tokens_in: prompt_words, tokens_out: parts, ttft_ms: 0, wall_ms: 0 }
}

pub fn prompt_words(prompt: &str) -> u64 {
    prompt.split_whitespace().count() as u64
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
