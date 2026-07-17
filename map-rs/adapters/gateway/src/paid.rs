//! `PaidGatewayProvider` — a gateway fronting an EXTERNAL x402-priced HTTP
//! service as a MAP capability (ops §5.1, TASK-025). Pure composition over
//! finished pieces: the TASK-016 [`GatewayProvider`] admit gate (embedded,
//! unchanged), the TASK-024 prepaid execution gate (fed in as a boolean —
//! `PrepaidProviderPayment::execution_permitted()` on the MAP leg), and the
//! TASK-023/025 x402 adapter's [`pay_402_loop`] on the rail leg.
//!
//! The ops §5.1 money rules, enforced structurally:
//! - **G-6 spend order**: [`PaidGatewayProvider::serve`] runs admit →
//!   settled-gate → pay → execute; the reject paths return BEFORE any
//!   upstream call or signature (tests count the canned upstream's calls).
//! - **G-5 legs never launder**: the upstream settlement evidence is emitted
//!   ONLY as a kind-4 (tool-event) transcript chunk — gateway-attested
//!   content. There is deliberately NO API converting an upstream settlement
//!   into a `PayProof` or vice versa; the only cross-leg coupling is the
//!   boolean settled gate.
//! - **G-7 bounded spend**: the [`SpendPolicy`] ceiling, checked inside the
//!   loop before signing.
//! - The two legs' failure economics differ: a failure BEFORE payment spends
//!   nothing; a failure AFTER payment (upstream took the money and died)
//!   still emits the evidence chunk — the attributable-loss record pay §6
//!   disputes will consume.
//!
//! Error values never carry upstream URLs, wallet material, or prompt
//! content (the TASK-016 §10 discipline).

use crate::provider::{GatewayError, GatewayProvider, GatewayReject};
use map_identity::AgentId;
use map_tasks::ChunkKind;
use map_taskrun::Executor;
use ollama_provider::MockBackend;
use std::cell::Cell;
use x402_adapter::http402::{pay_402_loop, HttpUpstream, PaymentEvidence, SpendPolicy};
use x402_adapter::{EvmSigner, X402Error};

/// Why a paid gateway refused to serve (before OR after money moved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaidServeError {
    /// The TASK-016 admit gate refused (zone/scope/chain) — G-6 step 1.
    Reject(GatewayReject),
    /// The MAP-leg settlement gate is closed (prepaid not settled) — G-6
    /// step 2. No upstream call was made; nothing was spent.
    NotSettled,
    /// The upstream leg failed BEFORE any payment (challenge unusable,
    /// over-cap, transport). Nothing was spent.
    UpstreamUnpaid(X402Error),
    /// The upstream leg failed AFTER a payment was sent — the money may be
    /// spent. The evidence chunk MUST be recorded (it is returned here so the
    /// caller cannot lose it).
    UpstreamPaid(X402Error, (ChunkKind, Vec<u8>)),
}

/// A gateway serving ONE assigned task against one x402-priced upstream URL.
/// `U`/`S` are the TASK-023/025 seams (canned in tests, real under
/// `real-x402`).
pub struct PaidGatewayProvider<U: HttpUpstream, S: EvmSigner> {
    /// The TASK-016 gate, embedded for `admit` ONLY — its backend is the
    /// mock and is never executed (this provider's Executor is the 402 loop).
    gate: GatewayProvider<MockBackend>,
    upstream: U,
    signer: S,
    upstream_url: String,
    policy: SpendPolicy,
    task: [u8; 32],
    cap: String,
    task_zone: Option<u8>,
    /// The MAP-leg settlement gate (G-6): fed from
    /// `PrepaidProviderPayment::execution_permitted()` by the pump.
    settled: Cell<bool>,
    /// The (task, attempt) counter for the deterministic upstream nonce. A
    /// retried serve at the SAME attempt re-presents the identical
    /// authorization (retry safety); `next_attempt` starts a genuinely new
    /// payment.
    attempt: Cell<u32>,
    /// Clock for the Executor path (admit's delegation windows + the
    /// authorization validity window).
    now_ms: Cell<u64>,
}

impl<U: HttpUpstream, S: EvmSigner> PaidGatewayProvider<U, S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        upstream: U,
        signer: S,
        agent: AgentId,
        namespace: impl Into<String>,
        fronted_cap: impl Into<String>,
        upstream_zone: u8,
        allowlisted: bool,
        chain: Option<Vec<Vec<u8>>>,
        upstream_url: impl Into<String>,
        policy: SpendPolicy,
        task: [u8; 32],
        task_zone: Option<u8>,
    ) -> Result<Self, GatewayError> {
        let cap: String = fronted_cap.into();
        let gate = GatewayProvider::new(
            MockBackend::new(),
            agent,
            "unused-paid-gateway",
            1,
            namespace,
            cap.clone(),
            upstream_zone,
            allowlisted,
            chain,
        )?;
        Ok(PaidGatewayProvider {
            gate,
            upstream,
            signer,
            upstream_url: upstream_url.into(),
            policy,
            task,
            cap,
            task_zone,
            settled: Cell::new(false),
            attempt: Cell::new(0),
            now_ms: Cell::new(0),
        })
    }

    pub fn effective_zone(&self) -> u8 {
        self.gate.effective_zone()
    }

    /// Feed the MAP-leg settlement state (G-6): the pump sets this true when
    /// `PrepaidProviderPayment::execution_permitted()` flips.
    pub fn set_settled(&self, settled: bool) {
        self.settled.set(settled);
    }

    /// Start a genuinely NEW upstream payment (a fresh nonce). Retries of the
    /// current attempt re-present the same authorization instead.
    pub fn next_attempt(&self) {
        self.attempt.set(self.attempt.get() + 1);
    }

    pub fn set_now_ms(&self, now_ms: u64) {
        self.now_ms.set(now_ms);
    }

    /// The G-5 evidence chunk for a payment: kind-4 (tool-event),
    /// gateway-attested content — never any proof type.
    fn evidence_chunk(ev: &PaymentEvidence) -> (ChunkKind, Vec<u8>) {
        (ChunkKind::ToolIo, ev.to_chunk_bytes())
    }

    /// Serve the assigned task: **admit → settled → pay → execute**, in that
    /// order (ops §5.1 G-6) — the reject paths return before any upstream
    /// exchange or signature.
    pub fn serve(&self, input: &[u8], now_ms: u64) -> Result<Vec<(ChunkKind, Vec<u8>)>, PaidServeError> {
        // 1. Admit (TASK-016 G-3/scope, reused verbatim).
        self.gate
            .admit(&self.cap, self.task_zone, now_ms)
            .map_err(PaidServeError::Reject)?;
        // 2. MAP-leg settlement gate (G-6; RECOMMENDED terms 1 prepaid).
        if !self.settled.get() {
            return Err(PaidServeError::NotSettled);
        }
        // 3+4. Pay (bounded, deterministic nonce) and execute.
        match pay_402_loop(
            &self.upstream,
            &self.signer,
            &self.upstream_url,
            input,
            &self.policy,
            &self.task,
            self.attempt.get(),
            now_ms / 1000,
        ) {
            Ok(paid) => {
                let mut chunks = vec![(ChunkKind::OutputText, paid.response.body)];
                if let Some(ev) = &paid.evidence {
                    chunks.push(Self::evidence_chunk(ev));
                }
                Ok(chunks)
            }
            Err(failure) => match failure.evidence {
                // Money may be spent: the evidence chunk travels with the
                // error so the transcript can record the attributable loss.
                Some(ev) => Err(PaidServeError::UpstreamPaid(failure.error, Self::evidence_chunk(&ev))),
                None => Err(PaidServeError::UpstreamUnpaid(failure.error)),
            },
        }
    }
}

impl<U: HttpUpstream, S: EvmSigner> Executor for PaidGatewayProvider<U, S> {
    /// Infallible Executor contract (as OllamaExecutor): failures degrade to
    /// empty output — EXCEPT a paid-then-failed upstream, which degrades to
    /// the evidence chunk alone (G-5: the spend must stay attributable).
    fn execute(&self, input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)> {
        match self.serve(input, self.now_ms.get()) {
            Ok(chunks) => chunks,
            Err(PaidServeError::UpstreamPaid(_, evidence)) => vec![evidence],
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;
    use x402_adapter::http402::{upstream_nonce, CannedMode, CannedUpstream, UpstreamResponse};
    use x402_adapter::{x402_nonce, CannedSigner, TransferAuthorization};

    const CAP: &str = "map:cap:inference/text-generation";
    const TASK: [u8; 32] = [0xE; 32];
    const NOW: u64 = 1_780_000_100_000;

    fn policy(cap_atomic: u128) -> SpendPolicy {
        SpendPolicy {
            max_atomic_per_task: cap_atomic,
            networks: vec!["base-sepolia".into()],
            assets: vec!["0x036CbD53842c5426634e7929541eC2318f3dCF7e".into()],
        }
    }

    /// A signer that counts signatures (order-enforcement assertions).
    struct CountingSigner {
        inner: CannedSigner,
        signs: Cell<u32>,
    }
    impl CountingSigner {
        fn new() -> Self {
            CountingSigner { inner: CannedSigner::new(), signs: Cell::new(0) }
        }
    }
    impl EvmSigner for CountingSigner {
        fn address(&self) -> String {
            self.inner.address()
        }
        fn sign_authorization(&self, auth: &TransferAuthorization) -> Result<Vec<u8>, X402Error> {
            self.signs.set(self.signs.get() + 1);
            self.inner.sign_authorization(auth)
        }
    }

    fn gateway(
        mode: CannedMode,
        cap_atomic: u128,
        upstream_zone: u8,
        task_zone: Option<u8>,
    ) -> PaidGatewayProvider<CannedUpstream, CountingSigner> {
        PaidGatewayProvider::new(
            CannedUpstream::new(mode),
            CountingSigner::new(),
            Keypair::from_seed([7u8; 32]).agent_id(),
            "gateway/example.com",
            CAP,
            upstream_zone,
            false,
            None,
            "https://paid.example/api",
            policy(cap_atomic),
            TASK,
            task_zone,
        )
        .unwrap()
    }

    fn assert_no_spend(g: &PaidGatewayProvider<CannedUpstream, CountingSigner>) {
        assert_eq!(g.upstream.calls.get(), 0, "upstream contacted on a reject path (G-6)");
        assert_eq!(g.signer.signs.get(), 0, "signer used on a reject path (G-6)");
        assert_eq!(g.upstream.payment_count(), 0);
    }

    // G-6 step 1: an inadmissible task spends NOTHING.
    #[test]
    fn g6_admit_reject_spends_nothing() {
        let g = gateway(CannedMode::PayThenOk { price_atomic: 10_000 }, 100_000, 2, Some(1));
        g.set_settled(true); // even settled: zone gate is first
        assert_eq!(g.serve(b"in", NOW), Err(PaidServeError::Reject(GatewayReject::Zone)));
        assert_no_spend(&g);
    }

    // G-6 step 2: an unsettled MAP leg spends NOTHING.
    #[test]
    fn g6_unsettled_spends_nothing() {
        let g = gateway(CannedMode::PayThenOk { price_atomic: 10_000 }, 100_000, 2, Some(2));
        assert_eq!(g.serve(b"in", NOW), Err(PaidServeError::NotSettled));
        assert_no_spend(&g);
        // Settle the MAP leg → the same call now proceeds.
        g.set_settled(true);
        assert!(g.serve(b"in", NOW).is_ok());
    }

    // G-7: an over-cap demand is refused BEFORE signing.
    #[test]
    fn g7_over_cap_refused_before_signing() {
        let g = gateway(CannedMode::OverCap { price_atomic: 200_000 }, 100_000, 2, Some(2));
        g.set_settled(true);
        match g.serve(b"in", NOW) {
            Err(PaidServeError::UpstreamUnpaid(X402Error::PriceCapExceeded)) => {}
            other => panic!("expected PriceCapExceeded, got {other:?}"),
        }
        assert_eq!(g.signer.signs.get(), 0, "G-7 must refuse before any signature");
        assert_eq!(g.upstream.payment_count(), 0);
    }

    // Happy path: paid call yields output + the G-5 evidence chunk.
    #[test]
    fn paid_call_yields_output_and_evidence_chunk() {
        let g = gateway(CannedMode::PayThenOk { price_atomic: 10_000 }, 100_000, 2, Some(2));
        g.set_settled(true);
        let chunks = g.serve(b"prompt", NOW).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0, ChunkKind::OutputText);
        assert_eq!(chunks[0].1, b"upstream:prompt");
        assert_eq!(chunks[1].0, ChunkKind::ToolIo); // kind 4: gateway-attested evidence
        let ev = String::from_utf8(chunks[1].1.clone()).unwrap();
        assert!(ev.contains("x402-upstream-payment") && ev.contains("10000"));
        assert_eq!(g.upstream.payment_count(), 1);
        // A free upstream yields output with NO evidence chunk.
        let free = gateway(CannedMode::Free, 100_000, 2, Some(2));
        free.set_settled(true);
        let chunks = free.serve(b"x", NOW).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(free.signer.signs.get(), 0);
    }

    // Greedy upstream: exactly ONE payment, then error (no auto-escalation).
    #[test]
    fn greedy_upstream_gets_exactly_one_payment() {
        let g = gateway(CannedMode::Greedy { price_atomic: 10_000 }, 100_000, 2, Some(2));
        g.set_settled(true);
        match g.serve(b"in", NOW) {
            Err(PaidServeError::UpstreamPaid(X402Error::Facilitator(_), evidence)) => {
                assert_eq!(evidence.0, ChunkKind::ToolIo);
            }
            other => panic!("expected UpstreamPaid, got {other:?}"),
        }
        assert_eq!(g.upstream.payment_count(), 1);
        assert_eq!(g.signer.signs.get(), 1);
    }

    // Paid-then-failed upstream: the money is spent; the evidence chunk still
    // records it — and the infallible Executor path emits it alone.
    #[test]
    fn fail_after_pay_still_records_evidence() {
        let g = gateway(CannedMode::FailAfterPay { price_atomic: 10_000 }, 100_000, 2, Some(2));
        g.set_settled(true);
        g.set_now_ms(NOW);
        let out = g.execute(b"in"); // Executor path
        assert_eq!(out.len(), 1, "paid-but-failed must degrade to the evidence chunk, not empty");
        assert_eq!(out[0].0, ChunkKind::ToolIo);
        assert!(String::from_utf8(out[0].1.clone()).unwrap().contains("x402-upstream-payment"));
        assert_eq!(g.upstream.payment_count(), 1);
    }

    // Retry safety: the same (task, attempt) re-presents the SAME
    // authorization — the upstream settles once; a new attempt pays anew.
    #[test]
    fn retry_same_attempt_settles_once_new_attempt_pays_again() {
        let g = gateway(CannedMode::PayThenOk { price_atomic: 10_000 }, 100_000, 2, Some(2));
        g.set_settled(true);
        assert!(g.serve(b"in", NOW).is_ok());
        assert!(g.serve(b"in", NOW).is_ok()); // retried serve, same attempt
        assert_eq!(g.upstream.payment_count(), 1, "same nonce must settle exactly once");
        g.next_attempt();
        assert!(g.serve(b"in", NOW).is_ok());
        assert_eq!(g.upstream.payment_count(), 2, "a fresh attempt is a fresh single-use nonce");
    }

    // Nonce derivation: fixed-output, attempt-sensitive, and domain-separated
    // from the pay §5.1 invoice nonce on identical inputs.
    #[test]
    fn upstream_nonce_deterministic_and_domain_separated() {
        let n0 = upstream_nonce(&TASK, 0);
        assert_eq!(n0, upstream_nonce(&TASK, 0), "deterministic");
        assert_ne!(n0, upstream_nonce(&TASK, 1), "attempt-sensitive");
        // Same 16 bytes fed to both derivations must NOT collide: an upstream
        // challenge can never elicit an authorization valid for a MAP invoice.
        let id16 = [0x42u8; 16];
        let mut task32 = [0u8; 32];
        task32[..16].copy_from_slice(&id16);
        assert_ne!(upstream_nonce(&task32, 0), x402_nonce(&id16));
    }

    // §10 discipline: no error value leaks the upstream URL or wallet bits.
    #[test]
    fn errors_leak_no_upstream_identifiers() {
        let g = gateway(CannedMode::OverCap { price_atomic: 200_000 }, 100_000, 2, Some(1));
        let e1 = format!("{:?}", g.serve(b"in", NOW)); // zone reject
        g.set_settled(true);
        let g2 = gateway(CannedMode::OverCap { price_atomic: 200_000 }, 100_000, 2, Some(2));
        g2.set_settled(true);
        let e2 = format!("{:?}", g2.serve(b"in", NOW)); // over-cap
        for s in [e1, e2] {
            assert!(!s.contains("paid.example") && !s.contains("c0ffee"), "leaked: {s}");
        }
    }

    // FR-4 E2E: the MAP leg (TASK-024 prepaid pair over the credit ledger)
    // gates the rail leg — requester prepays the gateway, execution_permitted
    // feeds set_settled, the gateway then pays upstream exactly once. Both
    // ledgers assert: MAP-leg balances moved, upstream settled once.
    #[test]
    fn e2e_prepaid_map_leg_gates_x402_upstream_leg() {
        use map_core::msg_type;
        use map_core::MsgId;
        use map_pay::codec::{AssetAmount, MethodOffer, PayQuote};
        use map_pay::credit_ledger::{LedgerOperator, CREDIT_LEDGER_METHOD};
        use map_pay::{Amount, Asset};
        use map_session::Incoming;
        use map_taskrun::{PrepaidProviderPayment, PrepaidRequesterPayment};

        let (ka, kb, kop) = (Keypair::from_seed([1; 32]), Keypair::from_seed([2; 32]), Keypair::from_seed([4; 32]));
        let usd = |m: i64| AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, m) };
        let quote = PayQuote {
            quote_id: [0x50; 16],
            pricing_model: 0,
            rate_table: vec![(0, usd(500))],
            total: Some(usd(500)),
            methods: vec![MethodOffer { uri: CREDIT_LEDGER_METHOD.into(), params: vec![] }],
            expiry_ms: NOW + 60_000,
            terms: Some(1),
            ext: None,
            task_binding: Some(TASK),
        };
        let mk = |mt: u16, content: map_core::cbor::Value| {
            let raw = map_core::cbor::encode(&content).unwrap();
            Incoming { msg_type: mt, content, msg_id: MsgId([0u8; 16]), seq: 0, raw }
        };
        // MAP leg: quote → agree → invoice → pay → receipt.
        let mut pp = PrepaidProviderPayment::new(kb.agent_id(), ka.agent_id(), kop.agent_id(), quote.clone());
        let mut rp = PrepaidRequesterPayment::new(TASK, ka.agent_id(), kb.agent_id());
        let mut op = LedgerOperator::new(Keypair::from_seed([4; 32]), -2);
        op.credit(&ka.agent_id(), 1000);
        let qinc = mk(msg_type::PAY_QUOTE, quote.to_value());
        pp.bind_quote_envelope(&qinc.raw);
        let agree = rp.on_quote(&qinc, NOW);
        assert!(pp.on_agree(&mk(msg_type::PAY_AGREE, agree[0].1.clone())).is_empty());

        // Rail leg: gateway serves only once the MAP leg settles.
        let g = gateway(CannedMode::PayThenOk { price_atomic: 10_000 }, 100_000, 2, Some(2));
        g.set_settled(pp.execution_permitted());
        assert_eq!(g.serve(b"prompt", NOW), Err(PaidServeError::NotSettled));
        assert_no_spend(&g);

        let inv = pp.invoice([0x60; 16], NOW);
        let proof = rp.on_invoice(&mk(msg_type::PAY_INVOICE, inv[0].1.clone()), &mut op, NOW);
        let rcpt = pp.on_proof(&mk(msg_type::PAY_PROOF, proof[0].1.clone()), NOW);
        assert_eq!(rcpt[0].0, msg_type::PAY_RECEIPT);
        assert!(pp.execution_permitted());
        g.set_settled(pp.execution_permitted());

        let chunks = g.serve(b"prompt", NOW).unwrap();
        assert_eq!(chunks.len(), 2); // output + G-5 evidence
        assert_eq!(g.upstream.payment_count(), 1);
        // MAP-leg money moved (requester → gateway), rail leg settled once —
        // and no artifact of one leg exists on the other (G-5: the only
        // coupling was the boolean gate).
        assert_eq!(op.balance(&ka.agent_id()), 500);
        assert_eq!(op.balance(&kb.agent_id()), 500);
    }

    // Malformed/empty challenge: refuse with no spend (fail-closed).
    #[test]
    fn empty_or_malformed_challenge_spends_nothing() {
        use x402_adapter::http402::parse_payment_requirements;
        assert!(parse_payment_requirements(b"not json").is_err());
        assert!(parse_payment_requirements(b"{\"accepts\":[]}").is_err());
        assert!(parse_payment_requirements(b"{\"accepts\":[{\"scheme\":\"exact\"}]}").is_err());
        // A challenge listing no policy-acceptable entry: refused, unsigned.
        struct WrongNetwork(CannedUpstream);
        impl HttpUpstream for WrongNetwork {
            fn call(&self, u: &str, b: &[u8], p: Option<&str>) -> Result<UpstreamResponse, x402_adapter::UpstreamError> {
                match self.0.call(u, b, p) {
                    Err(x402_adapter::UpstreamError::Challenge(mut reqs)) => {
                        reqs[0].network = "mainnet".into();
                        Err(x402_adapter::UpstreamError::Challenge(reqs))
                    }
                    other => other,
                }
            }
        }
        let g = PaidGatewayProvider::new(
            WrongNetwork(CannedUpstream::new(CannedMode::PayThenOk { price_atomic: 10 })),
            CountingSigner::new(),
            Keypair::from_seed([7u8; 32]).agent_id(),
            "gateway/example.com",
            CAP,
            2,
            false,
            None,
            "https://paid.example/api",
            policy(100_000),
            TASK,
            Some(2),
        )
        .unwrap();
        g.set_settled(true);
        assert!(matches!(g.serve(b"in", NOW), Err(PaidServeError::UpstreamUnpaid(_))));
        assert_eq!(g.signer.signs.get(), 0);
    }
}
