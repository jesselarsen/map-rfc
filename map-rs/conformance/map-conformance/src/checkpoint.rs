//! v2 checkpoint vectors (layer §7): TV-021 produce + consume, NV-013 and
//! NV-014 reject.

use crate::positive::{key_a, key_b};
use crate::vectors::TV002_TASK_ID_HEX;
use crate::vectors3::*;
use crate::{assert_bytes, from_hex};
use map_core::{TaskId, Usage};
use map_transcript::checkpoint::{sign_checkpoint, ROLE_PROVIDER, ROLE_REQUESTER};
use map_transcript::{resume, CheckpointArtifact, CheckpointCore, CheckpointError, Chunk, ChunkKind};

fn prefix() -> Vec<Chunk> {
    TV021_PREFIX
        .iter()
        .enumerate()
        .map(|(i, s)| Chunk { seq: i as u64, kind: ChunkKind::OutputText, content: s.as_bytes().to_vec() })
        .collect()
}

fn tv021_core() -> Result<CheckpointCore, String> {
    let root = <[u8; 32]>::try_from(from_hex(TV021_ROOT_HEX)?.as_slice()).map_err(|_| "root length")?;
    Ok(CheckpointCore {
        task: TaskId::from_slice(&from_hex(TV002_TASK_ID_HEX)?).ok_or("task id length")?,
        requester: key_a().agent_id(),
        provider: key_b().agent_id(),
        through_seq: 2,
        root,
        usage: Usage { tokens_in: 10, tokens_out: 3, ttft_ms: 120, wall_ms: 900 },
        zone: 2,
        timestamp_ms: 1_780_000_000_900,
        voucher_digest: None,
        resumed_from: None,
    })
}

/// TV-021 produce: the core bytes and both signatures reproduce byte-for-byte
/// from the seeds (Ed25519 is deterministic).
pub fn case_tv021_produce() -> Result<(), String> {
    let core = tv021_core()?;
    let core_bytes = core.encode().map_err(|e| format!("encode: {e}"))?;
    assert_bytes(TV021_CORE_HEX, &core_bytes)?;
    let art = CheckpointArtifact {
        sig_requester: Some(sign_checkpoint(&key_a(), ROLE_REQUESTER, &core_bytes)),
        sig_provider: Some(sign_checkpoint(&key_b(), ROLE_PROVIDER, &core_bytes)),
        core_bytes,
    };
    assert_bytes(TV021_ARTIFACT_HEX, &art.encode().map_err(|e| format!("encode: {e}"))?)
}

/// TV-021 consume: it is a valid resumption point, its root matches the
/// published prefix, and a successor continues at seq 3.
pub fn case_tv021_consume() -> Result<(), String> {
    let art = CheckpointArtifact::decode_bytes(&from_hex(TV021_ARTIFACT_HEX)?).map_err(|e| format!("decode: {e}"))?;
    let r = resume(&art, &prefix()).map_err(|e| format!("resume: {e}"))?;
    if r.core != tv021_core()? {
        return Err("decoded core differs from the published fields".into());
    }
    if r.next_seq != 3 {
        return Err(format!("next seq {} != 3", r.next_seq));
    }
    Ok(())
}

/// NV-013: corrupted requester countersignature → not a resumption point.
pub fn case_nv013_bad_countersig() -> Result<(), String> {
    let art = CheckpointArtifact::decode_bytes(&from_hex(NV013_HEX)?).map_err(|e| format!("decode: {e}"))?;
    match resume(&art, &prefix()) {
        Err(CheckpointError::Identity(_)) => Ok(()),
        other => Err(format!("NV-013 expected a signature failure, got {:?}", other.map(|r| r.next_seq))),
    }
}

/// NV-014: signatures valid, root does not match the prefix → resume refused.
pub fn case_nv014_root_mismatch() -> Result<(), String> {
    let art = CheckpointArtifact::decode_bytes(&from_hex(NV014_HEX)?).map_err(|e| format!("decode: {e}"))?;
    art.verify_resumption_point().map_err(|e| format!("NV-014 signatures must verify: {e}"))?;
    match resume(&art, &prefix()) {
        Err(CheckpointError::RootMismatch) => Ok(()),
        other => Err(format!("NV-014 expected RootMismatch, got {:?}", other.map(|r| r.next_seq))),
    }
}
