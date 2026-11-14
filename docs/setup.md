# Installation and setup

Choose one route: a portable desktop release, a source checkout, or a server/container deployment. These are different installations; the portable updater does not rewrite source checkouts or containers.

## Portable desktop

1. Open [GitHub Releases](https://github.com/KiaroSama/Numera-Browser/releases). Select a completed build and its matching platform asset: Windows x64, Linux x64, or macOS ARM64. Do not use an ARM64 archive on an Intel Mac.
2. Download the archive and its published checksums/signature metadata. Verify before extraction; see [updates and verification](updates.md). If the release has no matching asset, it is not a supported ready-build route for that machine.
3. Extract the **whole** `Numera Browser` folder into a writable location. Keep the engine, browser resources, notices and manifests beside the shell; copying only the executable does not install the application.
4. Start `numera.exe` on Windows or `numera` on Linux/macOS. The shell starts its own engine on a free loopback port and opens the dashboard. Close the window to shut down its owned engine.

Python and Node are not prerequisites for a completed portable bundle. The operating system still supplies the WebView and native libraries: WebView2 on Windows, WebKitGTK 4.1/GTK 3 on Linux, and WKWebView on macOS. Browser binaries also need their platform's system libraries. The CI build platform does not guarantee every Linux distribution or older OS release is compatible.

Native publisher signing uses the project's private-trust certificate. It is **self-signed**, not a publicly trusted Windows publisher certificate or Apple Developer ID/notarization. SmartScreen and Gatekeeper can still warn or block the app. Do not install an unknown root certificate or disable system security to make it run. Obtain an appropriately trusted build if local policy requires it.

Playwright, Patchright and Camoufox resources are bundled as compatible SDK/browser generations. CloakBrowser's proprietary browser binary is **not redistributed**: its acquisition is a separate end-user, consent-gated vendor operation, subject to its license and version availability. An installed SDK is not proof that its browser binary is ready.

### First profile

1. Confirm the footer says **Engine online**. **Demo mode** contains sample data, not a connection to your real profiles.
2. Open **Settings** and choose the default browser engine. Create a profile with a name and the settings required by your authorized test.
3. Launch it from **Profiles**. Close or stop the session before editing resources that require exclusive profile ownership.
4. Open **Settings → Software updates** to check, review a target, approve its download and separately approve installation/restart. See [update safety](updates.md).

`playwright` is the initial default. Explicit engine choices on profile creation or engine-changing edits can update the persisted default; an unrelated notes edit does not. Use Settings or `numera-engine default-engine` to set it deliberately.

### Data and backups

The portable shell resolves data beside its installation when writable, with explicit configuration taking precedence. **Settings → Storage** shows the actual root: use that location, not a guessed folder. Preserve the entire data root for a full backup; profile exports contain configuration, not necessarily browser sessions. `numera-engine backup`/`restore` handle an individual profile and its session.

Stop owned sessions before copying live browser storage. Protect backups like credentials: cookies, proxy credentials and API-token records can grant access. Do not put them into Git, release artifacts or public support attachments. Updates preserve mutable data and external browser overrides; application rollback is not a guarantee that a newer browser's database can be opened by an older browser.

## From source

Requirements: Rust **1.99.1** (pinned by `rust-toolchain.toml`), the
`wasm32-unknown-unknown` target, and `wasm-bindgen-cli` **0.2.129**. Desktop builds
also require Tauri **2.12.4** and the platform's WebView development libraries.
The application and its build tools use Cargo. Browser SDKs include their own
vendor Node executable; a separate Python or Node installation is unnecessary.

From the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo run --manifest-path dashboard/Cargo.toml --locked --bin dashboard-build
cargo build --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bins
cargo run --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bin numera-engine -- serve --port 8787
```

Open <http://127.0.0.1:8787>. This supports profile management before installing a
browser. Launching requires that backend's complete, matching vendor resources.
A reported installed SDK still needs its matching browser before launching.

### Explicit browser resources

```sh
cargo xtask provision-resources build/vendor-resources
```

This explicitly downloads reviewed platform artifacts with pinned SHA-256 hashes,
extracts only vendor SDK transports, installs each matching browser into its own
cache and inventories licenses/corresponding source. It includes Playwright,
Patchright and Camoufox browsers. Cloak's SDK transport is included; its proprietary
browser requires separate authorized vendor access. Cargo compilation never
silently invokes this acquisition command.

For a source run, set `NUMERA_APP_DIR` to the absolute `build/vendor-resources`
path, `NUMERA_BROWSER_DRIVERS` to its `drivers` directory and
`NUMERA_BROWSER_RUNTIME` to the absolute native worker built under
`engine-rust/target/debug/` (with `.exe` on Windows). Run the same engine command
above. Keep resources under this checkout so dashboard discovery can locate
`dashboard/dist`, or place the built dashboard in the selected application
location's `web/` directory. Complete product bundles configure this layout.

Driver pins are Playwright **1.65.1**, Patchright **1.64.1**, Camoufox's Playwright
transport **1.62.0**, and Cloak's Playwright transport **1.65.1**. Camoufox uses
browser **158.0.1-beta.41**, uBlock Origin **1.75.4** and the reviewed GeoIP data.
See [current upstream review](upstream-review-20261114.json) for release/source
compatibility limits. New source-only browser features are not assumed to exist
in an older packaged engine.

Playwright and Cloak use the separately pinned official `playwright-core` npm
archive for their 1.65.1 transport, with Node from the reviewed platform wheel.
The published Python 1.65.1 wheels still contain the older 1.65.0 transport;
provisioning checks the extracted package's actual name and version. No Python
or npm installation is needed to unpack or run these resources.

CloakBrowser **156.0.8078.47.1** requires a valid vendor license key, including
for headless CI. The vendor's free tier supports the current binary with one
concurrent session; a paid plan is not required for the basic acceptance test.
Store the key as the repository Actions secret `CLOAKBROWSER_LICENSE_KEY`.
Do not commit it or include it in test reports. The earlier key-free 146 browser
is not a substitute for verification of the current pinned release.

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

## Server and Docker

Use a complete native server package without the desktop WebView dependency.
For a container build, prepare the actual native server distribution first:

```sh
cargo xtask build-product build/container --resources build/vendor-resources --server-only
docker compose up -d --build
docker compose logs --tail 100
```

`build-product` requires a clean checkout and explicit build identity; see
[build tooling](../tools/xtask/README.md). The Dockerfile consumes these verified
native bytes and checks the installation inventory before starting. It runs as
a non-root user, persists `/data`, reserves browser shared memory and publishes
only `127.0.0.1:8787` by default. Set `NUMERA_API_TOKEN` in your private deployment
environment or ignored `.env`; keep that value out of source and support logs.

Do not delete the named data volume to upgrade. Containers and source checkouts
use their reviewed deployment process rather than the portable updater.

## Configuration and access

| Setting | Purpose |
|---|---|
| `--data-dir` / `NUMERA_DATA_DIR` | Explicit data-root selection; confirm the resolved path in Storage. |
| `NUMERA_API_TOKEN` | Bearer authentication. A non-loopback bind requires authentication. Named revocable tokens can also be managed in Settings. |
| `NUMERA_ALLOWED_ORIGINS` | Explicit browser origins allowed to call the API; avoid wildcard exposure. |
| `PLAYWRIGHT_BROWSERS_PATH` and backend-specific overrides | User-managed browser resources; outside managed replacement. |
| `--verbose` | Additional runtime diagnostics; see the engine's logging documentation. |

Keep the API on localhost. For remote access use an authenticated tunnel or a reverse proxy you control; a bearer token is not a reason to publish a browser-management API directly to the internet. Keep CDP endpoints private too.

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
Linux, Windows and macOS compilation and full-product builds. Portable server
archives and desktop installer formats are produced with Rust tooling. Default
installers are Linux AppImage/deb, Windows MSI/NSIS and macOS app/DMG. Each native
platform builds its own artifacts; an operating-system target badge is not a
claim that every older OS is supported.

Unsigned CI artifacts are review outputs. A completed signed release requires its
separate trusted release gates. No release or merge is part of
[PR #76](https://github.com/KiaroSama/Numera-Browser/pull/76). See
[build contracts](../tools/xtask/README.md), [licensing](licensing.md) and
[rewrite verification](rust-rewrite.md).
