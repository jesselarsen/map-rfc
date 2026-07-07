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
