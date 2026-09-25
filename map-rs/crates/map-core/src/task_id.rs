//! `TaskID`: the 32-byte task identifier shared by receipts (layer §6,
//! ReceiptCore key 1) and checkpoints (layer §7, CheckpointCore key 1). How a
//! binding derives it is binding-specific (binding-a2a.md §4); this type only
//! carries the bytes. It lives in map-core so that the trust and transcript
//! crates share it without depending on each other.

/// 32-byte task identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TaskId(pub [u8; 32]);

impl TaskId {
    pub fn random() -> Self {
        let mut b = [0u8; 32];
        getrandom::getrandom(&mut b).expect("OS RNG unavailable");
        TaskId(b)
    }
    pub fn from_slice(s: &[u8]) -> Option<Self> {
        s.try_into().ok().map(TaskId)
    }
}
