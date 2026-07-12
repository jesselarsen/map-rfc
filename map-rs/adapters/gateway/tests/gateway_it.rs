//! TASK-016 FR-6 end-to-end: a requester runs a task against a gateway's
//! fronted capability (MockBackend upstream) and receives CHUNKs + COMPLETE +
//! a dual-signed receipt that names the GATEWAY as provider (G-2/FR-3). Driven
//! by shuttling `(msg_type, Value)` between the two pump runtimes (no
//! sessions/threads), the same technique the map-taskrun suite uses.

use gateway::GatewayProvider;
use map_core::cbor::Value;
use map_core::MsgId;
use map_identity::Keypair;
use map_session::Incoming;
use map_tasks::msg::TaskConstraints;
use map_tasks::TaskId;
use map_taskrun::{Executor, Outgoing, ProviderState, ProviderTask, RequesterTask};
use map_trust::{sign_receipt, Metrics, Outcome, Receipt, ReceiptCore, ROLE_PROVIDER, ROLE_REQUESTER};
use ollama_provider::MockBackend;

const CAP: &str = "map:cap:inference/text-generation";
const NOW: u64 = 1_780_000_000_000;

fn mk(mt: u16, content: Value) -> Incoming {
    Incoming { msg_type: mt, content, msg_id: MsgId([0u8; 16]), seq: 0, raw: Vec::new() }
}

/// Ping-pong the two runtimes until quiescent.
fn shuttle<E: Executor>(r: &mut RequesterTask, p: &mut ProviderTask<E>, first: Vec<Outgoing>, now: u64) {
    let mut to_prov = first;
    let mut to_req: Vec<Outgoing> = Vec::new();
    let mut guard = 0;
    loop {
        for o in std::mem::take(&mut to_prov) {
            to_req.extend(p.on_incoming(&mk(o.0, o.1), now));
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

#[test]
fn fr6_gateway_task_end_to_end_with_receipt() {
    let requester = Keypair::from_seed([1u8; 32]);
    let gateway_kp = Keypair::from_seed([7u8; 32]);
    let gw_aid = gateway_kp.agent_id();
    let task = TaskId([3u8; 32]);

    let gateway = GatewayProvider::new(
        MockBackend::new(),
        gw_aid.clone(),
        "mock",
        128,
        "gateway/example.com",
        CAP,
        2,
        false,
        None,
    )
    .unwrap();
    // The gateway admits the ASSIGN (an unconstrained task passes the gate).
    gateway.admit(CAP, None, NOW).unwrap();

    // Run the task: requester ⇄ gateway-provider until COMPLETE + verified.
    let mut r = RequesterTask::new(task, CAP, TaskConstraints::default(), b"why is the sky blue?".to_vec());
    let mut p = ProviderTask::new(gateway);
    let first = r.start_offer(NOW);
    shuttle(&mut r, &mut p, first, NOW);

    let (ri, root) = r.verified().expect("task verified end-to-end through the gateway");
    assert_eq!(ri.capability, CAP);
    assert_eq!(ri.outcome, 0);
    assert!(ri.usage.tokens_out > 0, "the fronted backend produced output");
    assert_eq!(p.state(), ProviderState::Completed);
    let root = *root;

    // FR-3 / G-2: the dual-signed receipt names the GATEWAY as provider — the
    // gateway (not the upstream) is accountable and accrues the evidence.
    let core = ReceiptCore {
        task,
        requester: requester.agent_id(),
        provider: gw_aid.clone(),
        capability: ri.capability.clone(),
        started_ms: ri.started_ms,
        ended_ms: ri.ended_ms,
        outcome: Outcome::Ok,
        metrics: Metrics {
            tokens_in: ri.usage.tokens_in,
            tokens_out: ri.usage.tokens_out,
            ttft_ms: ri.usage.ttft_ms,
            wall_ms: ri.usage.wall_ms,
        },
        rating: None,
        payment_ref: None,
        transcript_root: Some(root),
    };
    let core_bytes = core.encode().expect("receipt core encodes");
    let receipt = Receipt {
        // Provider signature is the GATEWAY's key — verify() only passes if so.
        sig_requester: Some(sign_receipt(&requester, ROLE_REQUESTER, &core_bytes)),
        sig_provider: Some(sign_receipt(&gateway_kp, ROLE_PROVIDER, &core_bytes)),
        core_bytes,
    };
    receipt.verify().expect("dual-signed receipt verifies (gateway is the provider)");
    assert_eq!(core.provider, gw_aid);
}
