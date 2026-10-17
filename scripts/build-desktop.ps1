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

.PARAMETER IncludeCloakBinary
    Include the unmodified Cloak browser for local/internal use. Do not distribute
    this output without the vendor license required for third-party redistribution.

.EXAMPLE
    pwsh -File scripts/build-desktop.ps1
#>
[CmdletBinding()]
param(
    [string]$OutDir,
    [switch]$SkipFreeze,
    [switch]$IncludeCloakBinary,
    [switch]$PrebuiltShell
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
               "(22.22.2+, 24.15+, or 26+, per dashboard/package.json), or build it yourself " +
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
    $releaseRequirements = Join-Path $repo 'scripts\build\requirements.txt'
    & $py -m pip install --quiet --upgrade pip -r $releaseRequirements "$repo\engine[api,launch,secure,desktop]"
    if ($LASTEXITCODE -ne 0) { throw 'could not install the build dependencies' }

    Step 'Preparing the desktop browsers and GeoIP data'
    # bundle-browsers.py runs playwright install chromium and Patchright's own
    # installer with separate PLAYWRIGHT_BROWSERS_PATH caches.
    $bundleArgs = @('--output', (Join-Path $build 'all-browsers'),
                    '--cache', (Join-Path $build 'browser-downloads'))
    if ($IncludeCloakBinary) { $bundleArgs += '--include-cloak-binary' }
    & $py (Join-Path $repo 'scripts\bundle-browsers.py') @bundleArgs
    if ($LASTEXITCODE -ne 0) { throw 'could not prepare the desktop browser bundle' }

    # The committed one, shared with the POSIX job. Generating it here meant two
    # definitions of the same thing, and the other one was wrong - it froze
    # `engine/numera/__main__.py`, whose relative import cannot survive being
    # the PyInstaller entry.
    $entry = Join-Path $repo 'scripts\engine_entry.py'
    if (-not (Test-Path $entry)) { throw "missing the freeze entry point at $entry" }
    $identity = Join-Path $build 'identity'
    & $py (Join-Path $repo 'scripts\build\identity.py') prepare --directory $identity
    if ($LASTEXITCODE -ne 0) { throw 'could not prepare the embedded build identity' }

    # onedir, not onefile. A onefile bootloader that gets TerminateProcess'd --
    # which is exactly how the shell stops the sidecar -- leaves its extracted
    # _MEI temp directory behind every single run.
    & $py -m PyInstaller --noconfirm --clean --onedir --console `
        --name numera-engine `
        --distpath (Join-Path $build 'dist') `
        --workpath (Join-Path $build 'work') `
        --specpath $build `
        --paths $identity --version-file (Join-Path $identity 'windows-version.txt') `
        --icon (Join-Path $repo 'desktop\src-tauri\icons\icon.ico') `
        --add-data "$repo\dashboard\dist;numera/web" `
        --collect-all uvicorn `
        --collect-all playwright --collect-all patchright `
        --collect-all camoufox --collect-all cloakbrowser `
        --collect-all browserforge --collect-all apify_fingerprint_datapoints `
        --collect-all language_tags --collect-all ua_parser `
        --collect-all screeninfo --collect-all maxminddb `
        --recursive-copy-metadata camoufox --recursive-copy-metadata cloakbrowser `
        --copy-metadata patchright --copy-metadata playwright `
        --collect-submodules numera `
        $entry
    if ($LASTEXITCODE -ne 0) { throw 'PyInstaller failed' }
}

# --- 2. build the shell ------------------------------------------------------

if (-not $PrebuiltShell) {
    Step 'Building the Tauri shell (cargo build --release --locked)'
    # Cargo discovers +crt-static from this working directory, not --manifest-path.
    Push-Location (Join-Path $repo 'desktop\src-tauri')
    try {
        & $cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    } finally { Pop-Location }
} else {
    Step 'Using the shell already built by the hosted verification job'
}
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
Copy-Item -LiteralPath (Join-Path $repo 'docs/browser-notices.txt') -Destination (Join-Path $stage 'BROWSER-NOTICES.txt')
foreach ($notice in @('LICENSE', 'docs/licensing.md', 'docs/browser-notices.txt',
                      'docs/migration-random-notices.txt',
                      'engine-rust/src/http/documentation-assets/NOTICE.md')) {
    $sourceNotice = Join-Path $repo $notice
    if (-not (Test-Path -LiteralPath $sourceNotice -PathType Leaf)) {
        throw "Required distribution license/notice is missing: $notice"
    }
    $targetNotice = Join-Path $stage $notice
    New-Item -ItemType Directory -Force -Path (Split-Path $targetNotice -Parent) | Out-Null
    Copy-Item -LiteralPath $sourceNotice -Destination $targetNotice
}

# The manifest selects each vendor's executable, including when Chromium revision
# labels collide. Never ship a cached internal Cloak binary by accident.
$stageBrowsers = Join-Path $stage 'browsers'
$cachedBrowsers = Join-Path $build 'all-browsers'
$manifestPath = Join-Path $cachedBrowsers 'manifest.json'
if (-not (Test-Path $manifestPath)) { throw 'Browser manifest missing; build without -SkipFreeze' }
$manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
if (-not $IncludeCloakBinary) { $manifest.PSObject.Properties.Remove('cloak') }
if ($IncludeCloakBinary -and -not $manifest.PSObject.Properties['cloak']) {
    throw 'The cached bundle has no Cloak binary; build without -SkipFreeze'
}
New-Item -ItemType Directory -Force -Path $stageBrowsers | Out-Null
$folders = @('playwright', 'patchright', 'camoufox', 'cloak-data')
if ($IncludeCloakBinary) { $folders += 'cloak' }
foreach ($folder in $folders) {
    Copy-Item -LiteralPath (Join-Path $cachedBrowsers $folder) -Destination $stageBrowsers -Recurse
}
[IO.File]::WriteAllText((Join-Path $stageBrowsers 'manifest.json'), ($manifest | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
foreach ($engineName in @('playwright', 'patchright', 'camoufox')) {
    $browserExe = Join-Path $stageBrowsers $manifest.$engineName.executable
    if (-not (Test-Path -LiteralPath $browserExe)) { throw "Bundled $engineName executable is missing" }
}
# The Chromium executables are chrome.exe; the manifest also validates Camoufox.

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
