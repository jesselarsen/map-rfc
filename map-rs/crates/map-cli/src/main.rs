//! `map-cli` — MAP protocol command-line tooling (TASK-001).
//!
//! Two subcommands, both pure and reproducible:
//! * `keygen` — derive/generate an Ed25519 identity and print its seed,
//!   pubkey, AgentID (hex + multibase), and did:key (draft-map-identity-00 §3).
//! * `projection-rebuild-demo` — synthesize dual-signed receipts, fold a
//!   `TrustProjection` incrementally, then rebuild it from the store and assert
//!   the DDIA "derived state is rebuildable" property (ops §8, trust §7).
//!
//! No async, no network, no `unsafe`. Argument parsing is hand-rolled over
//! `std::env::args_os` (no clap). The demo uses only fixed seeds and counters —
//! never wall-clock time or OS randomness — so its output is reproducible.

mod interop;

use map_identity::Keypair;
use map_tasks::TaskId;
use map_trust::{
    sign_receipt, Ingest, Metrics, Outcome, ProjParams, Receipt, ReceiptCore, ReceiptStore,
    TrustProjection, ROLE_PROVIDER, ROLE_REQUESTER,
};
use sha2::{Digest, Sha256};
use std::process::exit;

/// Capability all synthetic receipts share (one projection cell).
const CAP: &str = "map:cap:inference/text-generation";
/// Fixed synthesis epoch (ms). Deterministic — never `SystemTime::now`.
const BASE_STARTED_MS: u64 = 1_780_000_000_000;
/// One day in milliseconds (recency-decay horizon for the demo `now`).
const ONE_DAY_MS: u64 = 86_400_000;

/// A CLI failure and the exit code it maps to: `Usage` → 2 (bad invocation),
/// `Failure` → 1 (demo mismatch / verification failure). Both print
/// `error: <msg>` to stderr (§10).
#[derive(Debug)]
enum CliError {
    Usage(String),
    Failure(String),
}

fn main() {
    // args_os + fallible conversion: `std::env::args()` panics on non-UTF8,
    // but §9 requires a graceful usage/exit-2 instead of a panic.
    let args: Vec<String> = match std::env::args_os().map(|a| a.into_string()).collect() {
        Ok(v) => v,
        Err(_) => {
            eprint!("{}", usage());
            exit(2);
        }
    };

    // `args.get(1)` — never index; a missing subcommand must not panic (§9).
    let result = match args.get(1).map(String::as_str) {
        Some("keygen") => cmd_keygen(&args),
        Some("projection-rebuild-demo") => cmd_projection_demo(&args),
        // The interop peer prints its own OK/FAIL line and exits directly.
        Some("interop") => interop::run(&args),
        #[cfg(feature = "real-backend")]
        Some("serve-ollama") => cmd_serve_ollama(&args),
        _ => {
            eprint!("{}", usage());
            exit(2);
        }
    };

    exit(match result {
        Ok(()) => 0,
        Err(CliError::Usage(m)) => {
            eprintln!("error: {m}");
            2
        }
        Err(CliError::Failure(m)) => {
            eprintln!("error: {m}");
            1
        }
    });
}

fn usage() -> String {
    "usage: map-cli <keygen [--seed <64-hex>] | projection-rebuild-demo [--receipts <N>]>\n"
        .to_string()
}

/// `--flag value` and `--flag=value` are both accepted; returns the first match.
fn parse_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().map(String::as_str);
        }
        if let Some(rest) = a.strip_prefix(name) {
            if let Some(v) = rest.strip_prefix('=') {
                return Some(v);
            }
        }
    }
    None
}

fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

// ---------------------------------------------------------------------
// keygen
// ---------------------------------------------------------------------

/// A 32-byte seed decoded from case-insensitive hex; exactly 64 hex chars.
fn parse_seed(hexstr: &str) -> Result<[u8; 32], CliError> {
    let bad = || CliError::Usage("--seed must be 64 hex chars".into());
    let bytes = hex::decode(hexstr).map_err(|_| bad())?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| bad())
}

/// The five identity fields keygen prints (lowercase hex / multibase).
struct KeyReport {
    seed: String,
    pubkey: String,
    agent_id: String,
    multibase: String,
    did_key: String,
}

impl KeyReport {
    /// Exactly the §7 format: one field per line, values aligned at column 11.
    fn render(&self) -> String {
        format!(
            "seed:      {}\npubkey:    {}\nagent-id:  {}\nmultibase: {}\ndid:key:   {}\n",
            self.seed, self.pubkey, self.agent_id, self.multibase, self.did_key
        )
    }
}

/// Pure: seed → report. AgentID is `ed01 || pubkey` (34 bytes → 68 hex).
fn build_key_report(seed: [u8; 32]) -> KeyReport {
    let kp = Keypair::from_seed(seed);
    let id = kp.agent_id();
    let id_bytes = id.as_bytes();
    KeyReport {
        seed: hex::encode(seed),
        pubkey: hex::encode(&id_bytes[2..]),
        agent_id: hex::encode(id_bytes),
        multibase: id.to_multibase(),
        did_key: id.did_key(),
    }
}

fn cmd_keygen(args: &[String]) -> Result<(), CliError> {
    let seed = match parse_flag(args, "--seed") {
        Some(hexstr) => parse_seed(hexstr)?,
        None => {
            // §5: no --seed → a fresh random identity. `Keypair::generate()`
            // hides its seed, so we draw the seed here and print it (keygen
            // intentionally reveals private seeds — test tooling, §11).
            let mut s = [0u8; 32];
            getrandom::getrandom(&mut s)
                .map_err(|_| CliError::Failure("OS RNG unavailable".into()))?;
            s
        }
    };
    print!("{}", build_key_report(seed).render());
    eprintln!("WARNING: test tooling — do not use for production identities.");
    Ok(())
}

// ---------------------------------------------------------------------
// projection-rebuild-demo
// ---------------------------------------------------------------------

/// Fixed requester (A) / provider (B) identities, from labelled seeds.
fn demo_keypairs() -> (Keypair, Keypair) {
    (
        Keypair::from_seed(sha256(b"MAP-CLI-DEMO-A")),
        Keypair::from_seed(sha256(b"MAP-CLI-DEMO-B")),
    )
}

/// N deterministic dual-signed receipts (§5). Requester=A, provider=B, one
/// capability ⇒ a single projection cell. Same inputs ⇒ identical bytes.
fn synth_receipts(a: &Keypair, b: &Keypair, n: u64) -> Vec<Receipt> {
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let started_ms = BASE_STARTED_MS + i * 1000;
        let outcome = match i % 5 {
            // {0,0,0,1,2} — mostly ok, one Failed, one Timeout.
            0 | 1 | 2 => Outcome::Ok,
            3 => Outcome::Failed,
            _ => Outcome::Timeout,
        };
        let core = ReceiptCore {
            task: TaskId(sha256(&(i).to_be_bytes())), // i is u64 → 8-byte input
            requester: a.agent_id(),
            provider: b.agent_id(),
            capability: CAP.to_string(),
            started_ms,
            ended_ms: started_ms + 42_000,
            outcome,
            metrics: Metrics {
                tokens_in: 100,
                tokens_out: 50,
                ttft_ms: 200,
                wall_ms: 42_000,
            },
            rating: Some((50 + (i * 7) % 50) as u8),
            payment_ref: None,
            transcript_root: None,
        };
        let core_bytes = core.encode().expect("synthetic receipt core encodes");
        out.push(Receipt {
            sig_requester: Some(sign_receipt(a, ROLE_REQUESTER, &core_bytes)),
            sig_provider: Some(sign_receipt(b, ROLE_PROVIDER, &core_bytes)),
            core_bytes,
        });
    }
    out
}

/// Evaluation `now`: one day after the newest receipt ended (§8).
fn demo_now_ms(n: u64) -> u64 {
    let max_ended = if n == 0 {
        BASE_STARTED_MS
    } else {
        BASE_STARTED_MS + (n - 1) * 1000 + 42_000
    };
    max_ended + ONE_DAY_MS
}

fn cmd_projection_demo(args: &[String]) -> Result<(), CliError> {
    let n: u64 = match parse_flag(args, "--receipts") {
        None => 50,
        Some(s) => s
            .parse()
            .map_err(|_| CliError::Usage("--receipts must be a non-negative integer".into()))?,
    };

    let (a, b) = demo_keypairs();
    let receipts = synth_receipts(&a, &b, n);
    let now_ms = demo_now_ms(n);
    let params = ProjParams::default();

    let mut store = ReceiptStore::new();
    let mut incremental = TrustProjection::new(params);

    // Ingest, applying only the receipts the store actually accepts. This
    // guard is what keeps duplicate delivery from double-counting evidence.
    for r in &receipts {
        match store
            .ingest(r.clone())
            .map_err(|e| CliError::Failure(format!("ingest failed: {e}")))?
        {
            Ingest::Ingested => {
                let v = r
                    .verify()
                    .map_err(|e| CliError::Failure(format!("verify failed: {e}")))?;
                incremental.apply(&v, now_ms, 1.0);
            }
            Ingest::Duplicate => {}
        }
    }

    // §9: ingest one duplicate on purpose and prove the store is unmoved.
    if let Some(first) = receipts.first() {
        let before = store.len();
        let res = store
            .ingest(first.clone())
            .map_err(|e| CliError::Failure(format!("ingest failed: {e}")))?;
        if res != Ingest::Duplicate || store.len() != before {
            return Err(CliError::Failure(
                "duplicate ingestion was not idempotent".into(),
            ));
        }
    }

    let rebuilt = TrustProjection::rebuild(&store, now_ms, params, |_| 1.0)
        .map_err(|e| CliError::Failure(format!("rebuild failed: {e}")))?;

    println!("receipts synthesized: {}", receipts.len());
    println!("store length:         {}", store.len());

    // Both fold receipts in identical store order ⇒ bit-exact equality.
    if incremental == rebuilt {
        println!("rebuild == incremental");
        return Ok(());
    }

    // Defensive float-association fallback (§8): the single (provider,
    // capability) cell within 1e-9 still counts as rebuildable.
    let bid = b.agent_id();
    match (incremental.score(&bid, CAP), rebuilt.score(&bid, CAP)) {
        (Some((s1, c1)), Some((s2, c2)))
            if (s1 - s2).abs() <= 1e-9 && (c1 - c2).abs() <= 1e-9 =>
        {
            println!("rebuild == incremental (within 1e-9)");
            Ok(())
        }
        (si, sr) => Err(CliError::Failure(format!(
            "rebuild != incremental: incremental={si:?} rebuilt={sr:?}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// serve-ollama demo (TASK-013 FR-5, feature `real-backend`)
// ---------------------------------------------------------------------------

/// `serve-ollama --model <m> --prompt <text> [--url <u>] [--max-tokens <n>]`:
/// run one real inference through a local Ollama server and print the output,
/// the transcript root over the produced chunks, and a dual-signed receipt.
///
/// This is the single-process demo variant: even though TASK-012 (map-net) is
/// merged, a self-contained requester+provider in one command is the only way
/// to print the full text→root→receipt loop end-to-end (a networked `serve`
/// needs a separate client). A MapNode-based `serve` is a documented follow-up.
#[cfg(feature = "real-backend")]
fn cmd_serve_ollama(args: &[String]) -> Result<(), CliError> {
    use map_tasks::TranscriptBuilder;
    use ollama_provider::{result_to_chunks, InferenceBackend, InferenceRequest, OllamaBackend};

    let model = parse_flag(args, "--model")
        .ok_or_else(|| CliError::Usage("serve-ollama needs --model <name>".into()))?
        .to_string();
    let prompt = parse_flag(args, "--prompt")
        .ok_or_else(|| CliError::Usage("serve-ollama needs --prompt <text>".into()))?
        .to_string();
    let url = parse_flag(args, "--url").unwrap_or("http://127.0.0.1:11434").to_string();
    let max_tokens: u32 = parse_flag(args, "--max-tokens").and_then(|s| s.parse().ok()).unwrap_or(128);

    // Real inference. Errors never carry prompt content (adapter §11).
    let backend = OllamaBackend::new(url, std::time::Duration::from_secs(120));
    let req = InferenceRequest { model: model.clone(), prompt, max_tokens };
    let result = backend
        .infer(&req)
        .map_err(|e| CliError::Failure(format!("inference failed: {e:?}")))?;

    println!(
        "--- output ({} prompt / {} completion tokens, {} ms) ---",
        result.prompt_tokens, result.completion_tokens, result.millis
    );
    println!("{}", result.text);

    // Transcript root over the produced chunks (kind-0 output is transcript-bearing).
    let chunks = result_to_chunks(&result);
    let mut tb = TranscriptBuilder::new();
    for c in &chunks {
        tb.push(c);
    }
    let root = tb.root();
    println!("transcript-root: {}", hex::encode(root));

    // Dual-signed ExecutionReceipt over the work (requester A, provider B — demo keys).
    let a = Keypair::from_seed([1u8; 32]);
    let b = Keypair::from_seed([2u8; 32]);
    let core = ReceiptCore {
        task: TaskId(sha256(model.as_bytes())),
        requester: a.agent_id(),
        provider: b.agent_id(),
        capability: CAP.to_string(),
        started_ms: BASE_STARTED_MS,
        ended_ms: BASE_STARTED_MS + result.millis,
        outcome: Outcome::Ok,
        metrics: Metrics {
            tokens_in: result.prompt_tokens,
            tokens_out: result.completion_tokens,
            ttft_ms: 0,
            wall_ms: result.millis,
        },
        rating: None,
        payment_ref: None,
        transcript_root: Some(root),
    };
    let core_bytes = core.encode().map_err(|e| CliError::Failure(format!("receipt encode: {e:?}")))?;
    let receipt = Receipt {
        sig_requester: Some(sign_receipt(&a, ROLE_REQUESTER, &core_bytes)),
        sig_provider: Some(sign_receipt(&b, ROLE_PROVIDER, &core_bytes)),
        core_bytes,
    };
    receipt
        .verify()
        .map_err(|e| CliError::Failure(format!("receipt verify: {e:?}")))?;
    println!(
        "receipt: dual-signed OK  requester={}  provider={}",
        a.agent_id(),
        b.agent_id()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_keygen_report_is_deterministic_and_shaped() {
        let r = build_key_report([0xaa; 32]);
        assert!(r.multibase.starts_with("z6Mk"), "multibase = {}", r.multibase);
        assert!(r.agent_id.starts_with("ed01"), "agent-id = {}", r.agent_id);
        assert_eq!(r.did_key, format!("did:key:{}", r.multibase));
        assert_eq!(r.seed, "aa".repeat(32));
        assert_eq!(r.pubkey.len(), 64);
        assert_eq!(r.agent_id.len(), 68);
        // Same seed ⇒ identical rendered report.
        assert_eq!(r.render(), build_key_report([0xaa; 32]).render());
    }

    #[test]
    fn t2_parse_flag_both_forms_and_absent() {
        let spaced = ["map-cli", "keygen", "--seed", "abc"].map(String::from);
        assert_eq!(parse_flag(&spaced, "--seed"), Some("abc"));
        let equals = ["map-cli", "--seed=def"].map(String::from);
        assert_eq!(parse_flag(&equals, "--seed"), Some("def"));
        let absent = ["map-cli", "keygen"].map(String::from);
        assert_eq!(parse_flag(&absent, "--seed"), None);
    }

    #[test]
    fn t3_seed_validation() {
        assert!(parse_seed(&"a".repeat(63)).is_err()); // odd length
        assert!(parse_seed(&"a".repeat(65)).is_err()); // too long
        assert!(parse_seed(&"zz".repeat(32)).is_err()); // non-hex, 64 chars
        assert!(parse_seed(&"aa".repeat(32)).is_ok());
        // Uppercase accepted, decodes to the same bytes (§9).
        assert_eq!(parse_seed(&"AA".repeat(32)).unwrap(), [0xaa; 32]);
    }

    #[test]
    fn t4_synth_receipts_are_deterministic() {
        let (a, b) = demo_keypairs();
        let enc = |rs: &[Receipt]| {
            rs.iter()
                .map(|r| r.encode().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(enc(&synth_receipts(&a, &b, 5)), enc(&synth_receipts(&a, &b, 5)));
    }

    #[test]
    fn t5_duplicate_ingest_is_idempotent() {
        let (a, b) = demo_keypairs();
        let rs = synth_receipts(&a, &b, 3);
        let mut store = ReceiptStore::new();
        assert_eq!(store.ingest(rs[0].clone()).unwrap(), Ingest::Ingested);
        assert_eq!(store.ingest(rs[0].clone()).unwrap(), Ingest::Duplicate);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn t6_rebuild_equals_incremental() {
        let (a, b) = demo_keypairs();
        let n = 50u64;
        let receipts = synth_receipts(&a, &b, n);
        let now_ms = demo_now_ms(n);
        let params = ProjParams::default();
        let mut store = ReceiptStore::new();
        let mut incremental = TrustProjection::new(params);
        for r in &receipts {
            if store.ingest(r.clone()).unwrap() == Ingest::Ingested {
                incremental.apply(&r.verify().unwrap(), now_ms, 1.0);
            }
        }
        let rebuilt = TrustProjection::rebuild(&store, now_ms, params, |_| 1.0).unwrap();
        assert_eq!(incremental, rebuilt);
        let bid = b.agent_id();
        assert_eq!(incremental.score(&bid, CAP), rebuilt.score(&bid, CAP));
    }
}
