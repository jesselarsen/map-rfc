//! MAP trust: dual-signed execution receipts and the observer-local trust
//! projection (spec/layer.md §6, §8).
//!
//! A receipt is a past-tense, immutable fact identified by its content
//! address. The store is append-only with idempotent, replay-safe ingestion.
//! Trust scores are *derived*: [`TrustProjection::rebuild`] from the store
//! reproduces any incrementally maintained projection (T-1), and the
//! admissibility rules R-1..R-4 live inside [`TrustProjection::apply`] so both
//! paths enforce them identically (T-2).

use map_core::cbor::{decode, encode, CborError, Value};
use map_identity::{verify, AgentId, IdentityError, Keypair};
use map_core::TaskId;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;

pub mod evidence;
pub mod lifecycle;

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
    /// Key 10: payment reference; carries a Multihash voucher-digest when
    /// metering is used (layer §9.3).
    pub payment_ref: Option<Vec<u8>>,
    pub transcript_root: Option<[u8; 32]>,
    /// Key 12: the data-sensitivity zone the task ran under (layer §10).
    pub zone: Option<u8>,
    /// Key 13: Multihash of the countersigned CheckpointArtifact this task
    /// resumed from (layer §7.4).
    pub resumed_from: Option<Vec<u8>>,
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
        if let Some(z) = self.zone {
            m.push((Value::Uint(12), Value::Uint(z as u64)));
        }
        if let Some(r) = &self.resumed_from {
            m.push((Value::Uint(13), Value::Bytes(r.clone())));
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
        let zone = match v.map_get(12) {
            None => None,
            Some(x) => Some(
                x.as_uint()
                    .filter(|z| *z <= 3)
                    .map(|z| z as u8)
                    .ok_or(TrustError::BadField(12))?,
            ),
        };
        let resumed_from = match v.map_get(13) {
            None => None,
            Some(x) => Some(
                x.as_bytes()
                    .filter(|b| b.len() == 34 && b[..2] == [0x12, 0x20])
                    .ok_or(TrustError::BadField(13))?
                    .to_vec(),
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
            zone,
            resumed_from,
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

/// A receipt whose present signatures verified against the identities in its
/// core (layer R-1). Which parties signed decides admissibility (R-4).
#[derive(Clone, Debug)]
pub struct Verified {
    pub core: ReceiptCore,
    /// Both signatures present and valid: the only class that can carry
    /// positive evidence.
    pub dual: bool,
    /// The requester (role 1) signature is present and valid.
    pub requester_signed: bool,
    /// The provider (role 2) signature is present and valid.
    pub provider_signed: bool,
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

    /// The receipt as a CBOR map value — the content of a RECEIPT/RECEIPT_ACK
    /// message or an EvidenceSet entry. `core_bytes` (key 1) is carried verbatim
    /// so a countersigner signs the *received* bytes (I1); `encode`/`decode_bytes`
    /// (the frozen TV-002 wire form) are unchanged.
    pub fn to_value(&self) -> Value {
        let mut m: Vec<(Value, Value)> =
            vec![(Value::Uint(1), Value::Bytes(self.core_bytes.clone()))];
        if let Some(s) = &self.sig_requester {
            m.push((Value::Uint(2), Value::Bytes(s.to_vec())));
        }
        if let Some(s) = &self.sig_provider {
            m.push((Value::Uint(3), Value::Bytes(s.to_vec())));
        }
        Value::Map(m)
    }

    pub fn from_value(v: &Value) -> Result<Receipt, TrustError> {
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
            requester_signed: self.sig_requester.is_some(),
            provider_signed: self.sig_provider.is_some(),
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
// Observer-local Beta-reputation projection (layer §8). Derived data: never
// authoritative over its evidence.
// ---------------------------------------------------------------------

/// Projection policy (layer §8). Every field is local policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjParams {
    /// Recency decay per day (layer §8.2 recommends ≈0.98), computed from the
    /// receipt's `ended`, never from ingestion time (T-1).
    pub daily_decay: f64,
    /// Symmetric Beta prior applied at read time (α₀ = β₀ = prior).
    pub prior: f64,
    /// R-3 skew bound: a receipt whose `ended` is later than the evaluation
    /// time plus this bound does not count at that evaluation time.
    pub skew_ms: u64,
    /// T-3 per-issuer cap: the most total (weighted, decayed) evidence one
    /// issuer may contribute to one (provider, capability) cell. Applied at
    /// read time, so the result is independent of ingestion order. `None`
    /// leaves it uncapped; [`ProjParams::recommended`] sets 50.
    pub issuer_cap: Option<f64>,
}

impl Default for ProjParams {
    /// Uncapped. Kept as it was so existing results (F1, the golden smoke
    /// hashes) stay reproducible; new deployments should start from
    /// [`ProjParams::recommended`].
    fn default() -> Self {
        ProjParams {
            daily_decay: 0.98,
            prior: 1.0,
            skew_ms: 120_000,
            issuer_cap: None,
        }
    }
}

impl ProjParams {
    /// The layer §8.2 parameter table: decay 0.98/day, prior 1/1, 120 s
    /// future skew, per-issuer cap 50. These are the values evaluation E1
    /// ran with (fixed before the runs, not tuned).
    pub fn recommended() -> Self {
        ProjParams {
            issuer_cap: Some(50.0),
            ..ProjParams::default()
        }
    }
}

/// Why [`TrustProjection::apply`] did not count a receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// R-2: requester == provider.
    SelfDealing,
    /// R-3: `ended` lies beyond the evaluation time plus the skew bound.
    FutureDated,
    /// R-4: not dual-signed and not an admissible requester-only negative.
    NotAdmissible,
    /// T-3: the issuer weighs 0 (or a non-finite weight was supplied).
    ZeroWeight,
    /// Outcome 3 (cancelled) carries no evidence by default (layer §8.2).
    Cancelled,
}

/// The admissibility gates [`TrustProjection::apply`] enforces, in order:
/// R-2 self-dealing, R-3 future dating (beyond `skew_ms`), R-4 (positive
/// evidence needs both signatures; a requester-only receipt counts only as
/// negative evidence), the cancelled rule, and T-3 (a zero or non-finite
/// issuer weight). Exposed so a caller that keeps its own accumulators (the
/// evaluation simulator) applies exactly the same rules; `apply` calls it.
pub fn admissible(v: &Verified, now_ms: u64, skew_ms: u64, issuer_weight: f64) -> Result<(), Skip> {
    let c = &v.core;
    if c.requester == c.provider {
        return Err(Skip::SelfDealing);
    }
    if c.ended_ms > now_ms.saturating_add(skew_ms) {
        return Err(Skip::FutureDated);
    }
    let negative = matches!(c.outcome, Outcome::Failed | Outcome::Timeout | Outcome::Disputed);
    let requester_only = v.requester_signed && !v.provider_signed;
    if !(v.dual || (requester_only && negative)) {
        return Err(Skip::NotAdmissible);
    }
    if c.outcome == Outcome::Cancelled {
        return Err(Skip::Cancelled);
    }
    if !(issuer_weight.is_finite() && issuer_weight > 0.0) {
        return Err(Skip::ZeroWeight);
    }
    Ok(())
}

/// Per-issuer (α, β) accumulations for one (provider, capability) cell.
type Cell = HashMap<AgentId, (f64, f64)>;

#[derive(Clone, Debug, PartialEq)]
pub struct TrustProjection {
    params: ProjParams,
    cells: HashMap<(AgentId, String), Cell>,
}

impl TrustProjection {
    pub fn new(params: ProjParams) -> Self {
        TrustProjection {
            params,
            cells: HashMap::new(),
        }
    }

    /// Fold one verified receipt in, enforcing the admissibility rules
    /// (layer R-2..R-4 and T-3) inside the projection itself, so incremental
    /// use and [`rebuild`](Self::rebuild) cannot diverge (T-2). R-1 is
    /// [`Receipt::verify`]. `issuer_weight` is the observer's weight for the
    /// receipt's requester (the issuer); see [`IssuerWeights`].
    pub fn apply(&mut self, v: &Verified, now_ms: u64, issuer_weight: f64) -> Result<(), Skip> {
        admissible(v, now_ms, self.params.skew_ms, issuer_weight)?;
        let c = &v.core;
        let days = now_ms.saturating_sub(c.ended_ms) as f64 / 86_400_000.0;
        let w = issuer_weight * self.params.daily_decay.powf(days);
        let entry = self
            .cells
            .entry((c.provider.clone(), c.capability.clone()))
            .or_default()
            .entry(c.requester.clone())
            .or_insert((0.0, 0.0));
        match c.outcome {
            Outcome::Ok => entry.0 += w,
            Outcome::Failed | Outcome::Timeout => entry.1 += w,
            Outcome::Disputed => entry.1 += 2.0 * w,
            Outcome::Cancelled => unreachable!("filtered above"),
        }
        Ok(())
    }

    /// `(score, confidence)` (layer §8.2): score = α/(α+β) with the prior
    /// folded in at read time; confidence = α+β. Each issuer's contribution
    /// is scaled down to `issuer_cap` first when a cap is set. Gate
    /// decisions on confidence, not score alone. `None` if no admissible
    /// evidence exists for the cell.
    pub fn score(&self, provider: &AgentId, capability: &str) -> Option<(f64, f64)> {
        let cell = self.cells.get(&(provider.clone(), capability.to_string()))?;
        let (mut a, mut b) = (0.0, 0.0);
        let mut issuers: Vec<_> = cell.iter().collect();
        issuers.sort_by(|x, y| x.0.cmp(y.0)); // deterministic summation order
        for (_, (ia, ib)) in issuers {
            let total = ia + ib;
            let scale = match self.params.issuer_cap {
                Some(cap) if total > cap && total > 0.0 => cap / total,
                _ => 1.0,
            };
            a += ia * scale;
            b += ib * scale;
        }
        let p = self.params.prior;
        Some(((a + p) / (a + b + 2.0 * p), a + b))
    }

    /// Recompute from evidence alone (T-1): this reproduces any incrementally
    /// maintained projection over the same receipts, weights and evaluation
    /// time.
    pub fn rebuild<F: Fn(&AgentId) -> f64>(
        store: &ReceiptStore,
        now_ms: u64,
        params: ProjParams,
        issuer_weight: F,
    ) -> Result<TrustProjection, TrustError> {
        let mut p = TrustProjection::new(params);
        for r in store.iter() {
            let v = r.verify()?; // defensive; ingest already verified
            let w = issuer_weight(&v.core.requester);
            let _ = p.apply(&v, now_ms, w); // skipped receipts are simply not evidence
        }
        Ok(p)
    }
}

/// Observer-local issuer weighting with the properties layer rule T-3
/// requires: the observer's own receipts weigh 1, issuers the observer has
/// explicitly configured weigh their configured value, and **every other
/// issuer weighs 0** (never the Beta prior's neutral 0.5). This zero
/// default is what defeats a Sybil ring of fresh requester identities.
///
/// How an observer *derives* weights for other issuers (transitive trust,
/// agreement with its own experience, …) is deferred to evaluation E1
/// (decision D5); this type only fixes the endpoints.
#[derive(Clone, Debug)]
pub struct IssuerWeights {
    observer: AgentId,
    known: HashMap<AgentId, f64>,
}

impl IssuerWeights {
    pub fn new(observer: AgentId) -> Self {
        IssuerWeights { observer, known: HashMap::new() }
    }

    /// Weigh `issuer` at `weight` (clamped to [0, 1]).
    pub fn with(mut self, issuer: AgentId, weight: f64) -> Self {
        let w = if weight.is_finite() { weight.clamp(0.0, 1.0) } else { 0.0 };
        self.known.insert(issuer, w);
        self
    }

    pub fn weight(&self, issuer: &AgentId) -> f64 {
        if *issuer == self.observer {
            1.0
        } else {
            self.known.get(issuer).copied().unwrap_or(0.0)
        }
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
            zone: None,
            resumed_from: None,
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
            receipt(&c, &b, 3, Outcome::Ok, false), // requester-only positive: not evidence (R-4)
            receipt(&a, &c, 4, Outcome::Disputed, true),
            receipt(&a, &b, 5, Outcome::Cancelled, true),
        ];
        for r in batch {
            if store.ingest(r.clone()).unwrap() == Ingest::Ingested {
                let _ = incremental.apply(&r.verify().unwrap(), now, 1.0);
            }
        }
        assert_eq!(store.len(), 5); // duplicate skipped

        let rebuilt = TrustProjection::rebuild(&store, now, params, |_| 1.0).unwrap();
        assert_eq!(rebuilt, incremental);

        let (score, conf) = rebuilt
            .score(&b.agent_id(), "map:cap:inference/text-generation")
            .unwrap();
        // b: one Ok and one Failed from `a` count; c's requester-only Ok does not.
        assert!((score - 0.5).abs() < 1e-12, "score {score}");
        assert!((conf - 2.0).abs() < 1e-3, "confidence {conf}"); // ~1 min of decay
        // Cancelled contributed no weight; disputed provider scored worse.
        let (score_c, _) = rebuilt
            .score(&c.agent_id(), "map:cap:inference/text-generation")
            .unwrap();
        assert!(score_c < score);
    }
}
