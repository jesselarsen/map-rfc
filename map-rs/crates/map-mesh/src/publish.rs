//! Descriptor publish/ingest (discovery §2/§5) and the republish timer.

use crate::MeshError;
use map_core::{msg_type, open, seal, Body, MsgId, PROTOCOL_VERSION_1_0};
use map_discovery::{multihash_sha256, Descriptor, DescriptorCache, Ingest};
use map_identity::Keypair;

/// Descriptor validity ceiling (discovery §2): ≤ 24 h.
pub const MAX_DESCRIPTOR_VALIDITY_MS: u64 = 24 * 3600 * 1000;
/// Tolerated clock skew for a descriptor's timestamp (layer R-3 default).
pub const DESCRIPTOR_CLOCK_SKEW_MS: u64 = 120_000;
/// Republish interval ceiling (discovery §5): ≤ 6 h.
pub const MAX_REPUBLISH_INTERVAL_MS: u64 = 6 * 3600 * 1000;

/// FR-2: build a self-signed descriptor envelope (out-of-session, msg-type
/// 0x0100, empty external_aad, ttl REQUIRED per core §7.4, 0 < ttl ≤ 24 h). The
/// descriptor's subject MUST be the signer (a descriptor is self-certifying).
pub fn publish_descriptor(
    kp: &Keypair,
    descriptor: &Descriptor,
    msg_id: [u8; 16],
    now_ms: u64,
    ttl_ms: u64,
) -> Result<Vec<u8>, MeshError> {
    if descriptor.agent != kp.agent_id() {
        return Err(MeshError::InvalidDescriptor("subject is not the signer".into()));
    }
    if ttl_ms == 0 || ttl_ms > MAX_DESCRIPTOR_VALIDITY_MS {
        return Err(MeshError::InvalidDescriptor("ttl out of range (0, 24h]".into()));
    }
    let content = descriptor.to_value().map_err(MeshError::Discovery)?;
    let body = Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: msg_type::DESCRIPTOR,
        msg_id: MsgId(msg_id),
        timestamp_ms: now_ms,
        recipient: None,
        ttl_ms: Some(ttl_ms),
        session_id: None,
        seq: None,
        content,
    };
    seal(kp, &body, b"").map_err(MeshError::Envelope)
}

/// FR-2: verify a received descriptor envelope and update a client cache.
/// Checks: signature (via `open`, over received bytes — I1), msg-type,
/// self-signature (kid == subject), ttl not expired; then seq-LWW into the
/// `DescriptorCache`. Returns the parsed descriptor and the cache outcome.
/// cid = `multihash_sha256(envelope-bytes)` (discovery §5). Atomic w.r.t. the
/// cache: all checks pass before the single `cache.ingest` (§6 no partial).
pub fn ingest_descriptor_envelope(
    bytes: &[u8],
    cache: &mut DescriptorCache,
    now_ms: u64,
) -> Result<(Descriptor, Ingest), MeshError> {
    let opened = open(bytes, b"").map_err(MeshError::Envelope)?;
    if opened.body.msg_type != msg_type::DESCRIPTOR {
        return Err(MeshError::InvalidDescriptor("not a DESCRIPTOR envelope".into()));
    }
    let desc = Descriptor::from_value(&opened.body.content).map_err(MeshError::Discovery)?;
    if desc.agent != opened.sender {
        return Err(MeshError::InvalidDescriptor("kid != subject".into()));
    }
    let ttl = opened
        .body
        .ttl_ms
        .ok_or_else(|| MeshError::InvalidDescriptor("descriptor missing ttl".into()))?;
    // Layer D-5 (finding A8): validity is checked on ingest, including the
    // 24 h ceiling and a bounded clock skew. Without these, a descriptor could
    // claim a century-long ttl or a future timestamp and never expire.
    if ttl == 0 || ttl > MAX_DESCRIPTOR_VALIDITY_MS {
        return Err(MeshError::InvalidDescriptor("descriptor ttl out of range (0, 24h]".into()));
    }
    if opened.body.timestamp_ms > now_ms.saturating_add(DESCRIPTOR_CLOCK_SKEW_MS) {
        return Err(MeshError::InvalidDescriptor("descriptor timestamp in the future".into()));
    }
    if now_ms > opened.body.timestamp_ms.saturating_add(ttl) {
        return Err(MeshError::InvalidDescriptor("descriptor ttl expired".into()));
    }
    let cid = multihash_sha256(bytes);
    let ingest = cache.ingest(&desc.agent, desc.seq, cid);
    Ok((desc, ingest))
}

/// FR-6: whether the descriptor should be republished (interval ≤ 6 h elapsed).
pub fn needs_republish(last_published_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_published_ms) >= MAX_REPUBLISH_INTERVAL_MS
}
