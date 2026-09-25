//! MAP identity bounded context (draft-map-identity-01).
//!
//! Aggregate root: the key lineage, identified by [`AgentId`].
//! Policy (ops §2/§8): identifiers are value types — raw byte strings never
//! cross API boundaries; validation lives in the constructor.
//!
//! Scaffold scope: AgentID (§3), keypairs, raw sign/verify. Delegation
//! certificates (§4), rotation/revocation (§7) are TODO and must be built
//! on `map-core`'s envelope (they are out-of-session envelopes).

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use std::fmt;

/// multicodec `ed25519-pub` (0xED as varint) — draft-map-identity-01 §3.1.
pub const ED25519_PUB_CODEC: [u8; 2] = [0xED, 0x01];
/// AgentID length for suite 1/2: 2-byte codec prefix + 32-byte key.
pub const ED25519_AGENT_ID_LEN: usize = 34;

#[derive(Debug)]
pub enum IdentityError {
    /// Wrong length or unknown multicodec prefix.
    BadAgentId,
    /// Multibase text form did not parse (missing 'z', bad base58, bad bytes).
    BadMultibase,
    /// The embedded public key is not a valid Ed25519 point.
    BadKey,
    /// Signature verification failed.
    BadSignature,
    /// Signature bytes were not 64 bytes long.
    BadSignatureLength,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IdentityError::BadAgentId => "invalid AgentID bytes",
            IdentityError::BadMultibase => "invalid multibase AgentID",
            IdentityError::BadKey => "invalid ed25519 public key",
            IdentityError::BadSignature => "signature verification failed",
            IdentityError::BadSignatureLength => "signature must be 64 bytes",
        };
        f.write_str(s)
    }
}
impl std::error::Error for IdentityError {}

/// Self-certifying agent identifier: `multicodec(ed25519-pub) || key`.
///
/// Byte-compatible with `did:key` and with libp2p Ed25519 PeerIDs
/// (identity §1). Always constructed validated.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentId(Vec<u8>);

impl AgentId {
    pub fn from_bytes(b: &[u8]) -> Result<Self, IdentityError> {
        if b.len() != ED25519_AGENT_ID_LEN || b[..2] != ED25519_PUB_CODEC {
            return Err(IdentityError::BadAgentId);
        }
        // Reject prefixes that don't decode to a curve point up front, so a
        // held AgentId is always verification-capable.
        let mut key = [0u8; 32];
        key.copy_from_slice(&b[2..]);
        VerifyingKey::from_bytes(&key).map_err(|_| IdentityError::BadKey)?;
        Ok(AgentId(b.to_vec()))
    }

    pub fn from_verifying_key(vk: &VerifyingKey) -> Self {
        let mut b = Vec::with_capacity(ED25519_AGENT_ID_LEN);
        b.extend_from_slice(&ED25519_PUB_CODEC);
        b.extend_from_slice(vk.as_bytes());
        AgentId(b)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        let mut key = [0u8; 32];
        key.copy_from_slice(&self.0[2..]);
        // Validated in the constructor; cannot fail here.
        VerifyingKey::from_bytes(&key).expect("AgentId key validated at construction")
    }

    /// Multibase base58btc text form, e.g. `z6Mk...` (identity §3.1).
    pub fn to_multibase(&self) -> String {
        format!("z{}", bs58::encode(&self.0).into_string())
    }

    pub fn from_multibase(s: &str) -> Result<Self, IdentityError> {
        let rest = s.strip_prefix('z').ok_or(IdentityError::BadMultibase)?;
        let bytes = bs58::decode(rest)
            .into_vec()
            .map_err(|_| IdentityError::BadMultibase)?;
        Self::from_bytes(&bytes)
    }

    /// `did:key` form (identity §3.1).
    pub fn did_key(&self) -> String {
        format!("did:key:{}", self.to_multibase())
    }
}

impl fmt::Debug for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AgentId({})", self.to_multibase())
    }
}
impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_multibase())
    }
}

/// An Ed25519 keypair. Root/device/session tiers (identity §2) are a
/// deployment discipline over this one type; the scaffold does not yet
/// model delegation chains.
#[derive(Clone)]
pub struct Keypair {
    signing: SigningKey,
}

impl Keypair {
    /// Generate from the OS RNG.
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).expect("OS RNG unavailable");
        Self::from_seed(seed)
    }

    /// Deterministic construction from a 32-byte seed (test vectors use
    /// SHA-256 of a label — see conformance crate).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Keypair {
            signing: SigningKey::from_bytes(&seed),
        }
    }

    pub fn agent_id(&self) -> AgentId {
        AgentId::from_verifying_key(&self.signing.verifying_key())
    }

    /// Sign arbitrary bytes. Callers in other contexts sign *deterministic
    /// CBOR structures*, never ad-hoc concatenations (critique C2).
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.signing.sign(msg).to_bytes()
    }
}

/// Verify `sig` over `msg` against the key embedded in `agent`.
pub fn verify(agent: &AgentId, msg: &[u8], sig: &[u8]) -> Result<(), IdentityError> {
    let sig: &[u8; 64] = sig
        .try_into()
        .map_err(|_| IdentityError::BadSignatureLength)?;
    let signature = Signature::from_bytes(sig);
    agent
        .verifying_key()
        .verify_strict(msg, &signature)
        .map_err(|_| IdentityError::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_multibase_and_sign() {
        let kp = Keypair::from_seed([7u8; 32]);
        let id = kp.agent_id();
        assert_eq!(id.as_bytes().len(), ED25519_AGENT_ID_LEN);
        assert!(id.to_multibase().starts_with('z'));
        assert_eq!(AgentId::from_multibase(&id.to_multibase()).unwrap(), id);
        assert!(id.did_key().starts_with("did:key:z"));

        let sig = kp.sign(b"map");
        verify(&id, b"map", &sig).unwrap();
        assert!(verify(&id, b"mAp", &sig).is_err());
    }

    #[test]
    fn rejects_bad_prefix() {
        let mut b = [0u8; ED25519_AGENT_ID_LEN];
        b[0] = 0x12;
        assert!(AgentId::from_bytes(&b).is_err());
    }
}
