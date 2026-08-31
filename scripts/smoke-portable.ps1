<#
.SYNOPSIS
    Prove an assembled portable build actually works, on a machine that has
    never run the app from source.

.DESCRIPTION
    Building an artifact and checking the files are present says nothing about
    whether it runs. This drives the frozen engine the way a user's machine
    would - its own data directory, no developer .venv, no Playwright cache -
    and fails loudly on the first thing that does not work:

      1. the engine answers `--version`
      2. it creates a profile
      3. it launches the *bundled* Chromium and scores the profile
         (`trust` opens a real browser; nothing else here proves the bundle)
      4. `serve` comes up and `/api/ready` answers
      5. after a restart the profile and its session are still there

    Every wait is bounded and polls for a condition. Nothing sleeps blindly.

.PARAMETER Stage
    The assembled folder, e.g. build\windows\Numera.

.PARAMETER TimeoutSeconds
    Ceiling for the whole run.
#>
[CmdletBinding()]
param(
    [string]$Stage,
    [int]$TimeoutSeconds = 300
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $Stage) { $Stage = Join-Path $repo 'build\windows\Numera' }
$Stage = (Resolve-Path $Stage).Path

$deadline = [datetime]::UtcNow.AddSeconds($TimeoutSeconds)
function Remaining { [int][math]::Max(1, ($deadline - [datetime]::UtcNow).TotalSeconds) }
function Step($m) { Write-Host "==> $m" -ForegroundColor Cyan }
function Fail($m) { throw "portable smoke failed: $m" }

$engine = Join-Path $Stage 'engine\numera-engine.exe'
if (-not (Test-Path $engine)) { Fail "no frozen engine at $engine" }

# The bundle is the point of the test: without it the engine serves fine and
# fails at the first launch, which is exactly the defect this exists to catch.
$browsers = Join-Path $Stage 'browsers'
if (-not (Test-Path $browsers)) { Fail "no bundled browser at $browsers" }
$chrome = Get-ChildItem -Path $browsers -Filter 'chrome.exe' -Recurse -File -ErrorAction SilentlyContinue |
    Select-Object -First 1
if (-not $chrome) { Fail "$browsers contains no chrome.exe" }
Step "Bundled browser: $($chrome.FullName)"

# A data directory of its own, and no inherited Playwright cache: this has to
# pass the way it would on a machine that has never seen this project.
$data = Join-Path ([System.IO.Path]::GetTempPath()) ("numera-smoke-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $data | Out-Null
$savedCache = $env:PLAYWRIGHT_BROWSERS_PATH
$env:PLAYWRIGHT_BROWSERS_PATH = $null
$server = $null

function Engine([string[]]$EngineArgs, [int]$Timeout = 120, [switch]$AllowFailure) {
    $out = New-TemporaryFile
    $err = New-TemporaryFile
    try {
        $p = Start-Process -FilePath $engine -PassThru -NoNewWindow `
            -ArgumentList (@('--data-dir', $data) + $EngineArgs) `
            -RedirectStandardOutput $out -RedirectStandardError $err
        if (-not $p.WaitForExit($Timeout * 1000)) {
            try { $p.Kill($true) } catch { }
            Fail "``$($EngineArgs -join ' ')`` did not finish within ${Timeout}s"
        }
        $stdout = Get-Content $out -Raw -ErrorAction SilentlyContinue
        $stderr = Get-Content $err -Raw -ErrorAction SilentlyContinue
        if ($p.ExitCode -ne 0 -and -not $AllowFailure) {
            Fail "``$($EngineArgs -join ' ')`` exited $($p.ExitCode)`n$stdout`n$stderr"
        }
        return $stdout
    } finally {
        Remove-Item $out, $err -Force -ErrorAction SilentlyContinue
    }
}

try {
    Step '1/5 the engine runs at all'
    $version = (Engine @('--version') 30).Trim()
    if (-not $version) { Fail '--version printed nothing' }
    Write-Host "    $version"

    Step '2/5 it creates a profile'
    Engine @('create', 'smoke') 60 | Out-Null
    $listed = Engine @('list') 60
    if ($listed -notmatch 'smoke') { Fail "the profile is not in ``list``:`n$listed" }

    Step '3/5 it launches the bundled browser'
    # `trust` opens a real headless browser and reads back what the page shows.
    # Nothing short of launching proves the bundle resolves.
    # A non-zero exit here means the *profile* scored imperfectly, which is not
    # what this test is about: the question is whether the bundled browser
    # opened at all. A report with real checks in it only exists if it did.
    $trust = Engine @('trust', 'smoke') (Remaining) -AllowFailure
    if ($trust -notmatch '(?i)trust score') {
        Fail "``trust`` produced no report - the browser never opened:`n$trust"
    }
    if ($trust -notmatch '(?i)\[PASS\]') {
        Fail "``trust`` reported no passing check - the page was never inspected:`n$trust"
    }
    Write-Host "    $(($trust -split "`n" | Where-Object { $_.Trim() } | Select-Object -First 3) -join ' | ')"

    Step '4/5 serve comes up and answers /api/ready'
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
    $server = Start-Process -FilePath $engine -PassThru -NoNewWindow `
        -ArgumentList @('--data-dir', $data, 'serve', '--port', $port)
    $ready = $null
    $until = [datetime]::UtcNow.AddSeconds([math]::Min(90, (Remaining)))
    while ([datetime]::UtcNow -lt $until) {
        if ($server.HasExited) { Fail "serve exited with $($server.ExitCode) before answering" }
        try {
            $ready = Invoke-RestMethod -Uri "http://127.0.0.1:$port/api/ready" -TimeoutSec 3
            break
        } catch { Start-Sleep -Milliseconds 250 }   # bounded poll, not a blind wait
    }
    if (-not $ready) { Fail "/api/ready never answered on port $port" }
    if (-not $ready.ok) { Fail "/api/ready answered but not ok: $($ready | ConvertTo-Json -Compress)" }
    Write-Host "    $($ready | ConvertTo-Json -Compress)"

    Step '5/5 the profile and its session survive a restart'
    $server.Kill($true); $server.WaitForExit(15000) | Out-Null; $server = $null
    $again = Engine @('list') 60
    if ($again -notmatch 'smoke') { Fail "the profile did not survive the restart:`n$again" }
    $sessions = Get-ChildItem -Path (Join-Path $data 'user-data') -Directory -ErrorAction SilentlyContinue
    if (-not $sessions) { Fail 'the launch left no session directory behind' }

    Write-Host ''
    Write-Host 'PORTABLE SMOKE: PASS' -ForegroundColor Green
} finally {
    if ($server -and -not $server.HasExited) {
        try { $server.Kill($true) } catch { }
    }
    $env:PLAYWRIGHT_BROWSERS_PATH = $savedCache
    Remove-Item $data -Recurse -Force -ErrorAction SilentlyContinue
}
