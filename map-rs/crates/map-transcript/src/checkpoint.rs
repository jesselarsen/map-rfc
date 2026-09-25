//! Checkpoints and cross-provider resumption (spec/layer.md §7).
//!
//! A `CheckpointArtifact` mirrors an ExecutionReceipt: both parties sign the
//! exact same core bytes with a role-tagged input
//! `enc(["MAPCheckpoint", role, core-bytes])`, so the cumulative usage, zone
//! and voucher digest are covered by both signatures. The v1 checkpoint
//! acknowledgement signed only (task, seq, root) and left usage unsigned
//! (reviewer finding R4-02); that byte format is retired.

use crate::{Chunk, TranscriptBuilder};
use map_core::cbor::{decode, encode, CborError, Value};
use map_core::{TaskId, Usage};
use map_identity::{verify, AgentId, IdentityError, Keypair};
use sha2::{Digest, Sha256};
use std::fmt;

pub const ROLE_REQUESTER: u8 = 1;
pub const ROLE_PROVIDER: u8 = 2;

#[derive(Debug)]
pub enum CheckpointError {
    Cbor(CborError),
    Identity(IdentityError),
    MissingField(u64),
    BadField(u64),
    /// No signatures at all.
    NoSignatures,
    /// requester == provider.
    SelfDealing,
    /// A valid resumption point must be dual-signed (C-1/C-2).
    NotDualSigned,
    /// C-3/C-4: through-seq did not advance, usage regressed, or the task or
    /// parties changed between checkpoints.
    NotMonotone(&'static str),
    /// §7.4 step 2: the recomputed root over the prefix does not match key 5.
    RootMismatch,
    /// The prefix is not exactly seq 0..=through-seq in order.
    BadPrefix(&'static str),
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CheckpointError {}
impl From<CborError> for CheckpointError {
    fn from(e: CborError) -> Self {
        CheckpointError::Cbor(e)
    }
}
impl From<IdentityError> for CheckpointError {
    fn from(e: IdentityError) -> Self {
        CheckpointError::Identity(e)
    }
}

/// `sha2-256` multihash of `data` (0x12 0x20 || digest).
pub fn multihash_sha256(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x12, 0x20];
    out.extend_from_slice(&Sha256::digest(data));
    out
}

fn multihash_field(v: &Value, k: u64) -> Result<Option<Vec<u8>>, CheckpointError> {
    match v.map_get(k) {
        None => Ok(None),
        Some(x) => Ok(Some(
            x.as_bytes()
                .filter(|b| b.len() == 34 && b[..2] == [0x12, 0x20])
                .ok_or(CheckpointError::BadField(k))?
                .to_vec(),
        )),
    }
}

/// CheckpointCore (layer §7.1). Keys: 1 TaskID, 2 requester, 3 provider,
/// 4 through-seq (inclusive), 5 transcript root (Multihash), 6 cumulative
/// Usage, 7 zone, 8 timestamp, ?9 voucher-digest, ?10 resumed-from.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckpointCore {
    pub task: TaskId,
    pub requester: AgentId,
    pub provider: AgentId,
    pub through_seq: u64,
    pub root: [u8; 32],
    pub usage: Usage,
    pub zone: u8,
    pub timestamp_ms: u64,
    pub voucher_digest: Option<Vec<u8>>,
    pub resumed_from: Option<Vec<u8>>,
}

impl CheckpointCore {
    pub fn to_value(&self) -> Value {
        let mut root_mh = vec![0x12, 0x20];
        root_mh.extend_from_slice(&self.root);
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.task.0.to_vec())),
            (Value::Uint(2), Value::Bytes(self.requester.as_bytes().to_vec())),
            (Value::Uint(3), Value::Bytes(self.provider.as_bytes().to_vec())),
            (Value::Uint(4), Value::Uint(self.through_seq)),
            (Value::Uint(5), Value::Bytes(root_mh)),
            (Value::Uint(6), self.usage.to_value()),
            (Value::Uint(7), Value::Uint(self.zone as u64)),
            (Value::Uint(8), Value::Uint(self.timestamp_ms)),
        ];
        if let Some(d) = &self.voucher_digest {
            m.push((Value::Uint(9), Value::Bytes(d.clone())));
        }
        if let Some(r) = &self.resumed_from {
            m.push((Value::Uint(10), Value::Bytes(r.clone())));
        }
        Value::Map(m)
    }

    pub fn encode(&self) -> Result<Vec<u8>, CheckpointError> {
        Ok(encode(&self.to_value())?)
    }

    pub fn from_value(v: &Value) -> Result<CheckpointCore, CheckpointError> {
        let bytes = |k: u64| {
            v.map_get(k)
                .ok_or(CheckpointError::MissingField(k))?
                .as_bytes()
                .ok_or(CheckpointError::BadField(k))
        };
        let uint = |k: u64| {
            v.map_get(k)
                .ok_or(CheckpointError::MissingField(k))?
                .as_uint()
                .ok_or(CheckpointError::BadField(k))
        };
        let task = TaskId::from_slice(bytes(1)?).ok_or(CheckpointError::BadField(1))?;
        let requester = AgentId::from_bytes(bytes(2)?).map_err(|_| CheckpointError::BadField(2))?;
        let provider = AgentId::from_bytes(bytes(3)?).map_err(|_| CheckpointError::BadField(3))?;
        let through_seq = uint(4)?;
        let root_mh = bytes(5)?;
        if root_mh.len() != 34 || root_mh[..2] != [0x12, 0x20] {
            return Err(CheckpointError::BadField(5));
        }
        let root = <[u8; 32]>::try_from(&root_mh[2..]).expect("34-byte multihash");
        let usage = Usage::from_value(v.map_get(6).ok_or(CheckpointError::MissingField(6))?)
            .map_err(|_| CheckpointError::BadField(6))?;
        let zone = u8::try_from(uint(7)?)
            .ok()
            .filter(|z| *z <= 3)
            .ok_or(CheckpointError::BadField(7))?;
        let timestamp_ms = uint(8)?;
        Ok(CheckpointCore {
            task,
            requester,
            provider,
            through_seq,
            root,
            usage,
            zone,
            timestamp_ms,
            voucher_digest: multihash_field(v, 9)?,
            resumed_from: multihash_field(v, 10)?,
        })
    }
}

/// `enc(["MAPCheckpoint", role, core-bytes])`: the role tag prevents a
/// provider signature being passed off as a requester countersignature.
pub fn checkpoint_signing_input(role: u8, core_bytes: &[u8]) -> Vec<u8> {
    encode(&Value::Array(vec![
        Value::Text("MAPCheckpoint".into()),
        Value::Uint(role as u64),
        Value::Bytes(core_bytes.to_vec()),
    ]))
    .expect("array of primitives cannot fail to encode")
}

pub fn sign_checkpoint(kp: &Keypair, role: u8, core_bytes: &[u8]) -> [u8; 64] {
    kp.sign(&checkpoint_signing_input(role, core_bytes))
}

/// The wire artifact `{1: core-bytes, ?2: sig_req, ?3: sig_prov}`. Signatures
/// are verified over the received core bytes, never a re-serialization.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckpointArtifact {
    pub core_bytes: Vec<u8>,
    pub sig_requester: Option<[u8; 64]>,
    pub sig_provider: Option<[u8; 64]>,
}

/// A checkpoint whose present signatures verified.
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedCheckpoint {
    pub core: CheckpointCore,
    /// Both signatures present and valid: a valid resumption point (§7.2).
    pub dual: bool,
}

impl CheckpointArtifact {
    /// Provider-first construction (C-1): the provider signs its own core.
    pub fn propose(provider: &Keypair, core: &CheckpointCore) -> Result<Self, CheckpointError> {
        let core_bytes = core.encode()?;
        Ok(CheckpointArtifact {
            sig_provider: Some(sign_checkpoint(provider, ROLE_PROVIDER, &core_bytes)),
            sig_requester: None,
            core_bytes,
        })
    }

    /// Requester countersignature (C-2). The caller MUST first have checked
    /// the root against the content it received ([`verify_prefix`]).
    pub fn countersign(&mut self, requester: &Keypair) {
        self.sig_requester = Some(sign_checkpoint(requester, ROLE_REQUESTER, &self.core_bytes));
    }

    pub fn encode(&self) -> Result<Vec<u8>, CheckpointError> {
        let mut m = vec![(Value::Uint(1), Value::Bytes(self.core_bytes.clone()))];
        if let Some(s) = &self.sig_requester {
            m.push((Value::Uint(2), Value::Bytes(s.to_vec())));
        }
        if let Some(s) = &self.sig_provider {
            m.push((Value::Uint(3), Value::Bytes(s.to_vec())));
        }
        Ok(encode(&Value::Map(m))?)
    }

    pub fn decode_bytes(b: &[u8]) -> Result<CheckpointArtifact, CheckpointError> {
        let v = decode(b)?;
        let core_bytes = v
            .map_get(1)
            .ok_or(CheckpointError::MissingField(1))?
            .as_bytes()
            .ok_or(CheckpointError::BadField(1))?
            .to_vec();
        let sig = |k: u64| -> Result<Option<[u8; 64]>, CheckpointError> {
            match v.map_get(k) {
                None => Ok(None),
                Some(x) => Ok(Some(
                    x.as_bytes()
                        .and_then(|b| <[u8; 64]>::try_from(b).ok())
                        .ok_or(CheckpointError::BadField(k))?,
                )),
            }
        };
        Ok(CheckpointArtifact {
            core_bytes,
            sig_requester: sig(2)?,
            sig_provider: sig(3)?,
        })
    }

    pub fn core(&self) -> Result<CheckpointCore, CheckpointError> {
        CheckpointCore::from_value(&decode(&self.core_bytes)?)
    }

    /// Verify every present signature against the identity it claims (the
    /// requester signature against core key 2, the provider's against key 3)
    /// and reject self-dealing. Does not require dual signing; see
    /// [`Self::verify_resumption_point`].
    pub fn verify(&self) -> Result<VerifiedCheckpoint, CheckpointError> {
        let core = self.core()?;
        if self.sig_requester.is_none() && self.sig_provider.is_none() {
            return Err(CheckpointError::NoSignatures);
        }
        if core.requester == core.provider {
            return Err(CheckpointError::SelfDealing);
        }
        if let Some(s) = &self.sig_requester {
            verify(&core.requester, &checkpoint_signing_input(ROLE_REQUESTER, &self.core_bytes), s)?;
        }
        if let Some(s) = &self.sig_provider {
            verify(&core.provider, &checkpoint_signing_input(ROLE_PROVIDER, &self.core_bytes), s)?;
        }
        Ok(VerifiedCheckpoint {
            dual: self.sig_requester.is_some() && self.sig_provider.is_some(),
            core,
        })
    }

    /// §7.2: a valid resumption point is dual-signed and verifies.
    pub fn verify_resumption_point(&self) -> Result<CheckpointCore, CheckpointError> {
        let v = self.verify()?;
        if !v.dual {
            return Err(CheckpointError::NotDualSigned);
        }
        Ok(v.core)
    }

    /// Multihash of the encoded artifact: the `resumed-from` value a
    /// successor's receipt (key 13) and checkpoints (key 10) carry.
    pub fn digest(&self) -> Result<Vec<u8>, CheckpointError> {
        Ok(multihash_sha256(&self.encode()?))
    }
}

/// C-3/C-4: `next` may follow `prev` only if it is for the same task and
/// parties, advances through-seq, and no usage component regresses.
pub fn check_successor(prev: &CheckpointCore, next: &CheckpointCore) -> Result<(), CheckpointError> {
    if next.task != prev.task || next.requester != prev.requester || next.provider != prev.provider {
        return Err(CheckpointError::NotMonotone("task or parties changed"));
    }
    if next.through_seq <= prev.through_seq {
        return Err(CheckpointError::NotMonotone("through-seq did not advance"));
    }
    if !next.usage.dominates(&prev.usage) {
        return Err(CheckpointError::NotMonotone("usage regressed"));
    }
    Ok(())
}

/// Recompute the transcript root over `prefix` and compare it with the
/// checkpoint's root (C-2 for the requester before countersigning; §7.4
/// step 2 for a successor). `prefix` must be exactly the chunks with seq
/// 0..=through-seq, in order.
pub fn verify_prefix(core: &CheckpointCore, prefix: &[Chunk]) -> Result<TranscriptBuilder, CheckpointError> {
    let mut expected_seq = 0u64;
    let mut builder = TranscriptBuilder::new();
    for c in prefix {
        if c.seq != expected_seq {
            return Err(CheckpointError::BadPrefix("seq not contiguous from 0"));
        }
        builder.push(c);
        expected_seq += 1;
    }
    if prefix.is_empty() || expected_seq - 1 != core.through_seq {
        return Err(CheckpointError::BadPrefix("prefix does not end at through-seq"));
    }
    if builder.root() != core.root {
        return Err(CheckpointError::RootMismatch);
    }
    Ok(builder)
}

/// A successor's resumption (§7.4): accept `artifact` only if it is a valid
/// resumption point AND its root matches `prefix`. Returns the transcript
/// builder to continue from (the next chunk's seq is `through_seq + 1`) and
/// the artifact digest the successor's receipt carries as key 13.
pub fn resume(artifact: &CheckpointArtifact, prefix: &[Chunk]) -> Result<Resumed, CheckpointError> {
    let core = artifact.verify_resumption_point()?;
    let builder = verify_prefix(&core, prefix)?;
    Ok(Resumed {
        next_seq: core.through_seq + 1,
        resumed_from: artifact.digest()?,
        core,
        builder,
    })
}

/// The state a successor provider continues from.
pub struct Resumed {
    pub core: CheckpointCore,
    pub builder: TranscriptBuilder,
    pub next_seq: u64,
    pub resumed_from: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChunkKind;

    fn chunks(texts: &[&str]) -> Vec<Chunk> {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| Chunk { seq: i as u64, kind: ChunkKind::OutputText, content: t.as_bytes().to_vec() })
            .collect()
    }

    fn core(req: &Keypair, prov: &Keypair, prefix: &[Chunk], usage: Usage) -> CheckpointCore {
        let mut b = TranscriptBuilder::new();
        prefix.iter().for_each(|c| b.push(c));
        CheckpointCore {
            task: TaskId([9; 32]),
            requester: req.agent_id(),
            provider: prov.agent_id(),
            through_seq: prefix.len() as u64 - 1,
            root: b.root(),
            usage,
            zone: 2,
            timestamp_ms: 1_790_000_000_000,
            voucher_digest: None,
            resumed_from: None,
        }
    }

    #[test]
    fn propose_countersign_resume() {
        let (req, prov) = (Keypair::from_seed([1; 32]), Keypair::from_seed([2; 32]));
        let prefix = chunks(&["a", "b", "c"]);
        let c = core(&req, &prov, &prefix, Usage { tokens_in: 5, tokens_out: 3, ttft_ms: 1, wall_ms: 9 });
        let mut art = CheckpointArtifact::propose(&prov, &c).unwrap();
        // Provider-only is not a resumption point.
        assert!(matches!(art.verify_resumption_point(), Err(CheckpointError::NotDualSigned)));
        verify_prefix(&c, &prefix).unwrap(); // requester checks before countersigning (C-2)
        art.countersign(&req);
        let wire = CheckpointArtifact::decode_bytes(&art.encode().unwrap()).unwrap();
        assert_eq!(wire, art);
        let r = resume(&wire, &prefix).unwrap();
        assert_eq!(r.next_seq, 3);
        assert_eq!(r.resumed_from, art.digest().unwrap());
    }

    #[test]
    fn usage_is_covered_by_the_countersignature() {
        // R4-02 regression: editing usage inside a dual-signed checkpoint
        // must break verification.
        let (req, prov) = (Keypair::from_seed([1; 32]), Keypair::from_seed([2; 32]));
        let prefix = chunks(&["a"]);
        let c = core(&req, &prov, &prefix, Usage { tokens_in: 100, tokens_out: 80, ttft_ms: 1, wall_ms: 1 });
        let mut art = CheckpointArtifact::propose(&prov, &c).unwrap();
        art.countersign(&req);
        let mut edited = c.clone();
        edited.usage.tokens_in = 1;
        let forged = CheckpointArtifact { core_bytes: edited.encode().unwrap(), ..art };
        assert!(matches!(forged.verify(), Err(CheckpointError::Identity(_))));
    }

    #[test]
    fn root_mismatch_and_bad_prefix_are_rejected() {
        let (req, prov) = (Keypair::from_seed([1; 32]), Keypair::from_seed([2; 32]));
        let prefix = chunks(&["a", "b"]);
        let c = core(&req, &prov, &prefix, Usage::default());
        let mut art = CheckpointArtifact::propose(&prov, &c).unwrap();
        art.countersign(&req);
        assert!(matches!(resume(&art, &chunks(&["a", "X"])), Err(CheckpointError::RootMismatch)));
        assert!(matches!(resume(&art, &chunks(&["a"])), Err(CheckpointError::BadPrefix(_))));
        let mut gap = chunks(&["a", "b"]);
        gap[1].seq = 5;
        assert!(matches!(resume(&art, &gap), Err(CheckpointError::BadPrefix(_))));
    }

    #[test]
    fn self_dealing_and_monotonicity() {
        let (req, prov) = (Keypair::from_seed([1; 32]), Keypair::from_seed([2; 32]));
        let prefix = chunks(&["a", "b"]);
        let solo = core(&prov, &prov, &prefix, Usage::default());
        let mut art = CheckpointArtifact::propose(&prov, &solo).unwrap();
        art.countersign(&prov);
        assert!(matches!(art.verify(), Err(CheckpointError::SelfDealing)));

        let a = core(&req, &prov, &prefix, Usage { tokens_in: 10, tokens_out: 10, ttft_ms: 1, wall_ms: 10 });
        let mut b = a.clone();
        b.through_seq = 3;
        b.usage.tokens_out = 12;
        check_successor(&a, &b).unwrap();
        let mut stale = b.clone();
        stale.through_seq = a.through_seq;
        assert!(check_successor(&a, &stale).is_err());
        let mut regress = b.clone();
        regress.usage.tokens_in = 9;
        assert!(check_successor(&a, &regress).is_err());
    }
}
