//! TASK-007 §16 tests. Runtimes are pure pump machines, so most cases drive
//! them by shuttling `(msg_type, Value)` between the two runtimes directly (no
//! sessions/threads); one integration test proves the full flow over two live
//! `InMemoryDuplex` sessions.

use crate::*;
use map_core::cbor::Value;
use map_core::msg_type;
use map_core::MsgId;
use map_session::Incoming;
use map_tasks::msg::{
    Accept, Assign, Cancel, ChunkMsg, FlowCredit, FlowInit, InputSpec, Quote, Reject, TaskConstraints,
};
use map_tasks::{ChunkKind, ReqState, TaskId};

const TS: u64 = 1_780_000_000_000;
const CAP: &str = "map:cap:inference/text-generation";

/// A stand-in "envelope frame" for session-less pump tests: a byte string
/// distinct from `encode(content)` (so it exercises the envelope-byte digest
/// path, not a content re-encoding — pay §3.1 / TASK-017). Deterministic in
/// (mt, content), so a provider can bind the same bytes a requester receives.
fn synth_env(mt: u16, content: &Value) -> Vec<u8> {
    map_core::cbor::encode(&Value::Array(vec![
        Value::Text("map-env".into()),
        Value::Uint(mt as u64),
        content.clone(),
    ]))
    .expect("synth env encodes")
}

fn mk(mt: u16, content: Value) -> Incoming {
    let raw = synth_env(mt, &content);
    Incoming { msg_type: mt, content, msg_id: MsgId([0u8; 16]), seq: 0, raw }
}
fn task() -> TaskId {
    TaskId([1u8; 32])
}
fn req(input: &[u8]) -> RequesterTask {
    RequesterTask::new(task(), CAP, TaskConstraints::default(), input.to_vec())
}
fn req_ttft(input: &[u8], max_ttft: u64) -> RequesterTask {
    let c = TaskConstraints { ttft_ms: Some(max_ttft), ..Default::default() };
    RequesterTask::new(task(), CAP, c, input.to_vec())
}
fn quote(ttft: u64, expiry: u64) -> Quote {
    Quote { task: task(), pay_quote_ref: None, ttft_ms: ttft, expiry_ms: expiry, ext: None }
}
fn tamper_chunk(v: &Value) -> Value {
    let mut c = ChunkMsg::from_value(v).unwrap();
    if c.content.is_empty() {
        c.content.push(0xff);
    } else {
        c.content[0] ^= 0xff;
    }
    c.to_value()
}

/// Ping-pong until quiescent, optionally tampering the first CHUNK to the requester.
fn shuttle<E: Executor>(r: &mut RequesterTask, p: &mut ProviderTask<E>, first_to_prov: Vec<Outgoing>, now: u64, tamper: bool) {
    let mut to_prov = first_to_prov;
    let mut to_req: Vec<Outgoing> = Vec::new();
    let mut tampered = false;
    let mut guard = 0;
    loop {
        for o in std::mem::take(&mut to_prov) {
            to_req.extend(p.on_incoming(&mk(o.0, o.1), now));
        }
        if tamper && !tampered {
            if let Some(o) = to_req.iter_mut().find(|o| o.0 == msg_type::CHUNK) {
                o.1 = tamper_chunk(&o.1);
                tampered = true;
            }
        }
        for o in std::mem::take(&mut to_req) {
            to_prov.extend(r.on_incoming(&mk(o.0, o.1), now));
        }
        if to_prov.is_empty() {
            break;
        }
        guard += 1;
        assert!(guard < 1000, "shuttle did not converge");
    }
}

// ---- happy paths ----------------------------------------------------------

#[test]
fn echo_happy_path_offer() {
    let mut r = req(b"hello world");
    let mut p = ProviderTask::new(EchoExecutor);
    let first = r.start_offer(TS);
    assert_eq!(first[0].0, msg_type::OFFER);
    shuttle(&mut r, &mut p, first, TS, false);
    let (ri, _root) = r.verified().expect("verified");
    assert_eq!(ri.capability, CAP);
    assert_eq!(ri.usage.tokens_in, 11);
    assert_eq!(ri.outcome, 0);
    assert_eq!(p.state(), ProviderState::Completed);
}

#[test]
fn echo_direct_assign() {
    let mut r = req(b"direct");
    let mut p = ProviderTask::new(EchoExecutor);
    let first = r.start_direct(TS);
    assert_eq!(first[0].0, msg_type::ASSIGN);
    shuttle(&mut r, &mut p, first, TS, false);
    assert!(r.verified().is_some());
    assert_eq!(p.state(), ProviderState::Completed);
}

#[test]
fn empty_input_still_verifies() {
    let mut r = req(b"");
    let mut p = ProviderTask::new(EchoExecutor);
    let first = r.start_offer(TS);
    shuttle(&mut r, &mut p, first, TS, false);
    assert_eq!(r.verified().expect("verified").0.usage.tokens_in, 0);
}

#[test]
fn receipt_inputs_capture_timing_and_usage() {
    let mut r = req(b"abc");
    let mut p = ProviderTask::new(EchoExecutor);
    let first = r.start_offer(1000);
    shuttle(&mut r, &mut p, first, 1000, false);
    let (ri, _) = r.verified().unwrap();
    assert_eq!(ri.started_ms, 1000);
    assert_eq!(ri.ended_ms, 1000);
    assert_eq!(ri.task, task());
    assert_eq!(ri.usage.tokens_in, 3);
    assert_eq!(ri.usage.tokens_out, 3); // echo: out bytes == in bytes
}

#[test]
fn property_random_payloads_roundtrip() {
    let mut s = 0xABCDu64;
    fn lcg(s: &mut u64) -> u64 {
        *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *s
    }
    for _ in 0..20 {
        let n = (lcg(&mut s) % 200) as usize;
        let payload: Vec<u8> = (0..n).map(|_| (lcg(&mut s) & 0xff) as u8).collect();
        let mut r = req(&payload);
        let mut p = ProviderTask::new(EchoExecutor);
        let first = r.start_offer(TS);
        shuttle(&mut r, &mut p, first, TS, false);
        assert_eq!(r.verified().expect("verified").0.usage.tokens_in, payload.len() as u64);
    }
}

// ---- security -------------------------------------------------------------

#[test]
fn tampered_chunk_fails_5000() {
    let mut r = req(b"hello");
    let mut p = ProviderTask::new(EchoExecutor);
    let first = r.start_offer(TS);
    shuttle(&mut r, &mut p, first, TS, true);
    assert!(r.verified().is_none());
    assert_eq!(r.error_code(), Some(5000));
    assert_eq!(r.state(), ReqState::Failed);
}

// ---- flow credit ----------------------------------------------------------

#[test]
fn provider_starves_not_overruns() {
    let mut p = ProviderTask::new(EchoExecutor);
    let assign = Assign {
        task: task(),
        capability: CAP.into(),
        constraints: Default::default(),
        input: InputSpec::inline(b"hi".to_vec()),
        flow: FlowInit { msg_credits: 1, byte_credits: 1_000_000 },
        pay_agree_ref: None,
        ext: None,
    };
    let out = p.on_incoming(&mk(msg_type::ASSIGN, assign.to_value()), TS);
    assert_eq!(p.state(), ProviderState::Starved);
    assert_eq!(p.chunks_sent(), 1); // MUST NOT overrun the 1 granted credit
    assert!(out.iter().all(|o| o.0 != msg_type::COMPLETE));

    let fc = FlowCredit { scope: [1u8; 32], msg_credits: 64, byte_credits: 1024 };
    let out2 = p.on_incoming(&mk(msg_type::FLOW_CREDIT, fc.to_value()), TS);
    assert_eq!(p.state(), ProviderState::Completed);
    assert_eq!(p.chunks_sent(), 2);
    assert!(out2.iter().any(|o| o.0 == msg_type::COMPLETE));
}

#[test]
fn provider_zero_credit_starves_immediately() {
    let mut p = ProviderTask::new(EchoExecutor);
    let assign = Assign {
        task: task(),
        capability: CAP.into(),
        constraints: Default::default(),
        input: InputSpec::inline(b"hi".to_vec()),
        flow: FlowInit { msg_credits: 0, byte_credits: 0 },
        pay_agree_ref: None,
        ext: None,
    };
    let out = p.on_incoming(&mk(msg_type::ASSIGN, assign.to_value()), TS);
    assert_eq!(p.state(), ProviderState::Starved);
    assert_eq!(p.chunks_sent(), 0);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, msg_type::ACCEPT);
}

struct NChunks(usize);
impl Executor for NChunks {
    fn execute(&self, _input: &[u8]) -> Vec<(ChunkKind, Vec<u8>)> {
        (0..self.0).map(|i| (ChunkKind::OutputText, vec![(i & 0xff) as u8])).collect()
    }
}

#[test]
fn requester_regrants_credit_when_low() {
    let mut r = req(b"x");
    let mut p = ProviderTask::new(NChunks(50));
    let first = r.start_offer(TS);
    shuttle(&mut r, &mut p, first, TS, false);
    assert!(r.verified().is_some());
    assert!(r.credits_granted() >= 1, "should have re-granted credit");
    assert_eq!(r.chunks_received(), 50);
}

// ---- negotiation / FSM edges ---------------------------------------------

#[test]
fn quote_expired_rejected_locally() {
    let mut r = req(b"x");
    r.start_offer(TS);
    let out = r.on_incoming(&mk(msg_type::QUOTE, quote(100, TS - 1).to_value()), TS);
    assert!(out.is_empty());
    assert_eq!(r.state(), ReqState::Failed);
    assert_eq!(r.error_code(), Some(6000));
}

#[test]
fn quote_ttft_above_constraint_rejected() {
    let mut r = req_ttft(b"x", 100);
    r.start_offer(TS);
    let out = r.on_incoming(&mk(msg_type::QUOTE, quote(200, TS + 1000).to_value()), TS);
    assert!(out.is_empty());
    assert_eq!(r.state(), ReqState::Failed);
}

#[test]
fn reject_after_assign_fails_with_code() {
    let mut r = req(b"x");
    r.start_offer(TS);
    r.on_incoming(&mk(msg_type::QUOTE, quote(100, TS + 1000).to_value()), TS); // → ASSIGN, Pending
    let out = r.on_incoming(&mk(msg_type::REJECT, Reject { code: 3001 }.to_value()), TS);
    assert!(out.is_empty());
    assert_eq!(r.state(), ReqState::Failed);
    assert_eq!(r.error_code(), Some(3001));
}

#[test]
fn chunk_seq_gap_fails_1003() {
    let mut r = req(b"x");
    r.start_offer(TS);
    r.on_incoming(&mk(msg_type::QUOTE, quote(100, TS + 1000).to_value()), TS);
    r.on_incoming(&mk(msg_type::ACCEPT, Accept.to_value()), TS); // → Active
    let gap = ChunkMsg { task: task(), seq: 1, kind: ChunkKind::OutputText, content: b"x".to_vec(), fin: false };
    let out = r.on_incoming(&mk(msg_type::CHUNK, gap.to_value()), TS);
    assert!(out.is_empty());
    assert_eq!(r.state(), ReqState::Failed);
    assert_eq!(r.error_code(), Some(1003));
}

#[test]
fn ttft_deadline_triggers_cancel() {
    let mut r = req_ttft(b"x", 1000);
    r.start_offer(TS);
    r.on_incoming(&mk(msg_type::QUOTE, quote(500, TS + 60_000).to_value()), TS); // → ASSIGN
    r.on_incoming(&mk(msg_type::ACCEPT, Accept.to_value()), TS); // Active; deadline TS + 500*1.1 = TS+550
    let out = r.poll_deadline(TS + 600);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, msg_type::CANCEL);
    assert_eq!(r.state(), ReqState::Cancelled);
    assert_eq!(r.error_code(), Some(5001));
}

#[test]
fn direct_assign_has_no_ttft_deadline() {
    let mut r = req(b"x");
    r.start_direct(TS);
    r.on_incoming(&mk(msg_type::ACCEPT, Accept.to_value()), TS); // → Active
    assert!(r.poll_deadline(TS + 10_000_000).is_empty());
    assert_eq!(r.state(), ReqState::Active);
}

// ---- idempotent terminals -------------------------------------------------

#[test]
fn messages_after_verify_are_ignored() {
    let mut r = req(b"hi");
    let mut p = ProviderTask::new(EchoExecutor);
    let first = r.start_offer(TS);
    shuttle(&mut r, &mut p, first, TS, false);
    assert!(r.verified().is_some());
    let late = ChunkMsg { task: task(), seq: 5, kind: ChunkKind::OutputText, content: b"z".to_vec(), fin: false };
    assert!(r.on_incoming(&mk(msg_type::CHUNK, late.to_value()), TS).is_empty());
}

#[test]
fn provider_cancel_is_terminal() {
    let mut p = ProviderTask::new(EchoExecutor);
    let out = p.on_incoming(&mk(msg_type::CANCEL, Cancel { task: task(), code: 5001 }.to_value()), TS);
    assert!(out.is_empty());
    assert_eq!(p.state(), ProviderState::Cancelled);
    let assign = Assign {
        task: task(),
        capability: CAP.into(),
        constraints: Default::default(),
        input: InputSpec::inline(b"hi".to_vec()),
        flow: FlowInit { msg_credits: 64, byte_credits: 1024 },
        pay_agree_ref: None,
        ext: None,
    };
    assert!(p.on_incoming(&mk(msg_type::ASSIGN, assign.to_value()), TS).is_empty());
}

#[test]
fn provider_rejects_malformed_assign() {
    let mut p = ProviderTask::new(EchoExecutor);
    let out = p.on_incoming(&mk(msg_type::ASSIGN, Value::Map(vec![])), TS);
    assert!(out.is_empty());
    assert_eq!(p.state(), ProviderState::Failed);
}

// ---- over two live sessions -----------------------------------------------

#[test]
fn echo_over_two_live_sessions() {
    use map_core::hello::Hello;
    use map_identity::Keypair;
    use map_session::{CountingMsgIds, FixedClock, InMemoryDuplex, Session, SessionConfig};

    fn hello() -> Hello {
        Hello {
            versions: vec![map_core::PROTOCOL_VERSION_1_0],
            suites: vec![1],
            extensions: vec![],
            max_message_size: 262_144,
            max_tasks: None,
            zones: vec![],
        }
    }

    let a_id = Keypair::from_seed([1u8; 32]).agent_id();
    let b_id = Keypair::from_seed([2u8; 32]).agent_id();
    let (ta, tb) = InMemoryDuplex::pair(&a_id, &b_id);

    let verified = std::thread::scope(|s| {
        let a_for_b = a_id.clone();
        let hprov = s.spawn(move || {
            let cfg = SessionConfig::new(Keypair::from_seed([2u8; 32]), a_for_b, hello());
            let mut sess = Session::respond(cfg, tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            let mut p = ProviderTask::new(EchoExecutor);
            loop {
                let inc = match sess.recv() {
                    Ok(i) => i,
                    Err(_) => break,
                };
                for (mt, c) in p.on_incoming(&inc, TS) {
                    sess.send(mt, c).unwrap();
                }
                if matches!(p.state(), ProviderState::Completed | ProviderState::Cancelled | ProviderState::Failed) {
                    break;
                }
            }
        });

        let cfg = SessionConfig::new(Keypair::from_seed([1u8; 32]), b_id.clone(), hello());
        let mut sess = Session::initiate(cfg, ta, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
        let mut r = req(b"hello over the wire");
        for (mt, c) in r.start_offer(TS) {
            sess.send(mt, c).unwrap();
        }
        loop {
            if r.verified().is_some() || r.state().is_terminal() {
                break;
            }
            let inc = sess.recv().unwrap();
            for (mt, c) in r.on_incoming(&inc, TS) {
                sess.send(mt, c).unwrap();
            }
        }
        hprov.join().unwrap();
        r.verified().is_some()
    });

    assert!(verified, "echo task verified end-to-end over live sessions");
}

// ===========================================================================
// TASK-008 — receipts and evidence
// ===========================================================================

mod receipts_evidence {
    use super::*;
    use crate::{
        apply_verified_evidence, make_receipt_core, provider_countersign, query_evidence,
        requester_issue, requester_on_ack, serve_evidence, ReceiptInputs, TaskOutcome,
    };
    use map_identity::Keypair;
    use map_tasks::msg::Usage;
    use map_trust::{
        sign_receipt, Outcome, ProjParams, Receipt, ReceiptStore, TrustProjection, ROLE_PROVIDER,
        ROLE_REQUESTER,
    };

    fn ka() -> Keypair {
        Keypair::from_seed([1u8; 32])
    }
    fn kb() -> Keypair {
        Keypair::from_seed([2u8; 32])
    }
    fn kc() -> Keypair {
        Keypair::from_seed([3u8; 32])
    }
    fn inputs_n(task_n: u8) -> ReceiptInputs {
        ReceiptInputs {
            task: TaskId([task_n; 32]),
            capability: CAP.into(),
            started_ms: TS,
            ended_ms: TS + 1000,
            usage: Usage { tokens_in: 10, tokens_out: 10, ttft_ms: 0, wall_ms: 1000 },
            outcome: 0,
        }
    }
    /// A receipt from `req` about `prov`, dual-signed iff `dual`.
    fn receipt_n(req: &Keypair, prov: &Keypair, dual: bool, task_n: u8) -> Receipt {
        let core = make_receipt_core(&inputs_n(task_n), &req.agent_id(), &prov.agent_id(), TaskOutcome::Verified, None, None);
        let core_bytes = core.encode().unwrap();
        Receipt {
            sig_requester: Some(sign_receipt(req, ROLE_REQUESTER, &core_bytes)),
            sig_provider: dual.then(|| sign_receipt(prov, ROLE_PROVIDER, &core_bytes)),
            core_bytes,
        }
    }

    #[test]
    fn make_receipt_core_outcome_mapping() {
        let i = inputs_n(1);
        let (a, b) = (ka().agent_id(), kb().agent_id());
        assert_eq!(make_receipt_core(&i, &a, &b, TaskOutcome::Verified, None, None).outcome, Outcome::Ok);
        assert_eq!(make_receipt_core(&i, &a, &b, TaskOutcome::Failed, None, None).outcome, Outcome::Failed);
        assert_eq!(make_receipt_core(&i, &a, &b, TaskOutcome::Cancelled, None, None).outcome, Outcome::Cancelled);
    }

    /// FR-2 requester→provider→ack lifecycle by direct calls; both stores end
    /// holding the dual-signed receipt.
    fn run_receipt_flow(a_store: &mut ReceiptStore, b_store: &mut ReceiptStore, observation: &ReceiptInputs) -> Vec<Outgoing> {
        let root = [7u8; 32];
        let issued = requester_issue(observation, root, &ka(), &kb().agent_id(), TaskOutcome::Verified, Some(80), a_store).unwrap();
        assert_eq!(issued[0].0, msg_type::RECEIPT);
        let ack = provider_countersign(&mk(msg_type::RECEIPT, issued[0].1.clone()), observation, &kb(), b_store);
        ack
    }

    #[test]
    fn receipt_lifecycle_dual_signs_and_stores() {
        let (mut a_store, mut b_store) = (ReceiptStore::new(), ReceiptStore::new());
        let obs = inputs_n(9);
        let ack = run_receipt_flow(&mut a_store, &mut b_store, &obs);
        assert_eq!(ack[0].0, msg_type::RECEIPT_ACK);
        requester_on_ack(&mk(msg_type::RECEIPT_ACK, ack[0].1.clone()), &mut a_store).unwrap();
        // B holds one dual receipt; verify() confirms dual.
        assert_eq!(b_store.len(), 1);
        let v = b_store.iter().next().unwrap().verify().unwrap();
        assert!(v.dual);
    }

    #[test]
    fn duplicate_receipt_is_idempotent() {
        let (mut a_store, mut b_store) = (ReceiptStore::new(), ReceiptStore::new());
        let obs = inputs_n(9);
        let issued = requester_issue(&obs, [7u8; 32], &ka(), &kb().agent_id(), TaskOutcome::Verified, None, &mut a_store).unwrap();
        let ack1 = provider_countersign(&mk(msg_type::RECEIPT, issued[0].1.clone()), &obs, &kb(), &mut b_store);
        let ack2 = provider_countersign(&mk(msg_type::RECEIPT, issued[0].1.clone()), &obs, &kb(), &mut b_store);
        assert_eq!(ack1, ack2); // same ACK re-derived (deterministic countersign)
        assert_eq!(b_store.len(), 1); // duplicate not double-stored
    }

    #[test]
    fn tolerance_violation_not_countersigned() {
        // Requester claims 10x the observed usage → outside ±5% → ERROR.
        let mut a_store = ReceiptStore::new();
        let claimed = ReceiptInputs { usage: Usage { tokens_in: 100, tokens_out: 100, ttft_ms: 0, wall_ms: 1000 }, ..inputs_n(9) };
        let observed = inputs_n(9); // tokens 10
        let issued = requester_issue(&claimed, [7u8; 32], &ka(), &kb().agent_id(), TaskOutcome::Verified, None, &mut a_store).unwrap();
        let out = provider_countersign(&mk(msg_type::RECEIPT, issued[0].1.clone()), &observed, &kb(), &mut ReceiptStore::new());
        assert_eq!(out[0].0, msg_type::ERROR);
        // ERRATA 2026-07-12: countersignature refusal is code 2006 (was 5000).
        let em = map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap();
        assert_eq!(em.code, 2006);
        assert!(!em.retryable);
    }

    #[test]
    fn receipt_for_unknown_task_not_countersigned() {
        // §11(c): provider's observation is task 9, receipt claims task 5.
        let mut a_store = ReceiptStore::new();
        let issued = requester_issue(&inputs_n(5), [7u8; 32], &ka(), &kb().agent_id(), TaskOutcome::Verified, None, &mut a_store).unwrap();
        let out = provider_countersign(&mk(msg_type::RECEIPT, issued[0].1.clone()), &inputs_n(9), &kb(), &mut ReceiptStore::new());
        assert_eq!(out[0].0, msg_type::ERROR);
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 2006);
    }

    #[test]
    fn tampered_core_byte_fails_verify() {
        // §11(b): flip a byte of the signed core → signature no longer verifies.
        let mut r = receipt_n(&ka(), &kb(), true, 9);
        r.core_bytes[10] ^= 0xff;
        assert!(r.verify().is_err());
    }

    #[test]
    fn single_signed_receipt_never_reaches_projection() {
        // §11(a): a unilateral (requester-only) receipt verifies structurally but
        // the dual gate keeps it out of a projection.
        let v = receipt_n(&ka(), &kb(), false, 9).verify().unwrap();
        assert!(!v.dual);
        let mut proj = TrustProjection::new(ProjParams::default());
        let applied = apply_verified_evidence(&[v], &kb().agent_id(), &mut proj, TS, 1.0);
        assert_eq!(applied, 0);
        assert!(proj.score(&kb().agent_id(), CAP).is_none());
    }

    #[test]
    fn dual_receipt_about_other_provider_not_applied() {
        // A dual receipt where B is the REQUESTER (provider is C) must not be
        // filed under B when C is the queried subject — and vice versa.
        let v = receipt_n(&kb(), &kc(), true, 9).verify().unwrap(); // provider = C
        let mut proj = TrustProjection::new(ProjParams::default());
        // Querying about B: this receipt names C as provider → not applied.
        assert_eq!(apply_verified_evidence(&[v], &kb().agent_id(), &mut proj, TS, 1.0), 0);
        assert!(proj.score(&kb().agent_id(), CAP).is_none());
    }

    #[test]
    fn evidence_filters_and_batch_split() {
        let mut store = ReceiptStore::new();
        for n in 0..9u8 {
            store.ingest(receipt_n(&ka(), &kb(), true, n)).unwrap(); // 9 distinct, B as provider
        }
        store.ingest(receipt_n(&ka(), &kc(), true, 200)).unwrap(); // one about C
        // Role filter 1 (as-provider) for B → 9 match, batch caps at 8 + more.
        let q = Value::Map(vec![
            (Value::Uint(1), Value::Bytes(kb().agent_id().as_bytes().to_vec())),
            (Value::Uint(5), Value::Uint(1)),
        ]);
        let out = serve_evidence(&mk(msg_type::EVIDENCE_QUERY, q), &store);
        assert_eq!(out[0].0, msg_type::EVIDENCE_SET);
        let set = &out[0].1;
        assert_eq!(set.map_get(1).unwrap().as_array().unwrap().len(), 8);
        assert!(matches!(set.map_get(2), Some(Value::Bool(true)))); // more
    }

    /// FR-4: full three-agent flow. A↔B echo, receipt lifecycle, then C queries
    /// B for evidence over a live session, verifies, applies → score(B) Some.
    #[test]
    fn three_agent_end_to_end() {
        use map_core::hello::Hello;
        use map_session::{CountingMsgIds, FixedClock, InMemoryDuplex, Session, SessionConfig};

        fn hello() -> Hello {
            Hello { versions: vec![map_core::PROTOCOL_VERSION_1_0], suites: vec![1], extensions: vec![], max_message_size: 262_144, max_tasks: None, zones: vec![] }
        }

        // 1. Echo task A↔B (runtimes), capture A's inputs + B's observation.
        let mut r = req(b"work for a receipt");
        let mut p = ProviderTask::new(EchoExecutor);
        let first = r.start_offer(TS);
        shuttle(&mut r, &mut p, first, TS, false);
        let (inputs, root) = r.verified().expect("verified");
        let observation = p.observation().expect("observation").clone();

        // 2. Receipt lifecycle by direct calls; B ends holding the dual receipt.
        let (mut a_store, mut b_store) = (ReceiptStore::new(), ReceiptStore::new());
        let issued = requester_issue(inputs, *root, &ka(), &kb().agent_id(), TaskOutcome::Verified, Some(90), &mut a_store).unwrap();
        let ack = provider_countersign(&mk(msg_type::RECEIPT, issued[0].1.clone()), &observation, &kb(), &mut b_store);
        assert_eq!(ack[0].0, msg_type::RECEIPT_ACK);
        requester_on_ack(&mk(msg_type::RECEIPT_ACK, ack[0].1.clone()), &mut a_store).unwrap();

        // 3. Agent C sessions to B and pulls evidence; B serves from b_store.
        let c_id = kc().agent_id();
        let b_id = kb().agent_id();
        let (tc, tb) = InMemoryDuplex::pair(&c_id, &b_id);
        let applied = std::thread::scope(|s| {
            let c_for_b = c_id.clone();
            let hb = s.spawn(move || {
                let cfg = SessionConfig::new(kb(), c_for_b, hello());
                let mut sess = Session::respond(cfg, tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
                let inc = sess.recv().unwrap();
                for (mt, c) in serve_evidence(&inc, &b_store) {
                    sess.send(mt, c).unwrap();
                }
            });
            let cfg = SessionConfig::new(kc(), b_id.clone(), hello());
            let mut sess = Session::initiate(cfg, tc, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
            let verified = query_evidence(&mut sess, &b_id, Some(CAP), Some(1)).unwrap();
            hb.join().unwrap();
            let mut proj = TrustProjection::new(ProjParams::default());
            let applied = apply_verified_evidence(&verified, &b_id, &mut proj, TS + 86_400_000, 1.0);
            (applied, proj.score(&b_id, CAP).is_some())
        });
        assert_eq!(applied.0, 1, "C applied the dual receipt about B");
        assert!(applied.1, "score(B, cap) is Some after applying evidence");
    }
}

// ===========================================================================
// TASK-009 — payments MVP (pay:credit-ledger)
// ===========================================================================

mod payments_flow {
    use super::*;
    use crate::{ProviderPayState, ProviderPayment, RequesterPayState, RequesterPayment};
    use map_identity::Keypair;
    use map_pay::codec::{pay_ref_digest, AssetAmount, MethodOffer, PayAgree, PayInvoice, PayQuote};
    use map_pay::credit_ledger::{verify_entry, EntryProof, LedgerOperator, CREDIT_LEDGER_METHOD};
    use map_pay::{Amount, Asset};

    const QID: [u8; 16] = [0x11; 16];
    const IID: [u8; 16] = [0x22; 16];

    fn ka() -> Keypair {
        Keypair::from_seed([1u8; 32]) // requester / payer
    }
    fn kb() -> Keypair {
        Keypair::from_seed([2u8; 32]) // provider / payee
    }
    fn kop() -> Keypair {
        Keypair::from_seed([4u8; 32]) // ledger operator
    }
    fn usd(mantissa: i64) -> AssetAmount {
        AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, mantissa) }
    }
    fn operator() -> LedgerOperator {
        let mut op = LedgerOperator::new(kop(), -2);
        op.credit(&ka().agent_id(), 1000);
        op
    }
    fn provider_pay() -> ProviderPayment {
        ProviderPayment::new(kb().agent_id(), ka().agent_id(), kop().agent_id())
    }
    fn requester_pay(task: Option<[u8; 32]>) -> RequesterPayment {
        RequesterPayment::new(task, ka().agent_id(), kb().agent_id())
    }

    /// Issue a PAY_QUOTE and bind the provider to the exact frame the requester
    /// will digest (`Incoming.raw`), mirroring the pump feeding back the bytes
    /// `Session::send` returned (TASK-017 §FR-3). Returns that Incoming.
    fn issue_bound(pp: &mut ProviderPayment, qid: [u8; 16], task: [u8; 32], total: AssetAmount, expiry: u64) -> Incoming {
        let q = pp.issue_quote(qid, task, total, expiry);
        let inc = mk(msg_type::PAY_QUOTE, q[0].1.clone());
        pp.bind_quote_envelope(&inc.raw);
        inc
    }

    /// Drive quote→agree→invoice→pay→proof→settle→receipt. Returns the parts so
    /// tests can inspect state and balances.
    fn full_leg() -> (ProviderPayment, RequesterPayment, LedgerOperator) {
        let (mut pp, mut rp, mut op) = (provider_pay(), requester_pay(Some([9; 32])), operator());
        let quote_inc = issue_bound(&mut pp, QID, [9; 32], usd(500), TS + 60_000);
        let agree = rp.on_quote(&quote_inc, TS);
        assert_eq!(agree[0].0, msg_type::PAY_AGREE);
        assert!(pp.on_agree(&mk(msg_type::PAY_AGREE, agree[0].1.clone())).is_empty());
        let inv = pp.invoice(IID, TS);
        assert_eq!(inv[0].0, msg_type::PAY_INVOICE);
        let proof = rp.on_invoice(&mk(msg_type::PAY_INVOICE, inv[0].1.clone()), &mut op, TS);
        assert_eq!(proof[0].0, msg_type::PAY_PROOF);
        let rcpt = pp.on_proof(&mk(msg_type::PAY_PROOF, proof[0].1.clone()), TS);
        assert_eq!(rcpt[0].0, msg_type::PAY_RECEIPT);
        rp.on_receipt(&mk(msg_type::PAY_RECEIPT, rcpt[0].1.clone()));
        (pp, rp, op)
    }

    #[test]
    fn happy_path_settles_and_moves_balance_once() {
        let (pp, rp, op) = full_leg();
        assert_eq!(pp.state(), ProviderPayState::Settled);
        assert_eq!(rp.state(), RequesterPayState::Settled);
        assert_eq!(op.balance(&ka().agent_id()), 500); // 1000 - 500
        assert_eq!(op.balance(&kb().agent_id()), 500);
        assert!(rp.receipt_bytes().is_some());
    }

    #[test]
    fn quote_carries_task_binding_and_method() {
        let q = provider_pay().issue_quote(QID, [9; 32], usd(500), TS + 1);
        let pq = PayQuote::from_value(&q[0].1).unwrap();
        assert_eq!(pq.task_binding, Some([9; 32]));
        assert_eq!(pq.pricing_model, 0);
        assert!(pq.methods.iter().any(|m| m.uri == CREDIT_LEDGER_METHOD));
    }

    #[test]
    fn retries_move_money_at_most_once() {
        let (mut pp, mut rp, mut op) = (provider_pay(), requester_pay(Some([9; 32])), operator());
        let quote_inc = issue_bound(&mut pp, QID, [9; 32], usd(500), TS + 60_000);
        let agree = rp.on_quote(&quote_inc, TS);
        pp.on_agree(&mk(msg_type::PAY_AGREE, agree[0].1.clone()));
        let inv = pp.invoice(IID, TS);
        // Pay once.
        let proof = rp.on_invoice(&mk(msg_type::PAY_INVOICE, inv[0].1.clone()), &mut op, TS);
        assert_eq!(op.balance(&ka().agent_id()), 500);
        // Retry the SAME invoice → operator idempotent → identical proof, no move.
        let proof2 = rp.on_invoice(&mk(msg_type::PAY_INVOICE, inv[0].1.clone()), &mut op, TS);
        assert_eq!(proof, proof2);
        assert_eq!(op.balance(&ka().agent_id()), 500);
        // Provider settles once; a retried PROOF → AlreadySettled → same receipt.
        let r1 = pp.on_proof(&mk(msg_type::PAY_PROOF, proof[0].1.clone()), TS);
        let r2 = pp.on_proof(&mk(msg_type::PAY_PROOF, proof[0].1.clone()), TS);
        assert_eq!(r1, r2);
        assert_eq!(op.balance(&ka().agent_id()), 500);
        assert_eq!(op.balance(&kb().agent_id()), 500);
    }

    #[test]
    fn receipt_entry_verifies_against_operator() {
        let (_pp, rp, _op) = full_leg();
        // The requester's stored receipt bytes decode to a PAY_RECEIPT.
        assert!(map_pay::codec::PayReceipt::from_value(
            &map_core::cbor::decode(rp.receipt_bytes().unwrap()).unwrap()
        )
        .is_ok());
    }

    #[test]
    fn task_binding_mismatch_rejected() {
        let mut rp = requester_pay(Some([9; 32])); // expects task 9
        let q = provider_pay().issue_quote(QID, [7; 32], usd(500), TS + 60_000); // bound to task 7
        let out = rp.on_quote(&mk(msg_type::PAY_QUOTE, q[0].1.clone()), TS);
        assert_eq!(out[0].0, msg_type::ERROR);
        // ERRATA 2026-07-12: task-binding mismatch is its own code 6007.
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6007);
        assert_eq!(rp.state(), RequesterPayState::Failed);
    }

    #[test]
    fn standing_quote_without_binding_is_agreeable() {
        let mut rp = requester_pay(None); // no task-bound offer
        let q = PayQuote {
            quote_id: QID,
            pricing_model: 0,
            rate_table: vec![(0, usd(500))],
            total: Some(usd(500)),
            methods: vec![MethodOffer { uri: CREDIT_LEDGER_METHOD.into(), params: vec![] }],
            expiry_ms: TS + 1000,
            terms: Some(0),
            ext: None,
            task_binding: None,
        };
        let out = rp.on_quote(&mk(msg_type::PAY_QUOTE, q.to_value()), TS);
        assert_eq!(out[0].0, msg_type::PAY_AGREE);
        assert_eq!(rp.state(), RequesterPayState::Agreed);
    }

    #[test]
    fn expired_quote_rejected_6000() {
        let mut rp = requester_pay(Some([9; 32]));
        let q = provider_pay().issue_quote(QID, [9; 32], usd(500), TS - 1); // expired
        let out = rp.on_quote(&mk(msg_type::PAY_QUOTE, q[0].1.clone()), TS);
        assert_eq!(out[0].0, msg_type::ERROR);
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6000);
    }

    #[test]
    fn cap_exceeded_refused() {
        let (mut pp, mut rp, mut op) = (provider_pay(), requester_pay(Some([9; 32])), operator());
        let q = pp.issue_quote(QID, [9; 32], usd(500), TS + 60_000);
        rp.on_quote(&mk(msg_type::PAY_QUOTE, q[0].1.clone()), TS); // cap = 500
        // Provider (mis)invoices 999 > cap → requester refuses, no balance move.
        let bad = PayInvoice { invoice_id: IID, quote_id: QID, amount: usd(999), watermark: None, method_payload: vec![], timestamp_ms: TS };
        let out = rp.on_invoice(&mk(msg_type::PAY_INVOICE, bad.to_value()), &mut op, TS);
        assert_eq!(out[0].0, msg_type::ERROR);
        // ERRATA 2026-07-12: authorized-cap exceeded is its own code 6005.
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6005);
        assert_eq!(op.balance(&ka().agent_id()), 1000); // untouched
    }

    #[test]
    fn price_switch_digest_mismatch_rejected() {
        let mut pp = provider_pay();
        let _ = issue_bound(&mut pp, QID, [9; 32], usd(500), TS + 60_000);
        // A PAY_AGREE bound to a DIFFERENT quote digest → price-switch → reject.
        let switched = PayAgree { quote_id: QID, quote_digest: pay_ref_digest(b"a different quote"), method: CREDIT_LEDGER_METHOD.into(), authorized_cap: Some(usd(500)) };
        let out = pp.on_agree(&mk(msg_type::PAY_AGREE, switched.to_value()));
        assert_eq!(out[0].0, msg_type::ERROR);
        // ERRATA 2026-07-12: price switch is agreement-digest-mismatch 6006.
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6006);
        assert_eq!(pp.state(), ProviderPayState::Failed);
    }

    #[test]
    fn payagree_binds_received_envelope_bytes_not_content() {
        // FR-6(b)/(d): the requester digests the RECEIVED PAY_QUOTE envelope
        // (Incoming.raw), so the PAY_AGREE digest equals pay_ref_digest(frame)
        // and NOT pay_ref_digest(encode(content)) (pay §3.1, I1).
        let q = PayQuote {
            quote_id: QID,
            pricing_model: 0,
            rate_table: vec![(0, usd(500))],
            total: Some(usd(500)),
            methods: vec![MethodOffer { uri: CREDIT_LEDGER_METHOD.into(), params: vec![] }],
            expiry_ms: TS + 1000,
            terms: Some(0),
            ext: None,
            task_binding: Some([9; 32]),
        };
        // A real signed COSE envelope carrying the quote — the "frame on the wire".
        let body = map_core::envelope::Body {
            version: map_core::PROTOCOL_VERSION_1_0,
            msg_type: msg_type::PAY_QUOTE,
            msg_id: MsgId([10u8; 16]),
            timestamp_ms: TS,
            recipient: Some(ka().agent_id()),
            ttl_ms: None,
            session_id: Some([0u8; 16]),
            seq: Some(0),
            content: q.to_value(),
        };
        let frame = map_core::envelope::seal(&kb(), &body, b"").unwrap();
        let content_digest = pay_ref_digest(&map_core::cbor::encode(&q.to_value()).unwrap());
        let frame_digest = pay_ref_digest(&frame);
        assert_ne!(frame_digest, content_digest, "envelope and content digests must differ");

        let inc = Incoming {
            msg_type: msg_type::PAY_QUOTE,
            content: q.to_value(),
            msg_id: MsgId([10u8; 16]),
            seq: 0,
            raw: frame.clone(),
        };
        let mut rp = requester_pay(Some([9; 32]));
        let out = rp.on_quote(&inc, TS);
        assert_eq!(out[0].0, msg_type::PAY_AGREE);
        let agree = PayAgree::from_value(&out[0].1).unwrap();
        assert_eq!(agree.quote_digest, frame_digest, "PAY_AGREE must bind the envelope digest");
        assert_ne!(agree.quote_digest, content_digest, "must NOT bind the content re-encoding");
    }

    #[test]
    fn forged_ledger_entry_rejected() {
        // Set the flow to Invoiced, then feed a PROOF signed by the WRONG operator.
        let (mut pp, mut rp, op) = (provider_pay(), requester_pay(Some([9; 32])), operator());
        let quote_inc = issue_bound(&mut pp, QID, [9; 32], usd(500), TS + 60_000);
        let agree = rp.on_quote(&quote_inc, TS);
        pp.on_agree(&mk(msg_type::PAY_AGREE, agree[0].1.clone()));
        pp.invoice(IID, TS);
        // Forge: a rogue operator signs an entry moving the same amount.
        let mut rogue = LedgerOperator::new(Keypair::from_seed([99u8; 32]), -2);
        rogue.credit(&ka().agent_id(), 1000);
        let forged = rogue.pay(IID, &ka().agent_id(), &kb().agent_id(), &usd(500), TS).unwrap();
        assert!(!verify_entry(&forged, &kop().agent_id(), IID, &ka().agent_id(), &kb().agent_id(), &usd(500)));
        let proof = map_pay::codec::PayProof { invoice_id: IID, method: CREDIT_LEDGER_METHOD.into(), proof_payload: forged.to_bytes() };
        let out = pp.on_proof(&mk(msg_type::PAY_PROOF, proof.to_value()), TS);
        assert_eq!(out[0].0, msg_type::ERROR);
        assert_ne!(pp.state(), ProviderPayState::Settled);
        assert_eq!(op.balance(&ka().agent_id()), 1000); // real operator untouched
        let _ = EntryProof::from_bytes;
    }

    /// FR-5 §15: TASK-007 echo task + the full payment leg together.
    #[test]
    fn echo_task_plus_payment_leg() {
        // Task A<->B echo to Verified.
        let mut r = req(b"paid work");
        let mut p = ProviderTask::new(EchoExecutor);
        let first = r.start_offer(TS);
        shuttle(&mut r, &mut p, first, TS, false);
        assert!(r.verified().is_some());
        assert_eq!(p.state(), ProviderState::Completed);
        // Payment leg settles and moves the balance exactly once.
        let (pp, rp, op) = full_leg();
        assert_eq!(pp.state(), ProviderPayState::Settled);
        assert_eq!(rp.state(), RequesterPayState::Settled);
        assert_eq!(op.balance(&kb().agent_id()), 500);
    }
}

// ===========================================================================
// TASK-022 — metered-incremental (terms 3) runtime
// ===========================================================================

mod metered_flow {
    use super::*;
    use crate::{MeteredProviderPayment, MeteredProviderState, MeteredRequesterPayment, MeteredRequesterState};
    use map_core::cbor::decode;
    use map_identity::Keypair;
    use map_pay::codec::{AssetAmount, MethodOffer, PayInvoice, PayQuote, Usage, UsageWatermark};
    use map_pay::credit_ledger::{LedgerOperator, CREDIT_LEDGER_METHOD};
    use map_pay::{Amount, Asset};

    const QID: [u8; 16] = [0x40; 16];
    const TASK: [u8; 32] = [0x9; 32];

    fn ka() -> Keypair { Keypair::from_seed([1u8; 32]) } // requester/payer
    fn kb() -> Keypair { Keypair::from_seed([2u8; 32]) } // provider/payee
    fn kop() -> Keypair { Keypair::from_seed([4u8; 32]) } // operator

    /// microUSD (exponent -6), the metered rate/amount exponent.
    fn micro(mantissa: i64) -> AssetAmount {
        AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-6, mantissa) }
    }

    /// Operator settling in microUSD with `credit` on the requester balance.
    fn operator(credit: i128) -> LedgerOperator {
        let mut op = LedgerOperator::new(kop(), -6);
        op.credit(&ka().agent_id(), credit);
        op
    }

    /// Terms-3, model-1 quote: 3000/token-in, 5000/token-out microUSD, cap `cap`.
    fn metered_quote(cap: AssetAmount) -> PayQuote {
        PayQuote {
            quote_id: QID,
            pricing_model: 1,
            rate_table: vec![(1, micro(3000)), (2, micro(5000))],
            total: Some(cap),
            methods: vec![MethodOffer { uri: CREDIT_LEDGER_METHOD.into(), params: vec![] }],
            expiry_ms: TS + 60_000,
            terms: Some(3),
            ext: None,
            task_binding: Some(TASK),
        }
    }

    fn usage(tin: u64, tout: u64, wall: u64) -> Usage {
        Usage { tokens_in: tin, tokens_out: tout, ttft_ms: 0, wall_ms: wall }
    }

    /// Establish agreement, returning provider/requester/operator ready to stream.
    fn agreed(cap: AssetAmount, credit: i128) -> (MeteredProviderPayment, MeteredRequesterPayment, LedgerOperator) {
        let q = metered_quote(cap);
        let mut pp = MeteredProviderPayment::new(kb().agent_id(), ka().agent_id(), kop().agent_id(), TASK, q.clone());
        let mut rp = MeteredRequesterPayment::new(TASK, ka().agent_id(), kb().agent_id());
        let qinc = mk(msg_type::PAY_QUOTE, q.to_value());
        pp.bind_quote_envelope(&qinc.raw);
        let agree = rp.on_quote(&qinc, TS);
        assert_eq!(agree[0].0, msg_type::PAY_AGREE);
        assert!(pp.on_agree(&mk(msg_type::PAY_AGREE, agree[0].1.clone())).is_empty());
        (pp, rp, operator(credit))
    }

    /// Drive one billing round: bill (CHUNK + INVOICE), meter, verify, pay,
    /// settle. Returns the requester on_invoice output (PAY_PROOF or ERROR).
    fn round(pp: &mut MeteredProviderPayment, rp: &mut MeteredRequesterPayment, op: &mut LedgerOperator,
             u: Usage, seq: u64, id: [u8; 16]) -> Vec<Outgoing> {
        let billed = pp.bill(u, seq, id, TS);
        assert_eq!(billed[0].0, msg_type::CHUNK);
        assert_eq!(billed[1].0, msg_type::PAY_INVOICE);
        rp.on_chunk(&mk(msg_type::CHUNK, billed[0].1.clone()));
        let out = rp.on_invoice(&mk(msg_type::PAY_INVOICE, billed[1].1.clone()), op, TS);
        if !out.is_empty() && out[0].0 == msg_type::PAY_PROOF {
            let rcpt = pp.on_proof(&mk(msg_type::PAY_PROOF, out[0].1.clone()), TS);
            assert_eq!(rcpt[0].0, msg_type::PAY_RECEIPT);
            rp.on_receipt(&mk(msg_type::PAY_RECEIPT, rcpt[0].1.clone()));
        }
        out
    }

    #[test]
    fn happy_path_two_incremental_invoices_settle_deltas() {
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 5_000_000);
        // Round 1: 100 in, 40 out -> 300000 + 200000 = 500000 microUSD.
        round(&mut pp, &mut rp, &mut op, usage(100, 40, 1000), 4, [1; 16]);
        assert_eq!(op.balance(&ka().agent_id()), 4_500_000);
        assert_eq!(op.balance(&kb().agent_id()), 500_000);
        // Round 2: cumulative 200 in, 80 out -> 1_000_000; delta 500000.
        round(&mut pp, &mut rp, &mut op, usage(200, 80, 2000), 9, [2; 16]);
        assert_eq!(op.balance(&ka().agent_id()), 4_000_000);
        assert_eq!(op.balance(&kb().agent_id()), 1_000_000);
        assert_eq!(pp.receipts().len(), 2);
        assert_eq!(rp.receipts().len(), 2);
    }

    #[test]
    fn invoice_before_any_chunk_fails_meter_consistency_6009() {
        // pay 4.1 check 4: bill without the requester having metered the usage.
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 5_000_000);
        let billed = pp.bill(usage(100, 40, 1000), 4, [1; 16], TS);
        let out = rp.on_invoice(&mk(msg_type::PAY_INVOICE, billed[1].1.clone()), &mut op, TS);
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6009);
        assert_eq!(op.balance(&ka().agent_id()), 5_000_000);
    }

    #[test]
    fn inflated_usage_beyond_meter_rejected_6009() {
        // security: provider bills MORE usage than the kind-3 chunk it emitted.
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 5_000_000);
        let billed = pp.bill(usage(100, 40, 1000), 4, [1; 16], TS);
        rp.on_chunk(&mk(msg_type::CHUNK, billed[0].1.clone()));
        let w = UsageWatermark { task: TASK, through_seq: 4, usage: usage(200, 80, 1000), cumulative_billed: micro(1_000_000) };
        let forged = PayInvoice { invoice_id: [1; 16], quote_id: QID, amount: micro(1_000_000), watermark: Some(w), method_payload: vec![], timestamp_ms: TS };
        let out = rp.on_invoice(&mk(msg_type::PAY_INVOICE, forged.to_value()), &mut op, TS);
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6009);
    }

    #[test]
    fn off_by_one_billing_rejected_6009() {
        // NV-010 shape, live: claimed cumulative 500001, rate x usage = 500000.
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 5_000_000);
        let billed = pp.bill(usage(100, 40, 1000), 4, [1; 16], TS);
        rp.on_chunk(&mk(msg_type::CHUNK, billed[0].1.clone()));
        let w = UsageWatermark { task: TASK, through_seq: 4, usage: usage(100, 40, 1000), cumulative_billed: micro(500_001) };
        let bad = PayInvoice { invoice_id: [1; 16], quote_id: QID, amount: micro(500_001), watermark: Some(w), method_payload: vec![], timestamp_ms: TS };
        let out = rp.on_invoice(&mk(msg_type::PAY_INVOICE, bad.to_value()), &mut op, TS);
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6009);
    }

    #[test]
    fn over_cap_rejected_6005_but_exact_cap_accepted() {
        let (mut pp, mut rp, mut op) = agreed(micro(500_000), 5_000_000);
        let out = round(&mut pp, &mut rp, &mut op, usage(100, 40, 1000), 4, [1; 16]);
        assert_eq!(out[0].0, msg_type::PAY_PROOF); // cumulative == cap: accepted
        let billed = pp.bill(usage(200, 80, 2000), 9, [2; 16], TS);
        rp.on_chunk(&mk(msg_type::CHUNK, billed[0].1.clone()));
        let out2 = rp.on_invoice(&mk(msg_type::PAY_INVOICE, billed[1].1.clone()), &mut op, TS);
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out2[0].1).unwrap().code, 6005);
    }

    #[test]
    fn task_binding_mismatch_rejected_6007() {
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 5_000_000);
        let billed = pp.bill(usage(100, 40, 1000), 4, [1; 16], TS);
        rp.on_chunk(&mk(msg_type::CHUNK, billed[0].1.clone()));
        let w = UsageWatermark { task: [0xCC; 32], through_seq: 4, usage: usage(100, 40, 1000), cumulative_billed: micro(500_000) };
        let bad = PayInvoice { invoice_id: [1; 16], quote_id: QID, amount: micro(500_000), watermark: Some(w), method_payload: vec![], timestamp_ms: TS };
        let out = rp.on_invoice(&mk(msg_type::PAY_INVOICE, bad.to_value()), &mut op, TS);
        assert_eq!(map_core::errors::ErrorMsg::from_value(&out[0].1).unwrap().code, 6007);
    }

    #[test]
    fn zero_delta_invoice_is_legal_and_bills_zero() {
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 5_000_000);
        round(&mut pp, &mut rp, &mut op, usage(100, 40, 1000), 4, [1; 16]);
        let before = op.balance(&ka().agent_id());
        let out = round(&mut pp, &mut rp, &mut op, usage(100, 40, 1000), 5, [2; 16]);
        assert_eq!(out[0].0, msg_type::PAY_PROOF); // monotone, delta 0
        assert_eq!(op.balance(&ka().agent_id()), before);
    }

    #[test]
    fn resent_invoice_settles_once() {
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 5_000_000);
        let b1 = pp.bill(usage(100, 40, 1000), 4, [1; 16], TS);
        let b2 = pp.bill(usage(100, 40, 1000), 4, [1; 16], TS); // same id
        assert_eq!(b1, b2, "re-billed invoice must be byte-identical");
        rp.on_chunk(&mk(msg_type::CHUNK, b1[0].1.clone()));
        let p1 = rp.on_invoice(&mk(msg_type::PAY_INVOICE, b1[1].1.clone()), &mut op, TS);
        let p2 = rp.on_invoice(&mk(msg_type::PAY_INVOICE, b1[1].1.clone()), &mut op, TS);
        assert_eq!(p1, p2);
        assert_eq!(op.balance(&ka().agent_id()), 4_500_000); // moved once
        let r1 = pp.on_proof(&mk(msg_type::PAY_PROOF, p1[0].1.clone()), TS);
        let r2 = pp.on_proof(&mk(msg_type::PAY_PROOF, p1[0].1.clone()), TS);
        assert_eq!(r1, r2);
        assert_eq!(pp.receipts().len(), 1);
    }

    #[test]
    fn exhaustion_pauses_6001_with_watermark_then_resumes_on_topup() {
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 500_000); // round 1 only
        round(&mut pp, &mut rp, &mut op, usage(100, 40, 1000), 4, [1; 16]);
        assert_eq!(op.balance(&ka().agent_id()), 0);
        let billed = pp.bill(usage(200, 80, 2000), 9, [2; 16], TS);
        rp.on_chunk(&mk(msg_type::CHUNK, billed[0].1.clone()));
        let out = rp.on_invoice(&mk(msg_type::PAY_INVOICE, billed[1].1.clone()), &mut op, TS);
        assert!(out.is_empty());
        assert_eq!(rp.state(), MeteredRequesterState::Insufficient);
        let pause = pp.pause([2; 16]);
        let em = map_core::errors::ErrorMsg::from_value(&pause[0].1).unwrap();
        assert_eq!(em.code, 6001);
        assert!(em.retryable);
        assert_eq!(pp.state(), MeteredProviderState::Paused);
        let detail = em.detail.expect("6001 carries detail");
        let w = UsageWatermark::from_value(detail.map_get(1).expect("detail key 1")).expect("watermark decodes");
        assert_eq!(w.cumulative_billed, micro(1_000_000));
        op.credit(&ka().agent_id(), 500_000);
        let proof = rp.retry_pending(&mut op, TS);
        assert_eq!(proof[0].0, msg_type::PAY_PROOF);
        let rcpt = pp.on_proof(&mk(msg_type::PAY_PROOF, proof[0].1.clone()), TS);
        assert_eq!(rcpt[0].0, msg_type::PAY_RECEIPT);
        assert_eq!(pp.state(), MeteredProviderState::Streaming); // resumed
        assert_eq!(op.balance(&kb().agent_id()), 1_000_000);
    }

    #[test]
    fn owed_at_checkpoint_bounded_by_unpaid_watermark() {
        let (mut pp, mut rp, mut op) = agreed(micro(10_000_000), 500_000);
        round(&mut pp, &mut rp, &mut op, usage(100, 40, 1000), 4, [1; 16]);
        let billed = pp.bill(usage(200, 80, 2000), 9, [2; 16], TS);
        rp.on_chunk(&mk(msg_type::CHUNK, billed[0].1.clone()));
        let _ = rp.on_invoice(&mk(msg_type::PAY_INVOICE, billed[1].1.clone()), &mut op, TS);
        let pause = pp.pause([2; 16]);
        let em = map_core::errors::ErrorMsg::from_value(&pause[0].1).unwrap();
        let detail = em.detail.unwrap();
        let unpaid = UsageWatermark::from_value(detail.map_get(1).unwrap()).unwrap();
        // Checkpoint at the round-1 usage (100/40) -> owed 500000 <= unpaid 1000000.
        let owed = pp.owed_at_checkpoint(&usage(100, 40, 1000));
        assert_eq!(owed, micro(500_000));
        assert!(owed.amount.mantissa <= unpaid.cumulative_billed.amount.mantissa);
    }

    #[test]
    fn usage_chunk_content_is_deterministic_cbor_usage() {
        // tasks 3 pin: the kind-3 chunk content decodes to the Usage map.
        let (mut pp, _rp, _op) = agreed(micro(10_000_000), 5_000_000);
        let billed = pp.bill(usage(100, 40, 1000), 4, [1; 16], TS);
        let chunk = map_tasks::msg::ChunkMsg::from_value(&billed[0].1).unwrap();
        assert_eq!(chunk.kind, map_tasks::ChunkKind::UsageUpdate);
        let u = Usage::from_value(&decode(&chunk.content).unwrap()).unwrap();
        assert_eq!(u, usage(100, 40, 1000));
    }
}
