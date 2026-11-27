# Installation and setup

Choose a desktop package, a CLI-only server archive, a container or a source checkout. The portable updater applies only to compatible signed portable generations.

## Desktop packages

Select a completed [GitHub Release](https://github.com/KiaroSama/Numera-Browser/releases) with a Windows or Linux x64 or arm64 asset. [Verify its signature and checksum](updates.md#verify-a-versioned-download) before installing or unpacking it. If there is no matching asset, that release has no ready-to-use package for your machine.

### Windows MSI (PowerShell 7)

1. Install or enable the Microsoft Edge WebView2 Runtime if the machine does not have it.
2. Choose the signed MSI for your CPU. From the download directory in PowerShell 7, substitute the filename as needed:

   ```powershell
   $msi = Get-Item -LiteralPath '.\numera-browser-0.37.0-windows-x64.msi'
   Get-AuthenticodeSignature -LiteralPath $msi.FullName
   msiexec.exe /i $msi.FullName
   ```

   Run the last line only after the detached verification succeeds and local policy permits the publisher. The project's signing certificate is self-signed; SmartScreen or local policy may still block it. Do not install an unknown root certificate or disable system protection to make it run.
3. Open Numera Browser from the Start menu. The desktop starts its own loopback manager; it does not need a separate `serve` command.

### Linux AppImage and DEB (Bash)

Linux desktop packages need WebKitGTK 4.1, GTK 3 and the installed browser's system libraries. Check your distribution's package names and version support.

```bash
sudo apt install ./numera-browser-0.37.0-linux-x64.deb
# Or, for AppImage:
chmod +x ./numera-browser-0.37.0-linux-x64.AppImage
./numera-browser-0.37.0-linux-x64.AppImage
```

### Portable desktop

Extract the whole `Numera Browser` folder into a writable location. Keep the engine, resources, notices and manifests beside the shell. Run `.\numera.exe` in PowerShell 7 or `./numera` in Bash from that folder. Closing the window shuts down the owned manager and its sessions.

A complete portable package needs no separate Python or Node installation. The OS still supplies the WebView and system libraries. ARM64 packages do not include vendor browsers. Install only SDK and browser builds that support your target.

Full x64 desktop packages bundle the reviewed Playwright, Patchright and Camoufox SDK/browser generations. CloakBrowser's proprietary browser binary is **not redistributed**: its acquisition is a separate end-user, consent-gated vendor operation, subject to its license and version availability. An installed SDK is not proof that its browser binary is ready.

### First profile

1. Confirm the footer says **Engine online**. This reports the manager service connection. **Demo mode** shows sample profiles in memory.
2. Open **Settings** and choose the default browser backend. Create a profile with a name and the settings required by your authorized test.
3. Launch it from **Profiles**. Close or stop the session before editing resources that require exclusive profile ownership.
4. Open **Settings → Software updates** to check, review a target, approve its download and separately approve installation/restart. See [update safety](updates.md).

`playwright` is the initial default. Explicit engine choices on profile creation or engine-changing edits can update the persisted default; an unrelated notes edit does not. Use Settings or `numera-engine default-engine` to set it deliberately.

### Data and backups

The portable shell resolves data beside its installation when writable, with explicit configuration taking precedence. **Settings → Storage** shows the actual root: use that location, not a guessed folder. Preserve the entire data root for a full backup; profile exports contain configuration, not necessarily browser sessions. `numera-engine backup`/`restore` handle an individual profile and its session.

Stop owned sessions before copying live browser storage. Protect backups like credentials: cookies, proxy credentials and API-token records can grant access. Do not put them into Git, release artifacts or public support attachments. Updates preserve mutable data and external browser overrides; application rollback is not a guarantee that a newer browser's database can be opened by an older browser.

## From source

Requirements: Rust **1.100.0** (pinned by `rust-toolchain.toml`), the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` **0.2.129**. Desktop
builds also require Tauri CLI **2.12.5** and the platform's WebView development
libraries. Complete portable bundles include their vendor driver executables
and serve the prebuilt dashboard.

From the repository root in Bash:

```bash
export PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD=1
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo run --manifest-path dashboard/Cargo.toml --locked --bin dashboard-build
cargo build --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bins
cargo run --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bin numera-engine -- serve --port 8787
```

In PowerShell 7, use the same commands after setting
`$env:PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD = '1'` instead of using `export`. Workflows
set the same value in their build environment.

Open <http://127.0.0.1:8787>. You can manage profiles before installing a browser.
Launching requires the backend's complete, matching vendor resources.

### Explicit browser resources

```bash
cargo run --manifest-path tools/xtask/Cargo.toml --locked -- provision-resources build/vendor-resources
```

This explicitly downloads reviewed platform artifacts with pinned SHA-256 hashes,
extracts only vendor SDK transports, installs each matching browser into its own
cache and inventories licenses/corresponding source. It includes Playwright,
Patchright and Camoufox browsers. Cloak's SDK transport is included; its proprietary
browser requires separate authorized vendor access. Cargo compilation never
silently invokes this acquisition command.

For a source run, set `NUMERA_APP_DIR` to the absolute `build/vendor-resources`
path, `NUMERA_BROWSER_DRIVERS` to its `drivers` directory and
`NUMERA_BROWSER_RUNTIME` to the absolute path of the `numera-browser-runtime`
worker under `engine-rust/target/debug/` (with `.exe` on Windows). For a desktop
override, `NUMERA_ENGINE` points to the `numera-engine` manager executable. Set
an external browser executable through its backend-specific path setting. Run
the same manager command above. Keep resources under this checkout so dashboard
discovery can locate `dashboard/dist`, or place the built dashboard in the selected
application location's `web/` directory. Complete product bundles configure this layout.

Driver pins are Playwright **1.65.1**, Patchright **1.64.3**, Camoufox's Playwright
transport **1.62.0**, and Cloak's Playwright transport **1.65.1**. Camoufox uses
browser **158.0.1-beta.41**, uBlock Origin **1.75.4** and the reviewed GeoIP data.
See [current upstream review](upstream-review-20261114.json) for release/source
compatibility limits. New source-only browser features are not assumed to exist
in an older packaged engine.

Playwright and Cloak use the separately pinned official `playwright-core` npm
archive for their 1.65.1 transport, with Node from the reviewed platform wheel.
Patchright installs its official `patchright-core` 1.64.3 package alongside the
already reviewed platform-specific Node executable.
The published Python 1.65.1 wheels still contain the older 1.65.0 transport;
provisioning checks the extracted package's actual name and version. No Python
or npm installation is needed to unpack or run these resources.

CloakBrowser **156.0.8078.47.1** requires a vendor license key for headless CI.
The [vendor's free key](https://cloakbrowser.dev/free) supports one concurrent
session. Live CI accepts that key when the vendor's Stable release matches the
approved version for the runner. Acquisition verifies the exact release signature
and platform checksum before launching the browser. Store the key as the
repository Actions secret `CLOAKBROWSER_LICENSE_KEY` and keep it out of source and
test reports.

### Dashboard development

Keep the engine running in one terminal, then start the Rust development server:

```sh
cargo run --manifest-path dashboard/Cargo.toml --locked --bin dashboard-dev
```

It serves the local frontend and proxies `/api` to loopback port 8787. A standalone
dashboard distinguishes demo data from a live API connection. An authenticated
API prompts for a token rather than exposing live data as a demo.

### CLI examples

The extracted server bundle exposes `numera-engine` directly; the desktop bundle
keeps it under `engine/`. Add `.exe` on Windows. From source, append these arguments
to the Cargo `numera-engine` invocation shown above.

```sh
numera-engine create qa-profile --os windows --locale en-US --engine playwright
numera-engine list --json
numera-engine launch qa-profile
numera-engine --help
```

The [CLI and MCP guide](agents.md) describes JSON automation. Profile consistency
checks are diagnostics; browser capabilities still differ by backend.

## CLI-only headless server

Download and [verify](updates.md#verify-a-versioned-download) the CLI-only ZIP
for Windows or tar.gz for Linux. Keep both executables in the same directory.
The archive has no desktop, WebView dependency or dashboard assets. Install
compatible vendor resources separately if you need to launch browsers.

PowerShell 7:

```powershell
Expand-Archive -LiteralPath '.\numera-browser-cli-0.37.0-windows-x64.zip' -DestinationPath '.\numera-cli'
Set-Location -LiteralPath '.\numera-cli'
.\numera-engine.exe --help
.\numera-engine.exe --data-dir .\numera-data serve --host 127.0.0.1 --port 8787
```

Bash:

```bash
mkdir -p ./numera-cli
tar -xzf ./numera-browser-cli-0.37.0-linux-x64.tar.gz -C ./numera-cli
cd ./numera-cli
./numera-engine --help
./numera-engine --data-dir ./numera-data serve --host 127.0.0.1 --port 8787
```

In a second terminal, check <http://127.0.0.1:8787/api/ready>. If the port is
in use, choose another port and update your client URL.

## Server and Docker

The Dockerfile uses a native Linux server distribution that includes the dashboard.
Build it on a matching Linux architecture from a clean checkout after provisioning
resources. Set the private `NUMERA_API_TOKEN` in your deployment environment or
ignored `.env` before starting Compose. Keep it out of source and support logs.

```bash
export PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD=1
export GITHUB_SHA="$(git rev-parse HEAD)"
export GITHUB_RUN_ID=1  # Local generation; not release authorization.
cargo run --manifest-path tools/xtask/Cargo.toml --locked -- build-product build/container --resources build/vendor-resources --server-only
docker compose up -d --build
docker compose logs --tail 100
```

`build/container` must not exist before the build. See the [build tooling](../tools/xtask/README.md)
for prerequisites. The image verifies its installation inventory and runs as a
non-root user. Compose publishes `127.0.0.1:8787`, reserves shared memory for browsers
and uses a named volume for `/data`. Keep that volume when rebuilding the image.
Containers and source checkouts use their own deployment process.

## Configuration and access

| Setting | Purpose |
|---|---|
| `--data-dir` / `NUMERA_DATA_DIR` | Explicit data-root selection; confirm the resolved path in Storage. |
| `NUMERA_API_TOKEN` | Bearer authentication. A non-loopback bind requires authentication. Named revocable tokens can also be managed in Settings. |
| `NUMERA_ALLOWED_ORIGINS` | Additional browser origins allowed to call the API, one per line. |
| `PLAYWRIGHT_BROWSERS_PATH` and backend-specific overrides | User-managed browser resources; outside managed replacement. |
| `--verbose` | Additional runtime diagnostics; see the engine's logging documentation. |

Keep the API on localhost. For remote access use an authenticated tunnel or a reverse proxy you control; a bearer token is not a reason to publish a browser-management API directly to the internet. Keep CDP endpoints private too.

### Project-local temp and cache

In the [MCP configuration](../README.md#project-local-mcp-configuration), replace
`<numera-dir>` with a writable absolute path. Set `NUMERA_DATA_DIR` and
`NUMERA_LOG_DIR` for Numera storage, `CLOAKBROWSER_CACHE_DIR` and `XDG_CACHE_HOME`
for cache locations, and `TMP`, `TEMP` and `TMPDIR` to an existing temp directory.
These settings apply to the client process and compatible child tools. In bridge
mode, set equivalent overrides on `serve` itself to control where the server writes.

## Logs and troubleshooting

The native engine writes UTF-8 run logs under the actual data root's `logs/`. Check [engine logging](../engine-rust/README.md) for exact controls. Logs use UTC timestamps and severity/component fields; file logging is diagnostic, console output is concise. Review/redact a log before attaching it to an issue. The native engine prunes its oldest run logs to a bounded retained set; preserve a diagnostic copy privately when investigating an unresolved failure.
| Symptom | Check |
|---|---|
| Demo mode | Start the API, verify its port, then reload. |
| `numera-engine` not found | Use the full extracted binary path or the Cargo invocation above. |
| Browser executable missing | Explicitly provision the selected SDK's matching browser, or use a complete portable bundle. |
| Update unavailable | Check the signed portable identity, supported platform, network, permissions and current update status. Do not bypass signature checks. |
| Busy/deferred update | Finish active sessions/jobs and review again; uncertain ownership must not be force-cleared. |

## Building distributions

[Native CI](../.github/workflows/native-ci.yml) runs automatic independent
Linux and Windows x64 and arm64 builds. Rust tooling produces portable bundles,
CLI-only archives, Linux AppImages and DEBs, and Windows MSIs. Each platform
uploads an installer artifact and a separate portable archive artifact with
distribution checksums.

Unsigned CI artifacts are review outputs. A completed signed release requires its
separate protected signing workflow. See
[build contracts](../tools/xtask/README.md), [licensing](licensing.md) and
[rewrite verification](rust-rewrite.md).
