//! Negative catalog (ops §6.2). Every case MUST be rejected with the listed
//! error CLASS — the matches assert the exact enum variant, not merely "some
//! error". Malformed CBOR is written as hand-built literal byte arrays (§8),
//! never produced by an "evil encoder".

use crate::from_hex;
use crate::positive::key_a;
use crate::vectors::*;
use map_core::cbor::{decode, CborError};
use map_core::envelope::{open, EnvelopeError};
use map_discovery::{shard, sharded_capability_key, DescriptorCache};
use map_identity::IdentityError;
use map_trust::{sign_receipt, Receipt, ReceiptStore, TrustError, ROLE_PROVIDER};
use sha2::{Digest, Sha256};

/// N1: flipping one byte of the TV-001 payload breaks the signature.
pub fn case_n1_tampered_payload() -> Result<(), String> {
    let mut env = from_hex(TV001_HEX)?;
    env[70] ^= 0x01; // offset 70 is inside the payload region [47, 100)
    match open(&env, b"") {
        Ok(_) => Err("tampered payload verified (must not)".into()),
        Err(EnvelopeError::Identity(IdentityError::BadSignature)) => Ok(()),
        Err(e) => Err(format!("expected bad-signature, got {e:?}")),
    }
}

/// N2: opening TV-001 under the wrong channel binding fails the signature.
pub fn case_n2_wrong_aad() -> Result<(), String> {
    let env = from_hex(TV001_HEX)?;
    match open(&env, b"x") {
        Ok(_) => Err("wrong external_aad verified (must not)".into()),
        Err(EnvelopeError::Identity(IdentityError::BadSignature)) => Ok(()),
        Err(e) => Err(format!("expected bad-signature, got {e:?}")),
    }
}

/// N3: the strict decoder rejects three profile violations (RFC 8949 §4.2.1).
pub fn case_n3_non_canonical() -> Result<(), String> {
    // (a) indefinite-length map: 0xbf opens a map with no count, 0xff breaks.
    match decode(&[0xbf, 0xff]) {
        Err(CborError::IndefiniteLength) => {}
        other => return Err(format!("N3a expected IndefiniteLength, got {other:?}")),
    }
    // (b) non-minimal length: the value 1 written with a 2-byte head
    //     (0x19 0x0001) instead of the 1-byte 0x01.
    match decode(&[0x19, 0x00, 0x01]) {
        Err(CborError::NonShortestForm) => {}
        other => return Err(format!("N3b expected NonShortestForm, got {other:?}")),
    }
    // (c) unsorted map keys: {2:0, 1:0} — key 1 must precede key 2 bytewise.
    match decode(&[0xa2, 0x02, 0x00, 0x01, 0x00]) {
        Err(CborError::UnsortedMapKeys) => {}
        other => return Err(format!("N3c expected UnsortedMapKeys, got {other:?}")),
    }
    Ok(())
}

/// N4: a truncated envelope fails to decode (EOF).
pub fn case_n4_truncated() -> Result<(), String> {
    let env = from_hex(TV001_HEX)?;
    match decode(&env[..env.len() - 1]) {
        Err(CborError::UnexpectedEof) => Ok(()),
        other => Err(format!("N4 expected UnexpectedEof, got {other:?}")),
    }
}

/// N5: the wrong outer tag (17 instead of COSE_Sign1's 18) is not a MAP envelope.
pub fn case_n5_wrong_tag() -> Result<(), String> {
    let mut env = from_hex(TV001_HEX)?;
    env[0] = 0xd1; // tag(17)
    match open(&env, b"") {
        Ok(_) => Err("wrong-tag envelope opened (must not)".into()),
        Err(EnvelopeError::NotCoseSign1) => Ok(()),
        Err(e) => Err(format!("N5 expected NotCoseSign1, got {e:?}")),
    }
}

/// N6: a lower descriptor seq is stale (LWW single-writer).
pub fn case_n6_stale_seq() -> Result<(), String> {
    let agent = key_a().agent_id();
    let mut cache = DescriptorCache::new();
    if cache.ingest(&agent, 5, vec![1]) != map_discovery::Ingest::Accepted {
        return Err("N6 first ingest (seq 5) not Accepted".into());
    }
    match cache.ingest(&agent, 4, vec![2]) {
        map_discovery::Ingest::Stale => Ok(()),
        other => Err(format!("N6 expected Stale, got {other:?}")),
    }
}

/// N7: shard is the first byte of the AgentID *digest* (not the 0x12 multihash
/// prefix), and distinct shards yield distinct sharded keys.
pub fn case_n7_wrong_shard() -> Result<(), String> {
    let agent = key_a().agent_id();
    let s = shard(&agent);
    let digest_first = Sha256::digest(agent.as_bytes())[0];
    if s != digest_first {
        return Err(format!(
            "N7 shard {s:#04x} != digest first byte {digest_first:#04x}"
        ));
    }
    let cap = "map:cap:inference/text-generation";
    if sharded_capability_key(cap, s) == sharded_capability_key(cap, s.wrapping_add(1)) {
        return Err("N7 sharded keys collided across different shards".into());
    }
    Ok(())
}

/// N8: re-ingesting the same receipt is idempotent (Duplicate, len unchanged).
pub fn case_n8_duplicate_receipt() -> Result<(), String> {
    let receipt = Receipt::decode_bytes(&from_hex(TV002_HEX)?)
        .map_err(|e| format!("decode: {e}"))?;
    let mut store = ReceiptStore::new();
    if store.ingest(receipt.clone()).map_err(|e| format!("ingest1: {e}"))?
        != map_trust::Ingest::Ingested
    {
        return Err("N8 first ingest not Ingested".into());
    }
    if store.ingest(receipt).map_err(|e| format!("ingest2: {e}"))?
        != map_trust::Ingest::Duplicate
    {
        return Err("N8 second ingest not Duplicate".into());
    }
    if store.len() != 1 {
        return Err(format!("N8 store len {} != 1", store.len()));
    }
    Ok(())
}

/// N9: swapping the two signatures breaks verification (each role tag binds a
/// specific signer).
pub fn case_n9_swapped_roles() -> Result<(), String> {
    let r = Receipt::decode_bytes(&from_hex(TV002_HEX)?)
        .map_err(|e| format!("decode: {e}"))?;
    let swapped = Receipt {
        core_bytes: r.core_bytes.clone(),
        sig_requester: r.sig_provider,
        sig_provider: r.sig_requester,
    };
    match swapped.verify() {
        Ok(_) => Err("N9 swapped-role receipt verified (must not)".into()),
        Err(TrustError::Identity(_)) => Ok(()),
        Err(e) => Err(format!("N9 expected identity error, got {e:?}")),
    }
}

/// N10: a signature over the provider role-domain, placed in the requester
/// field, fails — the role tag is part of the signed message (role binding).
pub fn case_n10_role_domain() -> Result<(), String> {
    let core_bytes = from_hex(TV002_CORE_HEX)?;
    // Key A signs the ROLE_PROVIDER domain, but we file it as the requester sig.
    let cross_domain = sign_receipt(&key_a(), ROLE_PROVIDER, &core_bytes);
    let receipt = Receipt {
        core_bytes,
        sig_requester: Some(cross_domain),
        sig_provider: None,
    };
    match receipt.verify() {
        Ok(_) => Err("N10 role-domain-crossed signature verified (must not)".into()),
        Err(TrustError::Identity(_)) => Ok(()),
        Err(e) => Err(format!("N10 expected identity error, got {e:?}")),
    }
}
