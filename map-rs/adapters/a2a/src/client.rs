//! A2A v1.0 JSON-RPC requester with the MAP extension (feature `http`).
//!
//! Drives one task against one provider over plain HTTP: opens it with the
//! `map/v2` session metadata (A2A-7), countersigns each checkpoint the
//! provider pauses on (A2A-21), countersigns the terminal receipt and returns
//! it on a `referenceTaskIds` Message. If the provider disappears mid-task the
//! caller gets back the `RequesterTask`, whose last countersigned checkpoint
//! is the resumption point for the next provider (A2A-23).

use crate::binding::{artifact_from_json, artifact_to_json, map_meta};
use crate::card::{verify_card, MAP_EXTENSION_URI};
use crate::flow::{BindingError, RequesterTask};
use crate::workload::{now_ms, observed_usage, prompt_words};
use map_core::Usage;
use map_identity::{AgentId, Keypair};
use serde_json::{json, Value};
use std::time::Duration;

/// A provider found through its signed Agent Card.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub base: String,
    pub rpc_url: String,
    pub agent: AgentId,
    pub card: Value,
}

#[derive(Debug)]
pub enum ClientError {
    /// Transport failure: connection refused/reset, timeout.
    Transport(String),
    /// The peer answered with a JSON-RPC error.
    Rpc(Value),
    /// The peer answered, but not with something this binding accepts.
    Protocol(String),
    /// A MAP check refused the provider's artifact.
    Binding(BindingError),
}

impl From<BindingError> for ClientError {
    fn from(e: BindingError) -> Self {
        ClientError::Binding(e)
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build()
}

/// Fetch and verify a provider's Agent Card (binding A2A-1..3, A2A-15): the
/// card must declare the MAP extension, and its JWS must verify under the
/// AgentID the extension names.
pub fn discover(base: &str) -> Result<Endpoint, ClientError> {
    let base = base.trim_end_matches('/').to_string();
    let url = format!("{base}/.well-known/agent-card.json");
    let body = agent().get(&url).call().map_err(|e| ClientError::Transport(e.to_string()))?;
    let card: Value = serde_json::from_str(&body.into_string().map_err(|e| ClientError::Transport(e.to_string()))?)
        .map_err(|e| ClientError::Protocol(format!("card is not JSON: {e}")))?;
    let agent_id = card["capabilities"]["extensions"]
        .as_array()
        .and_then(|xs| xs.iter().find(|x| x["uri"] == MAP_EXTENSION_URI))
        .and_then(|x| x["params"]["agentId"].as_str())
        .and_then(|s| AgentId::from_multibase(s).ok())
        .ok_or_else(|| ClientError::Protocol("card does not declare the MAP extension".into()))?;
    verify_card(&card, &agent_id).map_err(|e| ClientError::Protocol(format!("card signature: {e:?}")))?;
    let rpc_url = card["supportedInterfaces"]
        .as_array()
        .and_then(|xs| xs.iter().find(|x| x["protocolBinding"] == "JSONRPC"))
        .and_then(|x| x["url"].as_str())
        .ok_or_else(|| ClientError::Protocol("card has no JSONRPC interface".into()))?
        .to_string();
    Ok(Endpoint { base, rpc_url, agent: agent_id, card })
}

/// One JSON-RPC call. `map` activates the extension for this request.
pub fn rpc(url: &str, method: &str, params: Value, map: bool) -> Result<Value, ClientError> {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let mut req = agent().post(url).set("Content-Type", "application/json").set("A2A-Version", "1.0");
    if map {
        req = req.set("A2A-Extensions", MAP_EXTENSION_URI);
    }
    let resp = req.send_string(&body.to_string()).map_err(|e| ClientError::Transport(e.to_string()))?;
    let v: Value = serde_json::from_str(&resp.into_string().map_err(|e| ClientError::Transport(e.to_string()))?)
        .map_err(|e| ClientError::Protocol(format!("response is not JSON: {e}")))?;
    if let Some(err) = v.get("error") {
        return Err(ClientError::Rpc(err.clone()));
    }
    v.get("result").cloned().ok_or_else(|| ClientError::Protocol("no result".into()))
}

/// Where a finished or abandoned task stands.
pub struct Outcome {
    /// The A2A `Task.id` the provider allocated, once known.
    pub a2a_task_id: Option<String>,
    /// The requester's view of the task, once the TaskID is derivable.
    pub task: Option<RequesterTask>,
    /// Usage in the last checkpoint this requester countersigned (layer C-5:
    /// what the provider is owed if it stops now).
    pub acked: Option<Usage>,
    /// The dual-signed receipt, if the task completed.
    pub receipt: Option<Vec<u8>>,
    /// Why the task stopped short, if it did.
    pub error: Option<ClientError>,
}

/// The requester's voucher hook: given acknowledged cumulative usage, sign the
/// next cumulative voucher on the payment rail and return its digest (layer
/// §9.3). MAP never mints vouchers itself.
pub type Pay<'a> = &'a mut dyn FnMut(&AgentId, &Usage) -> Vec<u8>;

/// Resumption input (A2A-23): the task being resumed and its A2A id.
pub struct Resume<'a> {
    pub from: &'a RequesterTask,
    pub from_a2a_task_id: &'a str,
}

fn random_nonce() -> [u8; 16] {
    let mut n = [0u8; 16];
    getrandom::getrandom(&mut n).expect("OS randomness");
    n
}

fn message(parts: Vec<Value>, meta: Value) -> Value {
    json!({
        "messageId": format!("m-{}", now_ms()),
        "role": "ROLE_USER",
        "parts": parts,
        "extensions": [MAP_EXTENSION_URI],
        "metadata": {"map/v2": meta}
    })
}

/// Run one task on `ep` to completion (or until the provider fails).
pub fn run_task(
    ep: &Endpoint,
    kp: &Keypair,
    prompt: &str,
    capability: &str,
    zone: u8,
    resume: Option<Resume<'_>>,
    pay: Pay<'_>,
) -> Outcome {
    let mut out = Outcome { a2a_task_id: None, task: None, acked: None, receipt: None, error: None };
    if let Err(e) = drive(ep, kp, prompt, capability, zone, resume, pay, &mut out) {
        out.error = Some(e);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn drive(
    ep: &Endpoint,
    kp: &Keypair,
    prompt: &str,
    capability: &str,
    zone: u8,
    resume: Option<Resume<'_>>,
    pay: Pay<'_>,
    out: &mut Outcome,
) -> Result<(), ClientError> {
    let nonce = random_nonce();
    let pw = prompt_words(prompt);
    let started = now_ms();
    let mut meta = json!({"agentId": kp.agent_id().to_multibase(), "taskNonce": artifact_to_json(&nonce), "zone": zone});
    let mut parts = vec![json!({"text": prompt})];
    let mut msg_extra = json!({});
    if let Some(r) = &resume {
        let pkg = r.from.resume_package()?;
        meta["checkpoint"] = artifact_to_json(&pkg.checkpoint);
        parts.push(json!({"data": {"mapTranscriptPrefix": pkg.prefix_parts}}));
        msg_extra = json!({"referenceTaskIds": [r.from_a2a_task_id]});
    }
    let mut msg = message(parts, meta);
    if let Some(refs) = msg_extra.get("referenceTaskIds") {
        msg["referenceTaskIds"] = refs.clone();
    }
    let mut result = rpc(&ep.rpc_url, "SendMessage", json!({"message": msg}), true)?;

    let mut seen = 0usize;
    loop {
        let task = result.get("task").ok_or_else(|| ClientError::Protocol("SendMessage did not return a Task".into()))?;
        let a2a_id = task["id"].as_str().ok_or_else(|| ClientError::Protocol("Task without id".into()))?.to_string();
        if out.task.is_none() {
            // The provider allocates Task.id, so the TaskID (A2A-8) is
            // derivable only now.
            out.task = Some(match &resume {
                None => RequesterTask::start(kp.clone(), ep.agent.clone(), &nonce, &a2a_id, capability, zone),
                Some(r) => r.from.resume_on(ep.agent.clone(), &nonce, &a2a_id)?.0,
            });
            out.a2a_task_id = Some(a2a_id.clone());
        }
        let req = out.task.as_mut().expect("set above");
        let delivered = task["artifacts"][0]["parts"].as_array().cloned().unwrap_or_default();
        for p in &delivered[seen..] {
            req.on_part(p)?;
        }
        seen = delivered.len();
        let observed = observed_usage(pw, seen as u64);
        let state = task["status"]["state"].as_str().unwrap_or("");
        let carried = |k: &str| map_meta(task).and_then(|m| m.get(k)).and_then(artifact_from_json);
        match state {
            "TASK_STATE_INPUT_REQUIRED" => {
                let proposal = carried("checkpoint").ok_or_else(|| ClientError::Protocol("paused without a checkpoint".into()))?;
                let countersigned = req.on_checkpoint(&proposal, &observed)?;
                out.acked = Some(observed);
                let voucher = pay(&ep.agent, &observed);
                let mut next = message(
                    vec![json!({"text": "checkpoint countersigned"})],
                    json!({"checkpoint": artifact_to_json(&countersigned), "voucherDigest": artifact_to_json(&voucher)}),
                );
                next["taskId"] = json!(a2a_id);
                next["contextId"] = task["contextId"].clone();
                result = rpc(&ep.rpc_url, "SendMessage", json!({"message": next}), true)?;
            }
            "TASK_STATE_COMPLETED" => {
                let proposal = carried("receipt").ok_or_else(|| ClientError::Protocol("completed without a receipt".into()))?;
                let dual = req.on_receipt(&proposal, &observed, started, now_ms())?;
                out.acked = Some(observed);
                let voucher = pay(&ep.agent, &observed);
                let mut ack = message(
                    vec![json!({"text": "Countersigned receipt attached."})],
                    json!({"receipt": artifact_to_json(&dual), "voucherDigest": artifact_to_json(&voucher)}),
                );
                ack["referenceTaskIds"] = json!([a2a_id]);
                rpc(&ep.rpc_url, "SendMessage", json!({"message": ack}), true)?;
                out.receipt = Some(dual);
                return Ok(());
            }
            other => return Err(ClientError::Protocol(format!("unexpected task state {other}"))),
        }
    }
}
