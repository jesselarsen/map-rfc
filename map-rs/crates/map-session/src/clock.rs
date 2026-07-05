//! Injected time and message-id sources (FR-10). Production impls read the OS
//! (allowed — neither is consensus-relevant); every test uses a fixed impl so
//! no test reads the wall clock or OS RNG.

use std::cell::Cell;

/// Millisecond wall-clock source for envelope timestamps and the replay window.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// 16-byte message-id / session-id source. Values MUST be unique per call
/// (duplicates would trip the replay cache).
pub trait MsgIdSource {
    fn next_msg_id(&mut self) -> [u8; 16];
}

/// OS wall clock. Not consensus-relevant, so `SystemTime` is acceptable here.
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_millis() as u64
    }
}

/// OS-random message ids for production sessions.
pub struct RandomMsgIds;
impl MsgIdSource for RandomMsgIds {
    fn next_msg_id(&mut self) -> [u8; 16] {
        let mut b = [0u8; 16];
        getrandom::getrandom(&mut b).expect("OS RNG unavailable");
        b
    }
}

/// A constant clock for tests where the timestamp is irrelevant.
pub struct FixedClock {
    pub ms: u64,
}
impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.ms
    }
}

/// A monotonically advancing clock for tests that need distinct timestamps;
/// each `now_ms()` returns the current value then advances by `step`.
pub struct SteppingClock {
    now: Cell<u64>,
    step: u64,
}
impl SteppingClock {
    pub fn new(start: u64, step: u64) -> Self {
        SteppingClock { now: Cell::new(start), step }
    }
}
impl Clock for SteppingClock {
    fn now_ms(&self) -> u64 {
        let v = self.now.get();
        self.now.set(v + self.step);
        v
    }
}

/// Deterministic, unique message ids: a big-endian counter in the low 8 bytes.
pub struct CountingMsgIds {
    next: u64,
}
impl CountingMsgIds {
    pub fn new() -> Self {
        CountingMsgIds { next: 0 }
    }
    /// Start from a nonzero base so two peers' id streams can be made disjoint
    /// in a test if desired (not required — ids are keyed per sender).
    pub fn starting_at(base: u64) -> Self {
        CountingMsgIds { next: base }
    }
}
impl Default for CountingMsgIds {
    fn default() -> Self {
        Self::new()
    }
}
impl MsgIdSource for CountingMsgIds {
    fn next_msg_id(&mut self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[8..].copy_from_slice(&self.next.to_be_bytes());
        self.next += 1;
        b
    }
}
