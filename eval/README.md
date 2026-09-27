# eval/ — the pre-registered evaluation

- [`HYPOTHESES.md`](HYPOTHESES.md) is the plan: F1 (replication of
  arXiv:2603.03753 plus MAP arms in its scenario), E1 (trust under attack
  and decision D5), E2 (checkpoint handoff), E3 (discovery), E4
  (metering).
  - It was committed and tagged `prereg-v1` before any simulator code
    existed.
  - Later changes appear only as dated entries under *Amendments*.
- `configs/*.toml` holds one config per experiment. Each has a scenario,
  a seed count, fixed `[params]` and a cartesian `[sweep]`.
  - `smoke.toml` holds a few seconds of every scenario; its CSV hashes are
    pinned in `smoke.golden`.
- The simulator is `map-rs/sim` (crate `map-sim`): a deterministic
  discrete-event engine over the real `map-*` crates.
  - `cargo run -p map-sim --release -- <config.toml> --out DIR`
    produces the same CSV bytes for the same config at any thread count.
  - `--fast-crypto` skips Ed25519 signing and verification only.
    `tests/golden.rs` and the E1 realcheck assert it changes no result.
- `analysis/` (Python, pinned in `requirements.txt`):
  - `analyze.py` applies every pre-registered decision rule to the CSVs
    and writes `REPORT.md`;
  - `make_figs.py` draws the figures;
  - `minimax.py` (Amendment A9) writes `MINIMAX.md`;
  - `b2l.py` writes `B2L.md`, which is EXPLORATORY: B2L against Ours-L
    and Ours-L+C, and the A-unilateral ablation. It uses `e1-main.csv`
    only and bootstraps the arms independently, because they do not
    share random streams;
  - `prereg_extra.py` writes `PREREG-EXTRA.md`: pre-registered items
    `REPORT.md` does not cover. These are E1 time to detect and
    all-rounds share, checkpoint bytes per Part, the 16-core E3b, and the
    E3d message-rate crossover;
  - `sybil_sweep.py` (Amendment A10) writes `SYBIL-SWEEP.md` from
    `e1-sybil-sweep.csv`;
  - `discovery_flood.py` (Amendment A11) writes `DISCOVERY-FLOOD.md` from
    `e1-discovery-flood.csv`;
  - `w1_h1.py` writes `W1-H1.md`: the H1 criterion re-evaluated for W1,
    next to W2, with unpaired CIs (exploratory);
  - `stats.py` is the percentile bootstrap (10,000 resamples, seeds
    resampled jointly for comparisons).

## Reproduce

```
pwsh eval/run_all.ps1            # full run -> eval/results/<git-sha>/
pwsh eval/run_all.ps1 -Quick     # 2 seeds per point: plumbing only, not a result
```

`run_all.ps1` does the following:
1. builds `map-sim` and runs its release tests (golden hashes, fast = real,
   E1 pool = `TrustProjection`);
2. runs every config (E1 with `--fast-crypto`, per Amendment A1) and
   re-checks the E1 realcheck;
3. runs the Criterion benches (`map-rs/sim/benches/crypto.rs`) and copies
   their estimates in;
4. writes `REPORT.md`, `figs/`, `PREREG-EXTRA.md`, `MINIMAX.md` (the
   Amendment A9 rule), `B2L.md` (exploratory), `SYBIL-SWEEP.md` (A10) and
   `DISCOVERY-FLOOD.md` (A11) and `W1-H1.md`.
   All but the first two need 20 seeds per cell, so `-Quick` skips them.

`e1_main.toml` carries `weights = "D5_WINNER"` until the D5 decision has
been made from `e1-d5.csv` and recorded as an Amendment.
