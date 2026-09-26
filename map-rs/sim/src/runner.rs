//! Runs every (sweep point × seed) job of a config across threads and
//! writes one CSV row per job, in job order, so the file is byte-identical
//! whatever the thread count.

use crate::config::{cell, Config, Params};
use crate::crypto::CryptoMode;
use crate::rng::Rng;
use crate::scenarios;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub type RunFn = fn(&Params, &mut Rng, CryptoMode) -> Result<Vec<f64>, String>;

pub struct Scenario {
    pub name: &'static str,
    pub metrics: &'static [&'static str],
    pub run: RunFn,
}

/// One job's result, filled in by whichever thread ran it.
type Slot = Mutex<Option<Result<Vec<f64>, String>>>;

pub struct Output {
    pub csv: String,
    pub sha256: String,
    pub jobs: usize,
}

/// Format a metric value: integers without a fractional part, other values
/// in Rust's shortest round-trip form (deterministic on a given platform).
fn num(x: f64) -> String {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

pub fn run(
    cfg: &Config,
    mode: CryptoMode,
    threads: usize,
    seeds_override: Option<u64>,
) -> Result<Output, String> {
    let sc = scenarios::find(&cfg.scenario)
        .ok_or_else(|| format!("unknown scenario `{}`", cfg.scenario))?;
    let seeds = seeds_override.unwrap_or(cfg.seeds);
    let points = cfg.points();
    let jobs: Vec<(usize, u64)> = (0..points.len())
        .flat_map(|p| (0..seeds).map(move |s| (p, s)))
        .collect();
    let results: Vec<Slot> = jobs.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(&(p, s)) = jobs.get(i) else { break };
                let mut rng = Rng::for_run(sc.name, p, s);
                let r = (sc.run)(&points[p], &mut rng, mode);
                *results[i].lock().expect("no poisoned lock") = Some(r);
            });
        }
    });
    let sweep_keys: Vec<&String> = cfg.sweep.keys().collect();
    let mut csv = String::new();
    csv.push_str("point");
    for k in &sweep_keys {
        csv.push(',');
        csv.push_str(k);
    }
    csv.push_str(",seed");
    for m in sc.metrics {
        csv.push(',');
        csv.push_str(m);
    }
    csv.push('\n');
    for (i, &(p, s)) in jobs.iter().enumerate() {
        let r = results[i]
            .lock()
            .expect("no poisoned lock")
            .take()
            .expect("every job ran");
        let row = r.map_err(|e| format!("point {p} seed {s}: {e}"))?;
        if row.len() != sc.metrics.len() {
            return Err(format!(
                "scenario {} returned {} metrics, declares {}",
                sc.name,
                row.len(),
                sc.metrics.len()
            ));
        }
        csv.push_str(&p.to_string());
        for k in &sweep_keys {
            csv.push(',');
            csv.push_str(&cell(&points[p].swept[*k]));
        }
        csv.push(',');
        csv.push_str(&s.to_string());
        for x in row {
            csv.push(',');
            csv.push_str(&num(x));
        }
        csv.push('\n');
    }
    let sha256 = hex::encode(Sha256::digest(csv.as_bytes()));
    Ok(Output {
        csv,
        sha256,
        jobs: jobs.len(),
    })
}
