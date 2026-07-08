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

fn mk(mt: u16, content: Value) -> Incoming {
    Incoming { msg_type: mt, content, msg_id: MsgId([0u8; 16]), seq: 0 }
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
            let cfg = SessionConfig { keypair: Keypair::from_seed([2u8; 32]), expected_peer: a_for_b, hello: hello() };
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

        let cfg = SessionConfig { keypair: Keypair::from_seed([1u8; 32]), expected_peer: b_id.clone(), hello: hello() };
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
    }

    #[test]
    fn receipt_for_unknown_task_not_countersigned() {
        // §11(c): provider's observation is task 9, receipt claims task 5.
        let mut a_store = ReceiptStore::new();
        let issued = requester_issue(&inputs_n(5), [7u8; 32], &ka(), &kb().agent_id(), TaskOutcome::Verified, None, &mut a_store).unwrap();
        let out = provider_countersign(&mk(msg_type::RECEIPT, issued[0].1.clone()), &inputs_n(9), &kb(), &mut ReceiptStore::new());
        assert_eq!(out[0].0, msg_type::ERROR);
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
                let cfg = SessionConfig { keypair: kb(), expected_peer: c_for_b, hello: hello() };
                let mut sess = Session::respond(cfg, tb, FixedClock { ms: TS }, CountingMsgIds::new()).unwrap();
                let inc = sess.recv().unwrap();
                for (mt, c) in serve_evidence(&inc, &b_store) {
                    sess.send(mt, c).unwrap();
                }
            });
            let cfg = SessionConfig { keypair: kc(), expected_peer: b_id.clone(), hello: hello() };
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
