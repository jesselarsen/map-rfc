# Reviewer exploit probes (2026-09-18 external review)

These crates were written by external reviewers during the 2026-09-18 review.
They were originally kept in throwaway session scratchpads. They are preserved
here verbatim. The only edit is to the `path =` dependencies, which are now
relative so each crate builds from this directory.

Each probe is a standalone crate with an empty `[workspace]`. It links the
**unmodified** `map-rs` crates by path. The probes are not workspace members,
so `cargo test --workspace` does not build them.

The findings, rulings and dispositions are recorded in `map-spec/ERRATA.md`
(2026-09-18 entries) and in `MAP Review Results/`.

Run one probe:

```powershell
$env:PATH = "C:\msys64\mingw64\bin;" + $env:PATH
cargo +stable-x86_64-pc-windows-gnu run --release --manifest-path attic/probes/<probe>/Cargo.toml [--bin <bin>]
```

## Index

| Probe | Binary | Finding | What it demonstrates |
|---|---|---|---|
| `adv` | `adv` (main) | Role 7 A1 | A unilateral, provider-signed receipt that *names a reputable requester* verifies, is ingested, and raises the attacker's score under `TrustProjection::rebuild`. The dual-gated incremental path gives it no score, so rebuild and incremental disagree. |
| `adv` | `sybil` | Role 7 A1 | A Sybil ring of dual-signed receipts with free keys. Each unknown issuer is weighted 0.5 (the prior). 1000 receipts give score 0.998 with confidence 501 in ~150 ms, and one honest failure barely moves it. |
| `adv` | `replay` | Role 7 A8 | `ReplayCache::check` has quadratic cost. Per-message cost grows with cache length. |
| `adv` | `grind` | Role 7 A4 | Minting AgentIDs costs ~60k IDs/s/core. It prints the cost of an XOR-prefix match against a target DHT key. |
| `rt7probe` | `rt7probe` (main) | Role 7 A1 | (A) A receipt signed in both roles by one key reports `dual=true`. (B/C) Self-dealt receipts dated in the year 2100 never decay, so they overtake an honest provider as time passes. (D) Minting 10k of them takes ~1.3 s. |
| `rt7probe` | `grind` | Role 7 A3/A4 | Grinding AgentIDs into a chosen discovery shard (`map_discovery::shard == 141`). |
| `negcheck` | `negcheck` | R4-01, R4-17 | Negative `Amount`. (A) `pay(-x)` moves funds from payee to payer, and `verify_entry` accepts the entry. (B) `hold(-x)` + `release(-x)` drains the payee. (C) The refund entry carries the wrong asset. |
| `dispcheck` | `dispcheck` | R4-03, R4-04, R4-12 | (1) Claim 3 is applied to terms-1 prepaid. (2) Claim 1 accepts a *payer*-signed forged CHUNK. (3b) Claim 0 is rebutted by one bogus self-signed COMPLETE. |
| `paypoc` | `paypoc` | R4-01, R4-02, R4-04, R4-12 | (1) Checkpoint `usage` is not covered by the countersignature, so editing it flips the verdict. (2) Claim 0 is rebutted by a COMPLETE forged afterwards. (3) Claim 3 is applied to a conforming escrow hold. (4) A negative release or pay drains the payee. |

## Output at `a1df777` (re-run 2026-09-24, release build, Windows x86_64)

```
negcheck   A) pay(-50000) -> Ok; after: attacker=50000 victim=50000; verify_entry -> true
           B) hold(-50000)+release(-50000) -> Ok; after: payer=50000 payee=50000
           C) refund entry contains 'iso4217:USD' = false
dispcheck  1) terms-1 prepaid, checkpoint bills 130 vs invoice 500 -> Supported
           2) claim 1 with a PAYER-signed forged chunk -> Supported
           3b) claim 0, payee mints one bogus COMPLETE -> Rebutted
paypoc     PoC1 usage-edited checkpoint (countersig unchanged) -> Supported (honest -> Rebutted)
           PoC2 claim0 payee forges COMPLETE -> Rebutted
           PoC4 release(-9.00) -> true; PoC4b pay(-7.00) with payer balance 0 -> true
rt7probe   A self-dealing receipt: requester==provider=true dual=true
           now+10y  honest H score=0.500 conf=0.00 | self-dealer X score=0.981 conf=50.00
           D minted+ingested 10k self-dealt dual receipts in 1.31s
adv        [3] rebuild() score for attacker = 0.667 (conf 1.0)
           [4] dual-gated incremental score = None  <-- diverges from [3]
sybil      +1000 sybil receipts -> score=0.9980 confidence=501.0 minted in 158ms
           1000 sybil + 1 honest FAIL -> score 0.996
replay     n=64000 per-msg=40.82us (n=2000: 1.26us)
grind(adv) 60,990 ids/sec; 24-bit prefix ~275 core-seconds
grind(rt7) 16.51 us per keygen+shard; shard-141 hits=752 (expect 781)
```

## Status

- **Payment-layer probes (`negcheck`, `dispcheck`, `paypoc`)** target code that
  the accountability-layer rework deletes: escrow, disputes and the credit
  ledger. They stay here as the record of why it was removed. `Amount`
  non-negativity carries over into the kept metering code as a regression test.
- **Trust and discovery probes (`adv`, `rt7probe`)** target kept layers. They
  are ported as `#[ignore]` regression tests in the workspace. Each test
  asserts the *secure* behaviour, so it fails today and passes once the fix
  lands. See `docs/ADR-0001-thin-accountability-layer.md`.
  - `map-trust/tests/attack_farming.rs` (A1): self-dealing, future-dated,
    honest-overtaken, and forged-unilateral / rebuild≠incremental. The Sybil
    ring case waits for the issuer-weighting API (decision D5).
  - `map-core/tests/attack_replay_cache.rs` (A8): per-message cost ratio.
  - `map-mesh/tests/attack_shard_flood.rs` (A3): this is not from a probe; it
    is the Role 7 description turned into a test.
  - `map-discovery/tests/attack_grind.rs` (A4): a measurement that feeds E3,
    not a pass/fail check.

  Status at `4107aaa` with `--ignored`: all 6 regression tests FAIL as
  expected, and the grind measurement reports ~59.8k keygen+shard/s.
