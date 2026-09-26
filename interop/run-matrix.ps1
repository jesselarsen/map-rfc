# Live Rust<->Go interop matrix driver (accountability-layer P5).
#
# Three parts, all must pass:
#   (a) vectors: Rust and Go conformance each reproduce/consume every live
#       frozen vector in spec/test-vectors/map-test-vectors.md;
#   (b) A2A binding over HTTP: every combination of requester, provider A
#       and provider B in {rust, go} runs the failover task (A crashes after
#       producing 10 Parts, the requester resumes on B from A's last
#       countersigned checkpoint), plus plain-client cells across
#       implementations and, if MAP_A2A_SDK_PY is set, the unmodified
#       a2a-sdk against each provider;
#   (c) the core session cells over TCP, both orientations.
# Exit 0 iff every cell passes (SDK cells count only when run).
#
# Usage:  pwsh interop/run-matrix.ps1
# Requires: Go on PATH (C:\Program Files\Go\bin) and the GNU Rust toolchain
#           with mingw on PATH (C:\msys64\mingw64\bin).

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$interop = Join-Path $repo "interop"
$bin = Join-Path $interop "bin"
New-Item -ItemType Directory -Force $bin | Out-Null
$logs = Join-Path ([IO.Path]::GetTempPath()) "map-interop"
New-Item -ItemType Directory -Force $logs | Out-Null

# --- fixed test seeds (32-byte hex; identical key derivation in both impls) ---
$SEED_A  = "01" * 32   # dialer
$SEED_B  = "02" * 32   # listener

# "delegated" was retired with delegation (P3, layer §3.3).
$scenarios = @("echo","replay","seqgap","tamper","relay","versiondisjoint","wrongsid")

# --- build ---------------------------------------------------------------------
Write-Host "Building Go peers (map-interop, map-a2a-provider, map-a2a-requester)..."
Push-Location (Join-Path $repo "map-go")
& go build -o (Join-Path $interop "map-interop.exe") ./cmd/map-interop
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "go build failed" }
& go build -o "$bin/" ./cmd/map-a2a-provider ./cmd/map-a2a-requester
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "go build (a2a) failed" }
Pop-Location

Write-Host "Building Rust peers (map-cli, map-a2a --features http)..."
Push-Location (Join-Path $repo "map-rs")
& cargo +stable-x86_64-pc-windows-gnu build -p map-cli 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "cargo build failed" }
& cargo +stable-x86_64-pc-windows-gnu build -p map-a2a --features http --bins 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "cargo build (a2a) failed" }
Pop-Location

$GO   = Join-Path $interop "map-interop.exe"
$rtarget = Join-Path $repo "map-rs\target\x86_64-pc-windows-gnu\debug"
if (-not (Test-Path (Join-Path $rtarget "map-cli.exe"))) { $rtarget = Join-Path $repo "map-rs\target\debug" }
$RUST = Join-Path $rtarget "map-cli.exe"
$A2A = @{
  rust = @{ prov = (Join-Path $rtarget "map-a2a-provider.exe"); req = (Join-Path $rtarget "map-a2a-requester.exe") }
  go   = @{ prov = (Join-Path $bin "map-a2a-provider.exe");     req = (Join-Path $bin "map-a2a-requester.exe") }
}
foreach ($b in @($GO, $RUST, $A2A.rust.prov, $A2A.rust.req, $A2A.go.prov, $A2A.go.req)) {
  if (-not (Test-Path $b)) { Write-Error "missing binary $b" }
}

$allPass = $true
$counts = [ordered]@{}
function Tally($part, $pass) {
  if (-not $counts.Contains($part)) { $counts[$part] = @(0, 0) }
  $counts[$part] = @(($counts[$part][0] + [int]$pass), ($counts[$part][1] + 1))
  if (-not $pass) { $script:allPass = $false }
}
function Mark($pass) { if ($pass) { "PASS" } else { "FAIL" } }

# --- (a) vectors -------------------------------------------------------------------
Write-Host "`n=== (a) Frozen vectors ==="
Push-Location (Join-Path $repo "map-rs")
$rv = & cargo +stable-x86_64-pc-windows-gnu test -p map-conformance 2>&1 | Select-String "^test result" | Select-Object -First 1
$rok = ($LASTEXITCODE -eq 0)
Pop-Location
Write-Host ("  {0,-8} {1}  {2}" -f "rust", (Mark $rok), $rv)
Tally "vectors" $rok
Push-Location (Join-Path $repo "map-go")
$gv = & go test -count=1 ./conformance ./a2a 2>&1
$gok = ($LASTEXITCODE -eq 0)
Pop-Location
Write-Host ("  {0,-8} {1}  {2}" -f "go", (Mark $gok), (($gv | Select-Object -First 2) -join " | "))
Tally "vectors" $gok

# --- (b) A2A binding over HTTP -----------------------------------------------------
function Start-A2AProvider($kind, $port, $seed, $extra) {
  $a = @("--listen", "127.0.0.1:$port", "--seed", "$seed", "--parts", "24", "--checkpoint-every", "4") + $extra
  Start-Process -FilePath $A2A[$kind].prov -ArgumentList $a -PassThru -NoNewWindow `
    -RedirectStandardError (Join-Path $logs "prov-$kind-$port.err") -RedirectStandardOutput (Join-Path $logs "prov-$kind-$port.out")
}
function Wait-Card($port) {
  for ($i = 0; $i -lt 60; $i++) {
    try { Invoke-RestMethod "http://127.0.0.1:$port/.well-known/agent-card.json" -TimeoutSec 1 | Out-Null; return $true }
    catch { Start-Sleep -Milliseconds 100 }
  }
  return $false
}
function Invoke-Requester($kind, $argList) {
  $out = & $A2A[$kind].req @argList 2>&1
  $line = $out | Where-Object { "$_" -like "MAP-DEMO-RESULT *" } | Select-Object -Last 1
  if (-not $line) { return $null }
  return ("$line".Substring("MAP-DEMO-RESULT ".Length) | ConvertFrom-Json)
}

Write-Host "`n=== (b) A2A binding over HTTP ==="
$port = 18700
Write-Host ("  {0,-10} {1,-10} {2,-10} {3}" -f "requester", "provider A", "provider B", "failover: A crashes after 10 Parts, resume on B")
foreach ($rq in @("rust", "go")) {
  foreach ($kindA in @("rust", "go")) {
    foreach ($kindB in @("rust", "go")) {
      $port += 10
      $procA = Start-A2AProvider $kindA $port 2 @("--fail-after-parts", "10")
      $procB = Start-A2AProvider $kindB ($port + 1) 3 @()
      $detail = ""
      try {
        if (-not ((Wait-Card $port) -and (Wait-Card ($port + 1)))) { throw "provider did not come up" }
        $r = Invoke-Requester $rq @("--providers", "http://127.0.0.1:$port,http://127.0.0.1:$($port + 1)", "--seed", "1", "--zone", "2")
        $procA.WaitForExit(3000) | Out-Null
        if (-not $r) { throw "requester produced no result" }
        $sum = ($r.vouchers | Measure-Object tokensOut -Sum).Sum
        $checks = [ordered]@{
          "A crashed"        = ($procA.HasExited -and $procA.ExitCode -eq 3)
          "dual receipt"     = ($r.dualSigned -eq $true)
          "resumed-from"     = [bool]$r.resumedFrom
          "voucher digest"   = [bool]$r.voucherDigestInReceipt
          "24 Parts"         = ($r.partsDelivered -eq 24)
          "watermarks = 24"  = ($sum -eq 24)
          "A paid 8"         = ($r.hops[0].acked -eq 8)
        }
        $failed = @($checks.Keys | Where-Object { -not $checks[$_] })
        $pass = ($failed.Count -eq 0)
        if (-not $pass) { $detail = "failed: " + ($failed -join ", ") }
      } catch { $pass = $false; $detail = "$_" }
      finally { foreach ($p in @($procA, $procB)) { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } } }
      Write-Host ("  {0,-10} {1,-10} {2,-10} {3}  {4}" -f $rq, $kindA, $kindB, (Mark $pass), $detail)
      Tally "a2a-failover" $pass
    }
  }
}

Write-Host ("`n  {0,-22} {1}" -f "plain client -> provider", "(no extension: ordinary COMPLETED task, no MAP metadata; A2A-28)")
foreach ($pair in @(@("rust", "go"), @("go", "rust"))) {
  $port += 10
  $p = Start-A2AProvider $pair[1] $port 3 @()
  try {
    $pass = (Wait-Card $port)
    if ($pass) {
      $r = Invoke-Requester $pair[0] @("--providers", "http://127.0.0.1:$port", "--plain")
      $pass = ($r -and $r.state -eq "TASK_STATE_COMPLETED" -and $r.parts -eq 24)
    }
  } finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }
  Write-Host ("  {0,-22} {1}" -f "$($pair[0]) -> $($pair[1])", (Mark $pass))
  Tally "a2a-plain" $pass
}

Write-Host ("`n  {0,-22} {1}" -f "a2a-sdk 1.0.3 -> provider", "(card JWS verified by the SDK, negative control, task completes)")
foreach ($kind in @("rust", "go")) {
  if (-not $env:MAP_A2A_SDK_PY) { Write-Host ("  {0,-22} SKIP (set MAP_A2A_SDK_PY)" -f "sdk -> $kind"); continue }
  $port += 10
  $p = Start-A2AProvider $kind $port 3 @()
  try {
    $pass = (Wait-Card $port)
    if ($pass) {
      $sdk = & $env:MAP_A2A_SDK_PY (Join-Path $interop "a2a-sdk\check_sdk.py") "http://127.0.0.1:$port" 2>&1
      $pass = ($LASTEXITCODE -eq 0)
    }
  } finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }
  $why = if ($pass) { "" } else { ($sdk | Where-Object { "$_" -match "FAIL" }) -join "; " }
  Write-Host ("  {0,-22} {1}  {2}" -f "sdk -> $kind", (Mark $pass), $why)
  Tally "a2a-sdk" $pass
}

# --- (c) core session cells over TCP --------------------------------------------
# argument list for a peer, given impl kind ("go"/"rust"), role, port, scenario
function PeerArgs($kind, $role, $port, $scen) {
  if ($role -eq "dial")  { $seed = $SEED_A; $peer = $SEED_B }
  else                   { $seed = $SEED_B; $peer = $SEED_A }
  $addr = if ($role -eq "listen") { "127.0.0.1:0" } else { "127.0.0.1:$port" }
  $a = @("--role",$role,"--addr",$addr,"--seed",$seed,"--peer-seed",$peer,"--scenario",$scen)
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

Write-Host "`n=== (c) Core session (TCP) ==="
$fmt = "  {0,-16} {1,-10} {2,-10} {3}"
Write-Host ($fmt -f "scenario", "rust->go", "go->rust", "")
foreach ($scen in $scenarios) {
  $cells = @{}
  foreach ($orient in @(@("rust","go"), @("go","rust"))) {
    $cell = RunCell $orient[0] $orient[1] $scen
    $cells["$($orient[0])->$($orient[1])"] = $cell
    Tally "core" $cell.pass
  }
  $det = (@($cells.Values | Where-Object { -not $_.pass } | ForEach-Object { $_.detail })) -join " / "
  Write-Host ($fmt -f $scen, (Mark $cells["rust->go"].pass), (Mark $cells["go->rust"].pass), $det)
}

# --- summary -----------------------------------------------------------------------
Write-Host "`n=== Summary ==="
$tp = 0; $tt = 0
foreach ($k in $counts.Keys) {
  Write-Host ("  {0,-14} {1}/{2}" -f $k, $counts[$k][0], $counts[$k][1])
  $tp += $counts[$k][0]; $tt += $counts[$k][1]
}
if ($allPass) { Write-Host "RESULT: $tp/$tt PASS"; exit 0 }
Write-Host "RESULT: FAILURES PRESENT ($tp/$tt)"
exit 1
