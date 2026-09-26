//! The smoke gate: `eval/configs/smoke.toml` reproduces the CSV hashes in
//! `eval/configs/smoke.golden` (per platform; floating-point libm results
//! may differ across OS/arch), and real and fast crypto modes agree.

use map_sim::config::Config;
use map_sim::crypto::CryptoMode;
use map_sim::runner;

fn smoke() -> Vec<Config> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../eval/configs/smoke.toml"
    ))
    .unwrap();
    Config::parse_many(&text).unwrap()
}

fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[test]
#[cfg_attr(debug_assertions, ignore = "real Ed25519 is too slow unoptimized; run `cargo test --release -p map-sim`")]
fn smoke_matches_golden_and_fast_equals_real() {
    let golden = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../eval/configs/smoke.golden"
    ))
    .unwrap_or_default();
    let mut missing = Vec::new();
    for cfg in smoke() {
        let real = runner::run(&cfg, CryptoMode { fast: false }, 4, None).unwrap();
        let fast = runner::run(&cfg, CryptoMode { fast: true }, 3, None).unwrap();
        assert_eq!(
            real.csv,
            fast.csv,
            "{}: fast and real crypto disagree",
            cfg.stem()
        );
        let want = golden.lines().find_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.len() == 3 && f[0] == platform() && f[1] == cfg.stem()).then(|| f[2].to_string())
        });
        match want {
            Some(w) => assert_eq!(
                real.sha256,
                w,
                "{}: CSV hash differs from smoke.golden",
                cfg.stem()
            ),
            None => missing.push(format!("{} {} {}", platform(), cfg.stem(), real.sha256)),
        }
    }
    if !missing.is_empty() {
        panic!("no golden hash for this platform; if the run is intended, add to eval/configs/smoke.golden:\n{}", missing.join("\n"));
    }
}
