//! TV-023: receipt admissibility (layer R-2..R-4, §8.2). Each frozen receipt
//! is decoded, verified (R-1) and applied to a fresh projection at the
//! vector's evaluation time; the outcome must equal the pinned verdict.

use crate::from_hex;
use crate::vectors3::{TV023, TV023_NOW_MS, TV023_SKEW_MS};
use map_trust::{ProjParams, Receipt, Skip, TrustProjection};

fn verdict(r: Result<(), Skip>) -> &'static str {
    match r {
        Ok(()) => "count",
        Err(Skip::SelfDealing) => "self-dealing",
        Err(Skip::FutureDated) => "future-dated",
        Err(Skip::NotAdmissible) => "not-admissible",
        Err(Skip::Cancelled) => "cancelled",
        Err(Skip::ZeroWeight) => "zero-weight",
    }
}

pub fn case_tv023_admissibility() -> Result<(), String> {
    let params = ProjParams { skew_ms: TV023_SKEW_MS, ..ProjParams::default() };
    for (i, (want, hex)) in TV023.iter().enumerate() {
        let r = Receipt::decode_bytes(&from_hex(hex)?).map_err(|e| format!("case {:02}: decode: {e}", i + 1))?;
        let v = r.verify().map_err(|e| format!("case {:02}: verify: {e}", i + 1))?;
        let got = verdict(TrustProjection::new(params).apply(&v, TV023_NOW_MS, 1.0));
        if got != *want {
            return Err(format!("case {:02}: got {got}, want {want}", i + 1));
        }
    }
    Ok(())
}
