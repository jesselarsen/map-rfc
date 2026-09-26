//! `map-sim <config.toml>... [--out DIR] [--seeds N] [--threads N] [--fast-crypto]`
//!
//! Runs each config and writes `<DIR>/<scenario>.csv` (one row per sweep
//! point × seed) and `<DIR>/<scenario>.env.json` (toolchain, host, config
//! hash, CSV hash). Prints the CSV's SHA-256, which the golden test pins for
//! the smoke config.

use map_sim::config::Config;
use map_sim::crypto::CryptoMode;
use map_sim::runner;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

fn cmd_out(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut configs = Vec::new();
    let mut out = PathBuf::from("eval/results/local");
    let mut seeds = None;
    let mut threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut fast = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                out = PathBuf::from(&args[i]);
            }
            "--seeds" => {
                i += 1;
                seeds = Some(args[i].parse().expect("--seeds N"));
            }
            "--threads" => {
                i += 1;
                threads = args[i].parse().expect("--threads N");
            }
            "--fast-crypto" => fast = true,
            a => configs.push(PathBuf::from(a)),
        }
        i += 1;
    }
    if configs.is_empty() {
        eprintln!(
            "usage: map-sim <config.toml>... [--out DIR] [--seeds N] [--threads N] [--fast-crypto]"
        );
        std::process::exit(2);
    }
    std::fs::create_dir_all(&out).expect("create output dir");
    let git = cmd_out("git", &["rev-parse", "HEAD"]);
    let rustc = cmd_out("rustc", &["--version"]);
    for path in configs {
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let cfgs = Config::parse_many(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for cfg in cfgs {
            let t0 = Instant::now();
            let res = runner::run(&cfg, CryptoMode { fast }, threads, seeds).unwrap_or_else(|e| {
                eprintln!("{}: {e}", path.display());
                std::process::exit(1)
            });
            let secs = t0.elapsed().as_secs_f64();
            let csv_path = out.join(format!("{}.csv", cfg.stem()));
            std::fs::write(&csv_path, &res.csv).expect("write csv");
            let env = format!(
            "{{\n  \"scenario\": {},\n  \"config\": {},\n  \"config_sha256\": {},\n  \"csv_sha256\": {},\n  \"jobs\": {},\n  \"fast_crypto\": {},\n  \"threads\": {},\n  \"wall_seconds\": {:.3},\n  \"git\": {},\n  \"rustc\": {},\n  \"os\": {},\n  \"arch\": {}\n}}\n",
            json_str(cfg.stem()),
            json_str(&path.display().to_string()),
            json_str(&hex::encode(Sha256::digest(text.as_bytes()))),
            json_str(&res.sha256),
            res.jobs,
            fast,
            threads,
            secs,
            json_str(&git),
            json_str(&rustc),
            json_str(std::env::consts::OS),
            json_str(std::env::consts::ARCH),
        );
            std::fs::write(out.join(format!("{}.env.json", cfg.stem())), env)
                .expect("write env.json");
            println!(
                "{} {} jobs {:.1}s csv_sha256={}",
                cfg.stem(),
                res.jobs,
                secs,
                res.sha256
            );
        }
    }
}
