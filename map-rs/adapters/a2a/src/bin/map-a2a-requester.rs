//! A MAP-over-A2A requester: runs one task across an ordered list of
//! providers, resuming on the next one from the last countersigned
//! checkpoint whenever a provider fails (binding A2A-23).
//!
//! ```text
//! map-a2a-requester --providers http://127.0.0.1:8701,http://127.0.0.1:8702
//!     [--seed 1] [--zone 2] [--prompt "..."] [--plain]
//! ```
//!
//! On success it prints one `MAP-DEMO-RESULT {json}` line and exits 0. The
//! checks: the final receipt is dual-signed and names the checkpoint it
//! resumed from, its transcript root is the root over every Part received
//! from every provider, and the per-provider voucher watermarks sum to
//! exactly the Parts delivered — nobody is paid for unacknowledged work.
//! `--plain` instead talks to the first provider as an ordinary A2A client
//! (no extension) and checks that it gets a completed task with no MAP stalls
//! (A2A-28).

use map_a2a::client::{discover, rpc, run_task, Resume};
use map_a2a::flow::RequesterTask;
use map_core::Usage;
use map_identity::{AgentId, Keypair};
use map_meter::voucher::{digest_mpp_tempo, erc20};
use map_meter::{billed, AssetAmount, Amount, UNIT_TOKEN_IN, UNIT_TOKEN_OUT};
use map_trust::Receipt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const CAPABILITY: &str = "map:cap:inference/text-generation";

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn fail(msg: &str) -> ! {
    eprintln!("[requester] FAIL: {msg}");
    std::process::exit(1);
}

fn plain(base: &str, prompt: &str) {
    let ep = discover(base).unwrap_or_else(|e| fail(&format!("discover {base}: {e:?}")));
    let msg = json!({"messageId": "plain-1", "role": "ROLE_USER", "parts": [{"text": prompt}]});
    let r = rpc(&ep.rpc_url, "SendMessage", json!({"message": msg}), false).unwrap_or_else(|e| fail(&format!("{e:?}")));
    let state = r["task"]["status"]["state"].as_str().unwrap_or("");
    let parts = r["task"]["artifacts"][0]["parts"].as_array().map(Vec::len).unwrap_or(0);
    if state != "TASK_STATE_COMPLETED" || r["task"].get("metadata").is_some() {
        fail(&format!("plain client got state {state}, metadata {:?}", r["task"].get("metadata")));
    }
    println!("MAP-DEMO-RESULT {}", json!({"mode": "plain", "state": state, "parts": parts}));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let providers: Vec<String> = arg(&args, "--providers")
        .unwrap_or_else(|| "http://127.0.0.1:8701,http://127.0.0.1:8702".into())
        .split(',')
        .map(str::to_string)
        .collect();
    let seed: u8 = arg(&args, "--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let zone: u8 = arg(&args, "--zone").and_then(|s| s.parse().ok()).unwrap_or(2);
    let prompt = arg(&args, "--prompt").unwrap_or_else(|| "Write a short status report".into());
    if args.iter().any(|a| a == "--plain") {
        return plain(&providers[0], &prompt);
    }
    let kp = Keypair::from_seed([seed; 32]);

    // The payment rail stand-in: one MPP-Tempo-style session per provider,
    // priced per token in a 6-decimal stablecoin (layer §9.2-§9.3).
    let asset = erc20("eip155:8453", "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913");
    let rates = vec![
        (UNIT_TOKEN_IN, AssetAmount { asset: asset.clone(), amount: Amount::new(-6, 2) }),
        (UNIT_TOKEN_OUT, AssetAmount { asset: asset.clone(), amount: Amount::new(-6, 10) }),
    ];
    let mut ledger: BTreeMap<String, (Usage, AssetAmount)> = BTreeMap::new();
    let mut pay = |provider: &AgentId, usage: &Usage| -> Vec<u8> {
        let amount = billed(1, &rates, usage).expect("rate table prices model 1");
        let channel = Sha256::digest(provider.as_bytes());
        let digest = digest_mpp_tempo(&channel, &amount.amount, "map-demo", "tempo");
        ledger.insert(provider.to_multibase(), (*usage, amount));
        digest
    };

    let mut resume_from: Option<(RequesterTask, String)> = None;
    let mut finished = None;
    let mut hops = Vec::new();
    for base in &providers {
        let ep = match discover(base) {
            Ok(ep) => ep,
            Err(e) => {
                eprintln!("[requester] skip {base}: {e:?}");
                continue;
            }
        };
        eprintln!("[requester] task on {base} ({})", ep.agent.to_multibase());
        let resume = resume_from.as_ref().map(|(t, id)| Resume { from: t, from_a2a_task_id: id });
        let out = run_task(&ep, &kp, &prompt, CAPABILITY, zone, resume, &mut pay);
        hops.push(json!({"provider": base, "a2aTaskId": out.a2a_task_id, "acked": out.acked.map(|u| u.tokens_out),
                         "error": out.error.as_ref().map(|e| format!("{e:?}"))}));
        if let Some(dual) = out.receipt {
            finished = Some((dual, out.task.expect("a completed task has requester state")));
            break;
        }
        eprintln!("[requester] {base} stopped: {:?}", out.error);
        // Resume from this provider's last countersigned checkpoint if it
        // got one; otherwise from the previous resumption point.
        if let (Some(t), Some(id)) = (out.task, out.a2a_task_id) {
            if t.resume_package().is_ok() {
                resume_from = Some((t, id));
            }
        }
    }
    drop(pay);
    let Some((dual, task)) = finished else { fail("no provider completed the task") };

    let v = Receipt::decode_bytes(&dual)
        .and_then(|r| r.verify())
        .unwrap_or_else(|e| fail(&format!("receipt does not verify: {e:?}")));
    if !v.dual {
        fail("receipt is not dual-signed");
    }
    if v.core.transcript_root != Some(task.transcript_root()) {
        fail("receipt transcript root differs from the Parts received");
    }
    let resumed = resume_from.is_some();
    if resumed != v.core.resumed_from.is_some() {
        fail("receipt resumed-from does not match what happened");
    }
    let delivered = task.parts_received() as u64;
    let paid_tokens: u64 = ledger.values().map(|(u, _)| u.tokens_out).sum();
    let paid_atomic: u64 = ledger.values().map(|(_, a)| a.amount.mantissa).sum();
    if paid_tokens != delivered {
        fail(&format!("voucher watermarks sum to {paid_tokens} Parts but {delivered} were delivered"));
    }
    let vouchers: Vec<Value> = ledger
        .iter()
        .map(|(p, (u, a))| json!({"provider": p, "tokensOut": u.tokens_out, "cumulativeAtomic": a.amount.mantissa}))
        .collect();
    println!(
        "MAP-DEMO-RESULT {}",
        json!({
            "mode": "map",
            "dualSigned": v.dual,
            "resumedFrom": v.core.resumed_from.as_deref().map(hex),
            "voucherDigestInReceipt": v.core.payment_ref.as_deref().map(hex),
            "transcriptRoot": hex(&task.transcript_root()),
            "partsDelivered": delivered,
            "vouchers": vouchers,
            "voucherTotalAtomic": paid_atomic,
            "hops": hops
        })
    );
}
