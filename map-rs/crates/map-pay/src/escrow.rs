//! Prepaid (terms 1) and escrow (terms 2) invoice verification (pay §5.2).
//!
//! The §5.2.3 requester checks as a pure function, mirroring
//! [`crate::billing::verify_watermark`]'s shape: specific core §9 wire code
//! out, checks IN ORDER, first failure wins. This module only verifies; the
//! flow sequencing lives in map-taskrun.
//!
//! Check 2 of §5.2.3 (task binding) is a no-op at this layer by construction:
//! terms-1/2 invoices never carry a UsageWatermark (the only invoice field
//! with a TaskID), so binding is enforced at quote time (PayQuote key 9,
//! pay §3) and via the quote-id the caller has already matched.

use crate::billing::{billed, AgreedTerms};
use crate::codec::{AssetAmount, PayInvoice, Usage, PHASE_HOLD, PHASE_RELEASE, TERMS_ESCROW, TERMS_PREPAID};
use map_core::errors::codes;

/// The requester-side escrow state a terms-2 invoice is verified against
/// (pay §5.2.3). All fields are caller-supplied runtime state.
#[derive(Default)]
pub struct EscrowContext {
    /// The accepted hold's amount (None until a hold invoice was accepted).
    pub held: Option<AssetAmount>,
    /// COMPLETE received AND its transcript-root verified (tasks §4) — the
    /// canonical release trigger. A release invoice before this is 6010.
    pub complete_verified: bool,
    /// If the task ended by CANCEL: the last countersigned checkpoint's usage
    /// (tasks §8.2 key 5) — the §4.2 cancel anchor. The release amount must
    /// then equal the §4.1 formula applied to exactly this usage.
    pub cancel_checkpoint_usage: Option<Usage>,
}

/// `a == b` requiring the same asset AND exponent (pay §3: mantissa-only
/// comparison; a mismatch is never converted).
fn eq(a: &AssetAmount, b: &AssetAmount) -> bool {
    a.asset == b.asset && a.amount.exponent == b.amount.exponent && a.amount.mantissa == b.amount.mantissa
}
fn le(a: &AssetAmount, b: &AssetAmount) -> bool {
    a.asset == b.asset && a.amount.exponent == b.amount.exponent && a.amount.mantissa <= b.amount.mantissa
}

/// Verify a terms-1 (prepaid) or terms-2 (escrow) invoice against the
/// agreement and escrow context — pay §5.2.2 / §5.2.3, in order, first
/// failure wins. Returns the specific core §9 wire code on rejection.
pub fn verify_escrow_invoice(agreed: &AgreedTerms, ctx: &EscrowContext, invoice: &PayInvoice) -> Result<(), u16> {
    // Watermark presence: key 4 present iff terms 3 (pay §4.1 check 1) — a
    // terms-1/2 invoice carrying one is a watermark violation, not an escrow
    // violation.
    if invoice.watermark.is_some() {
        return Err(codes::INVOICE_WATERMARK_VIOLATION);
    }
    match agreed.terms {
        TERMS_PREPAID => {
            // §5.2.2: no phase key; the single invoice equals the quoted total.
            if invoice.phase.is_some() {
                return Err(codes::ESCROW_VIOLATION);
            }
            let total = agreed.quoted_total.as_ref().ok_or(codes::ESCROW_VIOLATION)?;
            if !eq(&invoice.amount, total) {
                return Err(codes::ESCROW_VIOLATION);
            }
            check_cap(agreed, &invoice.amount)
        }
        TERMS_ESCROW => match invoice.phase {
            // 1. phase presence (§5.2.1) — else 6010. (Codec already bounds
            //    the value to {1, 2}; absence is the reachable violation.)
            None => Err(codes::ESCROW_VIOLATION),
            Some(PHASE_HOLD) => {
                // 3. hold: amount == quote key 4 total; one hold per agreement.
                if ctx.held.is_some() {
                    return Err(codes::ESCROW_VIOLATION);
                }
                let total = agreed.quoted_total.as_ref().ok_or(codes::ESCROW_VIOLATION)?;
                if !eq(&invoice.amount, total) {
                    return Err(codes::ESCROW_VIOLATION);
                }
                // 6. cap: the hold is the committed outflow.
                check_cap(agreed, &invoice.amount)
            }
            Some(PHASE_RELEASE) => {
                let held = ctx.held.as_ref().ok_or(codes::ESCROW_VIOLATION)?;
                // 4. release only after the canonical trigger (COMPLETE +
                //    verified transcript-root), or a CANCEL checkpoint anchor.
                if !ctx.complete_verified && ctx.cancel_checkpoint_usage.is_none() {
                    return Err(codes::ESCROW_VIOLATION);
                }
                // 5. release amount ≤ held; and on CANCEL (without a verified
                //    COMPLETE, which wins if both are set — §5.2.3), exactly
                //    the §4.1 formula at the countersigned checkpoint.
                if !le(&invoice.amount, held) {
                    return Err(codes::ESCROW_VIOLATION);
                }
                if !ctx.complete_verified {
                    let usage = ctx.cancel_checkpoint_usage.as_ref().expect("checked above");
                    let owed = billed(agreed.pricing_model, &agreed.rate_table, usage)
                        .map_err(|_| codes::ESCROW_VIOLATION)?;
                    if !eq(&invoice.amount, &owed) {
                        return Err(codes::ESCROW_VIOLATION);
                    }
                }
                // 6. cap: release ≤ held ≤ cap already, but check directly for
                //    agreements capped below the hold (defense in depth).
                check_cap(agreed, &invoice.amount)
            }
            // Unreachable through the codec (BadField(7)), kept fail-closed.
            Some(_) => Err(codes::ESCROW_VIOLATION),
        },
        // Not a terms-1/2 agreement: a phase key is a violation; otherwise
        // this verifier has nothing to say (postpaid/metered paths own it).
        _ => {
            if invoice.phase.is_some() {
                return Err(codes::ESCROW_VIOLATION);
            }
            Ok(())
        }
    }
}

fn check_cap(agreed: &AgreedTerms, amount: &AssetAmount) -> Result<(), u16> {
    match &agreed.authorized_cap {
        Some(cap) if !le(amount, cap) => Err(codes::CAP_EXCEEDED),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{UNIT_TOKEN_IN, UNIT_TOKEN_OUT};
    use crate::{Amount, Asset};

    fn usd(mantissa: i64) -> AssetAmount {
        AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, mantissa) }
    }
    fn agreed(terms: u64, total: Option<AssetAmount>, cap: Option<AssetAmount>) -> AgreedTerms {
        AgreedTerms {
            task: [0xAB; 32],
            pricing_model: 1,
            rate_table: vec![(UNIT_TOKEN_IN, usd(3)), (UNIT_TOKEN_OUT, usd(5))],
            terms,
            authorized_cap: cap,
            quoted_total: total,
        }
    }
    fn invoice(amount: AssetAmount, phase: Option<u64>) -> PayInvoice {
        PayInvoice {
            invoice_id: [1; 16],
            quote_id: [2; 16],
            amount,
            watermark: None,
            method_payload: vec![],
            timestamp_ms: 1,
            phase,
        }
    }

    #[test]
    fn prepaid_exact_total_accepted_wrong_amount_6010() {
        let a = agreed(TERMS_PREPAID, Some(usd(500)), None);
        assert_eq!(verify_escrow_invoice(&a, &EscrowContext::default(), &invoice(usd(500), None)), Ok(()));
        assert_eq!(
            verify_escrow_invoice(&a, &EscrowContext::default(), &invoice(usd(499), None)),
            Err(codes::ESCROW_VIOLATION)
        );
        // Exponent mismatch is never converted (pay §3).
        let off_exp = AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-3, 5000) };
        assert_eq!(
            verify_escrow_invoice(&a, &EscrowContext::default(), &invoice(off_exp, None)),
            Err(codes::ESCROW_VIOLATION)
        );
    }

    #[test]
    fn phase_presence_iff_terms_2_6010() {
        // Phase under terms 1 / terms 0 → 6010; missing phase under terms 2 → 6010.
        let a1 = agreed(TERMS_PREPAID, Some(usd(500)), None);
        assert_eq!(
            verify_escrow_invoice(&a1, &EscrowContext::default(), &invoice(usd(500), Some(PHASE_HOLD))),
            Err(codes::ESCROW_VIOLATION)
        );
        let a0 = agreed(0, None, None);
        assert_eq!(
            verify_escrow_invoice(&a0, &EscrowContext::default(), &invoice(usd(500), Some(PHASE_HOLD))),
            Err(codes::ESCROW_VIOLATION)
        );
        assert_eq!(verify_escrow_invoice(&a0, &EscrowContext::default(), &invoice(usd(500), None)), Ok(()));
        let a2 = agreed(TERMS_ESCROW, Some(usd(500)), None);
        assert_eq!(
            verify_escrow_invoice(&a2, &EscrowContext::default(), &invoice(usd(500), None)),
            Err(codes::ESCROW_VIOLATION)
        );
    }

    #[test]
    fn hold_rules_amount_duplicate_cap() {
        let a = agreed(TERMS_ESCROW, Some(usd(500)), Some(usd(500)));
        let ctx = EscrowContext::default();
        assert_eq!(verify_escrow_invoice(&a, &ctx, &invoice(usd(500), Some(PHASE_HOLD))), Ok(()));
        assert_eq!(
            verify_escrow_invoice(&a, &ctx, &invoice(usd(400), Some(PHASE_HOLD))),
            Err(codes::ESCROW_VIOLATION)
        );
        // Duplicate hold (one hold per agreement).
        let held = EscrowContext { held: Some(usd(500)), ..Default::default() };
        assert_eq!(
            verify_escrow_invoice(&a, &held, &invoice(usd(500), Some(PHASE_HOLD))),
            Err(codes::ESCROW_VIOLATION)
        );
        // Cap below the quoted total → 6005.
        let capped = agreed(TERMS_ESCROW, Some(usd(500)), Some(usd(400)));
        assert_eq!(
            verify_escrow_invoice(&capped, &ctx, &invoice(usd(500), Some(PHASE_HOLD))),
            Err(codes::CAP_EXCEEDED)
        );
    }

    #[test]
    fn release_requires_trigger_and_bound() {
        let a = agreed(TERMS_ESCROW, Some(usd(500)), None);
        let no_trigger = EscrowContext { held: Some(usd(500)), ..Default::default() };
        // Premature release (no COMPLETE, no cancel anchor) → 6010.
        assert_eq!(
            verify_escrow_invoice(&a, &no_trigger, &invoice(usd(300), Some(PHASE_RELEASE))),
            Err(codes::ESCROW_VIOLATION)
        );
        let ok = EscrowContext { held: Some(usd(500)), complete_verified: true, ..Default::default() };
        assert_eq!(verify_escrow_invoice(&a, &ok, &invoice(usd(300), Some(PHASE_RELEASE))), Ok(()));
        // Oversized release (> held) → 6010.
        assert_eq!(
            verify_escrow_invoice(&a, &ok, &invoice(usd(501), Some(PHASE_RELEASE))),
            Err(codes::ESCROW_VIOLATION)
        );
        // Release with no hold at all → 6010.
        let no_hold = EscrowContext { complete_verified: true, ..Default::default() };
        assert_eq!(
            verify_escrow_invoice(&a, &no_hold, &invoice(usd(300), Some(PHASE_RELEASE))),
            Err(codes::ESCROW_VIOLATION)
        );
    }

    #[test]
    fn cancel_anchors_release_to_checkpoint_billing() {
        // model 1: 10×3 + 20×5 = 130 (cents).
        let a = agreed(TERMS_ESCROW, Some(usd(500)), None);
        let usage = Usage { tokens_in: 10, tokens_out: 20, ttft_ms: 1, wall_ms: 2 };
        let cancel = EscrowContext { held: Some(usd(500)), complete_verified: false, cancel_checkpoint_usage: Some(usage) };
        assert_eq!(verify_escrow_invoice(&a, &cancel, &invoice(usd(130), Some(PHASE_RELEASE))), Ok(()));
        assert_eq!(
            verify_escrow_invoice(&a, &cancel, &invoice(usd(131), Some(PHASE_RELEASE))),
            Err(codes::ESCROW_VIOLATION)
        );
        // COMPLETE wins over a cancel anchor when both are set (§5.2.3).
        let both = EscrowContext { held: Some(usd(500)), complete_verified: true, cancel_checkpoint_usage: Some(usage) };
        assert_eq!(verify_escrow_invoice(&a, &both, &invoice(usd(500), Some(PHASE_RELEASE))), Ok(()));
    }

    #[test]
    fn terms_2_invoice_with_watermark_is_6009() {
        use crate::codec::UsageWatermark;
        let a = agreed(TERMS_ESCROW, Some(usd(500)), None);
        let mut inv = invoice(usd(500), Some(PHASE_HOLD));
        inv.watermark = Some(UsageWatermark {
            task: [0xAB; 32],
            through_seq: 1,
            usage: Usage::default(),
            cumulative_billed: usd(0),
        });
        assert_eq!(
            verify_escrow_invoice(&a, &EscrowContext::default(), &inv),
            Err(codes::INVOICE_WATERMARK_VIOLATION)
        );
    }
}
