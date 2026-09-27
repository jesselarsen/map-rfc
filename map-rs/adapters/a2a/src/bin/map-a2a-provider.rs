//! A MAP-over-A2A provider: serves a signed Agent Card and A2A JSON-RPC.
//!
//! ```text
//! map-a2a-provider --listen 127.0.0.1:8701 --seed 2 [--parts 24]
//!     [--checkpoint-every 8] [--zones 1,2] [--fail-after-parts N]
//! ```
//!
//! The workload is the deterministic generator in `map_a2a::workload`, so a
//! successor provider reproduces the same output (resumption by replay).

use map_a2a::card::{build_card, sign_card, CardParams};
use map_a2a::server::{Provider, ServerConfig};
use map_a2a::workload::generate;
use map_identity::Keypair;

const CAPABILITY: &str = "map:cap:inference/text-generation";

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let listen = arg(&args, "--listen").unwrap_or_else(|| "127.0.0.1:8701".into());
    let seed: u8 = arg(&args, "--seed").and_then(|s| s.parse().ok()).unwrap_or(2);
    let parts: usize = arg(&args, "--parts").and_then(|s| s.parse().ok()).unwrap_or(24);
    let every: u64 = arg(&args, "--checkpoint-every").and_then(|s| s.parse().ok()).unwrap_or(8); // layer C-6
    let zones: Vec<u8> = arg(&args, "--zones")
        .unwrap_or_else(|| "1,2".into())
        .split(',')
        .filter_map(|z| z.trim().parse().ok())
        .collect();
    let fail_after_parts = arg(&args, "--fail-after-parts").and_then(|s| s.parse().ok());

    let kp = Keypair::from_seed([seed; 32]);
    let params = CardParams {
        name: format!("MAP demo provider {seed}"),
        description: "Deterministic text generator with MAP accountability".into(),
        version: "0.1.0".into(),
        jsonrpc_url: format!("http://{listen}/a2a"),
        skills: vec![(
            "text-generation".into(),
            "Text generation".into(),
            "Generates a deterministic word stream from a prompt".into(),
            vec!["text".into(), "demo".into()],
        )],
        supported_zones: zones.clone(),
        checkpoint_interval_parts: every,
        accepted_voucher_rails: vec!["pay:mpp-tempo".into()],
    };
    let unsigned = build_card(&kp.agent_id(), &params);
    let (card, _) = sign_card(&unsigned, &kp).expect("card signs");

    let cfg = ServerConfig {
        keypair: kp,
        card,
        capability: CAPABILITY.into(),
        supported_zones: zones,
        checkpoint_every: every,
        fail_after_parts,
        workload: Box::new(move |prompt| generate(prompt, parts)),
    };
    if let Err(e) = Provider::new(cfg).serve(&listen) {
        eprintln!("[provider] {e}");
        std::process::exit(1);
    }
}
