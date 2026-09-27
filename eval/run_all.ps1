# One-command evaluation (eval/HYPOTHESES.md). Results go to
# eval/results/<git-sha>/: one CSV + env.json per experiment, bench/,
# REPORT.md (every hypothesis verdict), MINIMAX.md (Amendment A9), B2L.md
# (exploratory), PREREG-EXTRA.md (pre-registered items outside REPORT.md), SYBIL-SWEEP.md (A10), DISCOVERY-FLOOD.md (A11), W1-H1.md, TARGETED-SLANDER.md (A12)
# and figs/.
#
# Usage:  pwsh eval/run_all.ps1 [-Quick] [-Out DIR] [-SkipBench]
#   -Quick     2 seeds per point (plumbing check; not a result)
#   -Out       output directory (default eval/results/<sha>, or <sha>-quick)
# Requires the GNU Rust toolchain with mingw on PATH, and a Python with
# eval/analysis/requirements.txt installed ($env:MAP_EVAL_PY, else python).

param([switch]$Quick, [string]$Out = "", [switch]$SkipBench)
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$eval = Join-Path $repo "eval"
$rs = Join-Path $repo "map-rs"
$py = if ($env:MAP_EVAL_PY) { $env:MAP_EVAL_PY } else { "python" }

$sha = (git -C $repo rev-parse --short HEAD).Trim()
$dirty = (git -C $repo status --porcelain | Measure-Object).Count
if ($dirty -gt 0) { Write-Warning "working tree has $dirty uncommitted change(s); results will not match commit $sha exactly" }
if (-not $Out) { $Out = Join-Path $eval ("results\" + $sha + $(if ($Quick) { "-quick" } else { "" })) }
New-Item -ItemType Directory -Force $Out | Out-Null
$seedArgs = if ($Quick) { @("--seeds", "2") } else { @() }

Write-Host "== build + smoke gate (golden hashes, fast = real) =="
Push-Location $rs
& cargo +stable-x86_64-pc-windows-gnu build -p map-sim --release 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "build failed" }
& cargo +stable-x86_64-pc-windows-gnu test --release -p map-sim 2>&1 | Select-String "^test result" | ForEach-Object { Write-Host "  $_" }
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "map-sim tests failed" }
Pop-Location
$sim = Join-Path $rs "target\release\map-sim.exe"

function Run($config, [switch]$Fast) {
  $a = @((Join-Path $eval "configs\$config.toml"), "--out", $Out) + $seedArgs
  if ($Fast) { $a += "--fast-crypto" }
  & $sim @a
  if ($LASTEXITCODE -ne 0) { Write-Error "map-sim failed on $config" }
}

Write-Host "`n== experiments =="
Run f1a
Run f1b
Run e2
Run e1_d5 -Fast
Run e1_sparse -Fast
$main = Get-Content (Join-Path $eval "configs\e1_main.toml") -Raw
if ($main -match 'weights = "D5_WINNER"') {
  Write-Warning "e1_main.toml still has weights = D5_WINNER: apply the D5 rule (REPORT.md), record it in HYPOTHESES.md Amendments, set it, and re-run."
} else {
  Run e1_main -Fast
  Run e1_camo_explore -Fast   # exploratory (Amendment A8)
  Run e1_minimax -Fast        # W0 and W1 on the full grid (Amendment A9)
  Run e1_sybil_sweep -Fast    # Sybils per adversarial provider (Amendment A10)
  Run e1_discovery_flood -Fast  # discovery flooding x trust (Amendment A11)
  Run e1_targeted_slander -Fast # camouflaged targeted slander (Amendment A12)
}
Run e3a
Run e3b
Run e3b_multicore   # exploratory (Amendment A5)
Run e3c
Run e3d
Run e4

Write-Host "`n== Amendment A1 check: e1_realcheck real vs fast =="
$rcFast = Join-Path $Out "realcheck-fast"; $rcReal = Join-Path $Out "realcheck-real"
& $sim (Join-Path $eval "configs\e1_realcheck.toml") --out $rcFast --fast-crypto | Tee-Object -Variable f | Out-Null
& $sim (Join-Path $eval "configs\e1_realcheck.toml") --out $rcReal | Tee-Object -Variable r | Out-Null
$hf = ("$f" -split "csv_sha256=")[1]; $hr = ("$r" -split "csv_sha256=")[1]
$same = ($hf -eq $hr)
"fast $hf`nreal $hr`nequal $same" | Set-Content (Join-Path $Out "realcheck.txt")
Write-Host "  fast = real: $same"
if (-not $same) { Write-Error "A1 violated: fast and real crypto disagree on e1_realcheck" }

if (-not $SkipBench) {
  Write-Host "`n== benches =="
  Push-Location $rs
  & cargo +stable-x86_64-pc-windows-gnu bench -p map-sim --bench crypto 2>&1 | Select-String "time:" | ForEach-Object { Write-Host "  $_" }
  Pop-Location
  foreach ($b in @("keygen_shard", "receipt_dual", "checkpoint_cycle", "transcript_push_root")) {
    $src = Join-Path $rs "target\criterion\$b\new\estimates.json"
    $dst = Join-Path $Out "bench\$b"
    New-Item -ItemType Directory -Force $dst | Out-Null
    Copy-Item $src (Join-Path $dst "estimates.json")
  }
}

Write-Host "`n== analysis =="
Push-Location (Join-Path $eval "analysis")
& $py analyze.py $Out
& $py make_figs.py $Out
# minimax.py (A9), b2l.py, prereg_extra.py, sybil_sweep.py (A10), discovery_flood.py (A11), w1_h1.py and targeted_slander.py (A12) require 20 seeds per cell.
if ($Quick) {
  Write-Host "  -Quick: skipping minimax.py, b2l.py, prereg_extra.py, sybil_sweep.py, discovery_flood.py, w1_h1.py and targeted_slander.py (they require 20 seeds per cell)"
} elseif (Test-Path (Join-Path $Out "e1-main.csv")) {
  & $py minimax.py $Out | Out-Null
  if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "minimax.py failed" }
  & $py b2l.py $Out | Out-Null
  if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "b2l.py failed" }
  & $py prereg_extra.py $Out | Out-Null
  if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "prereg_extra.py failed" }
  & $py sybil_sweep.py $Out | Out-Null
  if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "sybil_sweep.py failed" }
  & $py discovery_flood.py $Out | Out-Null
  if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "discovery_flood.py failed" }
  & $py w1_h1.py $Out | Out-Null
  if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "w1_h1.py failed" }
  & $py targeted_slander.py $Out | Out-Null
  if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "targeted_slander.py failed" }
}
Pop-Location
Write-Host "`nReport: $(Join-Path $Out 'REPORT.md')"
