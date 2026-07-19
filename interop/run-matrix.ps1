# TASK-020 — live Rust<->Go interop matrix driver.
#
# Builds both peer binaries, runs every scenario in BOTH orientations
# (Rust dials -> Go listens, and Go dials -> Rust listens) over TCP loopback,
# and prints the 16-cell result table. Exit 0 iff all cells pass.
#
# Usage:  pwsh interop/run-matrix.ps1
# Requires: Go on PATH (C:\Program Files\Go\bin) and the GNU Rust toolchain
#           with mingw on PATH (C:\msys64\mingw64\bin) — see memory.md §5/§7.

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$interop = Join-Path $repo "interop"

# --- fixed test seeds (32-byte hex; identical key derivation in both impls) ---
$SEED_A  = "01" * 32   # dialer (also the delegation ROOT / RIK)
$SEED_B  = "02" * 32   # listener
$SEED_DK = "03" * 32   # delegated leaf (dialer signs with this under --delegate)
$SEED_OP = "04" * 32   # credit-ledger operator (pay-* scenarios)

$scenarios = @("echo","replay","seqgap","tamper","relay","versiondisjoint","wrongsid","delegated")

# --- build both binaries ------------------------------------------------------
Write-Host "Building Go peer (map-interop)..."
Push-Location (Join-Path $repo "map-go")
& go build -o (Join-Path $interop "map-interop.exe") ./cmd/map-interop
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "go build failed" }
Pop-Location

Write-Host "Building Rust peer (map-cli)..."
Push-Location (Join-Path $repo "map-rs")
& cargo +stable-x86_64-pc-windows-gnu build -p map-cli 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "cargo build failed" }
Pop-Location

$GO   = Join-Path $interop "map-interop.exe"
$RUST = Join-Path $repo "map-rs\target\x86_64-pc-windows-gnu\debug\map-cli.exe"
if (-not (Test-Path $RUST)) { $RUST = Join-Path $repo "map-rs\target\debug\map-cli.exe" }
if (-not (Test-Path $GO))   { Write-Error "go binary missing" }
if (-not (Test-Path $RUST)) { Write-Error "rust binary missing" }

# argument list for a peer, given impl kind ("go"/"rust"), role, port, scenario
function PeerArgs($kind, $role, $port, $scen) {
  # seeds depend on scenario (delegated uses the DK leaf + issuer root)
  if ($scen -eq "delegated") {
    if ($role -eq "dial")  { $seed = $SEED_DK; $peer = $SEED_B;  $issuer = $SEED_A }
    else                   { $seed = $SEED_B;  $peer = $SEED_DK; $issuer = $SEED_A }
  } else {
    if ($role -eq "dial")  { $seed = $SEED_A; $peer = $SEED_B }
    else                   { $seed = $SEED_B; $peer = $SEED_A }
    $issuer = $null
  }
  $addr = if ($role -eq "listen") { "127.0.0.1:0" } else { "127.0.0.1:$port" }
  $a = @("--role",$role,"--addr",$addr,"--seed",$seed,"--peer-seed",$peer,"--scenario",$scen)
  if ($issuer) { $a += @("--issuer-seed",$issuer) }
  if ($kind -eq "rust") { $a = @("interop") + $a }
  return $a
}

# run one cell: returns $true on pass. dialKind/listenKind in {"go","rust"}.
function RunCell($dialKind, $listenKind, $scen) {
  $lbin = if ($listenKind -eq "go") { $GO } else { $RUST }
  $dbin = if ($dialKind   -eq "go") { $GO } else { $RUST }
  $lout = Join-Path $env:TEMP "interop_l_$($scen)_$($dialKind)$($listenKind).txt"
  $dout = Join-Path $env:TEMP "interop_d_$($scen)_$($dialKind)$($listenKind).txt"
  Remove-Item $lout,$dout -ErrorAction SilentlyContinue

  $lp = Start-Process -FilePath $lbin -ArgumentList (PeerArgs $listenKind "listen" 0 $scen) `
        -RedirectStandardOutput $lout -NoNewWindow -PassThru

  # wait (<=5s) for the LISTENING line to appear and extract the port
  $port = $null
  for ($i = 0; $i -lt 50 -and -not $port; $i++) {
    Start-Sleep -Milliseconds 100
    if (Test-Path $lout) {
      $m = Select-String -Path $lout -Pattern "LISTENING\s+\S+:(\d+)" | Select-Object -First 1
      if ($m) { $port = $m.Matches[0].Groups[1].Value }
    }
  }
  if (-not $port) {
    try { Stop-Process -Id $lp.Id -ErrorAction SilentlyContinue } catch {}
    return @{ pass = $false; detail = "listener never bound" }
  }

  $dp = Start-Process -FilePath $dbin -ArgumentList (PeerArgs $dialKind "dial" $port $scen) `
        -RedirectStandardOutput $dout -NoNewWindow -PassThru

  $dExited = $dp.WaitForExit(15000)
  $lExited = $lp.WaitForExit(15000)
  foreach ($p in @($dp,$lp)) { try { if (-not $p.HasExited) { Stop-Process -Id $p.Id -ErrorAction SilentlyContinue } } catch {} }

  $dline = if (Test-Path $dout) { ((Get-Content $dout | Select-String "^OK|^FAIL") -join " ") } else { "" }
  $lline = if (Test-Path $lout) { ((Get-Content $lout | Select-String "^OK|^FAIL") -join " ") } else { "" }
  $pass = ($dExited -and $lExited -and $dline -match "^OK" -and $lline -match "^OK")
  $detail = if ($pass) { "" } else { "dial='$dline' listen='$lline'" }
  return @{ pass = $pass; detail = $detail }
}

# --- run the matrix -----------------------------------------------------------
$results = @{}
$allPass = $true
foreach ($scen in $scenarios) {
  foreach ($orient in @(@("rust","go"), @("go","rust"))) {
    $cell = RunCell $orient[0] $orient[1] $scen
    $key = "$scen|$($orient[0])->$($orient[1])"
    $results[$key] = $cell
    if (-not $cell.pass) { $allPass = $false }
    $mark = if ($cell.pass) { "PASS" } else { "FAIL" }
    Write-Host ("  {0,-16} {1,4}->{2,-4} {3}  {4}" -f $scen, $orient[0], $orient[1], $mark, $cell.detail)
  }
}

Write-Host ""
Write-Host "=== Interop matrix (core session, TCP) ==="
$fmt = "{0,-16} {1,-14} {2,-14}"
Write-Host ($fmt -f "scenario", "rust->go", "go->rust")
foreach ($scen in $scenarios) {
  $rg = if ($results["$scen|rust->go"].pass) { "PASS" } else { "FAIL" }
  $gr = if ($results["$scen|go->rust"].pass) { "PASS" } else { "FAIL" }
  Write-Host ($fmt -f $scen, $rg, $gr)
}

# --- libp2p task matrix (task protocol over libp2p/QUIC) --------------------
# These cells prove the full task protocol (tasks §1-§13) end-to-end over
# libp2p/QUIC in BOTH orientations: rust->go and go->rust. The transcript codec
# is cross-impl-verified (TV-008), so the same roots appear on both sides.
$taskScenarios = @("task-stream", "task-multichunk", "task-transcript")
$taskResults = @{}
$taskPass = $true
Write-Host ""
Write-Host "=== Task matrix (libp2p/QUIC) ==="
$fmt2 = "{0,-18} {1,-14} {2,-14}"
Write-Host ($fmt2 -f "scenario", "rust->go", "go->rust")

# Run one task cell: dialKind/listenKind in {"go","rust"}. The listener prints
# LISTENING <multiaddr>/p2p/<peerid>; the dialer takes that as --addr and
# --peer-seed = the listener's seed (so the authenticated AgentID matches).
# The Go binary (map-interop) takes flags directly; the Rust binary (map-cli)
# needs the `interop` subcommand first.
function RunTaskCell($dialKind, $listenKind, $scen, $extra) {
  if ($null -eq $extra) { $extra = @() }
  $lbin = if ($listenKind -eq "go") { $GO } else { $RUST }
  $dbin = if ($dialKind   -eq "go") { $GO } else { $RUST }
  # --operator-seed (and any other pay flags) are appended at the END so the
  # --addr placeholder keeps its fixed index (5 for Go, 6 for Rust).
  $largs = if ($listenKind -eq "go") {
    @("--transport","libp2p","--role","listen","--addr","x","--seed",$SEED_B,"--peer-seed",$SEED_A,"--scenario",$scen) + $extra
  } else {
    @("interop","--transport","libp2p","--role","listen","--addr","x","--seed",$SEED_B,"--peer-seed",$SEED_A,"--scenario",$scen) + $extra
  }
  $dargs = if ($dialKind -eq "go") {
    @("--transport","libp2p","--role","dial","--addr",$null,"--seed",$SEED_A,"--peer-seed",$SEED_B,"--scenario",$scen) + $extra
  } else {
    @("interop","--transport","libp2p","--role","dial","--addr",$null,"--seed",$SEED_A,"--peer-seed",$SEED_B,"--scenario",$scen) + $extra
  }
  $lout = Join-Path $env:TEMP "task_l_$($scen)_$($dialKind)$($listenKind).txt"
  $dout = Join-Path $env:TEMP "task_d_$($scen)_$($dialKind)$($listenKind).txt"
  Remove-Item $lout,$dout -ErrorAction SilentlyContinue
  $lp = Start-Process -FilePath $lbin -ArgumentList $largs `
        -RedirectStandardOutput $lout -NoNewWindow -PassThru
  $addr = $null
  for ($i = 0; $i -lt 50 -and -not $addr; $i++) {
    Start-Sleep -Milliseconds 100
    if (Test-Path $lout) {
      $m = Select-String -Path $lout -Pattern "LISTENING\s+(\S+)" | Select-Object -First 1
      if ($m) { $addr = $m.Matches[0].Groups[1].Value }
    }
  }
  $ok = $false; $detail = "listener never bound"
  if ($addr) {
    # Replace the placeholder --addr value. Index 5 for Go (no subcommand),
    # index 6 for Rust (the `interop` subcommand shifts everything by one).
    if ($dialKind -eq "go") { $dargs[5] = $addr } else { $dargs[6] = $addr }
    $dp = Start-Process -FilePath $dbin -ArgumentList $dargs `
          -RedirectStandardOutput $dout -NoNewWindow -PassThru
    $dp.WaitForExit(15000)
    if (-not $lp.HasExited) { $lp.WaitForExit(15000) }
    $dline = if (Test-Path $dout) { ((Get-Content $dout | Select-String "^OK|^FAIL") -join " ") } else { "" }
    $lline = if (Test-Path $lout) { ((Get-Content $lout | Select-String "^OK") -join " ") } else { "" }
    $ok = ($dline -match "^OK" -and $lline -match "^OK")
    if (-not $ok) { $detail = "dial='$dline' listen='$lline'" }
  }
  foreach ($p in @($dp,$lp)) { try { if (-not $p.HasExited) { Stop-Process -Id $p.Id -ErrorAction SilentlyContinue } } catch {} }
  return @{ pass = $ok; detail = $detail }
}

foreach ($scen in $taskScenarios) {
  $rg = RunTaskCell "rust" "go" $scen
  $gr = RunTaskCell "go" "rust" $scen
  $taskResults["$scen|rust->go"] = $rg
  $taskResults["$scen|go->rust"] = $gr
  if (-not $rg.pass) { $taskPass = $false }
  if (-not $gr.pass) { $taskPass = $false }
  $mark1 = if ($rg.pass) { "PASS" } else { "FAIL" }
  $mark2 = if ($gr.pass) { "PASS" } else { "FAIL" }
  Write-Host ($fmt2 -f $scen, $mark1, $mark2)
  if (-not $rg.pass) { Write-Host ("    rust->go: {0}" -f $rg.detail) }
  if (-not $gr.pass) { Write-Host ("    go->rust: {0}" -f $gr.detail) }
}

# --- libp2p pay matrix (pay protocol over libp2p/QUIC) ----------------------
# The pay family (negotiation + credit-ledger settlement, pay §2-§5) run live
# over libp2p/QUIC in both orientations, after the task transcript verifies on
# the same session. pay-postpaid = one settlement; pay-metered = two
# incremental watermark invoices. The requester hosts the in-process ledger
# operator (--operator-seed), pre-credited.
$payScenarios = @("pay-postpaid", "pay-metered")
$payResults = @{}
$payPass = $true
$payExtra = @("--operator-seed", $SEED_OP)
Write-Host ""
Write-Host "=== Pay matrix (libp2p/QUIC) ==="
Write-Host ($fmt2 -f "scenario", "rust->go", "go->rust")
foreach ($scen in $payScenarios) {
  $rg = RunTaskCell "rust" "go" $scen $payExtra
  $gr = RunTaskCell "go" "rust" $scen $payExtra
  $payResults["$scen|rust->go"] = $rg
  $payResults["$scen|go->rust"] = $gr
  if (-not $rg.pass) { $payPass = $false }
  if (-not $gr.pass) { $payPass = $false }
  $mark1 = if ($rg.pass) { "PASS" } else { "FAIL" }
  $mark2 = if ($gr.pass) { "PASS" } else { "FAIL" }
  Write-Host ($fmt2 -f $scen, $mark1, $mark2)
  if (-not $rg.pass) { Write-Host ("    rust->go: {0}" -f $rg.detail) }
  if (-not $gr.pass) { Write-Host ("    go->rust: {0}" -f $gr.detail) }
}

Write-Host ""
if ($allPass -and $taskPass -and $payPass) {
  Write-Host "RESULT: 16/16 core PASS + 6/6 task + 4/4 pay (libp2p, both orientations) PASS"
  exit 0
} else {
  Write-Host "RESULT: FAILURES PRESENT (core=$allPass task=$taskPass pay=$payPass)"
  exit 1
}
