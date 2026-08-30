<#
.SYNOPSIS
    Build the portable Numera desktop folder: Tauri shell + frozen Python engine
    + the built dashboard, in one directory you can copy to a flash drive.

.DESCRIPTION
    Windows only. You cannot cross-compile a Tauri app to macOS or Linux from
    here -- those targets need their own build host or a CI runner.

    Output layout (under -OutDir):

        Numera/
          numera.exe               the shell; this is what the user runs
          engine/
            numera-engine.exe      frozen engine (PyInstaller, onedir)
            _internal/             its Python runtime and libraries
              numera/web/          the built dashboard, served by the engine

.PARAMETER OutDir
    Where to assemble. Default: <repo>/build/windows, so the finished app sits at
    build/windows/Numera/numera.exe. Everything under build/ is ignored by git and
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
$stage = Join-Path $OutDir 'Numera'

function Step($text) { Write-Host "==> $text" -ForegroundColor Cyan }

# --- preflight ---------------------------------------------------------------

$dash = Join-Path $repo 'dashboard\dist\index.html'
if (-not (Test-Path $dash)) {
    throw "The dashboard is not built. Run:  npm --prefix `"$repo\dashboard`" run build"
}

# Rust lives on G: on this machine and rustup must not fall back to %USERPROFILE%,
# which would start reinstalling a toolchain onto C:.
if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = 'G:\Program Files\Rust\rustup' }
if (-not $env:CARGO_HOME) { $env:CARGO_HOME = 'G:\Program Files\Rust\cargo' }
$cargo = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
if (-not (Test-Path $cargo)) { throw "cargo not found at $cargo. Set CARGO_HOME." }

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
    & $py -m pip install --quiet --upgrade pip pyinstaller "$repo\engine[api]"
    if ($LASTEXITCODE -ne 0) { throw 'could not install the build dependencies' }

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
    $dataKeep = Join-Path ([System.IO.Path]::GetTempPath()) ("numera-data-" + [guid]::NewGuid().ToString('N'))
    Move-Item $dataDir $dataKeep
    Step "Preserving existing data directory across the rebuild"
}
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
if ($dataKeep) {
    Move-Item $dataKeep $dataDir
    if (-not (Test-Path $dataDir)) { throw "failed to restore the data directory to $dataDir" }
}
Copy-Item $shell -Destination $stage
Copy-Item $frozen -Destination (Join-Path $stage 'engine') -Recurse

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
