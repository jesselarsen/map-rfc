//! `Usage`: cumulative usage counters since task start (spec/layer.md §6.1),
//! shared by CheckpointCore key 6 (map-transcript) and the metering price
//! function (map-meter). Wire form: `{1 tokens-in, 2 tokens-out, 3 ttft-ms,
//! 4 wall-ms}`, all uint, all REQUIRED.

use crate::cbor::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Usage {
    pub tokens_in: u64,  // 1
    pub tokens_out: u64, // 2
    pub ttft_ms: u64,    // 3
    pub wall_ms: u64,    // 4
}

impl Usage {
    pub fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Uint(1), Value::Uint(self.tokens_in)),
            (Value::Uint(2), Value::Uint(self.tokens_out)),
            (Value::Uint(3), Value::Uint(self.ttft_ms)),
            (Value::Uint(4), Value::Uint(self.wall_ms)),
        ])
    }

    /// Decode; on failure returns the first key that is missing or not a uint.
    pub fn from_value(v: &Value) -> Result<Usage, u64> {
        let uint = |k: u64| v.map_get(k).and_then(|x| x.as_uint()).ok_or(k);
        Ok(Usage {
            tokens_in: uint(1)?,
            tokens_out: uint(2)?,
            ttft_ms: uint(3)?,
            wall_ms: uint(4)?,
        })
    }

    /// Component-wise `self >= prev` (layer C-4 / M-2 monotonicity).
    pub fn dominates(&self, prev: &Usage) -> bool {
        self.tokens_in >= prev.tokens_in
            && self.tokens_out >= prev.tokens_out
            && self.ttft_ms >= prev.ttft_ms
            && self.wall_ms >= prev.wall_ms
    }
}
