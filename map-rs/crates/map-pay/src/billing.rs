//! Metered billing arithmetic and watermark verification (pay §4.1).
//!
//! Every billed amount is an EXACT `uint counter × decimal-fraction rate`
//! product — the pay §3 unit codes price a single counted unit, so no rounding
//! rule exists or is permitted. Arithmetic is checked (i128 intermediates);
//! overflow is an error, never a wrap. `verify_watermark` returns the specific
//! core §9 wire code so the runtime (TASK-022) and the conformance suite assert
//! identically.

use crate::codec::{AssetAmount, PayInvoice, Usage, UsageWatermark, UNIT_INVOCATION, UNIT_MS, UNIT_TOKEN_IN, UNIT_TOKEN_OUT};
use crate::Amount;
use map_core::errors::codes;

/// Why a billing computation could not produce an exact amount.
#[derive(Debug, PartialEq, Eq)]
pub enum BillingError {
    /// The rate table lacks a unit the pricing model requires.
    MissingRate(u64),
    /// Rates the model combines disagree on decimal exponent (§3 same-exponent).
    ExponentMismatch,
    /// Rates the model combines are in different assets (§3 same-asset).
    AssetMismatch,
    /// A `counter × mantissa` product or a sum overflowed i64 mantissa range.
    Overflow,
    /// Pricing model has no billing rule here (0 flat is not metered; 4/5+…).
    UnsupportedModel(u64),
}

fn rate<'a>(table: &'a [(u64, AssetAmount)], unit: u64) -> Result<&'a AssetAmount, BillingError> {
    table
        .iter()
        .find(|(u, _)| *u == unit)
        .map(|(_, aa)| aa)
        .ok_or(BillingError::MissingRate(unit))
}

/// Exact `counter × mantissa` in i128 (both fit; product may not fit i64 — the
/// caller validates the final mantissa).
fn term(counter: u64, mantissa: i64) -> Result<i128, BillingError> {
    (counter as i128)
        .checked_mul(mantissa as i128)
        .ok_or(BillingError::Overflow)
}

fn make(exponent: i32, asset: &crate::Asset, total: i128) -> Result<AssetAmount, BillingError> {
    let mantissa = i64::try_from(total).map_err(|_| BillingError::Overflow)?;
    Ok(AssetAmount { asset: asset.clone(), amount: Amount::new(exponent, mantissa) })
}

/// The exact amount owed for `usage` under `model` and `rate_table` (pay §4.1):
/// model 1 per-token `in×rate[1] + out×rate[2]`; model 2 per-ms `wall_ms×rate[3]`;
/// model 3 per-invocation `1×rate[4]`. Models 0 (flat) and unknown are errors —
/// the flat total is not computed from usage.
pub fn billed(model: u64, rate_table: &[(u64, AssetAmount)], usage: &Usage) -> Result<AssetAmount, BillingError> {
    match model {
        1 => {
            let ri = rate(rate_table, UNIT_TOKEN_IN)?;
            let ro = rate(rate_table, UNIT_TOKEN_OUT)?;
            if ri.asset != ro.asset {
                return Err(BillingError::AssetMismatch);
            }
            if ri.amount.exponent != ro.amount.exponent {
                return Err(BillingError::ExponentMismatch);
            }
            let total = term(usage.tokens_in, ri.amount.mantissa)?
                .checked_add(term(usage.tokens_out, ro.amount.mantissa)?)
                .ok_or(BillingError::Overflow)?;
            make(ri.amount.exponent, &ri.asset, total)
        }
        2 => {
            let r = rate(rate_table, UNIT_MS)?;
            make(r.amount.exponent, &r.asset, term(usage.wall_ms, r.amount.mantissa)?)
        }
        3 => {
            let r = rate(rate_table, UNIT_INVOCATION)?;
            make(r.amount.exponent, &r.asset, r.amount.mantissa as i128) // one invocation
        }
        other => Err(BillingError::UnsupportedModel(other)),
    }
}

/// The agreed billing basis a requester verifies each metered invoice against
/// (from the PAY_QUOTE it agreed to, pay §3).
pub struct AgreedTerms {
    pub task: [u8; 32],
    pub pricing_model: u64,
    pub rate_table: Vec<(u64, AssetAmount)>,
    /// PayQuote key 7 (0 postpaid … 3 metered-incremental); watermarks are
    /// carried iff this is 3.
    pub terms: u64,
    /// PayAgree.4 authorized cap (hard ceiling), if the agreement set one.
    pub authorized_cap: Option<AssetAmount>,
    /// PayQuote key 4 (estimated/maximum total). REQUIRED under terms 1/2
    /// (pay §5.2): the prepaid invoice / escrow hold MUST equal it exactly.
    pub quoted_total: Option<AssetAmount>,
}

/// `a <= b` requiring the same asset AND exponent (§3: comparisons are
/// mantissa-only; a mismatch is not comparable and thus a violation).
fn le(a: &AssetAmount, b: &AssetAmount) -> Option<bool> {
    if a.asset != b.asset || a.amount.exponent != b.amount.exponent {
        return None;
    }
    Some(a.amount.mantissa <= b.amount.mantissa)
}

/// Verify a metered invoice's UsageWatermark against the agreement and the last
/// ACCEPTED watermark (pay §4.1 checks 1, 2, 3, 5, 6 — in order, first failure
/// wins). Check 4 (meter consistency vs received kind-3 chunks) needs runtime
/// chunk state and is applied by the caller (TASK-022). Returns the specific
/// core §9 wire code on rejection.
pub fn verify_watermark(agreed: &AgreedTerms, last_accepted: Option<&UsageWatermark>, invoice: &PayInvoice) -> Result<(), u16> {
    // 1. presence: key 4 present iff terms 3.
    let w = match (&invoice.watermark, agreed.terms == 3) {
        (Some(w), true) => w,
        (None, false) => return Ok(()), // non-metered invoice: nothing to verify here
        _ => return Err(codes::INVOICE_WATERMARK_VIOLATION),
    };
    // 2. task binding.
    if w.task != agreed.task {
        return Err(codes::TASK_BINDING_MISMATCH);
    }
    // 3. monotonicity vs the last accepted watermark.
    if let Some(prev) = last_accepted {
        let usage_monotone = w.usage.tokens_in >= prev.usage.tokens_in
            && w.usage.tokens_out >= prev.usage.tokens_out
            && w.usage.ttft_ms >= prev.usage.ttft_ms
            && w.usage.wall_ms >= prev.usage.wall_ms;
        let billed_monotone = le(&prev.cumulative_billed, &w.cumulative_billed).unwrap_or(false);
        if w.through_seq < prev.through_seq || !usage_monotone || !billed_monotone {
            return Err(codes::INVOICE_WATERMARK_VIOLATION);
        }
    }
    // 5. arithmetic: cumulative-billed == rate × usage, and the invoice amount is
    //    the delta since the last accepted watermark.
    let computed = match billed(agreed.pricing_model, &agreed.rate_table, &w.usage) {
        Ok(a) => a,
        Err(_) => return Err(codes::INVOICE_WATERMARK_VIOLATION),
    };
    if computed != w.cumulative_billed {
        return Err(codes::INVOICE_WATERMARK_VIOLATION);
    }
    let prev_billed = last_accepted.map(|p| p.cumulative_billed.amount.mantissa).unwrap_or(0);
    // delta amount uses the shared exponent/asset of cumulative-billed.
    let expected_delta = w
        .cumulative_billed
        .amount
        .mantissa
        .checked_sub(prev_billed)
        .filter(|d| *d >= 0);
    let amount_ok = expected_delta.is_some_and(|d| {
        invoice.amount.asset == w.cumulative_billed.asset
            && invoice.amount.amount.exponent == w.cumulative_billed.amount.exponent
            && invoice.amount.amount.mantissa == d
    });
    if !amount_ok {
        return Err(codes::INVOICE_WATERMARK_VIOLATION);
    }
    // 6. cap: cumulative-billed does not exceed the authorized cap.
    if let Some(cap) = &agreed.authorized_cap {
        if le(&w.cumulative_billed, cap) != Some(true) {
            return Err(codes::CAP_EXCEEDED);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Asset;

    fn micro(mantissa: i64) -> AssetAmount {
        AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-6, mantissa) }
    }
    /// The canonical TV-016 rate table: 3000 µUSD/token-in, 5000 µUSD/token-out.
    fn rates() -> Vec<(u64, AssetAmount)> {
        vec![(UNIT_TOKEN_IN, micro(3000)), (UNIT_TOKEN_OUT, micro(5000))]
    }
    fn usage() -> Usage {
        Usage { tokens_in: 1523, tokens_out: 812, ttft_ms: 431, wall_ms: 42000 }
    }

    #[test]
    fn model1_exact_product_matches_frozen_constant() {
        // 1523×3000 + 812×5000 = 4_569_000 + 4_060_000 = 8_629_000 (pay §4.1).
        assert_eq!(billed(1, &rates(), &usage()).unwrap(), micro(8_629_000));
    }

    #[test]
    fn model2_per_millisecond_and_model3_per_invocation() {
        let r2 = vec![(UNIT_MS, micro(2))]; // 2 µUSD/ms
        assert_eq!(billed(2, &r2, &usage()).unwrap(), micro(84_000)); // 42000×2
        let r3 = vec![(UNIT_INVOCATION, micro(500_000))];
        assert_eq!(billed(3, &r3, &usage()).unwrap(), micro(500_000)); // 1×
    }

    #[test]
    fn zero_usage_bills_zero() {
        assert_eq!(billed(1, &rates(), &Usage::default()).unwrap(), micro(0));
    }

    #[test]
    fn missing_rate_and_bad_model_error() {
        assert_eq!(billed(1, &[(UNIT_TOKEN_IN, micro(3000))], &usage()), Err(BillingError::MissingRate(UNIT_TOKEN_OUT)));
        assert_eq!(billed(0, &rates(), &usage()), Err(BillingError::UnsupportedModel(0)));
    }

    #[test]
    fn mixed_exponent_rates_rejected() {
        let bad = vec![(UNIT_TOKEN_IN, micro(3000)), (UNIT_TOKEN_OUT, AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, 5) })];
        assert_eq!(billed(1, &bad, &usage()), Err(BillingError::ExponentMismatch));
    }

    #[test]
    fn overflow_is_checked_not_wrapped() {
        let huge = vec![(UNIT_MS, micro(i64::MAX))];
        let u = Usage { wall_ms: u64::MAX, ..Default::default() };
        assert_eq!(billed(2, &huge, &u), Err(BillingError::Overflow));
    }

    fn agreed(cap: Option<AssetAmount>) -> AgreedTerms {
        AgreedTerms { task: [0xAB; 32], pricing_model: 1, rate_table: rates(), terms: 3, authorized_cap: cap, quoted_total: None }
    }
    fn watermark(billed_mantissa: i64) -> UsageWatermark {
        UsageWatermark { task: [0xAB; 32], through_seq: 8, usage: usage(), cumulative_billed: micro(billed_mantissa) }
    }
    fn invoice(amount: i64, w: Option<UsageWatermark>) -> PayInvoice {
        PayInvoice { invoice_id: [1; 16], quote_id: [2; 16], amount: micro(amount), watermark: w, method_payload: vec![], timestamp_ms: 1, phase: None }
    }

    #[test]
    fn verify_accepts_correct_first_invoice() {
        // Amount == cumulative-billed for the first invoice (no prior watermark).
        let inv = invoice(8_629_000, Some(watermark(8_629_000)));
        assert_eq!(verify_watermark(&agreed(Some(micro(10_000_000))), None, &inv), Ok(()));
    }

    #[test]
    fn verify_rejects_off_by_one_arithmetic_6009() {
        // NV-010 shape: claimed cumulative-billed 8_629_001 but rate×usage = 8_629_000.
        let inv = invoice(8_629_000, Some(watermark(8_629_001)));
        assert_eq!(verify_watermark(&agreed(None), None, &inv), Err(codes::INVOICE_WATERMARK_VIOLATION));
    }

    #[test]
    fn verify_rejects_task_binding_6007() {
        let mut w = watermark(8_629_000);
        w.task = [0xCD; 32];
        assert_eq!(verify_watermark(&agreed(None), None, &invoice(8_629_000, Some(w))), Err(codes::TASK_BINDING_MISMATCH));
    }

    #[test]
    fn verify_rejects_over_cap_6005() {
        let inv = invoice(8_629_000, Some(watermark(8_629_000)));
        assert_eq!(verify_watermark(&agreed(Some(micro(8_000_000))), None, &inv), Err(codes::CAP_EXCEEDED));
    }

    #[test]
    fn verify_rejects_missing_watermark_under_terms3_6009() {
        assert_eq!(verify_watermark(&agreed(None), None, &invoice(1, None)), Err(codes::INVOICE_WATERMARK_VIOLATION));
    }

    #[test]
    fn verify_delta_and_monotonicity_across_invoices() {
        // Second invoice: cumulative 8_629_000, prior 5_000_000 → delta amount 3_629_000.
        let prev = UsageWatermark { task: [0xAB; 32], through_seq: 4, usage: Usage { tokens_in: 1000, tokens_out: 400, ttft_ms: 431, wall_ms: 20000 }, cumulative_billed: micro(5_000_000) };
        let inv = invoice(3_629_000, Some(watermark(8_629_000)));
        assert_eq!(verify_watermark(&agreed(Some(micro(10_000_000))), Some(&prev), &inv), Ok(()));
        // A watermark that goes backwards in usage is a monotonicity violation.
        let mut back = watermark(8_629_000);
        back.usage.tokens_in = 999; // < prev.tokens_in (1000), but breaks the rate identity too
        let inv_back = invoice(3_629_000, Some(back));
        assert_eq!(verify_watermark(&agreed(None), Some(&prev), &inv_back), Err(codes::INVOICE_WATERMARK_VIOLATION));
    }
}
