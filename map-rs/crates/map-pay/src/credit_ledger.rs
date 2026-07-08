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
}

/// In-process credit-ledger operator. Balances are minor units (i128 mantissa)
/// at one configured exponent.
pub struct LedgerOperator {
    kp: Keypair,
    exponent: i32,
    balances: HashMap<AgentId, i128>,
    seen: HashMap<[u8; 16], EntryProof>,
}

impl LedgerOperator {
    pub fn new(kp: Keypair, exponent: i32) -> Self {
        LedgerOperator { kp, exponent, balances: HashMap::new(), seen: HashMap::new() }
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
}
