<p align="center">
  <img src="docs/brand/Logo.png" width="240" alt="Numera Browser logo" />
</p>

# Numera Browser

<div align="center">

[![Native CI](https://img.shields.io/github/actions/workflow/status/KiaroSama/Numera-Browser/native-ci.yml?style=flat-square&color=2563eb)](https://github.com/KiaroSama/Numera-Browser/actions/workflows/native-ci.yml)
[![License: noncommercial](https://img.shields.io/badge/License-noncommercial-64748b?style=flat-square)](LICENSE)
[![Version](https://img.shields.io/github/v/tag/KiaroSama/Numera-Browser?style=flat-square&color=16a34a&label=Version)](https://github.com/KiaroSama/Numera-Browser/tags)
[![Latest release](https://img.shields.io/github/v/release/KiaroSama/Numera-Browser?style=flat-square&color=a21caf)](https://github.com/KiaroSama/Numera-Browser/releases)
[![Native tests](https://img.shields.io/github/actions/workflow/status/KiaroSama/Numera-Browser/rust-candidate.yml?style=flat-square&color=65a30d)](#development-and-verification)
[![Rust 1.100.0](https://img.shields.io/badge/Rust-1.100.0-8b5cf6?style=flat-square)](rust-toolchain.toml)
[![Windows and Linux x64/arm64; macOS arm64](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-0891b2?style=flat-square)](#install)
[![MSI, AppImage, DEB and CLI-only](https://img.shields.io/badge/Installer-MSI%20%7C%20AppImage%20%7C%20DEB%20%7C%20CLI--only-ea580c?style=flat-square)](#install)
[![Documentation](https://img.shields.io/badge/Docs-setup-0d9488?style=flat-square)](docs/setup.md)
[![Repository size](https://img.shields.io/github/repo-size/KiaroSama/Numera-Browser?style=flat-square&color=ca8a04)](https://github.com/KiaroSama/Numera-Browser)
[![Architecture](https://img.shields.io/badge/Architecture-native-475569?style=flat-square)](docs/architecture.md)
[![Built with Tauri and Rust/WASM](https://img.shields.io/badge/Built%20with-Tauri%20%7C%20Rust%2FWASM-c2410c?style=flat-square)](dashboard/README.md)
[![MCP: stdio and HTTP](https://img.shields.io/badge/MCP-stdio%20%7C%20HTTP-1d4ed8?style=flat-square)](#mcp-integration)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a?style=flat-square)](#donate)

</div>

Numera Browser is a local browser profile manager. Version 0.37.0 has a Rust engine, CLI,
Axum HTTP API, MCP server and Rust/WASM dashboard in a Tauri desktop window. It runs
compatible Chromium or Firefox builds through the selected browser driver.

## Key features

| Feature | What it does |
| --- | --- |
| Profiles and fingerprints | Isolate sessions, cookies, proxy settings and browser identity per profile. Generate coherent fingerprints, check for collisions and organize profiles with folders and tags. Profile MAC address spoofing supports locally administered unicast addresses and does not change the host network interface. |
| Proxy pool | Import, test and assign proxies from a shared pool. Checks report the exit IP and country, with a flag when the country is known. |
| Batch work and migration | Create, edit, launch or stop multiple profiles in the dashboard. Import GoLogin, AdsPower and Multilogin JSON exports. Import and export cookies, with JSON and Netscape export formats. |
| Credential storage | The optional encrypted vault protects stored secrets such as proxy passwords. Cookie exports and profile backups still need private storage. |
| Browser drivers | Playwright, Patchright, Camoufox and Cloak have separate registrations in the Rust `DriverRegistry`. |
| Desktop and updates | The dashboard has light and dark themes and respects reduced motion. Signed portable app updates can include matched browser updates; preferences control automatic installation. |

## Install

Download only assets listed under [Releases & Tags](#releases-and-tags). Pick the
asset that matches the operating system and CPU. Verify it before running or
extracting it. A CI artifact is not a signed release.

| Target | Desktop packages | Headless server |
| --- | --- | --- |
| Windows x64 (`windows-x64`) | MSI or portable ZIP | CLI-only ZIP |
| Windows arm64 (`windows-arm64`) | MSI or portable ZIP | CLI-only ZIP |
| Linux x64 (`linux-x64`) | AppImage, DEB or portable ZIP | CLI-only tar.gz |
| Linux arm64 (`linux-arm64`) | AppImage, DEB or portable ZIP | CLI-only tar.gz |
| macOS arm64 (`macos-arm64`) | App ZIP, DMG or portable ZIP | CLI-only tar.gz |

Windows desktop builds need WebView2. Linux desktop builds need WebKitGTK 4.1
and GTK 3. macOS packages target Apple Silicon and use the system WebKit.
Before installing, follow the [signature and checksum checks](docs/updates.md#verify-a-versioned-download).
The Windows publisher certificate is self-signed. Do not disable platform security
or install an unknown root to bypass a warning.

On Windows, the signed MSI is the primary installer. For example, in PowerShell 7,
run this after verifying the matching download:

```powershell
$msi = Get-Item -LiteralPath '.\numera-browser-0.37.0-windows-x64.msi'
msiexec.exe /i $msi.FullName
```

On Linux, use the package manager for the DEB, or make the AppImage executable:

```bash
sudo apt install ./numera-browser-0.37.0-linux-x64.deb
# Or, for AppImage:
chmod +x ./numera-browser-0.37.0-linux-x64.AppImage
./numera-browser-0.37.0-linux-x64.AppImage
```

On macOS, open the verified DMG and copy **Numera Browser.app** to Applications,
or extract the App ZIP. Versioned Mac packages have detached publisher signatures;
Apple notarization and publicly trusted Developer ID signing are not claimed.
The [setup guide](docs/setup.md#macos-apple-silicon) distinguishes these packages
from the separately signed manual review artifacts.

For a portable ZIP, extract the whole `Numera Browser` directory and leave the
adjacent `engine/` folder in place. For a CLI-only ZIP or tar.gz, keep `numera-engine`
and `numera-browser-runtime` together. See the [setup guide](docs/setup.md) for extraction,
platform prerequisites and Docker.

## Quick start

### Desktop GUI

Start Numera Browser from the installed app menu, or run `numera.exe` on Windows
or `./numera` on Linux or macOS from the portable folder. The app starts its own loopback
manager. Check that the footer says **Engine online**, then create and launch a
profile. See [browser backends](#browser-backends) if the selected browser is missing.

### Headless server

Run these commands from the extracted CLI-only folder. The server listens on
loopback; the CLI-only archive has no desktop shell, dashboard assets or vendor browsers.

PowerShell 7:

```powershell
.\numera-engine.exe --data-dir .\numera-data serve
```

Bash:

```bash
./numera-engine --data-dir ./numera-data serve
```

In another terminal, check readiness at <http://127.0.0.1:8787/api/ready>.

### CLI without a server

On Linux, with `numera-engine` on `PATH`, run:

```bash
numera-engine create qa-profile --engine playwright
numera-engine list --json
numera-engine check qa-profile --json
numera-engine --help
```

On Windows, use `.\numera-engine.exe` from the extracted folder. With a desktop
installation, find the CLI under `engine/` and use the same data root as the dashboard.

### Source build

From the repository root, with Rust 1.100.0 and the [build prerequisites](docs/setup.md#from-source):

```bash
export PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD=1
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo run --manifest-path dashboard/Cargo.toml --locked --bin dashboard-build
cargo build --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bins
cargo run --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bin numera-engine -- serve --port 8787
```

Use `$env:PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD = '1'` in PowerShell 7 instead of `export`.
Open <http://127.0.0.1:8787>. To acquire pinned resources for browser launches, run
`cargo run --manifest-path tools/xtask/Cargo.toml --locked -- provision-resources build/vendor-resources`,
then follow the [resource setup](docs/setup.md#explicit-browser-resources). For full
packages, use the [native build tooling](tools/xtask/README.md).

## CLI command reference

Run `numera-engine <command> --help` for the installed build's syntax. Pass
`--data-dir <path>` before the command to select the data root. Pass `--json`
after the command for structured stdout, or `--detach` after it to start a
background job and get a handle. Stdio MCP must stay attached. Read the exit
code and JSON together; [the agent guide](docs/agents.md) explains exceptions.

| Command | Purpose |
| --- | --- |
| `create <name>` | Create a profile and generate its fingerprint. |
| `list` | List profiles; `--tag <tag>` filters the list. |
| `show <ref>` | Read a profile by name or ID. Output can contain credentials. |
| `edit <ref>` | Change notes, tags or proxy settings. |
| `regen <ref>` | Regenerate the fingerprint. |
| `delete <ref>...` | Delete profiles; use `-y` only when deletion is intended. |
| `launch <ref>` | Launch a browser. Add `--headless` without a window. |
| `stop <ref>` | Stop sessions that belong to the selected profile. |
| `jobs list`, `jobs show <id>`, `jobs stop <id>`, `jobs clear` | List, inspect, stop or clear detached jobs. |
| `bulk-create --count <n>` | Generate multiple profiles at once. |
| `apply` | Apply one change to a group of profiles. |
| `folder list`, `folder create`, `folder rename`, `folder delete` | Manage folders; deleting one reassigns its profiles. |
| `pool list`, `pool add`, `pool remove`, `pool edit`, `pool rotate`, `pool archive`, `pool export`, `pool test`, `pool assign` | Manage the shared proxy pool. |
| `cookies list <ref>`, `cookies export <ref>`, `cookies import <ref> <file>` | Read, export or import a profile's cookies. |
| `export <ref> <file>`, `import <file>` | Export or import native profile JSON. |
| `import-from <file>` | Import one or more profiles from GoLogin, AdsPower or Multilogin JSON. |
| `backup <ref> <file>`, `restore <file>` | Back up or restore a profile and its session together. |
| `collisions`, `health` | Find shared fingerprints and inspect a summary of stored profiles. |
| `check <ref>` | Validate fingerprint coherence. |
| `trust <ref>`, `align <ref>` | Check the identity a browser presents, or update a profile to match it. |
| `proxy test <ref>`, `tls <ref>`, `dns <ref>` | Diagnose proxy exit, TLS and DNS behavior. |
| `warmup <ref>`, `schedule`, `attach <ref>` | Run or schedule profile warmup, or start a local DevTools attachment. |
| `engines`, `default-engine [name]`, `config` | Inspect installed engines, set the default and change shared settings. |
| `token list`, `token create <name>`, `token revoke <id>` | Manage named API tokens. |
| `vault status`, `vault enable`, `vault change`, `vault resume` | Inspect the vault, enable it, or change or recover its password rotation. |
| `ext` | Manage installed, unpacked and default extensions. |
| `serve` | Start the HTTP service and serve the dashboard if its files are present. |
| `mcp` | Start standalone MCP over stdio or bridge to an existing service. |
| `page`, `workflow`, `fleet`, `profile` | Run supported local QA actions, saved workflows, session sync and profile IP checks. |
| `update check`, `update config`, `update apply` | Check signed offers, change preferences or request an update through the desktop. |
| `capabilities` | List supported commands and MCP input schemas. |

## HTTP API and authentication

`numera-engine serve` listens on `127.0.0.1:8787` by default. The [agent guide](docs/agents.md#http-api)
covers request validation and ownership. The running server serves its OpenAPI
schema at `/openapi.json` and interactive documentation at `/docs`.

| Method and route | Purpose |
| --- | --- |
| `GET /api/ready` | Unauthenticated readiness, version and `authRequired`. |
| `GET /api/health` | Health check; requires authentication when the API is locked. |
| `GET /api/profiles`, `POST /api/profiles` | List or create profiles. |
| `PUT /api/profiles/{pid}`, `DELETE /api/profiles/{pid}` | Update or delete a profile by ID. |
| `POST /api/profiles/{pid}/launch`, `POST /api/profiles/{pid}/stop` | Start or stop a profile session. |
| `GET /api/engines` | Report browser engine availability. |
| `GET /api/proxies`, `POST /api/proxies` | List or add proxy-pool entries. |
| `GET /api/profiles/{pid}/cookies`, `POST /api/profiles/{pid}/cookies` | Read or import one profile's cookies. |
| `GET /api/profiles/{id}/cookies/export` | Download a profile's cookie export. |
| `POST /api/mcp` | Exchange MCP JSON-RPC messages. Always requires a valid bearer token. |

Send `Authorization: Bearer <token>` where required. Set `NUMERA_API_TOKEN`
before starting the server or create a named token with `numera-engine token create <name>`.
Numera prints the named token once, stores its hash and lets you revoke it by ID.
Use a separate token per client. If you need cross-origin browser access, explicitly
set `NUMERA_ALLOWED_ORIGINS`. Keep the manager and CDP ports private.

## MCP integration

`numera-engine mcp` speaks newline-delimited JSON-RPC over stdio. The bridge
forwards messages to `POST /api/mcp` on a running service. Start `serve` and
configure a valid bearer before connecting. The loopback bridge can read
`NUMERA_API_TOKEN` from its environment; keep the value out of shared configs
and command history. The tools exposed over HTTP and stdio can differ, so
call `tools/list` on the transport you plan to use.

### Project-local MCP configuration

Replace `<numera-dir>` with an absolute path to a writable Numera folder. Set
`command` to the actual engine path: the extracted CLI-only directory or the
desktop bundle's `engine/` directory, for example. Use forward slashes or escaped
backslashes in JSON. Omit `.exe` on Linux and macOS. Create the cache and temp directories
before starting the client:

```json
{
  "mcpServers": {
    "numera-browser": {
      "command": "<numera-dir>/bin/numera-engine.exe",
      "args": ["mcp", "--bridge", "http://127.0.0.1:8787"],
      "env": {
        "NUMERA_DATA_DIR": "<numera-dir>/data",
        "NUMERA_LOG_DIR": "<numera-dir>/logs",
        "CLOAKBROWSER_CACHE_DIR": "<numera-dir>/cache/cloak",
        "XDG_CACHE_HOME": "<numera-dir>/cache",
        "TMP": "<numera-dir>/temp",
        "TEMP": "<numera-dir>/temp",
        "TMPDIR": "<numera-dir>/temp"
      }
    }
  }
}
```

For standalone stdio, replace the bridge arguments with `"args": ["mcp"]`.
Standalone mode uses the data directory in the client environment; bridge mode
uses the running server's data directory, browser resources and logs. Avoid running
both against the same profile while it is in use.

| Client | Where to set it up |
| --- | --- |
| Claude Code | A project `.mcp.json` or the `claude mcp` command. Keep credentials out of shared project files. |
| Claude Desktop | The `claude_desktop_config.json` opened from developer settings. |
| Codex | Use `codex mcp add` or `[mcp_servers.numera-browser]` in Codex's TOML config. Translate `command`, `args` and `env`; do not paste the JSON wrapper. |
| Cursor | A workspace `.cursor/mcp.json` or the user-level MCP settings. |

## Environment variables

| Variable | What it controls |
| --- | --- |
| `NUMERA_DATA_DIR` | Persistent data root; `--data-dir` takes precedence. For desktop, check the actual location in Settings > Storage. |
| `NUMERA_LOG_DIR` | Directory override for run logs. |
| `NUMERA_APP_DIR` | Application resource root; by default the engine uses its executable's directory. |
| `NUMERA_API_TOKEN` | Server bearer credential; the loopback MCP bridge can read it too. |
| `NUMERA_ALLOWED_ORIGINS` | Additional browser origins, one per line. Same-origin clients do not need it. |
| `NUMERA_PASSWORD` | Vault master password for noninteractive use. Keep it in a private environment. |
| `CLOAKBROWSER_CACHE_DIR` | CloakBrowser cache location. |
| `CLOAKBROWSER_LICENSE_KEY` | Vendor license key for CloakBrowser where required. |
| `PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD` | Build environment: set to `1` to suppress implicit Playwright driver downloads. It is not a runtime browser-provisioning switch. |
| `XDG_CACHE_HOME` | Cache convention for compatible child tools on Linux. Numera uses the explicit Cloak cache setting above. |
| `TMP`, `TEMP`, `TMPDIR` | OS and child-tool temporary directories. Set them to an existing writable location when isolating an MCP process. |
| `NUMERA_BROWSER_RUNTIME`, `NUMERA_BROWSER_DRIVERS`, `NUMERA_ENGINE` | Override the browser worker path, vendor driver directory or desktop's manager executable, respectively. |
| `PLAYWRIGHT_BROWSERS_PATH`, `CLOAKBROWSER_BINARY_PATH` | Set user-managed browser resource paths outside the portable updater. |

## Browser backends

| Backend | Resources |
| --- | --- |
| `playwright` | Playwright SDK and its matched Chromium. |
| `patchright` | Its own SDK and matched Chromium. |
| `camoufox` | Firefox build, add-ons and data. Camoufox generates parts of its own identity. |
| `cloak` | Native preparation plus a separately licensed vendor browser. Public packages exclude the proprietary binary. |

Full x64 and macOS ARM64 desktop packages include reviewed browser resources;
Windows/Linux ARM64 desktop packages and CLI-only archives do not. Use the
[setup guide](docs/setup.md) for platform-specific prerequisites. Cloak requires
explicit consent, a license and access to the selected vendor generation.

`DriverRegistry` selects a backend; `BrowserEngineProvider` defines the engine
and update interface. The planned Numera Core Browser is outside this release.

## Data and updates

Use **Settings → Storage** to find the actual data directory. Back up that
entire directory before upgrading; a profile export is not a backup of browser
sessions. `--data-dir` and `NUMERA_DATA_DIR` take precedence over the default.

The portable updater handles application and browser-core updates. Its settings
control what can update automatically. The updater verifies signed metadata,
defers while owned work is active or uncertain, and leaves the `data/` directory
alone. See [updates and signature verification](docs/updates.md) for supported
feeds, manual approval, recovery and the distinction between a release download
and an updater-compatible generation.

## Releases & Tags

<a id="releases-and-tags"></a>

- [Releases](https://github.com/KiaroSama/Numera-Browser/releases): signed downloads, detached signatures and checksums once a run publishes them.
- [Tags](https://github.com/KiaroSama/Numera-Browser/tags): version references, including tags without published assets.
- [Changelog](CHANGELOG.md): current changes and links to older versions.

Before extracting or running a package, verify `SHA256SUMS.sig` and the matching
asset's `.sig` using a separately trusted copy of the publisher certificate.
[Release verification commands](docs/updates.md#verify-a-versioned-download) show
how to verify the signatures and compare the package hash.

## Windows x64 benchmark snapshot

These measurements are from the `3944b84` native snapshot and its legacy baseline.
The memory row compares a Python import with a running Rust server. The disk rows
measure different layouts, not equivalent signed downloads with vendor browsers.

| Measurement | Legacy | Rust 0.37.0 |
| --- | --- | --- |
| CLI `--version` | 433.4 ms | 94.5 ms warm; 17.0 ms cached |
| Resident memory (RSS) | 25.41 MB, Python import only | 11.29 MB, live Axum server |
| Runtime footprint | 593.96 MB, PyInstaller bundle | 34.24 MB, native binaries |
| Dashboard footprint | 182.63 MB, React with dependencies | 0.72 MB, Rust/WASM |

The combined app footprint was 90.4% smaller in this snapshot. These figures do
not measure browser startup or memory once a browser opens.

## Logs and troubleshooting

The engine writes UTF-8 logs in `<data-dir>/logs/` unless `NUMERA_LOG_DIR` overrides it.
Look for `native-cli_YYYY-MM-DD_HH-mm-ss_UTC.log` or `native-http_YYYY-MM-DD_HH-mm-ss_UTC.log`,
with a numeric suffix if names collide. Entries use `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`.
The runtime prunes
old inactive matching logs toward 20 files per component; active files and pruning
errors can leave more. Desktop, updater and vault-enable diagnostics use their own
component names. See [engine logging](engine-rust/README.md) for exact controls.
Review and redact copies before sharing. Never attach raw cookies, tokens or profiles.

| Symptom | What to do |
| --- | --- |
| Port 8787 is already in use | Use an unused port with `serve --port <port>` and point the client at that port. Check `/api/ready` to identify the intended service. |
| Browser missing or download waiting for consent | Check `numera-engine engines`, provision the matched browser, and approve the exact vendor acquisition only if intended. |
| Vault locked | Check `numera-engine vault status`. Supply the correct master password before retrying a secret-dependent operation. Do not delete the vault to bypass a lock. |
| Session remains busy after a crash | Inspect `numera-engine jobs list --json`, try `numera-engine stop <ref>` and keep the latest logs. Do not remove ownership records while a browser may still be using the data. |
| Update stopped or interrupted | Preserve logs, staging and recovery state. Follow [update recovery](docs/updates.md#data-safety-and-recovery). |

## Development and verification

| Directory | Contains |
| --- | --- |
| [`engine-rust/`](engine-rust/) | Profile storage, CLI, Axum HTTP API, MCP, browser worker |
| [`dashboard/`](dashboard/) | Rust/WASM dashboard and tests |
| [`desktop/`](desktop/) | Tauri shell, engine lifecycle and updater |
| [`tools/xtask/`](tools/xtask/) | CI, resource provisioning, package and signature tools |

[Native CI](.github/workflows/native-ci.yml) builds the desktop and CLI-only
packages on all five 64-bit targets. macOS ARM64 also runs the native functional
suite, desktop/ownership contracts and real Playwright/Patchright/Camoufox SDK lifecycle
checks. Its required checks cover the maintained
suite inventory, CLI/MCP/dashboard contracts, real browser behavior, relocation,
process ownership, dependencies and CodeQL (`actions` and `rust`). Live Cloak
needs the vendor license; its offline and contract tests do not. Hosted timing
and memory comparisons are published in the `browser-benchmark` artifact for
diagnosis. A functional, ownership or cleanup failure still fails CI.

## Responsible use

Use Numera only on systems and accounts you are authorized to test, and within
the terms of its [license](LICENSE). Do not use it for fraud, credential abuse,
spam or bypassing access controls.

## Author

[KiaroSama](https://github.com/KiaroSama) · [Kiaro.Sama.Dev@gmail.com](mailto:Kiaro.Sama.Dev@gmail.com)

## License

Copyright (c) 2026 KiaroSama. Numera Browser is licensed under the [Noncommercial Network Share-Alike License 1.0](LICENSE).

## Donate

If this project helps you, donations are appreciated.

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
