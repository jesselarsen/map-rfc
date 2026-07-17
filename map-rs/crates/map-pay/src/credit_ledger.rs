//! The `pay:credit-ledger` settlement method (pay §5.1): a ledger-operator-
//! signed entry is the proof. The operator is a TRUSTED third party BY DESIGN
//! for this method — the payee prices that trust in (a private-mesh ledger, not
//! a public rail); MAP surfaces the tradeoff, it does not hide it.

use crate::codec::AssetAmount;
use crate::PayError;
use map_core::cbor::{decode, encode, Value};
use map_identity::{verify, AgentId, Keypair};
use std::collections::HashMap;

pub const CREDIT_LEDGER_METHOD: &str = "pay:credit-ledger";
const ENTRY_DOMAIN: &str = "MAPLedgerEntry";
// Escrow extension domains (pay §5.2.4).
const HOLD_DOMAIN: &str = "MAPLedgerHold";
const RELEASE_AUTH_DOMAIN: &str = "MAPLedgerReleaseAuth";
const REFUND_AUTH_DOMAIN: &str = "MAPLedgerRefundAuth";
const RELEASE_DOMAIN: &str = "MAPLedgerRelease";
const REFUND_DOMAIN: &str = "MAPLedgerRefund";

/// A signed ledger entry — the PAY_PROOF payload for this method.
/// `entry_bytes = enc(["MAPLedgerEntry", invoice-id, payer, payee,
/// [asset,amount], ts])`, `sig` = operator Ed25519 over those bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct EntryProof {
    pub entry_bytes: Vec<u8>,
    pub sig: [u8; 64],
}

impl EntryProof {
    /// CBOR `[entry_bytes, sig]` — the PAY_PROOF field-3 payload (§7).
    pub fn to_bytes(&self) -> Vec<u8> {
        encode(&Value::Array(vec![
            Value::Bytes(self.entry_bytes.clone()),
            Value::Bytes(self.sig.to_vec()),
        ]))
        .expect("array of bstr encodes")
    }
    pub fn from_bytes(b: &[u8]) -> Result<EntryProof, PayError> {
        let v = decode(b)?;
        let arr = v.as_array().ok_or(PayError::BadField(3))?;
        let [eb, sg] = arr else {
            return Err(PayError::BadField(3));
        };
        Ok(EntryProof {
            entry_bytes: eb.as_bytes().ok_or(PayError::BadField(3))?.to_vec(),
            sig: sg
                .as_bytes()
                .and_then(|s| <[u8; 64]>::try_from(s).ok())
                .ok_or(PayError::BadField(3))?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LedgerError {
    /// Payer's balance is below the amount.
    Insufficient,
    /// Amount exponent differs from the operator's configured exponent
    /// (same-exponent scaffold rule; no cross-exponent normalization).
    ExponentMismatch,
    /// No open hold under that hold-invoice-id (pay §5.2.4).
    HoldMissing,
    /// A release was attempted at/after the hold's expiry, or a refund
    /// before it without a payee authorization (pay §5.2.3 expiry rules).
    Expired,
    NotExpired,
    /// The payer release-auth / payee refund-auth signature did not verify
    /// over exactly the requested (hold, release, amount) — pay §5.2.3
    /// authorization symmetry.
    AuthInvalid,
    /// Release amount exceeds the held amount.
    OverHeld,
}

/// An open escrow hold (pay §5.2.4), keyed by hold-invoice-id.
struct Hold {
    payer: AgentId,
    payee: AgentId,
    minor: i128,
    expiry_ts: u64,
}

/// In-process credit-ledger operator. Balances are minor units (i128 mantissa)
/// at one configured exponent.
pub struct LedgerOperator {
    kp: Keypair,
    exponent: i32,
    balances: HashMap<AgentId, i128>,
    seen: HashMap<[u8; 16], EntryProof>,
    /// Open escrow holds (funds are in neither party's balance while held).
    holds: HashMap<[u8; 16], Hold>,
    /// Idempotency for the escrow ops, each by its governing invoice-id.
    seen_holds: HashMap<[u8; 16], EntryProof>,
    seen_releases: HashMap<[u8; 16], EntryProof>,
    seen_refunds: HashMap<[u8; 16], EntryProof>,
}

impl LedgerOperator {
    pub fn new(kp: Keypair, exponent: i32) -> Self {
        LedgerOperator {
            kp,
            exponent,
            balances: HashMap::new(),
            seen: HashMap::new(),
            holds: HashMap::new(),
            seen_holds: HashMap::new(),
            seen_releases: HashMap::new(),
            seen_refunds: HashMap::new(),
        }
    }
    pub fn agent_id(&self) -> AgentId {
        self.kp.agent_id()
    }
    pub fn credit(&mut self, who: &AgentId, minor: i128) {
        *self.balances.entry(who.clone()).or_insert(0) += minor;
    }
    pub fn balance(&self, who: &AgentId) -> i128 {
        self.balances.get(who).copied().unwrap_or(0)
    }

    fn entry_bytes(&self, invoice_id: [u8; 16], payer: &AgentId, payee: &AgentId, aa: &AssetAmount, ts: u64) -> Vec<u8> {
        encode(&Value::Array(vec![
            Value::Text(ENTRY_DOMAIN.into()),
            Value::Bytes(invoice_id.to_vec()),
            Value::Bytes(payer.as_bytes().to_vec()),
            Value::Bytes(payee.as_bytes().to_vec()),
            aa.to_value(),
            Value::Uint(ts),
        ]))
        .expect("ledger entry encodes")
    }

    /// Move `amount` from `payer` to `payee` and emit a signed entry. Idempotent
    /// by invoice-id: a re-seen invoice returns its ORIGINAL entry with NO
    /// balance movement (FR-7). Insufficient balance / exponent mismatch → err.
    pub fn pay(
        &mut self,
        invoice_id: [u8; 16],
        payer: &AgentId,
        payee: &AgentId,
        aa: &AssetAmount,
        ts: u64,
    ) -> Result<EntryProof, LedgerError> {
        if let Some(existing) = self.seen.get(&invoice_id) {
            return Ok(existing.clone());
        }
        if aa.amount.exponent != self.exponent {
            return Err(LedgerError::ExponentMismatch);
        }
        let minor = aa.amount.mantissa as i128;
        if self.balance(payer) < minor {
            return Err(LedgerError::Insufficient);
        }
        *self.balances.entry(payer.clone()).or_insert(0) -= minor;
        *self.balances.entry(payee.clone()).or_insert(0) += minor;
        let entry_bytes = self.entry_bytes(invoice_id, payer, payee, aa, ts);
        let sig = self.kp.sign(&entry_bytes);
        let proof = EntryProof { entry_bytes, sig };
        self.seen.insert(invoice_id, proof.clone());
        Ok(proof)
    }

    /// Total minor units locked in open escrow holds (test invariant support:
    /// sum of balances + escrow is conserved by every operation).
    pub fn escrow_total(&self) -> i128 {
        self.holds.values().map(|h| h.minor).sum()
    }

    fn sign_entry(&self, values: Vec<Value>) -> EntryProof {
        let entry_bytes = encode(&Value::Array(values)).expect("ledger entry encodes");
        let sig = self.kp.sign(&entry_bytes);
        EntryProof { entry_bytes, sig }
    }

    /// Place an escrow hold (pay §5.2.4): move `aa` from the payer's balance
    /// into a bucket keyed by `hold_invoice_id`, emitting the signed
    /// `MAPLedgerHold` entry. Idempotent by that id (original entry, no second
    /// movement); insufficient balance / exponent mismatch → error, no entry.
    pub fn hold(
        &mut self,
        hold_invoice_id: [u8; 16],
        payer: &AgentId,
        payee: &AgentId,
        aa: &AssetAmount,
        expiry_ts: u64,
        ts: u64,
    ) -> Result<EntryProof, LedgerError> {
        if let Some(existing) = self.seen_holds.get(&hold_invoice_id) {
            return Ok(existing.clone());
        }
        if aa.amount.exponent != self.exponent {
            return Err(LedgerError::ExponentMismatch);
        }
        let minor = aa.amount.mantissa as i128;
        if self.balance(payer) < minor {
            return Err(LedgerError::Insufficient);
        }
        *self.balances.entry(payer.clone()).or_insert(0) -= minor;
        self.holds.insert(
            hold_invoice_id,
            Hold { payer: payer.clone(), payee: payee.clone(), minor, expiry_ts },
        );
        let proof = self.sign_entry(vec![
            Value::Text(HOLD_DOMAIN.into()),
            Value::Bytes(hold_invoice_id.to_vec()),
            Value::Bytes(payer.as_bytes().to_vec()),
            Value::Bytes(payee.as_bytes().to_vec()),
            aa.to_value(),
            Value::Uint(expiry_ts),
            Value::Uint(ts),
        ]);
        self.seen_holds.insert(hold_invoice_id, proof.clone());
        Ok(proof)
    }

    /// Release `aa` from the hold to the payee, refunding the remainder to the
    /// payer atomically and closing the hold (pay §5.2.4). Requires a valid
    /// PAYER signature over [`release_auth_bytes`] for exactly this
    /// (hold, release, amount) triple — the payee cannot self-release — and
    /// `now_ts < expiry`. Idempotent by `release_invoice_id`.
    pub fn release(
        &mut self,
        release_invoice_id: [u8; 16],
        hold_invoice_id: [u8; 16],
        payer_auth_sig: &[u8; 64],
        aa: &AssetAmount,
        now_ts: u64,
    ) -> Result<EntryProof, LedgerError> {
        if let Some(existing) = self.seen_releases.get(&release_invoice_id) {
            return Ok(existing.clone());
        }
        let hold = self.holds.get(&hold_invoice_id).ok_or(LedgerError::HoldMissing)?;
        if aa.amount.exponent != self.exponent {
            return Err(LedgerError::ExponentMismatch);
        }
        let auth = release_auth_bytes(hold_invoice_id, release_invoice_id, aa);
        if verify(&hold.payer, &auth, payer_auth_sig).is_err() {
            return Err(LedgerError::AuthInvalid);
        }
        let minor = aa.amount.mantissa as i128;
        if minor > hold.minor {
            return Err(LedgerError::OverHeld);
        }
        if now_ts >= hold.expiry_ts {
            return Err(LedgerError::Expired);
        }
        let hold = self.holds.remove(&hold_invoice_id).expect("checked above");
        *self.balances.entry(hold.payee.clone()).or_insert(0) += minor;
        *self.balances.entry(hold.payer.clone()).or_insert(0) += hold.minor - minor;
        let proof = self.sign_entry(vec![
            Value::Text(RELEASE_DOMAIN.into()),
            Value::Bytes(hold_invoice_id.to_vec()),
            Value::Bytes(release_invoice_id.to_vec()),
            Value::Bytes(hold.payer.as_bytes().to_vec()),
            Value::Bytes(hold.payee.as_bytes().to_vec()),
            aa.to_value(),
            Value::Uint(now_ts),
        ]);
        self.seen_releases.insert(release_invoice_id, proof.clone());
        Ok(proof)
    }

    /// Return the held funds to the payer and close the hold (pay §5.2.4):
    /// allowed iff the expiry has passed OR a valid PAYEE signature over
    /// [`refund_auth_bytes`] is presented (pre-expiry refund). Idempotent by
    /// the hold-invoice-id.
    pub fn refund(
        &mut self,
        hold_invoice_id: [u8; 16],
        payee_auth_sig: Option<&[u8; 64]>,
        now_ts: u64,
    ) -> Result<EntryProof, LedgerError> {
        if let Some(existing) = self.seen_refunds.get(&hold_invoice_id) {
            return Ok(existing.clone());
        }
        let hold = self.holds.get(&hold_invoice_id).ok_or(LedgerError::HoldMissing)?;
        if now_ts < hold.expiry_ts {
            let sig = payee_auth_sig.ok_or(LedgerError::NotExpired)?;
            let auth = refund_auth_bytes(hold_invoice_id);
            if verify(&hold.payee, &auth, sig).is_err() {
                return Err(LedgerError::AuthInvalid);
            }
        }
        let hold = self.holds.remove(&hold_invoice_id).expect("checked above");
        *self.balances.entry(hold.payer.clone()).or_insert(0) += hold.minor;
        let aa = AssetAmount {
            asset: crate::Asset::new("credit:ledger").expect("static asset"),
            amount: crate::Amount::new(self.exponent, hold.minor as i64),
        };
        let proof = self.sign_entry(vec![
            Value::Text(REFUND_DOMAIN.into()),
            Value::Bytes(hold_invoice_id.to_vec()),
            Value::Bytes(hold.payer.as_bytes().to_vec()),
            aa.to_value(),
            Value::Uint(now_ts),
        ]);
        self.seen_refunds.insert(hold_invoice_id, proof.clone());
        Ok(proof)
    }
}

/// The bytes a PAYER signs to authorize a release (pay §5.2.4):
/// `enc(["MAPLedgerReleaseAuth", hold-id, release-id, [Asset, Amount]])`.
/// An inner signature (decision-9 style), not an envelope.
pub fn release_auth_bytes(hold_invoice_id: [u8; 16], release_invoice_id: [u8; 16], aa: &AssetAmount) -> Vec<u8> {
    encode(&Value::Array(vec![
        Value::Text(RELEASE_AUTH_DOMAIN.into()),
        Value::Bytes(hold_invoice_id.to_vec()),
        Value::Bytes(release_invoice_id.to_vec()),
        aa.to_value(),
    ]))
    .expect("release auth encodes")
}

/// The bytes a PAYEE signs to authorize a pre-expiry refund (pay §5.2.4):
/// `enc(["MAPLedgerRefundAuth", hold-id])`.
pub fn refund_auth_bytes(hold_invoice_id: [u8; 16]) -> Vec<u8> {
    encode(&Value::Array(vec![
        Value::Text(REFUND_AUTH_DOMAIN.into()),
        Value::Bytes(hold_invoice_id.to_vec()),
    ]))
    .expect("refund auth encodes")
}

/// The payee's proof check (FR-4): `verify_strict` (via `map_identity::verify`)
/// against the operator AgentID, and the signed entry's fields match the
/// invoice being settled (domain, invoice-id, parties, [asset,amount]). The
/// entry's `ts` is read from the entry, not supplied.
pub fn verify_entry(
    proof: &EntryProof,
    operator: &AgentId,
    invoice_id: [u8; 16],
    payer: &AgentId,
    payee: &AgentId,
    aa: &AssetAmount,
) -> bool {
    if verify(operator, &proof.entry_bytes, &proof.sig).is_err() {
        return false;
    }
    let Ok(v) = decode(&proof.entry_bytes) else {
        return false;
    };
    let Some(arr) = v.as_array() else {
        return false;
    };
    arr.len() == 6
        && arr[0].as_text() == Some(ENTRY_DOMAIN)
        && arr[1].as_bytes() == Some(&invoice_id[..])
        && arr[2].as_bytes() == Some(payer.as_bytes())
        && arr[3].as_bytes() == Some(payee.as_bytes())
        && AssetAmount::from_value(&arr[4]).map(|got| &got == aa).unwrap_or(false)
}

/// Payee check of a `MAPLedgerHold` entry (pay §5.2.4): operator signature +
/// field equality against the phase-1 invoice. Returns the entry's expiry-ts
/// on success (the payee sanity-checks it against its own deadline+margin).
pub fn verify_hold_entry(
    proof: &EntryProof,
    operator: &AgentId,
    hold_invoice_id: [u8; 16],
    payer: &AgentId,
    payee: &AgentId,
    aa: &AssetAmount,
) -> Option<u64> {
    if verify(operator, &proof.entry_bytes, &proof.sig).is_err() {
        return None;
    }
    let v = decode(&proof.entry_bytes).ok()?;
    let arr = v.as_array()?;
    let ok = arr.len() == 7
        && arr[0].as_text() == Some(HOLD_DOMAIN)
        && arr[1].as_bytes() == Some(&hold_invoice_id[..])
        && arr[2].as_bytes() == Some(payer.as_bytes())
        && arr[3].as_bytes() == Some(payee.as_bytes())
        && AssetAmount::from_value(&arr[4]).map(|got| &got == aa).unwrap_or(false);
    if !ok {
        return None;
    }
    arr[5].as_uint()
}

/// Payee check of a `MAPLedgerRelease` entry (pay §5.2.4): operator signature +
/// field equality against the phase-2 invoice and the hold it draws on.
pub fn verify_release_entry(
    proof: &EntryProof,
    operator: &AgentId,
    hold_invoice_id: [u8; 16],
    release_invoice_id: [u8; 16],
    payer: &AgentId,
    payee: &AgentId,
    aa: &AssetAmount,
) -> bool {
    if verify(operator, &proof.entry_bytes, &proof.sig).is_err() {
        return false;
    }
    let Ok(v) = decode(&proof.entry_bytes) else {
        return false;
    };
    let Some(arr) = v.as_array() else {
        return false;
    };
    arr.len() == 7
        && arr[0].as_text() == Some(RELEASE_DOMAIN)
        && arr[1].as_bytes() == Some(&hold_invoice_id[..])
        && arr[2].as_bytes() == Some(&release_invoice_id[..])
        && arr[3].as_bytes() == Some(payer.as_bytes())
        && arr[4].as_bytes() == Some(payee.as_bytes())
        && AssetAmount::from_value(&arr[5]).map(|got| &got == aa).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Amount, Asset};

    fn usd(mantissa: i64) -> AssetAmount {
        AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, mantissa) }
    }
    fn kp(seed: u8) -> Keypair {
        Keypair::from_seed([seed; 32])
    }

    #[test]
    fn pay_moves_balance_once_and_is_idempotent() {
        let mut op = LedgerOperator::new(kp(9), -2);
        let (payer, payee) = (kp(1).agent_id(), kp(2).agent_id());
        op.credit(&payer, 1000);
        let p1 = op.pay([7; 16], &payer, &payee, &usd(500), 1).unwrap();
        assert_eq!(op.balance(&payer), 500);
        assert_eq!(op.balance(&payee), 500);
        // Re-paying the same invoice → original entry, no further movement.
        let p2 = op.pay([7; 16], &payer, &payee, &usd(500), 1).unwrap();
        assert_eq!(p1, p2);
        assert_eq!(op.balance(&payer), 500);
        assert_eq!(op.balance(&payee), 500);
    }

    #[test]
    fn insufficient_and_exponent_mismatch_rejected() {
        let mut op = LedgerOperator::new(kp(9), -2);
        let (payer, payee) = (kp(1).agent_id(), kp(2).agent_id());
        op.credit(&payer, 100);
        assert_eq!(op.pay([7; 16], &payer, &payee, &usd(500), 1), Err(LedgerError::Insufficient));
        let wrong_exp = AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-6, 500) };
        assert_eq!(op.pay([8; 16], &payer, &payee, &wrong_exp, 1), Err(LedgerError::ExponentMismatch));
        assert_eq!(op.balance(&payer), 100); // untouched
    }

    #[test]
    fn verify_entry_good_forged_and_mismatched() {
        let mut op = LedgerOperator::new(kp(9), -2);
        let (payer, payee) = (kp(1).agent_id(), kp(2).agent_id());
        op.credit(&payer, 1000);
        let proof = op.pay([7; 16], &payer, &payee, &usd(500), 1).unwrap();
        assert!(verify_entry(&proof, &op.agent_id(), [7; 16], &payer, &payee, &usd(500)));
        // Wrong operator key (forged) → rejected.
        assert!(!verify_entry(&proof, &kp(3).agent_id(), [7; 16], &payer, &payee, &usd(500)));
        // Mismatched amount / invoice → rejected (proof replay defense).
        assert!(!verify_entry(&proof, &op.agent_id(), [7; 16], &payer, &payee, &usd(999)));
        assert!(!verify_entry(&proof, &op.agent_id(), [8; 16], &payer, &payee, &usd(500)));
    }

    // ---- escrow extension (pay §5.2.4) -------------------------------------

    /// Sum of all balances + open escrow must be invariant under every op.
    fn conserved(op: &LedgerOperator, parties: &[&AgentId]) -> i128 {
        parties.iter().map(|p| op.balance(p)).sum::<i128>() + op.escrow_total()
    }

    #[test]
    fn hold_release_with_remainder_conserves_and_is_idempotent() {
        let mut op = LedgerOperator::new(kp(9), -2);
        let (payer_kp, payee_kp) = (kp(1), kp(2));
        let (payer, payee) = (payer_kp.agent_id(), payee_kp.agent_id());
        op.credit(&payer, 1000);
        let total_before = conserved(&op, &[&payer, &payee]);

        // Hold 500: payer down, escrow up, payee untouched.
        let hold = op.hold([7; 16], &payer, &payee, &usd(500), 100, 1).unwrap();
        assert_eq!(op.balance(&payer), 500);
        assert_eq!(op.escrow_total(), 500);
        assert_eq!(conserved(&op, &[&payer, &payee]), total_before);
        assert_eq!(verify_hold_entry(&hold, &op.agent_id(), [7; 16], &payer, &payee, &usd(500)), Some(100));
        // Idempotent re-hold: original entry, no second movement.
        let hold2 = op.hold([7; 16], &payer, &payee, &usd(500), 100, 1).unwrap();
        assert_eq!(hold, hold2);
        assert_eq!(op.balance(&payer), 500);

        // Release 300 < held 500: payee +300, payer refunded +200, hold closed.
        let auth = payer_kp.sign(&release_auth_bytes([7; 16], [8; 16], &usd(300)));
        let rel = op.release([8; 16], [7; 16], &auth, &usd(300), 50).unwrap();
        assert_eq!(op.balance(&payee), 300);
        assert_eq!(op.balance(&payer), 700);
        assert_eq!(op.escrow_total(), 0);
        assert_eq!(conserved(&op, &[&payer, &payee]), total_before);
        assert!(verify_release_entry(&rel, &op.agent_id(), [7; 16], [8; 16], &payer, &payee, &usd(300)));
        // Idempotent re-release.
        let rel2 = op.release([8; 16], [7; 16], &auth, &usd(300), 50).unwrap();
        assert_eq!(rel, rel2);
        assert_eq!(op.balance(&payee), 300);
    }

    #[test]
    fn release_exact_held_leaves_zero_remainder() {
        let mut op = LedgerOperator::new(kp(9), -2);
        let (payer_kp, payee) = (kp(1), kp(2).agent_id());
        let payer = payer_kp.agent_id();
        op.credit(&payer, 500);
        op.hold([7; 16], &payer, &payee, &usd(500), 100, 1).unwrap();
        let auth = payer_kp.sign(&release_auth_bytes([7; 16], [8; 16], &usd(500)));
        op.release([8; 16], [7; 16], &auth, &usd(500), 50).unwrap();
        assert_eq!(op.balance(&payer), 0);
        assert_eq!(op.balance(&payee), 500);
        assert_eq!(op.escrow_total(), 0);
    }

    #[test]
    fn authorization_symmetry_negatives() {
        let mut op = LedgerOperator::new(kp(9), -2);
        let (payer_kp, payee_kp, mallory) = (kp(1), kp(2), kp(3));
        let (payer, payee) = (payer_kp.agent_id(), payee_kp.agent_id());
        op.credit(&payer, 500);
        op.hold([7; 16], &payer, &payee, &usd(500), 100, 1).unwrap();

        // Payee (or anyone but the payer) cannot self-release.
        let forged = payee_kp.sign(&release_auth_bytes([7; 16], [8; 16], &usd(500)));
        assert_eq!(op.release([8; 16], [7; 16], &forged, &usd(500), 50), Err(LedgerError::AuthInvalid));
        let mallory_sig = mallory.sign(&release_auth_bytes([7; 16], [8; 16], &usd(500)));
        assert_eq!(op.release([8; 16], [7; 16], &mallory_sig, &usd(500), 50), Err(LedgerError::AuthInvalid));
        // A payer auth over a DIFFERENT amount / release-id does not transfer.
        let wrong_amount = payer_kp.sign(&release_auth_bytes([7; 16], [8; 16], &usd(400)));
        assert_eq!(op.release([8; 16], [7; 16], &wrong_amount, &usd(500), 50), Err(LedgerError::AuthInvalid));
        let wrong_release = payer_kp.sign(&release_auth_bytes([7; 16], [9; 16], &usd(500)));
        assert_eq!(op.release([8; 16], [7; 16], &wrong_release, &usd(500), 50), Err(LedgerError::AuthInvalid));
        // Over-held release rejected even with a matching auth.
        let over = payer_kp.sign(&release_auth_bytes([7; 16], [8; 16], &usd(600)));
        assert_eq!(op.release([8; 16], [7; 16], &over, &usd(600), 50), Err(LedgerError::OverHeld));
        // Payer cannot unwind pre-expiry without the payee's signature.
        assert_eq!(op.refund([7; 16], None, 50), Err(LedgerError::NotExpired));
        let bad_refund = payer_kp.sign(&refund_auth_bytes([7; 16]));
        assert_eq!(op.refund([7; 16], Some(&bad_refund), 50), Err(LedgerError::AuthInvalid));
        // Nothing moved through all of the above.
        assert_eq!(op.balance(&payer), 0);
        assert_eq!(op.balance(&payee), 0);
        assert_eq!(op.escrow_total(), 500);
    }

    #[test]
    fn expiry_rules_refund_and_release() {
        let mut op = LedgerOperator::new(kp(9), -2);
        let (payer_kp, payee_kp) = (kp(1), kp(2));
        let (payer, payee) = (payer_kp.agent_id(), payee_kp.agent_id());
        op.credit(&payer, 500);
        op.hold([7; 16], &payer, &payee, &usd(500), 100, 1).unwrap();

        // Release at/after expiry fails on the rail (pay §5.2.3).
        let auth = payer_kp.sign(&release_auth_bytes([7; 16], [8; 16], &usd(500)));
        assert_eq!(op.release([8; 16], [7; 16], &auth, &usd(500), 100), Err(LedgerError::Expired));
        // Post-expiry refund needs no payee auth; funds return in full.
        let refund = op.refund([7; 16], None, 100).unwrap();
        assert_eq!(op.balance(&payer), 500);
        assert_eq!(op.escrow_total(), 0);
        // Idempotent re-refund; and the hold is closed (release now HoldMissing).
        assert_eq!(op.refund([7; 16], None, 100).unwrap(), refund);
        assert_eq!(op.balance(&payer), 500);
        assert_eq!(op.release([8; 16], [7; 16], &auth, &usd(500), 100), Err(LedgerError::HoldMissing));

        // Pre-expiry refund WITH a valid payee auth succeeds.
        op.credit(&payer, 0); // no-op, keeps shape
        op.hold([10; 16], &payer, &payee, &usd(200), 100, 1).unwrap();
        let payee_auth = payee_kp.sign(&refund_auth_bytes([10; 16]));
        op.refund([10; 16], Some(&payee_auth), 50).unwrap();
        assert_eq!(op.balance(&payer), 500);
        assert_eq!(op.escrow_total(), 0);
    }
}
