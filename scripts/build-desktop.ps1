<#
.SYNOPSIS
    Build the portable Numera Browser desktop folder: Tauri shell + frozen Python engine
    + the built dashboard, in one directory you can copy to a flash drive.

.DESCRIPTION
    Windows only. You cannot cross-compile a Tauri app to macOS or Linux from
    here -- those targets need their own build host or a CI runner.

    Output layout (under -OutDir):

        Numera Browser/
          numera.exe               the shell; this is what the user runs
          engine/
            numera-engine.exe      frozen engine (PyInstaller, onedir)
            _internal/             its Python runtime and libraries
              numera/web/          the built dashboard, served by the engine

.PARAMETER OutDir
    Where to assemble. Default: <repo>/build/windows, so the finished app sits at
    build/windows/Numera Browser/numera.exe. Everything under build/ is ignored by git
    and
    is what gets attached to a GitHub Release.

.PARAMETER SkipFreeze
    Reuse the engine already frozen in the build cache. Saves a couple of
    minutes when only the Rust side changed.

.EXAMPLE
    pwsh -File scripts/build-desktop.ps1
#>
[CmdletBinding()]
param(
    [string]$OutDir,
    [switch]$SkipFreeze
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
# Source and builds stay apart: everything produced lands under build/, one
# directory per platform, and nothing there is ever edited by hand. That is also
# what gets attached to a GitHub Release.
if (-not $OutDir) { $OutDir = Join-Path $repo 'build\windows' }
$build = Join-Path $repo 'build\.cache'
$stage = Join-Path $OutDir 'Numera Browser'

function Step($text) { Write-Host "==> $text" -ForegroundColor Cyan }

# --- preflight ---------------------------------------------------------------

# The dashboard is a build input, so this builds it. It used to demand that
# `dashboard/dist/index.html` already existed - and `dashboard/dist` is
# git-ignored, so a fresh clone could never satisfy it. Both the Windows CI job
# and the README tell you to run this script directly, and both were broken from
# a clean checkout for exactly that reason.
$dash = Join-Path $repo 'dashboard\dist\index.html'
if (-not (Test-Path $dash)) {
    Step 'Building the dashboard (no dashboard/dist yet)'
    $npm = (Get-Command npm -ErrorAction SilentlyContinue)
    if (-not $npm) {
        throw ("the dashboard is not built and npm is not on PATH. Install Node.js " +
               "(>=20.19 or >=22.12, per dashboard/package.json), or build it yourself " +
               "with:  npm --prefix `"$repo\dashboard`" ci; npm --prefix `"$repo\dashboard`" run build")
    }
    # `ci` not `install`: deterministic from the committed lockfile, which is
    # what a release build has to be.
    & npm --prefix "$repo\dashboard" ci
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed for the dashboard' }
    & npm --prefix "$repo\dashboard" run build
    if ($LASTEXITCODE -ne 0) { throw 'npm run build failed for the dashboard' }
}
if (-not (Test-Path $dash)) {
    throw "the dashboard build reported success but $dash is missing"
}

# Resolve cargo from the environment or PATH. This used to hardcode one
# machine's G:\ install, so the script only ever worked there - and worse, it
# set RUSTUP_HOME/CARGO_HOME to paths that do not exist elsewhere, which makes
# rustup start installing a toolchain rather than fail.
$cargo = $null
if ($env:CARGO_HOME) {
    $candidate = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
    if (Test-Path $candidate) { $cargo = $candidate }
}
if (-not $cargo) {
    $onPath = Get-Command cargo -ErrorAction SilentlyContinue
    if ($onPath) { $cargo = $onPath.Source }
}
if (-not $cargo) {
    throw @'
cargo was not found.
  Install Rust (https://rustup.rs) and make sure `cargo` is on PATH, or set
  CARGO_HOME to the directory containing bin\cargo.exe.
'@
}
# Only pin RUSTUP_HOME when CARGO_HOME was given and a sibling rustup exists;
# guessing it is what caused the toolchain to be reinstalled somewhere else.
if ($env:CARGO_HOME -and -not $env:RUSTUP_HOME) {
    $maybe = Join-Path (Split-Path $env:CARGO_HOME -Parent) 'rustup'
    if (Test-Path $maybe) { $env:RUSTUP_HOME = $maybe }
}

# --- 1. freeze the engine ----------------------------------------------------

$frozen = Join-Path $build 'dist\numera-engine'
if ($SkipFreeze) {
    if (-not (Test-Path (Join-Path $frozen 'numera-engine.exe'))) {
        throw "-SkipFreeze given but no frozen engine in $frozen"
    }
    Step 'Reusing the frozen engine'
} else {
    Step 'Freezing the engine (PyInstaller)'
    New-Item -ItemType Directory -Force -Path $build | Out-Null

    # A build venv of its own: PyInstaller must see the same packages the frozen
    # engine will need, and the project .venv should not gain build-only tools.
    $venv = Join-Path $build 'venv'
    $py = Join-Path $venv 'Scripts\python.exe'
    if (-not (Test-Path $py)) {
        $host_py = Join-Path $repo '.venv\Scripts\python.exe'
        if (-not (Test-Path $host_py)) { $host_py = 'python' }
        & $host_py -m venv $venv
        if ($LASTEXITCODE -ne 0) { throw 'could not create the build venv' }
    }
    # `secure` too. `cryptography` lives in that extra, and a frozen portable app
    # cannot ask the user to pip-install into its embedded runtime - so shipping
    # without it meant `numera vault enable` was unavailable in exactly the build
    # where the encrypted vault is advertised as built in.
    & $py -m pip install --quiet --upgrade pip pyinstaller "$repo\engine[api,launch,secure]"
    if ($LASTEXITCODE -ne 0) { throw 'could not install the build dependencies' }

    # `pip install playwright` installs the package, not the browser: that is a
    # separate download into a per-user cache. A portable build has no such
    # cache on a machine that has never run Playwright, so without this the app
    # starts, serves the dashboard and fails at the first launch with
    # "Executable doesn't exist". Cached in the build folder, so a rebuild does
    # not re-download it.
    Step 'Fetching the Playwright browser'
    $browsers = Join-Path $build 'browsers'
    $previousBrowsers = $env:PLAYWRIGHT_BROWSERS_PATH
    $env:PLAYWRIGHT_BROWSERS_PATH = $browsers
    try {
        & $py -m playwright install chromium
        if ($LASTEXITCODE -ne 0) { throw 'could not download the Playwright browser' }
    } finally { $env:PLAYWRIGHT_BROWSERS_PATH = $previousBrowsers }
    $chromium = Get-ChildItem -Path $browsers -Filter 'chromium-*' -Directory -ErrorAction SilentlyContinue
    if (-not $chromium) { throw "playwright install reported success but $browsers has no chromium-* folder" }
    $chromeExe = Get-ChildItem -Path $browsers -Filter 'chrome.exe' -Recurse -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $chromeExe) { throw "the downloaded browser has no chrome.exe - the bundle would not launch" }

    $entry = Join-Path $build 'engine_entry.py'
    Set-Content -Path $entry -Encoding utf8 -Value @'
"""PyInstaller entry point: the numera CLI, nothing else."""
import sys

from numera.cli import main

sys.exit(main())
'@

    # onedir, not onefile. A onefile bootloader that gets TerminateProcess'd --
    # which is exactly how the shell stops the sidecar -- leaves its extracted
    # _MEI temp directory behind every single run.
    & $py -m PyInstaller --noconfirm --clean --onedir --console `
        --name numera-engine `
        --distpath (Join-Path $build 'dist') `
        --workpath (Join-Path $build 'work') `
        --specpath $build `
        --add-data "$repo\dashboard\dist;numera/web" `
        --collect-all uvicorn `
        --collect-submodules numera `
        $entry
    if ($LASTEXITCODE -ne 0) { throw 'PyInstaller failed' }
}

# --- 2. build the shell ------------------------------------------------------

Step 'Building the Tauri shell (cargo build --release)'
# From the crate directory, not via --manifest-path: cargo discovers
# .cargo/config.toml by walking up from the working directory, and that file
# carries +crt-static. Build from elsewhere and the shell silently regains its
# VCRUNTIME140 dependency.
Push-Location (Join-Path $repo 'desktop\src-tauri')
try {
    & $cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally { Pop-Location }
$shell = Join-Path $repo 'desktop\src-tauri\target\release\numera.exe'
if (-not (Test-Path $shell)) { throw "cargo reported success but $shell is missing" }

# Prove the static CRT actually took. Import-table names are plain ASCII in the
# PE, so this needs no toolchain, and it fails loudly rather than shipping a
# folder that dies on a machine with no VC++ redistributable.
$bytes = [IO.File]::ReadAllBytes($shell)
$ascii = [Text.Encoding]::ASCII.GetString($bytes)
if ($ascii -match 'VCRUNTIME140') {
    throw "$shell still imports VCRUNTIME140 -- .cargo/config.toml (+crt-static) did not apply."
}

# --- 3. assemble -------------------------------------------------------------

Step "Assembling $stage"
# The app is portable: run it from here and it writes profiles, sessions, proxies
# and logs into <stage>\data. Wiping the whole folder to rebuild would therefore
# destroy real user data - so `data` is carried across the rebuild rather than
# recreated. Nothing else survives: everything outside `data` is build output.
$dataDir = Join-Path $stage 'data'
$dataKeep = $null
if (Test-Path $dataDir) {
    # Beside the build output, not in the system temp directory, and under a
    # name a person can recognise. If this script is killed mid-rebuild - Ctrl-C,
    # a power cut, an antivirus lock - the profiles are findable instead of
    # sitting in %TEMP% under a GUID nobody will ever look for.
    # A sibling of the stage, derived from $stage alone - the assemble block is
    # also run in isolation by the regression test, which binds $stage and
    # nothing else.
    $dataKeep = "$stage.data-preserved"
    if (Test-Path $dataKeep) {
        throw ("$dataKeep already exists, which means an earlier build was interrupted " +
               "after moving the data out. That folder holds your profiles: move it back " +
               "to `"$dataDir`" before building again.")
    }
    Move-Item $dataDir $dataKeep
    Step "Preserving existing data directory across the rebuild ($dataKeep)"
}

# From here to the restore, the user's profiles live in $dataKeep and nowhere
# else. Every exit path has to put them back - the previous version only handled
# the one failure it could name (a locked stage folder), so any other error
# between here and the restore left real data orphaned under a temporary name
# and the next build started from an empty data directory.
$restored = $false
function Restore-Data {
    if ($script:dataKeep -and -not $script:restored -and (Test-Path $script:dataKeep)) {
        New-Item -ItemType Directory -Force -Path $script:stage | Out-Null
        Move-Item $script:dataKeep $script:dataDir
        $script:restored = $true
    }
}

trap {
    # Anything from here on - a failed copy, a locked file, Ctrl-C - puts the
    # profiles back before the error propagates. Defined after `Restore-Data`
    # so the handler always has something to call.
    try { Restore-Data } catch { Write-Warning "could not restore the data directory: $_" }
    break
}

$clearFailed = $null
if (Test-Path $stage) {
    try { Remove-Item $stage -Recurse -Force -ErrorAction Stop }
    catch { $clearFailed = $_ }
    if (-not $clearFailed -and (Test-Path $stage)) { $clearFailed = "the folder is still there" }
}
if ($clearFailed) {
    Restore-Data
    throw ("could not clear $stage - close Numera Browser if it is running from there, then " +
           "build again. Continuing would ship the previous build. ($clearFailed)")
}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
if ($dataKeep) {
    Restore-Data
    if (-not (Test-Path $dataDir)) { throw "failed to restore the data directory to $dataDir" }
    Step "Restored the data directory"
}
Copy-Item $shell -Destination $stage
Copy-Item $frozen -Destination (Join-Path $stage 'engine') -Recurse

# A sibling of `engine`, not a child: `paths.use_bundled_browsers()` looks in
# both, and keeping the 150MB out of the frozen folder means re-freezing the
# engine does not mean re-copying the browser.
$stageBrowsers = Join-Path $stage 'browsers'
$cachedBrowsers = Join-Path $build 'browsers'
# The cache survives -SkipFreeze, which is the point of caching it there.
if (Test-Path $cachedBrowsers) {
    New-Item -ItemType Directory -Force -Path $stageBrowsers | Out-Null
    # Everything except the headless shell. `playwright install chromium` also
    # fetches `chromium_headless_shell`, 272MB that this app never runs: the
    # launcher passes channel="chromium" for headless precisely to avoid it -
    # the shell cannot load extensions and announces itself as HeadlessChrome
    # over CDP. Shipping it would be a third of the download for a binary the
    # product refuses to use.
    Get-ChildItem -Path $cachedBrowsers -Directory |
        Where-Object { $_.Name -notlike 'chromium_headless_shell-*' } |
        ForEach-Object { Copy-Item $_.FullName -Destination $stageBrowsers -Recurse }
}
if (-not (Test-Path $stageBrowsers)) {
    throw ("$stageBrowsers is missing - the portable build cannot launch a browser. " +
           "Build once without -SkipFreeze to download it.")
}
if (-not (Get-ChildItem -Path $stageBrowsers -Filter 'chrome.exe' -Recurse -File -ErrorAction SilentlyContinue |
          Select-Object -First 1)) {
    throw "$stageBrowsers has no chrome.exe - the portable build cannot launch a browser"
}

# The shell resolves <its own dir>/engine/numera-engine.exe. If that moved,
# every build would silently fall back to a source checkout's .venv instead.
$sidecar = Join-Path $stage 'engine\numera-engine.exe'
if (-not (Test-Path $sidecar)) { throw "assembled folder has no engine at $sidecar" }
if (-not (Test-Path (Join-Path $stage 'engine\_internal\numera\web\index.html'))) {
    throw 'the frozen engine does not contain the dashboard build'
}

$mb = [math]::Round((Get-ChildItem $stage -Recurse -File | Measure-Object Length -Sum).Sum / 1MB, 1)
Write-Host ''
Write-Host "Portable folder: $stage  ($mb MB)" -ForegroundColor Green
Write-Host "Run it with:     `"$stage\numera.exe`""
