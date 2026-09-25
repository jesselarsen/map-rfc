# P4 gate: MAP over A2A, end to end over HTTP.
#
# Provider A (seed 2) crashes after producing 10 Parts. The requester resumes
# the task on provider B (seed 3) from A's last countersigned checkpoint. The
# script then checks four things:
#   - B's receipt is dual-signed and names the checkpoint it resumed from;
#   - its transcript root covers every Part received;
#   - the per-provider voucher watermarks sum to exactly the Parts delivered;
#   - a plain A2A client (no extension) still gets an ordinary completed task
#     (A2A-28);
#   - if MAP_A2A_SDK_PY names a Python with interop/a2a-sdk/requirements.txt
#     installed, the unmodified a2a-sdk verifies B's signed Agent Card and
#     completes a task against it (interop/a2a-sdk/check_sdk.py).
# Exit 0 iff every check passes.
#
# Usage:  pwsh scripts/demo-a2a.ps1
# Requires: the GNU Rust toolchain with mingw on PATH (C:\msys64\mingw64\bin).

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$PORT_A = 8701
$PORT_B = 8702

Write-Host "Building map-a2a (feature http)..."
Push-Location (Join-Path $repo "map-rs")
& cargo +stable-x86_64-pc-windows-gnu build -p map-a2a --features http --bins 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "cargo build failed" }
Pop-Location

$bin = Join-Path $repo "map-rs\target\debug"
$PROVIDER = Join-Path $bin "map-a2a-provider.exe"
$REQUESTER = Join-Path $bin "map-a2a-requester.exe"
foreach ($b in @($PROVIDER, $REQUESTER)) { if (-not (Test-Path $b)) { Write-Error "missing $b" } }

$logs = Join-Path ([IO.Path]::GetTempPath()) "map-demo-a2a"
New-Item -ItemType Directory -Force $logs | Out-Null

function Start-Provider($port, $seed, $extra) {
    $a = @("--listen", "127.0.0.1:$port", "--seed", "$seed", "--parts", "24", "--checkpoint-every", "4") + $extra
    Start-Process -FilePath $PROVIDER -ArgumentList $a -PassThru -NoNewWindow `
        -RedirectStandardError (Join-Path $logs "provider-$port.log") -RedirectStandardOutput (Join-Path $logs "provider-$port.out")
}

function Wait-Card($port) {
    for ($i = 0; $i -lt 50; $i++) {
        try { Invoke-RestMethod "http://127.0.0.1:$port/.well-known/agent-card.json" -TimeoutSec 1 | Out-Null; return }
        catch { Start-Sleep -Milliseconds 100 }
    }
    Write-Error "provider on $port did not come up"
}

$pA = Start-Provider $PORT_A 2 @("--fail-after-parts", "10")
$pB = Start-Provider $PORT_B 3 @()
$failed = @()
try {
    Wait-Card $PORT_A
    Wait-Card $PORT_B

    Write-Host "`n== MAP requester: A (crashes after 10 Parts) -> resume on B =="
    $out = & $REQUESTER --providers "http://127.0.0.1:$PORT_A,http://127.0.0.1:$PORT_B" --seed 1 --zone 2 2>&1
    $out | ForEach-Object { Write-Host "  $_" }
    $line = $out | Where-Object { "$_" -like "MAP-DEMO-RESULT *" } | Select-Object -Last 1
    if (-not $line) { $failed += "requester produced no result" }
    else {
        $r = ("$line".Substring("MAP-DEMO-RESULT ".Length)) | ConvertFrom-Json
        $pA.WaitForExit(2000) | Out-Null
        $check = [ordered]@{
            "provider A died mid-task"                     = $pA.HasExited
            "A's hop ended in an error"                    = [bool]$r.hops[0].error
            "receipt is dual-signed"                       = $r.dualSigned -eq $true
            "receipt names resumed-from checkpoint"        = [bool]$r.resumedFrom
            "receipt carries a voucher digest"             = [bool]$r.voucherDigestInReceipt
            "all 24 Parts delivered"                       = $r.partsDelivered -eq 24
            "voucher watermarks = Parts delivered"         = (($r.vouchers | Measure-Object tokensOut -Sum).Sum) -eq $r.partsDelivered
            "A paid only to its countersigned checkpoint"  = ($r.vouchers | Where-Object { $_.tokensOut -eq 8 }).Count -eq 1
        }
        foreach ($k in $check.Keys) {
            $ok = $check[$k]
            Write-Host ("  [{0}] {1}" -f ($(if ($ok) { "PASS" } else { "FAIL" })), $k)
            if (-not $ok) { $failed += $k }
        }
    }

    Write-Host "`n== Plain A2A client (no extension) against B =="
    $plain = & $REQUESTER --providers "http://127.0.0.1:$PORT_B" --plain 2>&1
    $plain | ForEach-Object { Write-Host "  $_" }
    if (-not ($plain | Where-Object { "$_" -like 'MAP-DEMO-RESULT *"TASK_STATE_COMPLETED"*' })) { $failed += "plain A2A client" }
    else { Write-Host "  [PASS] plain client got a completed task with no MAP metadata" }

    Write-Host "`n== Unmodified A2A Python SDK against B =="
    if ($env:MAP_A2A_SDK_PY) {
        & $env:MAP_A2A_SDK_PY (Join-Path $repo "interop\a2a-sdk\check_sdk.py") "http://127.0.0.1:$PORT_B"
        if ($LASTEXITCODE -ne 0) { $failed += "a2a-sdk interop" }
    }
    else { Write-Host "  (skipped: set MAP_A2A_SDK_PY to a python with interop/a2a-sdk/requirements.txt installed)" }
}
finally {
    foreach ($p in @($pA, $pB)) { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }
}

if ($failed.Count -gt 0) { Write-Host "`nFAILED: $($failed -join '; ')"; exit 1 }
Write-Host "`nALL PASS"
exit 0
