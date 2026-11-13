<div align="center">

<img src="docs/brand/logo-256.png" alt="Numera Browser logo" width="240" height="240">

# Numera Browser

### Local-first browser profiles, persistent sessions and automation

Manage separate browser profiles, proxy settings and sessions from a desktop window,
web dashboard or CLI. Choose Playwright, Patchright, Camoufox or CloakBrowser for
workflows you are authorized to run.

[![Native CI](https://github.com/KiaroSama/Numera-Browser/actions/workflows/native-ci.yml/badge.svg?branch=rewrite%2Frust-complete-20261110)](https://github.com/KiaroSama/Numera-Browser/actions/workflows/native-ci.yml)
[![Rust candidate builds](https://github.com/KiaroSama/Numera-Browser/actions/workflows/rust-candidate.yml/badge.svg?branch=rewrite%2Frust-complete-20261110)](https://github.com/KiaroSama/Numera-Browser/actions/workflows/rust-candidate.yml)
[![License: noncommercial](https://img.shields.io/badge/License-noncommercial-d04a9a)](LICENSE)
[![Version: 0.36.0](https://img.shields.io/badge/Version-0.36.0-blue)](CHANGELOG.md)
[![Rust: 1.99.1](https://img.shields.io/badge/Rust-1.99.1-dea584)](rust-toolchain.toml)

[![Desktop build targets](https://img.shields.io/badge/Targets-Windows%20x64%20%7C%20Linux%20x64%20%7C%20macOS%20ARM64-555)](.github/workflows/build-desktop.yml)
[![Documentation](https://img.shields.io/badge/Docs-setup%20%26%20usage-4c9a2a)](docs/setup.md)
[![Dashboard: Rust + WebAssembly](https://img.shields.io/badge/Dashboard-Rust%20%2B%20WebAssembly-654ff0)](dashboard/Cargo.toml)
[![Tauri: 2](https://img.shields.io/badge/Tauri-2-24c8d8)](desktop/src-tauri/Cargo.toml)
[![HTTP API: Axum](https://img.shields.io/badge/API-Axum-009688)](engine-rust/Cargo.toml)
[![Browser: Playwright](https://img.shields.io/badge/Browser-Playwright-45ba4b)](Dockerfile)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

[Download releases](https://github.com/KiaroSama/Numera-Browser/releases) ·
[Installation and setup](docs/setup.md) · [Updates](docs/updates.md) ·
[CLI and MCP](docs/agents.md)

![Numera Browser dashboard](docs/screenshot-dashboard.png)

</div>

The application, HTTP API, CLI, MCP, dashboard, desktop integration and build tools
are implemented in Rust. The dashboard runs as WebAssembly. Browser automation
uses explicitly provisioned vendor Node drivers and Chromium/Firefox binaries;
these third-party dependencies retain their own languages and licenses.

Browser identity and diagnostic logic is also authored in
[Rust](browser-page-rust/README.md). Its checked compiler-generated browser
artifact preserves strict page CSP and needs no compiler at runtime.

The [rewrite PR](https://github.com/KiaroSama/Numera-Browser/pull/76) continues the
native checkpoint and remains unmerged. Complete test-port verification,
interpreter retirement and full three-platform CI are still in progress; see the
[acceptance record](docs/rust-rewrite.md). A build badge does not establish that a
complete release has shipped.

## What it does

- **Profiles and sessions:** persistent browser storage, editable profile settings,
  folders, tags, search, bulk creation/editing, configuration import/export and
  individual profile/session backups.
- **Browser backends:** select a backend per profile, manage extensions, launch a
  browser or attach automation to an owned session.
- **Proxy management:** import and assign proxies, inspect health, request provider
  rotation and run diagnostic IP/WebRTC/DNS/TLS checks.
- **Dashboard and automation:** profile editor, health overview, scheduled tasks,
  CLI JSON output, HTTP API and MCP. Optional vault protection encrypts stored
  proxy credentials; it does not encrypt every browser file.
- **Portable updates:** review a signed release generation, approve downloading,
  then separately approve installation/restart. SDKs and their managed browser
  binaries update as a compatible unit; see [limits and recovery](docs/updates.md).

The consistency meter checks profile settings against the project's rules. It is
**not** proof of anonymity, undetectability, account isolation at a remote service,
or permission to bypass that service's controls. Browser engines apply different
settings; Camoufox generates parts of its own Firefox identity.

## Install

### Portable desktop

Download a completed matching archive from
[GitHub Releases](https://github.com/KiaroSama/Numera-Browser/releases), verify its
published signature/checksums, and extract the **entire** `Numera Browser` folder.
Run `numera.exe` on Windows or `numera` on Linux/macOS. Keep its engine and browser
resources beside it.

| Build target | Host prerequisites |
|---|---|
| Windows x64 | Compatible Windows and WebView2 runtime. |
| Linux x64 | Compatible glibc distribution, GTK 3/WebKitGTK 4.1 and the browser's system libraries. |
| macOS ARM64 | Apple Silicon and compatible macOS/WKWebView. Not an Intel Mac build. |

A complete portable bundle needs no separate Python or Node installation. OS
warnings can still apply: native signing is **self-signed/private-trust**, not a
publicly trusted publisher certificate or Apple notarization. Do not disable OS
security or install an unknown root certificate to bypass a warning.

Only assets actually present in a completed release are available downloads. The
release pipeline is under verification; a workflow definition or build-target
badge is not evidence that the latest tree has passed or an asset has shipped.

### Source installation

Install Rust **1.99.1**, the WASM target and the pinned binding tool, then build
and serve the dashboard with the native engine:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo run --manifest-path dashboard/Cargo.toml --locked --bin dashboard-build
cargo build --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bins
cargo run --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bin numera-engine -- serve --port 8787
```

Open <http://127.0.0.1:8787>. Browser launching additionally needs its matching
vendor resources; [setup](docs/setup.md#from-source) describes explicit acquisition.
For frontend development, keep the engine running and start
`cargo run --manifest-path dashboard/Cargo.toml --locked --bin dashboard-dev`
in a second terminal. The Rust development server proxies `/api` to port 8787.

Detailed platform prerequisites, alternative browser setup, Docker, headless
configuration and troubleshooting are in [docs/setup.md](docs/setup.md).

## Use

Create a profile:

```bash
numera-engine create qa-profile --os windows --locale en-US --engine playwright
```

List saved profiles:

```bash
numera-engine list --json
```

Launch the selected profile:

```bash
numera-engine launch qa-profile
```

Inspect installed browser backends:

```bash
numera-engine engines
```

Set the default deliberately:

```bash
numera-engine default-engine playwright
```

`playwright` is the initial default. Explicit engine choices on creation or
engine-changing edits can update it; editing unrelated fields does not. The
server is authoritative, and Settings shows its saved default.

For command help, use `numera-engine --help`. See
[engine-rust/README.md](engine-rust/README.md) for native commands and development,
[docs/agents.md](docs/agents.md) for JSON/MCP workflows, and
[dashboard/README.md](dashboard/README.md) for frontend development.

## Browser engines

| Backend | Browser and distribution boundary |
|---|---|
| `playwright` | Playwright's matched Chromium and explicitly provisioned SDK transport. |
| `patchright` | A separate Patchright SDK and matched Chromium; not Playwright's shared binary. |
| `camoufox` | Camoufox's Firefox build, addons and associated data. Its own generated identity differs from Chromium settings. |
| `cloak` | Native Cloak preparation plus a separately licensed vendor browser. The proprietary binary is excluded from public bundles. |

Portable builds bundle Playwright, Patchright and Camoufox resources with their
matching SDK versions. Cloak acquisition requires explicit consent and permitted
vendor access. A vendor tier that cannot supply the requested exact generation
cannot be presented as a tested managed pin. External browser-path overrides
remain user-managed. Third-party licenses, notices and source obligations are
listed in [docs/licensing.md](docs/licensing.md) and
[docs/browser-notices.txt](docs/browser-notices.txt).

## Data, access and updates

**Back up your data, not just the executable.** Settings → Storage shows the actual
data root. Portable data normally lives beside the app when writable; explicit
`--data-dir`/`NUMERA_DATA_DIR` selection takes precedence. Profiles, sessions, proxy
configuration and token records are sensitive. A profile configuration export is
not a full browser-session backup.

Keep the API and CDP endpoints private. `numera-engine serve` binds to localhost by
default; use bearer authentication and a controlled tunnel/reverse proxy for
remote access. A non-loopback bind requires authentication. `NUMERA_ALLOWED_ORIGINS`
restricts browser callers; CORS is not a substitute for authentication.

The portable updater checks the configured project release feed, verifies signed
metadata and asks before downloading or restarting. Active or uncertain work
**defers** activation instead of being force-killed. Source checkouts and containers
use their package/deployment workflow. See [docs/updates.md](docs/updates.md) for
signature verification, recovery, filesystem and trust limits.

## Logs

The native engine writes UTF-8 logs under the actual data root's `logs/`, one
`numera_YYYY-MM-DD_HH-mm-ss_UTC.log` per run, with a collision suffix when needed.
Entries use `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`. Verbose mode adds
diagnostics; redaction is applied before file output. The engine retains a bounded
set of run logs; see [engine-rust/README.md](engine-rust/README.md) for its controls.

Desktop/update and build tooling keep their own lifecycle diagnostics. The native
runtime records fixed vault-enable phases in `logs/native-vault-enable_*_UTC.log`
(with collision suffixes); these diagnostic-only logs never fall back to console
output or extend command-capture idle deadlines. On creating a log, the native
runtime prunes the oldest matching runs per component toward a 20-file limit,
ordered by modification time. Active or linked files are preserved; pruning
failures can leave extra logs. Missing logs mean unavailable evidence, not completion.
Other logging initialization failures produce a safe console fallback rather than
pretending a log exists. Review logs before sharing them: redaction is a safety measure, not
permission to publish raw cookies, tokens or profile data.

## Development and verification

| Directory | Responsibility |
|---|---|
| `engine-rust/` | Native application, storage, browser preparation, CLI, Axum API and MCP. |
| `dashboard/` | Rust/WebAssembly dashboard, native unit and real browser interaction tests. |
| `desktop/` | Tauri window, owned engine lifecycle and portable updater. |
| `tools/xtask/` | Rust CI, vendor provisioning, provenance, signing and distribution tools. |

[Native CI](.github/workflows/native-ci.yml) runs on pushes and pull requests.
Independent jobs compile the real engine and worker on Linux, Windows and macOS
before test gates. Full-product jobs build the dashboard, desktop installers and
portable server archives. Other required jobs cover the maintained suite counts,
functional and real-browser behavior, ownership, dependencies and security.
Current exact-source results are in
[Actions](https://github.com/KiaroSama/Numera-Browser/actions).

The original inventory retains **3,418 source cases**, **195 dashboard cases in
26 suites**, and **294 preexisting native cases**. Port maps record individual
case identities, types and platform applicability. The migration and complete
CI remain under verification; see [rewrite status](docs/rust-rewrite.md),
[architecture](docs/architecture.md) and [changelog](CHANGELOG.md).

## Responsible use

Use Numera only for authorized QA, automation development, privacy research and
other lawful noncommercial work permitted by its license. Do not use it for
fraud, credential abuse, spam or bypassing access controls. You are responsible
for your targets, accounts, permissions and applicable service terms.

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

## License

[Noncommercial Network Share-Alike License 1.0](LICENSE): ordinary noncommercial
use, modification and no-charge redistribution are permitted under its conditions.
Sale, business use and commercial exploitation of covered copies, modifications
and derivatives are not licensed. Redistribution requires matching Complete Source;
modified network versions have source-offer obligations. This is source-available,
not OSI-approved open source. Third-party licenses and prior valid grants remain
unchanged; see [licensing and notices](docs/licensing.md).
