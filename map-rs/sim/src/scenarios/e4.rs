//! E4 (eval/HYPOTHESES.md): metering on vouchers (H5).
//!
//! A task of L Parts (1 output token each) runs under countersigned
//! checkpoints every c Parts, with provider failures at hazard λ per Part
//! (as in E2). Pricing is the layer §9.5 rate table: 2 and 8 micro-units
//! per token in/out, 6-decimal asset.
//!
//! After each countersigned checkpoint the requester signs the next
//! cumulative voucher for that provider. Its amount is
//! `map_meter::billed` of the acknowledged usage, checked against the M-6
//! `voucher_ceiling` (credit = one interval). Its digest is the RAIL-5
//! `digest_mpp_tempo`. The provider keeps a ledger of delivered vouchers
//! whose claimable amount is the largest cumulative. With probability 0.1
//! an earlier voucher is re-delivered.
//!
//! H5-a is asserted on every step; a violation aborts the run:
//! - vouchers are monotone per provider;
//! - re-delivering any earlier voucher leaves the claimable amount
//!   unchanged (idempotence of cumulative vouchers);
//! - every amount round-trips through CBOR as non-negative (M-4);
//! - a provider's exposure (price of delivered Parts minus its voucher) is
//!   ≤ price(c Parts), in every state.
//!
//! H5-b is computed by the analysis from `sig_ops` here and the measured
//! per-operation cost (criterion bench). `checkpoints` and
//! `voucher_digest_bytes` feed the byte-overhead report (checkpoint sizes
//! come from E2's exact encodings).

use crate::config::Params;
use crate::crypto::CryptoMode;
use crate::rng::Rng;
use map_core::cbor::{decode, encode};
use map_core::Usage;
use map_meter::voucher::{digest_mpp_tempo, erc20, voucher_ceiling, within_ceiling};
use map_meter::{billed, Amount, AssetAmount, UNIT_TOKEN_IN, UNIT_TOKEN_OUT};

pub const METRICS: &[&str] = &[
    "max_exposure_atomic",
    "exposure_bound_atomic",
    "vouchers",
    "redeliveries",
    "sig_ops",
    "checkpoints",
    "voucher_digest_bytes",
    "parts",
];

fn usdc(m: u64) -> AssetAmount {
    AssetAmount { asset: erc20("eip155:8453", "0xusdc"), amount: Amount::new(-6, m) }
}

pub fn run(p: &Params, rng: &mut Rng, _mode: CryptoMode) -> Result<Vec<f64>, String> {
    let l = p.u64("L")?;
    let c = p.u64("c")?;
    if c == 0 {
        return Err("E4 needs a finite checkpoint interval".into());
    }
    let lambda = p.f64("lambda_L")? / l as f64;
    let prompt_tokens = p.u64_or("prompt_tokens", 100)?;
    let rates = vec![(UNIT_TOKEN_IN, usdc(2)), (UNIT_TOKEN_OUT, usdc(8))];
    let price_parts = |parts: u64| -> Result<u64, String> {
        billed(1, &rates, &Usage { tokens_in: 0, tokens_out: parts, ttft_ms: 0, wall_ms: 0 })
            .map(|a| a.amount.mantissa)
            .map_err(|e| format!("{e:?}"))
    };
    let bound = price_parts(c)?;
    let credit = usdc(bound);
    let (mut max_exposure, mut vouchers, mut redeliveries, mut sig_ops, mut ck_bytes, mut v_bytes) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut start = 0u64;
    let mut provider_n = 0u64;
    loop {
        provider_n += 1;
        let lifetime = if lambda > 0.0 { rng.exp(lambda) } else { f64::INFINITY };
        let end = if start as f64 + lifetime >= l as f64 { l } else { start + lifetime.floor() as u64 };
        let channel = provider_n.to_le_bytes();
        // This provider's voucher state: (last cumulative, last digest), and
        // its ledger of delivered vouchers (digest, cumulative).
        let mut last: Option<(u64, Vec<u8>)> = None;
        let mut ledger: Vec<(Vec<u8>, u64)> = Vec::new();
        let claimable = |l: &[(Vec<u8>, u64)]| l.iter().map(|(_, m)| *m).max().unwrap_or(0);
        let mut acked = start;
        for delivered in start + 1..=end {
            // Exposure: price of what this provider delivered minus its voucher.
            let paid = last.as_ref().map(|(m, _)| *m).unwrap_or(0);
            let exposure = price_parts(delivered - start)?.saturating_sub(paid);
            if exposure > bound {
                return Err(format!("H5-a violated: exposure {exposure} > price(c) {bound}"));
            }
            max_exposure = max_exposure.max(exposure);
            if (delivered - start) % c == 0 {
                acked = delivered;
                sig_ops += 4; // propose + countersign, each verified by the other side
                ck_bytes += 1;
                // The prompt was billed to the first provider only; this bills
                // the provider's own output, as in E2.
                let usage = Usage {
                    tokens_in: if provider_n == 1 { prompt_tokens } else { 0 },
                    tokens_out: acked - start,
                    ttft_ms: 0,
                    wall_ms: 0,
                };
                let amount = billed(1, &rates, &usage).map_err(|e| format!("{e:?}"))?;
                let ceiling = voucher_ceiling(1, &rates, &usage, &credit).map_err(|e| format!("{e:?}"))?;
                if !within_ceiling(&amount, &ceiling).map_err(|e| format!("{e:?}"))? {
                    return Err("M-6: voucher above its ceiling".into());
                }
                // M-4: the amount round-trips through CBOR as non-negative.
                let enc = encode(&amount.amount.to_value()).map_err(|e| e.to_string())?;
                let back = Amount::from_value(&decode(&enc).map_err(|e| e.to_string())?).map_err(|e| format!("{e:?}"))?;
                if back != amount.amount {
                    return Err("M-4: amount did not round-trip".into());
                }
                let prompt_cost = if provider_n == 1 { 2 * prompt_tokens } else { 0 };
                let output_paid = price_parts(acked - start)?;
                if amount.amount.mantissa != prompt_cost + output_paid {
                    return Err("billing: voucher amount is not prompt + output price".into());
                }
                if let Some((prev, _)) = &last {
                    if output_paid < *prev {
                        return Err("H5-a violated: voucher not monotone".into());
                    }
                }
                let digest = digest_mpp_tempo(&channel, &amount.amount, "map-e4", "tempo");
                v_bytes += digest.len() as u64;
                vouchers += 1;
                last = Some((output_paid, digest.clone()));
                ledger.push((digest, output_paid));
                if rng.bernoulli(0.1) {
                    // Re-deliver a random earlier voucher: cumulative semantics
                    // make this a no-op for the claimable amount.
                    redeliveries += 1;
                    let before = claimable(&ledger);
                    let old = ledger[rng.below(ledger.len())].clone();
                    ledger.push(old);
                    if claimable(&ledger) != before {
                        return Err("H5-a violated: re-delivery changed the claimable amount".into());
                    }
                }
            }
        }
        if end == l {
            sig_ops += 4; // the final receipt
            break;
        }
        start = acked; // countersigned resume (layer §7.4)
    }
    Ok(vec![max_exposure as f64, bound as f64, vouchers as f64, redeliveries as f64, sig_ops as f64, ck_bytes as f64, v_bytes as f64, l as f64])
}
