<#
.SYNOPSIS
    Build the server / CLI artifact: the engine and its dashboard, with no
    desktop shell and no bundled browser.

.DESCRIPTION
    This is the headless target. A server operator should never download a GUI
    shell or a browser binary they did not ask for, so this produces a plain
    Python wheel plus an sdist and nothing else.

    **The wheel is platform-independent** (py3-none-any). One build here runs on
    Linux, macOS and Windows alike, because the engine is pure Python -- unlike
    the desktop shell, which is a native binary per platform and cannot be
    cross-compiled. That is the whole reason these are two scripts.

    Installing it on a Linux server:

        python -m pip install numera_engine-<version>-py3-none-any.whl[api,launch]
        numera serve --host 0.0.0.0 --port 8787

    Browser engines stay opt-in: nothing here downloads Chromium. A server that
    only drives an already-running browser over CDP (`numera attach`) needs none.

.PARAMETER OutDir
    Where to put the artifacts. Default: <repo>/build/server, which is ignored.

.EXAMPLE
    pwsh -File scripts/build-server.ps1
#>
[CmdletBinding()]
param(
    [string] $OutDir
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $repo 'build\server' }

$python = Join-Path $repo '.venv\Scripts\python.exe'
if (-not (Test-Path $python)) { $python = 'python' }

# Setuptools discovers the full staged LICENSE without making an editable source
# install depend on a generated file. The same helper stages independent notices.
$stagedLicense = Join-Path $repo 'engine\LICENSE'
$stagedLegal = Join-Path $repo 'engine\numera\legal'
if ((Test-Path $stagedLicense) -or (Test-Path $stagedLegal)) {
    throw 'Distribution staging already exists; inspect it before building again'
}
Write-Output "==> Building the dashboard (the engine serves it)"
Push-Location (Join-Path $repo 'dashboard')
try {
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw "dashboard build failed ($LASTEXITCODE)" }
} finally { Pop-Location }

# The engine serves whatever sits at numera/web, so the built dashboard has to
# be inside the package before the wheel is made, not alongside it.
$web = Join-Path $repo 'engine\numera\web'
if (Test-Path $web) { Remove-Item $web -Recurse -Force }
Copy-Item (Join-Path $repo 'dashboard\dist') $web -Recurse
Write-Output "    dashboard staged at engine/numera/web"

Write-Output "==> Building the wheel and sdist"
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
Push-Location (Join-Path $repo 'engine')
try {
    & $python (Join-Path $repo 'scripts\build\distribution.py') stage-wheel
    if ($LASTEXITCODE -ne 0) { throw 'could not stage complete distribution notices' }
    & $python -m pip install --quiet -r (Join-Path $repo 'scripts\build\requirements.txt')
    if ($LASTEXITCODE -ne 0) { throw "could not install the build backend" }
    & $python -m build --outdir $OutDir
    if ($LASTEXITCODE -ne 0) { throw "build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
    # Staged copy, not source: leaving it behind would make the next `git status`
    # noisy and the next wheel stale.
    if (Test-Path $web) { Remove-Item $web -Recurse -Force }
    & $python (Join-Path $repo 'scripts\build\distribution.py') clear-wheel-staging
    if ($LASTEXITCODE -ne 0) { throw 'could not safely clear distribution staging' }
}

Write-Output ''
Get-ChildItem $OutDir | ForEach-Object {
    "{0,-52} {1,8:N0} KB" -f $_.Name, ($_.Length / 1KB)
}

# Prove the wheel is actually portable rather than assuming it: a wheel tagged
# for one platform would install nowhere else, and the tag is the only evidence.
$wheel = Get-ChildItem $OutDir -Filter '*.whl' | Select-Object -First 1
if (-not $wheel) { throw 'no wheel was produced' }
if ($wheel.Name -notmatch 'py3-none-any') {
    throw "wheel is not platform-independent: $($wheel.Name)"
}
Write-Output ''
Write-Output "Platform-independent wheel: $($wheel.Name)"
Write-Output "Install on any OS with:  python -m pip install `"$($wheel.FullName)`"[api,launch]"
