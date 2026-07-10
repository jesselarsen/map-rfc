//! `GatewayProvider` — a running federation gateway (ops §5, TASK-016). It is
//! an ordinary MAP provider that executes a task by calling an upstream
//! [`InferenceBackend`] and stays fully accountable: it signs as itself and
//! evidence accrues to its own AgentID (G-2, a property of the receipt path).
//!
//! It enforces the provenance rules in code:
//! - **G-3 / FR-2** — [`GatewayProvider::admit`] rejects an ASSIGN whose zone
//!   label is stricter (lower) than the gateway's [`effective_zone`], so a
//!   gateway can never *launder* a payload into a looser zone.
//! - **FR-5** — when the gateway runs under a DelegationCert chain, its own
//!   [`EffectiveScope`] must `permits(cap, zone)` the ASSIGN too.
//! - **G-1 / FR-4** — there is deliberately NO method that signs anything with a
//!   foreign subject; the descriptor's subject is always the gateway (see
//!   [`crate::build_gateway_descriptor`]).
//!
//! Upstream response content is untrusted: it becomes CHUNK payload but never
//! influences control flow. Error values never carry upstream URLs or prompt
//! content (§10).

use crate::{effective_zone, valid_namespace};
use map_core::delegation::validate_chain;
use map_core::errors::codes;
use map_identity::AgentId;
use map_tasks::ChunkKind;
use map_taskrun::Executor;
use ollama_provider::{InferenceBackend, OllamaExecutor};

/// Construction error: a malformed gateway namespace (G-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    InvalidNamespace(String),
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayError::InvalidNamespace(ns) => write!(f, "invalid gateway namespace: {ns}"),
        }
    }
}
impl std::error::Error for GatewayError {}

/// Why a gateway refused an ASSIGN. Each maps to a core §9 auth code; the
/// message carries NO upstream identifier or prompt content (§10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayReject {
    /// The gateway's fronted zone would exceed the task's ZoneLabel (G-3).
    Zone,
    /// The delegation chain does not authorize this capability/zone (FR-5).
    Scope,
    /// The configured delegation chain failed to validate.
    BadChain,
}

impl GatewayReject {
    /// The REJECT wire code (core §9). Zone→2004, scope→2005, bad-chain→2003.
    /// (TASK-016 FR-2 says "3xxx"; core §9 already defines precise auth codes,
    /// which win per the conflict rule — reported.)
    pub fn wire_code(&self) -> u16 {
        match self {
            GatewayReject::Zone => codes::ZONE_VIOLATION,
            GatewayReject::Scope => codes::NOT_AUTHORIZED,
            GatewayReject::BadChain => codes::BAD_DELEGATION,
        }
    }
}

/// A gateway provider fronting an upstream backend for one capability.
pub struct GatewayProvider<B: InferenceBackend> {
    inner: OllamaExecutor<B>,
    agent: AgentId,
    namespace: String,
    fronted_cap: String,
    upstream_zone: u8,
    allowlisted: bool,
    chain: Option<Vec<Vec<u8>>>,
}

impl<B: InferenceBackend> GatewayProvider<B> {
    /// Build a gateway. `namespace` MUST be a valid `gateway/<anchor>` (G-4) or
    /// construction fails. `agent` is the gateway's own identity; `chain` is an
    /// optional DelegationCert chain (leaf..root) the gateway operates under.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        backend: B,
        agent: AgentId,
        model: impl Into<String>,
        max_tokens: u32,
        namespace: impl Into<String>,
        fronted_cap: impl Into<String>,
        upstream_zone: u8,
        allowlisted: bool,
        chain: Option<Vec<Vec<u8>>>,
    ) -> Result<Self, GatewayError> {
        let namespace = namespace.into();
        if !valid_namespace(&namespace) {
            return Err(GatewayError::InvalidNamespace(namespace));
        }
        Ok(GatewayProvider {
            inner: OllamaExecutor::new(backend, model, max_tokens),
            agent,
            namespace,
            fronted_cap: fronted_cap.into(),
            upstream_zone,
            allowlisted,
            chain,
        })
    }

    pub fn agent(&self) -> &AgentId {
        &self.agent
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn fronted_cap(&self) -> &str {
        &self.fronted_cap
    }

    /// The zone this gateway stamps on fronted traffic (G-3, tighten-only).
    pub fn effective_zone(&self) -> u8 {
        effective_zone(self.upstream_zone, self.allowlisted)
    }

    /// Gate an ASSIGN before executing (§8 order: chain scope → zone; first
    /// failure wins). `cap` is the ASSIGN's capability, `task_zone` its
    /// ZoneLabel (Constraints.5, absent = unrestricted), `now_ms` bounds any
    /// delegation-chain validity window. `Ok(())` means the gateway may serve it.
    pub fn admit(&self, cap: &str, task_zone: Option<u8>, now_ms: u64) -> Result<(), GatewayReject> {
        // FR-5: a delegated gateway must be authorized for (cap, zone) itself.
        if let Some(chain) = &self.chain {
            let scope =
                validate_chain(chain, &self.agent, None, now_ms).map_err(|_| GatewayReject::BadChain)?;
            if !scope.permits(cap, task_zone.unwrap_or(0)) {
                return Err(GatewayReject::Scope);
            }
        }
        // FR-2 (G-3): the fronted zone MUST NOT exceed the task's ZoneLabel —
        // i.e. never serve a task stricter than the gateway's effective zone.
        if let Some(z) = task_zone {
            if self.effective_zone() > z {
                return Err(GatewayReject::Zone);
            }
        }
        Ok(())
    }
}

impl<B: InferenceBackend> Executor for GatewayProvider<B> {
    fn execute(&self, input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)> {
        // Delegates to the TASK-013 executor: upstream call → transcript chunks.
        // Infallible by the Executor contract — errors degrade to empty output.
        self.inner.execute(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_core::delegation::{issue, Caveat, DelegateBody};
    use map_identity::Keypair;
    use ollama_provider::MockBackend;

    const CAP: &str = "map:cap:inference/text-generation";
    const NOW: u64 = 1_780_000_100_000;

    fn gw_agent() -> AgentId {
        Keypair::from_seed([7u8; 32]).agent_id()
    }

    fn provider(
        upstream_zone: u8,
        allowlisted: bool,
        chain: Option<Vec<Vec<u8>>>,
    ) -> GatewayProvider<MockBackend> {
        GatewayProvider::new(
            MockBackend::new(),
            gw_agent(),
            "mock",
            128,
            "gateway/example.com",
            CAP,
            upstream_zone,
            allowlisted,
            chain,
        )
        .unwrap()
    }

    #[test]
    fn constructor_rejects_bad_namespace() {
        let r = GatewayProvider::new(
            MockBackend::new(),
            gw_agent(),
            "mock",
            128,
            "not-a-gateway-namespace",
            CAP,
            2,
            false,
            None,
        );
        assert!(matches!(r, Err(GatewayError::InvalidNamespace(_))));
    }

    #[test]
    fn zone_gate_matrix_g3() {
        // effective_zone(2,false) = 2: a Zone-1 (stricter) task is rejected;
        // a Zone-2 or Zone-3 (looser or equal) task is admitted (FR-2).
        let g = provider(2, false, None);
        assert_eq!(g.effective_zone(), 2);
        assert_eq!(g.admit(CAP, Some(1), NOW), Err(GatewayReject::Zone));
        assert!(g.admit(CAP, Some(2), NOW).is_ok());
        assert!(g.admit(CAP, Some(3), NOW).is_ok());
        // No zone label ⇒ no zone constraint.
        assert!(g.admit(CAP, None, NOW).is_ok());

        // Allowlisted upstream floors at 1: a Zone-1 task is now admitted.
        let g1 = provider(1, true, None);
        assert_eq!(g1.effective_zone(), 1);
        assert!(g1.admit(CAP, Some(1), NOW).is_ok());
        assert_eq!(g1.admit(CAP, Some(0), NOW), Err(GatewayReject::Zone));

        // Tighten-only edge: upstream 3 stays 3 even when allowlisted.
        let g3 = provider(3, true, None);
        assert_eq!(g3.effective_zone(), 3);
        assert_eq!(g3.admit(CAP, Some(2), NOW), Err(GatewayReject::Zone));
    }

    #[test]
    fn scope_gate_fr5_rejects_out_of_scope_cap() {
        // Chain: org RIK delegates to the gateway, scoped to `map:cap:gateway`
        // ONLY. A fronted inference ASSIGN is then outside scope → rejected.
        let rik = Keypair::from_seed([1u8; 32]);
        let gw_kp = Keypair::from_seed([7u8; 32]); // same seed as gw_agent()
        let cert = issue(
            &rik,
            &DelegateBody {
                issuer: rik.agent_id(),
                subject: gw_kp.agent_id(),
                not_before_ms: NOW - 1000,
                not_after_ms: NOW + 1_000_000,
                caps: Some(vec!["map:cap:gateway".to_string()]),
                zones: None,
                max_depth: None,
                caveats: vec![],
            },
        )
        .unwrap();
        let scoped = provider(2, false, Some(vec![cert]));
        assert_eq!(scoped.admit(CAP, None, NOW), Err(GatewayReject::Scope));
        // But the gateway service capability itself IS in scope.
        assert!(scoped.admit("map:cap:gateway/example.com", None, NOW).is_ok());
    }

    #[test]
    fn scope_gate_fr5_admits_in_scope_cap() {
        let rik = Keypair::from_seed([1u8; 32]);
        let gw_kp = Keypair::from_seed([7u8; 32]);
        let cert = issue(
            &rik,
            &DelegateBody {
                issuer: rik.agent_id(),
                subject: gw_kp.agent_id(),
                not_before_ms: NOW - 1000,
                not_after_ms: NOW + 1_000_000,
                caps: Some(vec!["map:cap:inference".to_string()]), // covers the fronted cap
                zones: None,
                max_depth: None,
                caveats: vec![],
            },
        )
        .unwrap();
        let scoped = provider(2, false, Some(vec![cert]));
        assert!(scoped.admit(CAP, Some(2), NOW).is_ok());
    }

    #[test]
    fn bad_chain_is_rejected_not_a_panic() {
        // A chain whose leaf subject is NOT the gateway fails validation.
        let rik = Keypair::from_seed([1u8; 32]);
        let other = Keypair::from_seed([9u8; 32]);
        let cert = issue(
            &rik,
            &DelegateBody {
                issuer: rik.agent_id(),
                subject: other.agent_id(), // not the gateway → BrokenLink
                not_before_ms: NOW - 1000,
                not_after_ms: NOW + 1_000_000,
                caps: None,
                zones: None,
                max_depth: None,
                caveats: vec![Caveat::TaskPin([0u8; 32])],
            },
        )
        .unwrap();
        let g = provider(2, false, Some(vec![cert]));
        assert_eq!(g.admit(CAP, None, NOW), Err(GatewayReject::BadChain));
    }

    #[test]
    fn execute_fronts_the_backend() {
        // The gateway's Executor runs the upstream and yields transcript chunks.
        let g = provider(2, false, None);
        let chunks = g.execute(b"hello");
        assert_eq!(chunks.len(), 2); // output-text + usage
        assert_eq!(chunks[0].0, ChunkKind::OutputText);
        assert_eq!(chunks[0].1, b"echo: hello");
    }

    #[test]
    fn reject_wire_codes_and_no_content_leak() {
        assert_eq!(GatewayReject::Zone.wire_code(), codes::ZONE_VIOLATION);
        assert_eq!(GatewayReject::Scope.wire_code(), codes::NOT_AUTHORIZED);
        assert_eq!(GatewayReject::BadChain.wire_code(), codes::BAD_DELEGATION);
        // No reject/error value carries prompt or upstream identifiers (§10).
        for s in [
            format!("{:?}", GatewayReject::Zone),
            format!("{:?}", GatewayReject::Scope),
            format!("{}", GatewayError::InvalidNamespace("gateway/bad".into())),
        ] {
            assert!(!s.contains("11434") && !s.contains("http"));
        }
    }
}
