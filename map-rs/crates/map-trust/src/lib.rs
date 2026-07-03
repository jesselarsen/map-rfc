//! MAP trust bounded context (draft-map-trust-00).
//!
//! Data semantics (trust §1, normative): a receipt is a past-tense,
//! immutable domain event identified by its content address. The store is
//! append-only with idempotent, replay-safe ingestion. Trust scores are
//! *derived* — [`TrustProjection::rebuild`] from the store must reproduce
//! any incrementally maintained projection, and the test suite enforces
//! exactly that (ops §6.2 "projection rebuild").

use map_core::cbor::{decode, encode, CborError, Value};
use map_identity::{verify, AgentId, IdentityError, Keypair};
use map_tasks::TaskId;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;

pub const ROLE_REQUESTER: u8 = 1;
pub const ROLE_PROVIDER: u8 = 2;

#[derive(Debug)]
pub enum TrustError {
    Cbor(CborError),
    Identity(IdentityError),
    MissingField(u64),
    BadField(u64),
    /// A receipt must carry at least one signature (trust §3).
    NoSignatures,
}

impl fmt::Display for TrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrustError::Cbor(e) => write!(f, "cbor: {e}"),
            TrustError::Identity(e) => write!(f, "identity: {e}"),
            TrustError::MissingField(k) => write!(f, "receipt missing field {k}"),
            TrustError::BadField(k) => write!(f, "receipt field {k} invalid"),
            TrustError::NoSignatures => write!(f, "receipt carries no signatures"),
        }
    }
}
impl std::error::Error for TrustError {}
impl From<CborError> for TrustError {
    fn from(e: CborError) -> Self {
        TrustError::Cbor(e)
    }
}
impl From<IdentityError> for TrustError {
    fn from(e: IdentityError) -> Self {
        TrustError::Identity(e)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Ok = 0,
    Failed = 1,
    Timeout = 2,
    Cancelled = 3,
    Disputed = 4,
}

impl Outcome {
    pub fn from_u64(v: u64) -> Option<Self> {
        Some(match v {
            0 => Outcome::Ok,
            1 => Outcome::Failed,
            2 => Outcome::Timeout,
            3 => Outcome::Cancelled,
            4 => Outcome::Disputed,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Metrics {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub ttft_ms: u64,
    pub wall_ms: u64,
}

/// ReceiptCore (trust §3) — the signed fact. Wire format matches TV-002.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceiptCore {
    pub task: TaskId,
    pub requester: AgentId,
    pub provider: AgentId,
    pub capability: String,
    pub started_ms: u64,
    pub ended_ms: u64,
    pub outcome: Outcome,
    pub metrics: Metrics,
    /// Subjective assertion by the requester; never "verified" (trust §3).
    pub rating: Option<u8>,
    pub payment_ref: Option<Vec<u8>>,
    pub transcript_root: Option<[u8; 32]>,
}

impl ReceiptCore {
    pub fn to_value(&self) -> Value {
        let mut m: Vec<(Value, Value)> = vec![
            (Value::Uint(1), Value::Bytes(self.task.0.to_vec())),
            (
                Value::Uint(2),
                Value::Bytes(self.requester.as_bytes().to_vec()),
            ),
            (
                Value::Uint(3),
                Value::Bytes(self.provider.as_bytes().to_vec()),
            ),
            (Value::Uint(4), Value::Text(self.capability.clone())),
            (Value::Uint(5), Value::Uint(self.started_ms)),
            (Value::Uint(6), Value::Uint(self.ended_ms)),
            (Value::Uint(7), Value::Uint(self.outcome as u64)),
            (
                Value::Uint(8),
                Value::Map(vec![
                    (Value::Uint(1), Value::Uint(self.metrics.tokens_in)),
                    (Value::Uint(2), Value::Uint(self.metrics.tokens_out)),
                    (Value::Uint(3), Value::Uint(self.metrics.ttft_ms)),
                    (Value::Uint(4), Value::Uint(self.metrics.wall_ms)),
                ]),
            ),
        ];
        if let Some(r) = self.rating {
            m.push((Value::Uint(9), Value::Uint(r as u64)));
        }
        if let Some(p) = &self.payment_ref {
            m.push((Value::Uint(10), Value::Bytes(p.clone())));
        }
        if let Some(t) = &self.transcript_root {
            m.push((Value::Uint(11), Value::Bytes(t.to_vec())));
        }
        Value::Map(m)
    }

    pub fn encode(&self) -> Result<Vec<u8>, TrustError> {
        Ok(encode(&self.to_value())?)
    }

    pub fn from_value(v: &Value) -> Result<ReceiptCore, TrustError> {
        let bytes_field = |k: u64| {
            v.map_get(k)
                .ok_or(TrustError::MissingField(k))?
                .as_bytes()
                .ok_or(TrustError::BadField(k))
        };
        let uint_field = |k: u64| {
            v.map_get(k)
                .ok_or(TrustError::MissingField(k))?
                .as_uint()
                .ok_or(TrustError::BadField(k))
        };
        let task = TaskId::from_slice(bytes_field(1)?).ok_or(TrustError::BadField(1))?;
        let requester =
            AgentId::from_bytes(bytes_field(2)?).map_err(|_| TrustError::BadField(2))?;
        let provider =
            AgentId::from_bytes(bytes_field(3)?).map_err(|_| TrustError::BadField(3))?;
        let capability = v
            .map_get(4)
            .ok_or(TrustError::MissingField(4))?
            .as_text()
            .ok_or(TrustError::BadField(4))?
            .to_string();
        let started_ms = uint_field(5)?;
        let ended_ms = uint_field(6)?;
        if ended_ms < started_ms {
            return Err(TrustError::BadField(6));
        }
        let outcome = Outcome::from_u64(uint_field(7)?).ok_or(TrustError::BadField(7))?;
        let mv = v.map_get(8).ok_or(TrustError::MissingField(8))?;
        let mu = |k: u64| mv.map_get(k).and_then(|x| x.as_uint()).ok_or(TrustError::BadField(8));
        let metrics = Metrics {
            tokens_in: mu(1)?,
            tokens_out: mu(2)?,
            ttft_ms: mu(3)?,
            wall_ms: mu(4)?,
        };
        let rating = match v.map_get(9) {
            None => None,
            Some(x) => Some(
                x.as_uint()
                    .and_then(|n| u8::try_from(n).ok())
                    .filter(|n| *n <= 100)
                    .ok_or(TrustError::BadField(9))?,
            ),
        };
        let payment_ref = match v.map_get(10) {
            None => None,
            Some(x) => Some(x.as_bytes().ok_or(TrustError::BadField(10))?.to_vec()),
        };
        let transcript_root = match v.map_get(11) {
            None => None,
            Some(x) => Some(
                x.as_bytes()
                    .and_then(|b| <[u8; 32]>::try_from(b).ok())
                    .ok_or(TrustError::BadField(11))?,
            ),
        };
        Ok(ReceiptCore {
            task,
            requester,
            provider,
            capability,
            started_ms,
            ended_ms,
            outcome,
            metrics,
            rating,
            payment_ref,
            transcript_root,
        })
    }
}

/// enc(["MAPReceipt", role, core-bytes]) — the role tag prevents signature
/// confusion between the parties (trust §3; TV-002).
pub fn receipt_signing_input(role: u8, core_bytes: &[u8]) -> Vec<u8> {
    encode(&Value::Array(vec![
        Value::Text("MAPReceipt".into()),
        Value::Uint(role as u64),
        Value::Bytes(core_bytes.to_vec()),
    ]))
    .expect("array of primitives cannot fail to encode")
}

pub fn sign_receipt(kp: &Keypair, role: u8, core_bytes: &[u8]) -> [u8; 64] {
    kp.sign(&receipt_signing_input(role, core_bytes))
}

/// The wire receipt: exact signed core bytes (invariant I1 applies — verify
/// what was received, never a re-serialization) plus up to two signatures.
#[derive(Clone, Debug, PartialEq)]
pub struct Receipt {
    pub core_bytes: Vec<u8>,
    pub sig_requester: Option<[u8; 64]>,
    pub sig_provider: Option<[u8; 64]>,
}

/// A receipt whose signatures verified against the identities in its core.
#[derive(Clone, Debug)]
pub struct Verified {
    pub core: ReceiptCore,
    /// Both signatures present and valid — the strongest evidence class.
    pub dual: bool,
}

impl Receipt {
    pub fn encode(&self) -> Result<Vec<u8>, TrustError> {
        let mut m: Vec<(Value, Value)> =
            vec![(Value::Uint(1), Value::Bytes(self.core_bytes.clone()))];
        if let Some(s) = &self.sig_requester {
            m.push((Value::Uint(2), Value::Bytes(s.to_vec())));
        }
        if let Some(s) = &self.sig_provider {
            m.push((Value::Uint(3), Value::Bytes(s.to_vec())));
        }
        Ok(encode(&Value::Map(m))?)
    }

    pub fn decode_bytes(b: &[u8]) -> Result<Receipt, TrustError> {
        let v = decode(b)?;
        let core_bytes = v
            .map_get(1)
            .ok_or(TrustError::MissingField(1))?
            .as_bytes()
            .ok_or(TrustError::BadField(1))?
            .to_vec();
        let sig = |k: u64| -> Result<Option<[u8; 64]>, TrustError> {
            match v.map_get(k) {
                None => Ok(None),
                Some(x) => Ok(Some(
                    x.as_bytes()
                        .and_then(|b| <[u8; 64]>::try_from(b).ok())
                        .ok_or(TrustError::BadField(k))?,
                )),
            }
        };
        Ok(Receipt {
            core_bytes,
            sig_requester: sig(2)?,
            sig_provider: sig(3)?,
        })
    }

    pub fn core(&self) -> Result<ReceiptCore, TrustError> {
        ReceiptCore::from_value(&decode(&self.core_bytes)?)
    }

    pub fn verify(&self) -> Result<Verified, TrustError> {
        let core = self.core()?;
        if self.sig_requester.is_none() && self.sig_provider.is_none() {
            return Err(TrustError::NoSignatures);
        }
        if let Some(s) = &self.sig_requester {
            verify(
                &core.requester,
                &receipt_signing_input(ROLE_REQUESTER, &self.core_bytes),
                s,
            )?;
        }
        if let Some(s) = &self.sig_provider {
            verify(
                &core.provider,
                &receipt_signing_input(ROLE_PROVIDER, &self.core_bytes),
                s,
            )?;
        }
        Ok(Verified {
            dual: self.sig_requester.is_some() && self.sig_provider.is_some(),
            core,
        })
    }

    /// Content address (dedup key). SHA-256 digest of the encoded receipt;
    /// storage layers multihash-wrap as needed.
    pub fn content_address(&self) -> Result<[u8; 32], TrustError> {
        Ok(Sha256::digest(self.encode()?).into())
    }
}

// ---------------------------------------------------------------------
// Append-only, idempotent store (ops §2 rules 1/3; trust §1).
// ---------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum Ingest {
    Ingested,
    Duplicate,
}

#[derive(Default)]
pub struct ReceiptStore {
    log: Vec<Receipt>,
    seen: HashSet<[u8; 32]>,
}

impl ReceiptStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Verify, then append iff unseen. Safe under duplicate delivery and
    /// replay by construction.
    pub fn ingest(&mut self, r: Receipt) -> Result<Ingest, TrustError> {
        r.verify()?;
        let ca = r.content_address()?;
        if !self.seen.insert(ca) {
            return Ok(Ingest::Duplicate);
        }
        self.log.push(r);
        Ok(Ingest::Ingested)
    }

    pub fn len(&self) -> usize {
        self.log.len()
    }
    pub fn is_empty(&self) -> bool {
        self.log.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &Receipt> {
        self.log.iter()
    }
}

// ---------------------------------------------------------------------
// Beta-reputation projection (trust §7, informative formula; normative
// rebuild rule). Derived data: never authoritative over its evidence.
// ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjParams {
    /// Recency decay per day (trust §7 recommends ≈0.98).
    pub daily_decay: f64,
    /// Symmetric Beta prior applied at read time (α₀ = β₀ = prior).
    pub prior: f64,
}

impl Default for ProjParams {
    fn default() -> Self {
        ProjParams {
            daily_decay: 0.98,
            prior: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrustProjection {
    params: ProjParams,
    cells: HashMap<(AgentId, String), (f64, f64)>, // (alpha, beta)
}

impl TrustProjection {
    pub fn new(params: ProjParams) -> Self {
        TrustProjection {
            params,
            cells: HashMap::new(),
        }
    }

    /// Fold one verified receipt in. `trust_in_issuer` weights the evidence
    /// by the evaluator's trust in the requester-signer (trust §7).
    pub fn apply(&mut self, v: &Verified, now_ms: u64, trust_in_issuer: f64) {
        let days = now_ms.saturating_sub(v.core.ended_ms) as f64 / 86_400_000.0;
        let w = trust_in_issuer * self.params.daily_decay.powf(days);
        let cell = self
            .cells
            .entry((v.core.provider.clone(), v.core.capability.clone()))
            .or_insert((0.0, 0.0));
        match v.core.outcome {
            Outcome::Ok => cell.0 += w,
            Outcome::Failed | Outcome::Timeout => cell.1 += w,
            Outcome::Cancelled => {}
            Outcome::Disputed => cell.1 += 2.0 * w,
        }
    }

    /// `(score, confidence)` per trust §7: score = α/(α+β) with the prior
    /// folded in at read time; gate on confidence, not score alone.
    pub fn score(&self, provider: &AgentId, capability: &str) -> Option<(f64, f64)> {
        let (a, b) = self
            .cells
            .get(&(provider.clone(), capability.to_string()))?;
        let p = self.params.prior;
        Some(((a + p) / (a + b + 2.0 * p), a + b))
    }

    /// Recompute from evidence alone. The normative rebuild rule (trust §7):
    /// this MUST reproduce any incrementally maintained projection.
    pub fn rebuild<F: Fn(&AgentId) -> f64>(
        store: &ReceiptStore,
        now_ms: u64,
        params: ProjParams,
        trust_in_issuer: F,
    ) -> Result<TrustProjection, TrustError> {
        let mut p = TrustProjection::new(params);
        for r in store.iter() {
            let v = r.verify()?; // defensive; ingest already verified
            let w = trust_in_issuer(&v.core.requester);
            p.apply(&v, now_ms, w);
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(seed: u8) -> Keypair {
        Keypair::from_seed([seed; 32])
    }

    fn receipt(
        req: &Keypair,
        prov: &Keypair,
        n: u8,
        outcome: Outcome,
        dual: bool,
    ) -> Receipt {
        let core = ReceiptCore {
            task: TaskId([n; 32]),
            requester: req.agent_id(),
            provider: prov.agent_id(),
            capability: "map:cap:inference/text-generation".into(),
            started_ms: 1_780_000_000_000,
            ended_ms: 1_780_000_042_000,
            outcome,
            metrics: Metrics {
                tokens_in: 100,
                tokens_out: 50,
                ttft_ms: 200,
                wall_ms: 42_000,
            },
            rating: Some(80),
            payment_ref: None,
            transcript_root: None,
        };
        let core_bytes = core.encode().unwrap();
        Receipt {
            sig_requester: Some(sign_receipt(req, ROLE_REQUESTER, &core_bytes)),
            sig_provider: dual.then(|| sign_receipt(prov, ROLE_PROVIDER, &core_bytes)),
            core_bytes,
        }
    }

    #[test]
    fn dual_signature_verification_and_role_separation() {
        let (a, b) = (agent(1), agent(2));
        let r = receipt(&a, &b, 1, Outcome::Ok, true);
        let v = r.verify().unwrap();
        assert!(v.dual);

        // Swapping the role tags must fail: a provider signature is not a
        // requester signature over the same core.
        let swapped = Receipt {
            core_bytes: r.core_bytes.clone(),
            sig_requester: r.sig_provider,
            sig_provider: r.sig_requester,
        };
        assert!(swapped.verify().is_err());

        // Wire roundtrip.
        let decoded = Receipt::decode_bytes(&r.encode().unwrap()).unwrap();
        assert_eq!(decoded, r);
    }

    #[test]
    fn store_ingest_is_idempotent() {
        let (a, b) = (agent(1), agent(2));
        let mut store = ReceiptStore::new();
        let r = receipt(&a, &b, 1, Outcome::Ok, true);
        assert_eq!(store.ingest(r.clone()).unwrap(), Ingest::Ingested);
        assert_eq!(store.ingest(r).unwrap(), Ingest::Duplicate);
        assert_eq!(store.len(), 1);
        // Unsigned receipts are rejected before storage.
        let core_bytes = receipt(&a, &b, 2, Outcome::Ok, true).core_bytes;
        let unsigned = Receipt {
            core_bytes,
            sig_requester: None,
            sig_provider: None,
        };
        assert!(store.ingest(unsigned).is_err());
    }

    #[test]
    fn projection_rebuild_equals_incremental() {
        // The DDIA test (ops §6.2): delete the projection, recompute from
        // the receipt store, compare decisions.
        let (a, b, c) = (agent(1), agent(2), agent(3));
        let now = 1_780_000_100_000u64;
        let params = ProjParams::default();
        let mut store = ReceiptStore::new();
        let mut incremental = TrustProjection::new(params);

        let batch = vec![
            receipt(&a, &b, 1, Outcome::Ok, true),
            receipt(&a, &b, 1, Outcome::Ok, true), // duplicate delivery
            receipt(&a, &b, 2, Outcome::Failed, true),
            receipt(&c, &b, 3, Outcome::Ok, false), // unilateral
            receipt(&a, &c, 4, Outcome::Disputed, true),
            receipt(&a, &b, 5, Outcome::Cancelled, true),
        ];
        for r in batch {
            if store.ingest(r.clone()).unwrap() == Ingest::Ingested {
                incremental.apply(&r.verify().unwrap(), now, 1.0);
            }
        }
        assert_eq!(store.len(), 5); // duplicate skipped

        let rebuilt = TrustProjection::rebuild(&store, now, params, |_| 1.0).unwrap();
        assert_eq!(rebuilt, incremental);

        let (score, conf) = rebuilt
            .score(&b.agent_id(), "map:cap:inference/text-generation")
            .unwrap();
        assert!(score > 0.5 && score < 1.0);
        assert!(conf > 0.0);
        // Cancelled contributed no weight; disputed provider scored worse.
        let (score_c, _) = rebuilt
            .score(&c.agent_id(), "map:cap:inference/text-generation")
            .unwrap();
        assert!(score_c < score);
    }
}
