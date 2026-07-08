//! Delegation certificates and chain verification (identity §4, core ProtHdr
//! key 33).
//!
//! A `DelegationCert` IS an envelope (msg-type `DELEGATE` = 0x0502,
//! out-of-session, empty `external_aad`) whose `kid` is the ISSUER. A data
//! envelope may carry a chain of them in ProtHdr key 33, ordered leaf..root;
//! [`crate::envelope::open_delegated`] verifies the leaf signature and this
//! module validates the chain.
//!
//! ## Where this lives (code-organization note)
//! TASK-010 §4 suggested `delegation.rs` in `map-identity`, but a cert IS an
//! envelope (owned by `map-core`) and the canonical CBOR codec lives here too;
//! `map-identity` depends on neither. Hosting delegation in `map-identity`
//! would invert the dependency graph, which NFR §3 forbids ("dependency graph
//! unchanged"). The DelegateBody *wire format* (identity §4 CDDL) is unchanged
//! — only the host crate differs, an internal detail invisible to the wire.
//!
//! ## Load-bearing security properties (identity §4, decision 5)
//! - **Tighten-only attenuation**: a child link may only NARROW cap scope,
//!   zone scope, and delegation depth relative to its parent. A widening →
//!   [`DelegError::WidenedScope`].
//! - **Fail-closed caveats**: an unknown caveat key invalidates the whole
//!   chain (a caveat is a restriction; ignoring one would widen scope).
//!
//! Revocation (identity §7) is OUT of scope: there is no distribution
//! mechanism yet, so chains rely on short `not-after` windows instead.

use crate::cbor::Value;
use crate::envelope::{open, seal, Body, EnvelopeError, MsgId, PROTOCOL_VERSION_1_0};
use crate::msg_type;
use map_identity::{AgentId, IdentityError, Keypair};

/// Registered caveat key: task pinning — value is a 32-byte TaskID (tasks §1).
pub const CAVEAT_TASK_PIN: u64 = 1;
/// Registered caveat key: audience — value is a 34-byte AgentID (identity §3).
pub const CAVEAT_AUDIENCE: u64 = 2;

/// Hard cap on chain length (NFR §3; DoS bound — 8 Ed25519 verifies worst case).
pub const MAX_CHAIN_LEN: usize = 8;

/// A parsed, registered caveat (identity §4 key 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caveat {
    /// Chain is valid only for envelopes whose Body references this TaskID.
    /// Enforced by the CALLER (`open_delegated` returns it; taskrun checks).
    TaskPin([u8; 32]),
    /// Chain is valid only when the validating party's AgentID equals this.
    Audience(AgentId),
}

/// The `DelegateBody` carried in Body key 10 of a DELEGATE envelope
/// (identity §4 CDDL). Field numbers are exactly per the draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegateBody {
    pub issuer: AgentId,           // 1
    pub subject: AgentId,          // 2
    pub not_before_ms: u64,        // 3
    pub not_after_ms: u64,         // 4
    pub caps: Option<Vec<String>>, // 5 (absent = inherit parent scope)
    pub zones: Option<Vec<u8>>,    // 6
    pub max_depth: Option<u64>,    // 7
    pub caveats: Vec<Caveat>,      // 8
}

/// Parse-level failure for a single body (index-free; the chain adds the index).
enum FieldErr {
    Malformed,
    /// A caveat key not in the registry — fail closed (identity §4 ruling).
    UnknownCaveat(i128),
}

impl DelegateBody {
    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (Value::Uint(1), Value::Bytes(self.issuer.as_bytes().to_vec())),
            (Value::Uint(2), Value::Bytes(self.subject.as_bytes().to_vec())),
            (Value::Uint(3), Value::Uint(self.not_before_ms)),
            (Value::Uint(4), Value::Uint(self.not_after_ms)),
        ];
        if let Some(caps) = &self.caps {
            m.push((
                Value::Uint(5),
                Value::Array(caps.iter().map(|c| Value::Text(c.clone())).collect()),
            ));
        }
        if let Some(zones) = &self.zones {
            m.push((
                Value::Uint(6),
                Value::Array(zones.iter().map(|z| Value::Uint(*z as u64)).collect()),
            ));
        }
        if let Some(d) = self.max_depth {
            m.push((Value::Uint(7), Value::Uint(d)));
        }
        if !self.caveats.is_empty() {
            let cm = self
                .caveats
                .iter()
                .map(|c| match c {
                    Caveat::TaskPin(t) => {
                        (Value::Uint(CAVEAT_TASK_PIN), Value::Bytes(t.to_vec()))
                    }
                    Caveat::Audience(a) => (
                        Value::Uint(CAVEAT_AUDIENCE),
                        Value::Bytes(a.as_bytes().to_vec()),
                    ),
                })
                .collect();
            m.push((Value::Uint(8), Value::Map(cm)));
        }
        Value::Map(m)
    }

    fn from_value(v: &Value) -> Result<DelegateBody, FieldErr> {
        let agent = |k: u64| -> Result<AgentId, FieldErr> {
            AgentId::from_bytes(v.map_get(k).and_then(|x| x.as_bytes()).ok_or(FieldErr::Malformed)?)
                .map_err(|_| FieldErr::Malformed)
        };
        let uint = |k: u64| -> Result<u64, FieldErr> {
            v.map_get(k).and_then(|x| x.as_uint()).ok_or(FieldErr::Malformed)
        };
        let issuer = agent(1)?;
        let subject = agent(2)?;
        let not_before_ms = uint(3)?;
        let not_after_ms = uint(4)?;

        let caps = match v.map_get(5) {
            None => None,
            Some(x) => {
                let arr = x.as_array().ok_or(FieldErr::Malformed)?;
                let mut out = Vec::with_capacity(arr.len());
                for c in arr {
                    out.push(c.as_text().ok_or(FieldErr::Malformed)?.to_string());
                }
                Some(out)
            }
        };
        let zones = match v.map_get(6) {
            None => None,
            Some(x) => {
                let arr = x.as_array().ok_or(FieldErr::Malformed)?;
                let mut out = Vec::with_capacity(arr.len());
                for z in arr {
                    out.push(u8::try_from(z.as_uint().ok_or(FieldErr::Malformed)?)
                        .map_err(|_| FieldErr::Malformed)?);
                }
                Some(out)
            }
        };
        let max_depth = match v.map_get(7) {
            None => None,
            Some(x) => Some(x.as_uint().ok_or(FieldErr::Malformed)?),
        };
        let caveats = match v.map_get(8) {
            None => Vec::new(),
            Some(x) => {
                let m = x.as_map().ok_or(FieldErr::Malformed)?;
                let mut out = Vec::with_capacity(m.len());
                for (k, val) in m {
                    let key = k.as_i128().ok_or(FieldErr::Malformed)?;
                    let bytes = val.as_bytes().ok_or(FieldErr::Malformed)?;
                    match u64::try_from(key).ok() {
                        Some(CAVEAT_TASK_PIN) => out.push(Caveat::TaskPin(
                            <[u8; 32]>::try_from(bytes).map_err(|_| FieldErr::Malformed)?,
                        )),
                        Some(CAVEAT_AUDIENCE) => out.push(Caveat::Audience(
                            AgentId::from_bytes(bytes).map_err(|_| FieldErr::Malformed)?,
                        )),
                        // Fail closed: an unknown caveat is a restriction we
                        // cannot evaluate, so the chain is invalid.
                        _ => return Err(FieldErr::UnknownCaveat(key)),
                    }
                }
                out
            }
        };
        Ok(DelegateBody {
            issuer,
            subject,
            not_before_ms,
            not_after_ms,
            caps,
            zones,
            max_depth,
            caveats,
        })
    }
}

/// Chain/certificate validation failure. All map to wire code 2003
/// (BAD_DELEGATION) except [`DelegError::Empty`]/[`DelegError::Malformed`]
/// which are MALFORMED (1000) — a wire-format problem, not an authz decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegError {
    /// A cert's envelope signature failed, or its issuer field != its kid.
    BadSignature { index: usize },
    Expired { index: usize },
    NotYetValid { index: usize },
    /// A link doesn't join (subject≠next issuer), or a self/duplicate link.
    BrokenLink { index: usize },
    DepthExceeded,
    /// A child link widened a cap/zone scope relative to its parent.
    WidenedScope { index: usize },
    /// A caveat key outside the registry — fail closed.
    UnknownCaveat { index: usize, key: i128 },
    /// A known caveat's condition was not met (e.g. audience mismatch).
    CaveatUnsatisfied { index: usize, key: u64 },
    TooLong,
    Empty,
    Malformed { index: usize },
}

impl DelegError {
    /// Wire error code (core §9). Chain-authz failures are 2003; structural
    /// wire problems are 1000 (edge case: empty key-33 array).
    pub fn wire_code(&self) -> u16 {
        use crate::errors::codes;
        match self {
            DelegError::Empty | DelegError::Malformed { .. } => codes::MALFORMED,
            _ => codes::BAD_DELEGATION,
        }
    }
}

impl std::fmt::Display for DelegError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "delegation error (wire {}): {:?}", self.wire_code(), self)
    }
}
impl std::error::Error for DelegError {}

/// Reason a single cert failed, before the chain assigns it an index.
enum CertFail {
    BadSignature,
    Expired,
    NotYetValid,
    Malformed,
    UnknownCaveat(i128),
}

impl CertFail {
    fn at(self, index: usize) -> DelegError {
        match self {
            CertFail::BadSignature => DelegError::BadSignature { index },
            CertFail::Expired => DelegError::Expired { index },
            CertFail::NotYetValid => DelegError::NotYetValid { index },
            CertFail::Malformed => DelegError::Malformed { index },
            CertFail::UnknownCaveat(key) => DelegError::UnknownCaveat { index, key },
        }
    }
}

/// Open a cert envelope, verify the issuer signature (kid==issuer), and check
/// the validity window. Returns the parsed body on success.
fn verify_cert_bytes(bytes: &[u8], now_ms: u64) -> Result<DelegateBody, CertFail> {
    let opened = open(bytes, b"").map_err(|e| match e {
        EnvelopeError::Identity(IdentityError::BadSignature) => CertFail::BadSignature,
        _ => CertFail::Malformed,
    })?;
    if opened.body.msg_type != msg_type::DELEGATE {
        return Err(CertFail::Malformed);
    }
    let body = DelegateBody::from_value(&opened.body.content).map_err(|e| match e {
        FieldErr::Malformed => CertFail::Malformed,
        FieldErr::UnknownCaveat(k) => CertFail::UnknownCaveat(k),
    })?;
    // The issuer field must equal the signing key (kid) — else the cert claims
    // an authority it was not signed by.
    if body.issuer != opened.sender {
        return Err(CertFail::BadSignature);
    }
    if now_ms < body.not_before_ms {
        return Err(CertFail::NotYetValid);
    }
    if now_ms > body.not_after_ms {
        return Err(CertFail::Expired);
    }
    Ok(body)
}

/// Issue a DelegationCert: seal a DELEGATE envelope signed by the parent.
/// `body.issuer` MUST equal `parent_kp.agent_id()` (the kid); the returned
/// bytes are what a peer carries in a chain.
pub fn issue(parent_kp: &Keypair, body: &DelegateBody) -> Result<Vec<u8>, EnvelopeError> {
    let env_body = Body {
        version: PROTOCOL_VERSION_1_0,
        msg_type: msg_type::DELEGATE,
        msg_id: MsgId::random(),
        timestamp_ms: body.not_before_ms,
        recipient: None,
        ttl_ms: None,
        session_id: None,
        seq: None,
        content: body.to_value(),
    };
    seal(parent_kp, &env_body, b"")
}

/// Validate a single certificate (signature, window, issuer==kid).
pub fn validate_cert(bytes: &[u8], now_ms: u64) -> Result<DelegateBody, DelegError> {
    verify_cert_bytes(bytes, now_ms).map_err(|f| f.at(0))
}

/// The authority a validated chain confers. `caps`/`zones` = `None` means
/// "unrestricted at this level" (no cert on the path narrowed it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveScope {
    /// The accountable root identity (chain terminus; receipts accrue here).
    pub root: AgentId,
    pub caps: Option<Vec<String>>,
    pub zones: Option<Vec<u8>>,
    /// Remaining further-delegation depth the leaf subject retains.
    pub depth_left: u32,
    /// Task pin (if any cert pinned one) — the CALLER enforces it against the
    /// envelope's Body content (§8 split: this module returns, taskrun checks).
    pub task_pin: Option<[u8; 32]>,
}

impl EffectiveScope {
    /// Enforcement primitive (used by tool grants / gateway, TASK-016).
    pub fn permits(&self, cap: &str, zone: u8) -> bool {
        let cap_ok = match &self.caps {
            None => true,
            Some(cs) => cs.iter().any(|p| cap_within(cap, p)),
        };
        let zone_ok = match &self.zones {
            None => true,
            Some(zs) => zs.contains(&zone),
        };
        cap_ok && zone_ok
    }
}

/// `child` is same-or-narrower than `parent` under prefix-segment matching
/// (discovery §4). Mirrors `map_discovery::cap_matches(child, parent)`;
/// inlined to keep `map-core` free of a `map-discovery` dependency (which
/// would create a cycle — `map-discovery` builds on `map-core`).
fn cap_within(child: &str, parent: &str) -> bool {
    child == parent || child.strip_prefix(parent).is_some_and(|r| r.starts_with('/'))
}

/// Validate a delegation chain (ordered leaf..root per core ProtHdr key 33).
///
/// - `leaf_signer` is the data envelope's kid; `chain[0].subject` MUST equal it.
/// - `audience` is the validating party's own AgentID, needed to evaluate an
///   audience caveat (§8). Pass `None` if not applicable; a chain bearing an
///   audience caveat then fails closed. (This parameter extends TASK-010 FR-2's
///   listed signature — the §8 audience caveat cannot be checked without it.)
///
/// Returns the [`EffectiveScope`] the chain confers, including the accountable
/// root. Never panics.
pub fn validate_chain(
    chain: &[Vec<u8>],
    leaf_signer: &AgentId,
    audience: Option<&AgentId>,
    now_ms: u64,
) -> Result<EffectiveScope, DelegError> {
    if chain.is_empty() {
        return Err(DelegError::Empty);
    }
    if chain.len() > MAX_CHAIN_LEN {
        return Err(DelegError::TooLong);
    }

    // 1. Each cert: signature + window + issuer==kid.
    let mut certs = Vec::with_capacity(chain.len());
    for (i, c) in chain.iter().enumerate() {
        certs.push(verify_cert_bytes(c, now_ms).map_err(|f| f.at(i))?);
    }

    // 2. Structural links (leaf..root). No self-delegation, no duplicate link.
    if certs[0].subject != *leaf_signer {
        return Err(DelegError::BrokenLink { index: 0 });
    }
    for (i, c) in certs.iter().enumerate() {
        if c.issuer == c.subject {
            return Err(DelegError::BrokenLink { index: i });
        }
    }
    for i in 0..certs.len() - 1 {
        if certs[i].issuer != certs[i + 1].subject {
            return Err(DelegError::BrokenLink { index: i + 1 });
        }
    }
    // A subject appearing twice indicates a loop / replayed link.
    for i in 0..certs.len() {
        for j in (i + 1)..certs.len() {
            if certs[i].subject == certs[j].subject && certs[i].issuer == certs[j].issuer {
                return Err(DelegError::BrokenLink { index: j });
            }
        }
    }

    let root = certs.last().unwrap().issuer.clone();

    // 3. Attenuation + caveats, walking root -> leaf (authority narrows down).
    let mut eff_caps: Option<Vec<String>> = None;
    let mut eff_zones: Option<Vec<u8>> = None;
    let mut budget: Option<u64> = None; // None = unbounded
    let mut task_pin: Option<[u8; 32]> = None;

    for (step, i) in (0..certs.len()).rev().enumerate() {
        let cert = &certs[i];

        // Each step below the root consumes one delegation level of the
        // parent's remaining budget.
        if step > 0 {
            match budget {
                Some(0) => return Err(DelegError::DepthExceeded),
                Some(b) => budget = Some(b - 1),
                None => {}
            }
        }
        if let Some(md) = cert.max_depth {
            budget = Some(match budget {
                Some(b) => b.min(md),
                None => md,
            });
        }

        // Caps: a child scope MUST be a subset of the parent effective scope.
        if let Some(cc) = &cert.caps {
            if let Some(ec) = &eff_caps {
                for c in cc {
                    if !ec.iter().any(|p| cap_within(c, p)) {
                        return Err(DelegError::WidenedScope { index: i });
                    }
                }
            }
            eff_caps = Some(cc.clone());
        }

        // Zones: child zones MUST be a subset of the parent effective zones.
        if let Some(cz) = &cert.zones {
            if let Some(ez) = &eff_zones {
                if !cz.iter().all(|z| ez.contains(z)) {
                    return Err(DelegError::WidenedScope { index: i });
                }
            }
            eff_zones = Some(cz.clone());
        }

        // Caveats.
        for cav in &cert.caveats {
            match cav {
                Caveat::TaskPin(t) => match &task_pin {
                    Some(existing) if existing != t => {
                        return Err(DelegError::CaveatUnsatisfied {
                            index: i,
                            key: CAVEAT_TASK_PIN,
                        });
                    }
                    _ => task_pin = Some(*t),
                },
                Caveat::Audience(a) => {
                    if audience != Some(a) {
                        return Err(DelegError::CaveatUnsatisfied {
                            index: i,
                            key: CAVEAT_AUDIENCE,
                        });
                    }
                }
            }
        }
    }

    Ok(EffectiveScope {
        root,
        caps: eff_caps,
        zones: eff_zones,
        depth_left: budget.map(|b| b.min(u32::MAX as u64) as u32).unwrap_or(u32::MAX),
        task_pin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbor::encode;

    const T0: u64 = 1_780_000_000_000;
    const NOW: u64 = 1_780_000_100_000;
    const LATER: u64 = 1_780_000_600_000;

    fn kp(seed: u8) -> Keypair {
        Keypair::from_seed([seed; 32])
    }

    // issuer -> subject cert with an optional cap scope, valid [T0, LATER].
    fn cert(
        issuer: &Keypair,
        subject: &Keypair,
        caps: Option<Vec<&str>>,
        zones: Option<Vec<u8>>,
        max_depth: Option<u64>,
        caveats: Vec<Caveat>,
    ) -> Vec<u8> {
        let body = DelegateBody {
            issuer: issuer.agent_id(),
            subject: subject.agent_id(),
            not_before_ms: T0,
            not_after_ms: LATER,
            caps: caps.map(|v| v.into_iter().map(String::from).collect()),
            zones,
            max_depth,
            caveats,
        };
        issue(issuer, &body).unwrap()
    }

    #[test]
    fn cert_roundtrip_and_validate() {
        let (rik, dk) = (kp(1), kp(2));
        let c = cert(&rik, &dk, Some(vec!["map:cap:inference"]), None, Some(1), vec![]);
        let body = validate_cert(&c, NOW).unwrap();
        assert_eq!(body.issuer, rik.agent_id());
        assert_eq!(body.subject, dk.agent_id());
        // Content round-trips byte-for-byte through the codec (produce-guard).
        let content = &open(&c, b"").unwrap().body.content;
        let reparsed = DelegateBody::from_value(content).ok().unwrap();
        assert_eq!(encode(&reparsed.to_value()).unwrap(), encode(content).unwrap());
    }

    #[test]
    fn cert_time_windows() {
        let (rik, dk) = (kp(1), kp(2));
        let c = cert(&rik, &dk, None, None, None, vec![]);
        assert!(matches!(validate_cert(&c, T0 - 1), Err(DelegError::NotYetValid { .. })));
        assert!(matches!(validate_cert(&c, LATER + 1), Err(DelegError::Expired { .. })));
        assert!(validate_cert(&c, NOW).is_ok());
    }

    #[test]
    fn issuer_must_equal_kid() {
        // Sign an "issuer=X" body with a different key Y -> BadSignature.
        let (rik, dk, evil) = (kp(1), kp(2), kp(9));
        let body = DelegateBody {
            issuer: rik.agent_id(),
            subject: dk.agent_id(),
            not_before_ms: T0,
            not_after_ms: LATER,
            caps: None,
            zones: None,
            max_depth: None,
            caveats: vec![],
        };
        let forged = issue(&evil, &body).unwrap(); // kid = evil, issuer field = rik
        assert!(matches!(validate_cert(&forged, NOW), Err(DelegError::BadSignature { .. })));
    }

    #[test]
    fn happy_two_link_chain() {
        let (rik, dk, sk) = (kp(1), kp(2), kp(3));
        let c_a = cert(&rik, &dk, Some(vec!["map:cap:inference"]), Some(vec![1, 2]), Some(2), vec![]);
        let c_b = cert(&dk, &sk, Some(vec!["map:cap:inference/text"]), Some(vec![1]), None, vec![]);
        let chain = vec![c_b, c_a]; // leaf..root
        let scope = validate_chain(&chain, &sk.agent_id(), None, NOW).unwrap();
        assert_eq!(scope.root, rik.agent_id());
        assert_eq!(scope.caps.as_deref(), Some(&["map:cap:inference/text".to_string()][..]));
        assert_eq!(scope.zones.as_deref(), Some(&[1u8][..]));
        // RIK gave depth 2; one hop consumed -> SK retains 1.
        assert_eq!(scope.depth_left, 1);
        assert!(scope.permits("map:cap:inference/text/code", 1));
        assert!(!scope.permits("map:cap:inference/text", 2)); // zone 2 not granted
    }

    #[test]
    fn widened_cap_scope_rejected() {
        // Child tries to broaden beyond the parent's cap scope.
        let (rik, dk, sk) = (kp(1), kp(2), kp(3));
        let c_a = cert(&rik, &dk, Some(vec!["map:cap:inference/text"]), None, None, vec![]);
        let c_b = cert(&dk, &sk, Some(vec!["map:cap:inference"]), None, None, vec![]); // wider!
        let chain = vec![c_b, c_a];
        assert!(matches!(
            validate_chain(&chain, &sk.agent_id(), None, NOW),
            Err(DelegError::WidenedScope { index: 0 })
        ));
    }

    #[test]
    fn widened_zone_scope_rejected() {
        let (rik, dk, sk) = (kp(1), kp(2), kp(3));
        let c_a = cert(&rik, &dk, None, Some(vec![1]), None, vec![]);
        let c_b = cert(&dk, &sk, None, Some(vec![1, 2]), None, vec![]); // zone 2 not granted
        let chain = vec![c_b, c_a];
        assert!(matches!(
            validate_chain(&chain, &sk.agent_id(), None, NOW),
            Err(DelegError::WidenedScope { index: 0 })
        ));
    }

    #[test]
    fn depth_exceeded() {
        // RIK allows 0 further delegations, but DK delegates to SK anyway.
        let (rik, dk, sk) = (kp(1), kp(2), kp(3));
        let c_a = cert(&rik, &dk, None, None, Some(0), vec![]);
        let c_b = cert(&dk, &sk, None, None, None, vec![]);
        let chain = vec![c_b, c_a];
        assert!(matches!(
            validate_chain(&chain, &sk.agent_id(), None, NOW),
            Err(DelegError::DepthExceeded)
        ));
    }

    #[test]
    fn broken_link_and_wrong_leaf() {
        let (rik, dk, sk, other) = (kp(1), kp(2), kp(3), kp(4));
        let c_a = cert(&rik, &dk, None, None, None, vec![]);
        let c_b = cert(&dk, &sk, None, None, None, vec![]);
        // leaf_signer != chain[0].subject
        assert!(matches!(
            validate_chain(&[c_b.clone(), c_a.clone()], &other.agent_id(), None, NOW),
            Err(DelegError::BrokenLink { index: 0 })
        ));
        // Disjoint link: c_b's issuer (dk) != c_c's subject.
        let c_c = cert(&rik, &other, None, None, None, vec![]); // subject=other, not dk
        assert!(matches!(
            validate_chain(&[c_b, c_c], &sk.agent_id(), None, NOW),
            Err(DelegError::BrokenLink { index: 1 })
        ));
    }

    #[test]
    fn unknown_caveat_fails_closed() {
        // Hand-build a body with caveat key 7 (unregistered).
        let (rik, dk) = (kp(1), kp(2));
        let mut val = DelegateBody {
            issuer: rik.agent_id(),
            subject: dk.agent_id(),
            not_before_ms: T0,
            not_after_ms: LATER,
            caps: None,
            zones: None,
            max_depth: None,
            caveats: vec![],
        }
        .to_value();
        if let Value::Map(m) = &mut val {
            m.push((Value::Uint(8), Value::Map(vec![(Value::Uint(7), Value::Bytes(vec![0]))])));
        }
        let env_body = Body {
            version: PROTOCOL_VERSION_1_0,
            msg_type: msg_type::DELEGATE,
            msg_id: MsgId([5u8; 16]),
            timestamp_ms: T0,
            recipient: None,
            ttl_ms: None,
            session_id: None,
            seq: None,
            content: val,
        };
        let c = seal(&rik, &env_body, b"").unwrap();
        assert!(matches!(
            validate_cert(&c, NOW),
            Err(DelegError::UnknownCaveat { index: 0, key: 7 })
        ));
    }

    #[test]
    fn audience_caveat_honored_and_enforced() {
        let (rik, dk, sk, aud) = (kp(1), kp(2), kp(3), kp(8));
        let c_a = cert(&rik, &dk, None, None, None, vec![]);
        let c_b = cert(&dk, &sk, None, None, None, vec![Caveat::Audience(aud.agent_id())]);
        let chain = vec![c_b, c_a];
        // Correct audience validates.
        assert!(validate_chain(&chain, &sk.agent_id(), Some(&aud.agent_id()), NOW).is_ok());
        // Wrong / absent audience fails closed.
        assert!(matches!(
            validate_chain(&chain, &sk.agent_id(), None, NOW),
            Err(DelegError::CaveatUnsatisfied { key: CAVEAT_AUDIENCE, .. })
        ));
        assert!(matches!(
            validate_chain(&chain, &sk.agent_id(), Some(&rik.agent_id()), NOW),
            Err(DelegError::CaveatUnsatisfied { key: CAVEAT_AUDIENCE, .. })
        ));
    }

    #[test]
    fn task_pin_returned_for_caller() {
        let (rik, dk, sk) = (kp(1), kp(2), kp(3));
        let pin = [0x42u8; 32];
        let c_a = cert(&rik, &dk, None, None, None, vec![]);
        let c_b = cert(&dk, &sk, None, None, None, vec![Caveat::TaskPin(pin)]);
        let scope = validate_chain(&[c_b, c_a], &sk.agent_id(), None, NOW).unwrap();
        assert_eq!(scope.task_pin, Some(pin));
    }

    #[test]
    fn too_long_and_empty() {
        let leaf = kp(3).agent_id();
        assert_eq!(validate_chain(&[], &leaf, None, NOW).unwrap_err(), DelegError::Empty);
        let junk: Vec<Vec<u8>> = (0..(MAX_CHAIN_LEN + 1)).map(|_| vec![0u8]).collect();
        assert_eq!(validate_chain(&junk, &leaf, None, NOW).unwrap_err(), DelegError::TooLong);
    }

    #[test]
    fn single_link_chain() {
        let (rik, sk) = (kp(1), kp(3));
        let c = cert(&rik, &sk, Some(vec!["map:cap:inference"]), None, None, vec![]);
        let scope = validate_chain(&[c], &sk.agent_id(), None, NOW).unwrap();
        assert_eq!(scope.root, rik.agent_id());
        assert_eq!(scope.caps.as_deref(), Some(&["map:cap:inference".to_string()][..]));
    }
}
