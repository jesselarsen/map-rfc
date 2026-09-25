//! A2A v1.0 JSON-RPC provider with the MAP extension (feature `http`).
//!
//! Serves `GET /.well-known/agent-card.json` and JSON-RPC `SendMessage` /
//! `GetTask` on `POST /a2a`. Checkpoints use the binding's mandatory
//! request/response path (A2A-21): the task pauses in
//! `TASK_STATE_INPUT_REQUIRED` with a checkpoint proposal in
//! `Task.metadata."map/v2".checkpoint`, and the requester's next
//! `SendMessage` on that task carries the countersignature. A client that
//! does not send `A2A-Extensions: <MAP URI>` gets a plain A2A task that runs
//! to completion with no MAP metadata and no pauses (A2A-28).

use crate::binding::{artifact_from_json, artifact_to_json, codes, error_json, map_meta, set_map_meta};
use crate::card::MAP_EXTENSION_URI;
use crate::flow::{ProviderConfig, ProviderTask};
use crate::workload::{now_ms, observed_usage, prompt_words};
use map_identity::{AgentId, Keypair};
use map_trust::Outcome;
use serde_json::{json, Value};
use std::collections::HashMap;

/// The task's output generator: prompt -> output words, deterministic. A
/// successor provider regenerates and skips what was already delivered
/// (resumption by replay, layer §7.4).
pub type Workload = Box<dyn Fn(&str) -> Vec<String> + Send>;

pub struct ServerConfig {
    pub keypair: Keypair,
    /// The signed Agent Card served at the well-known path.
    pub card: Value,
    pub capability: String,
    /// Zone labels this provider accepts tasks under (card `supportedZones`, descriptor key 5).
    pub supported_zones: Vec<u8>,
    pub checkpoint_every: u64,
    /// Simulate a crash: exit the process once this many Parts were produced.
    pub fail_after_parts: Option<u64>,
    pub workload: Workload,
}

struct TaskState {
    id: String,
    context_id: String,
    words: Vec<String>,
    /// Index into `words` of the next word to emit.
    cursor: usize,
    parts: Vec<Value>,
    state: &'static str,
    map: Option<MapState>,
    started_ms: u64,
    /// The MAP artifact riding on the current response: ("checkpoint" |
    /// "receipt", bytes).
    carried: Option<(&'static str, Vec<u8>)>,
}

struct MapState {
    task: ProviderTask,
    prompt_words: u64,
    /// Parts this provider itself delivered (a successor bills its own work).
    own_parts: u64,
    receipt_proposal: Option<Vec<u8>>,
}

pub struct Provider {
    cfg: ServerConfig,
    tasks: HashMap<String, TaskState>,
    produced: u64,
    next_id: u64,
}

fn rpc_ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_err(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// A binding error (A2A-26) surfaced through JSON-RPC's own error object
/// (A2A-27): invalid params, with the MAP code in `data."map/v2".error`.
fn map_err(id: &Value, code: u64, detail: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {
        "code": -32602, "message": detail, "data": {"map/v2": {"error": error_json(code, detail)}}}})
}

impl Provider {
    pub fn new(cfg: ServerConfig) -> Self {
        Provider { cfg, tasks: HashMap::new(), produced: 0, next_id: 1 }
    }

    /// Handle one JSON-RPC request body. `map_active` reflects the
    /// `A2A-Extensions` request header. Returns the response body.
    pub fn handle_rpc(&mut self, body: &Value, map_active: bool) -> Value {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        match body.get("method").and_then(Value::as_str) {
            Some("SendMessage") => match body.get("params").and_then(|p| p.get("message")) {
                Some(msg) => self.send_message(&id, msg, map_active),
                None => rpc_err(&id, -32602, "params.message is required"),
            },
            Some("GetTask") => {
                let tid = body.get("params").and_then(|p| p.get("id")).and_then(Value::as_str).unwrap_or("");
                match self.tasks.get(tid) {
                    Some(t) => rpc_ok(&id, task_json(t)),
                    None => rpc_err(&id, -32001, "task not found"),
                }
            }
            Some(_) => rpc_err(&id, -32601, "method not found"),
            None => rpc_err(&id, -32600, "invalid request"),
        }
    }

    fn send_message(&mut self, id: &Value, msg: &Value, map_active: bool) -> Value {
        // 1. A countersigned receipt for a finished task (A2A-7): a Message
        //    that references the task instead of reopening it.
        if msg.get("taskId").is_none() {
            if let Some(receipt) = map_meta(msg).and_then(|m| m.get("receipt")).and_then(artifact_from_json) {
                let tid = msg
                    .get("referenceTaskIds")
                    .and_then(Value::as_array)
                    .and_then(|r| r.first())
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return match self.accept_receipt(tid, &receipt) {
                    Ok(()) => rpc_ok(id, json!({"message": {
                        "messageId": format!("ack-{tid}"),
                        "role": "ROLE_AGENT",
                        "parts": [{"text": "receipt countersignature accepted"}]}})),
                    Err(e) => map_err(id, codes::COUNTERSIGNATURE_REFUSED, &e),
                };
            }
        }
        // 2. Continuation of a paused task, carrying a countersigned checkpoint.
        if let Some(tid) = msg.get("taskId").and_then(Value::as_str) {
            let tid = tid.to_string();
            let Some(t) = self.tasks.get_mut(&tid) else {
                return rpc_err(id, -32001, "task not found");
            };
            if let Some(cs) = map_meta(msg).and_then(|m| m.get("checkpoint")).and_then(artifact_from_json) {
                let Some(m) = t.map.as_mut() else {
                    return rpc_err(id, -32602, "task is not running the MAP extension");
                };
                if let Err(e) = m.task.on_countersigned(&cs) {
                    return map_err(id, codes::BAD_CHECKPOINT, &format!("{e:?}"));
                }
                // The requester's latest cumulative voucher, bound into the
                // next checkpoint and the receipt (layer §9.3).
                if let Some(d) = map_meta(msg).and_then(|m| m.get("voucherDigest")).and_then(artifact_from_json) {
                    m.task.set_voucher_digest(d);
                }
            }
            return self.run(id, &tid);
        }
        // 3. A new task, possibly resuming another provider's task.
        self.new_task(id, msg, map_active)
    }

    fn new_task(&mut self, id: &Value, msg: &Value, map_active: bool) -> Value {
        let prompt: String = msg
            .get("parts")
            .and_then(Value::as_array)
            .and_then(|ps| ps.iter().find_map(|p| p.get("text").and_then(Value::as_str)))
            .unwrap_or("")
            .to_string();
        let tid = format!("task-{}", self.next_id);
        self.next_id += 1;
        let mut state = TaskState {
            id: tid.clone(),
            context_id: msg.get("contextId").and_then(Value::as_str).unwrap_or("ctx-1").to_string(),
            words: (self.cfg.workload)(&prompt),
            cursor: 0,
            parts: Vec::new(),
            state: "TASK_STATE_WORKING",
            map: None,
            started_ms: now_ms(),
            carried: None,
        };
        if map_active {
            match self.start_map(&tid, msg) {
                Ok((task, skip)) => {
                    state.cursor = skip;
                    state.map = Some(MapState {
                        task,
                        prompt_words: prompt_words(&prompt),
                        own_parts: 0,
                        receipt_proposal: None,
                    });
                }
                Err((code, detail)) => return map_err(id, code, &detail),
            }
        }
        self.tasks.insert(tid.clone(), state);
        self.run(id, &tid)
    }

    /// Set up MAP state from the first Message's `map/v2` metadata: a fresh
    /// task, or a resumption carrying a countersigned checkpoint and the
    /// transcript prefix as a DataPart (A2A-23).
    fn start_map(&self, tid: &str, msg: &Value) -> Result<(ProviderTask, usize), (u64, String)> {
        let bad = |d: &str| (codes::BAD_CHECKPOINT, d.to_string());
        let m = map_meta(msg).ok_or_else(|| bad("map/v2 metadata missing on the opening Message"))?;
        let nonce: [u8; 16] = m
            .get("taskNonce")
            .and_then(artifact_from_json)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| bad("taskNonce missing or not 16 bytes"))?;
        let requester = m
            .get("agentId")
            .and_then(Value::as_str)
            .and_then(|s| AgentId::from_multibase(s).ok())
            .ok_or_else(|| bad("agentId missing or invalid"))?;
        let zone = m.get("zone").and_then(Value::as_u64).ok_or_else(|| bad("zone missing"))?;
        if !self.cfg.supported_zones.iter().any(|z| *z as u64 == zone) {
            return Err((codes::ZONE_VIOLATION, format!("this provider accepts zones {:?}", self.cfg.supported_zones)));
        }
        let pcfg = ProviderConfig { capability: self.cfg.capability.clone(), zone: zone as u8, checkpoint_every: self.cfg.checkpoint_every };
        let kp = self.cfg.keypair.clone();
        let Some(cp) = m.get("checkpoint").and_then(artifact_from_json) else {
            return Ok((ProviderTask::start(kp, requester, &nonce, tid, pcfg), 0));
        };
        let prefix: Vec<Value> = msg
            .get("parts")
            .and_then(Value::as_array)
            .and_then(|ps| ps.iter().find_map(|p| p.get("data")?.get("mapTranscriptPrefix")?.as_array().cloned()))
            .ok_or_else(|| bad("resumption without a mapTranscriptPrefix DataPart"))?;
        let t = ProviderTask::resume(kp, requester, &nonce, tid, pcfg, &cp, &prefix).map_err(|e| {
            use crate::flow::BindingError as B;
            use map_transcript::CheckpointError as C;
            match e {
                B::Checkpoint(C::RootMismatch) => (codes::ROOT_MISMATCH, "prefix does not match the countersigned root".into()),
                B::ZoneWidened => (codes::ZONE_VIOLATION, "resumption would widen the checkpoint's zone".into()),
                other => (codes::BAD_CHECKPOINT, format!("{other:?}")),
            }
        })?;
        let skip = t.next_seq() as usize;
        Ok((t, skip))
    }

    /// Emit Parts until the next checkpoint proposal (MAP) or completion.
    fn run(&mut self, id: &Value, tid: &str) -> Value {
        let fail_after = self.cfg.fail_after_parts;
        let t = self.tasks.get_mut(tid).expect("task exists");
        t.carried = None;
        t.state = "TASK_STATE_WORKING";
        while t.cursor < t.words.len() {
            let part = json!({"text": format!("{} ", t.words[t.cursor]), "mediaType": "text/plain"});
            t.cursor += 1;
            t.parts.push(part.clone());
            self.produced += 1;
            let proposal = match t.map.as_mut() {
                None => None,
                Some(m) => {
                    m.own_parts += 1;
                    match m.task.deliver(&part, observed_usage(m.prompt_words, m.own_parts), now_ms()) {
                        Ok(p) => p,
                        Err(e) => return rpc_err(id, -32603, &format!("{e:?}")),
                    }
                }
            };
            if fail_after.is_some_and(|n| self.produced >= n) {
                eprintln!("[provider] simulated crash after producing {} parts", self.produced);
                std::process::exit(3);
            }
            if let Some(p) = proposal {
                t.state = "TASK_STATE_INPUT_REQUIRED";
                t.carried = Some(("checkpoint", p));
                return rpc_ok(id, json!({"task": task_json(t)}));
            }
        }
        t.state = "TASK_STATE_COMPLETED";
        if let Some(m) = t.map.as_mut() {
            let usage = observed_usage(m.prompt_words, m.own_parts);
            match m.task.propose_receipt(Outcome::Ok, usage, t.started_ms, now_ms()) {
                Ok(p) => {
                    m.receipt_proposal = Some(p.clone());
                    t.carried = Some(("receipt", p));
                }
                Err(e) => return rpc_err(id, -32603, &format!("{e:?}")),
            }
        }
        rpc_ok(id, json!({"task": task_json(t)}))
    }

    fn accept_receipt(&mut self, tid: &str, countersigned: &[u8]) -> Result<(), String> {
        let t = self.tasks.get(tid).ok_or("task not found")?;
        let m = t.map.as_ref().ok_or("task is not running the MAP extension")?;
        let proposal = m.receipt_proposal.as_ref().ok_or("no receipt proposed yet")?;
        m.task.on_countersigned_receipt(proposal, countersigned).map_err(|e| format!("{e:?}"))?;
        eprintln!("[provider] dual-signed receipt stored for {tid}");
        Ok(())
    }

    /// Serve until the process exits, on `addr` (e.g. "127.0.0.1:8701").
    pub fn serve(mut self, addr: &str) -> std::io::Result<()> {
        let server = tiny_http::Server::http(addr).map_err(|e| std::io::Error::other(e.to_string()))?;
        eprintln!("[provider] {} listening on http://{addr}", self.cfg.keypair.agent_id().to_multibase());
        let json_ct: tiny_http::Header = "Content-Type: application/json".parse().expect("static header");
        let ext_hdr: tiny_http::Header = format!("A2A-Extensions: {MAP_EXTENSION_URI}").parse().expect("static header");
        for mut req in server.incoming_requests() {
            let map_active = req.headers().iter().any(|h| {
                h.field.equiv("A2A-Extensions") && h.value.as_str().split(',').any(|u| u.trim() == MAP_EXTENSION_URI)
            });
            let (status, body) = match (req.method(), req.url()) {
                (tiny_http::Method::Get, "/.well-known/agent-card.json") => (200, self.cfg.card.clone()),
                (tiny_http::Method::Post, "/a2a") => {
                    let mut raw = String::new();
                    match req.as_reader().read_to_string(&mut raw).ok().and_then(|_| serde_json::from_str::<Value>(&raw).ok()) {
                        Some(v) => (200, self.handle_rpc(&v, map_active)),
                        None => (200, rpc_err(&Value::Null, -32700, "parse error")),
                    }
                }
                _ => (404, json!({"error": "not found"})),
            };
            let mut resp = tiny_http::Response::from_string(body.to_string()).with_status_code(status).with_header(json_ct.clone());
            if map_active {
                // Echo the activated extension (A2A extensions topic).
                resp = resp.with_header(ext_hdr.clone());
            }
            let _ = req.respond(resp);
        }
        Ok(())
    }
}

fn task_json(t: &TaskState) -> Value {
    let mut task = json!({
        "id": t.id,
        "contextId": t.context_id,
        "status": {"state": t.state},
        "artifacts": [{"artifactId": "output", "name": "output", "parts": t.parts}]
    });
    if let Some((field, bytes)) = &t.carried {
        set_map_meta(&mut task, field, artifact_to_json(bytes));
    }
    task
}
