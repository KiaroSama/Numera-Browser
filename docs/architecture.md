# Architecture

Numera's application, browser preparation, dashboard, desktop shell and build tools
are implemented in Rust. The dashboard compiles to WebAssembly; Tauri supplies the
native window and operating-system WebView. The HTTP server also serves the same
built dashboard for a server installation.

The rewrite continues checkpoint `3bf1d839fd33e42a10b982bd8607a7052ea7c4cd`
in [PR #76](https://github.com/KiaroSama/Numera-Browser/pull/76). The PR remains
unmerged while the remaining test ports, interpreter retirement and complete
three-platform CI are verified. See [verification status](rust-rewrite.md).

## Components

| Component | Responsibility |
|---|---|
| `engine-rust/src/bin/engine.rs` | Native CLI, HTTP server and MCP entry point. |
| `engine-rust/src/http/` | Axum routes, authentication, request bounds, dashboard serving and session operations. |
| `engine-rust/src/cli/` | Profile, proxy, settings, vault, job and import/export commands. |
| `engine-rust/src/storage/` | Compatible persisted JSON, data-root selection, file locks, atomic replacement and archives. |
| `engine-rust/src/vault/` | Credential encryption, descriptors, journaled transitions and recovery. |
| `engine-rust/src/browser/` | Launch admission, settings, persistent profiles, proxy relay and framed worker management. |
| `engine-rust/src/browser/sdk/` | Native browser preparation and the vendor automation protocol. |
| `browser-page-rust/` | Rust browser-realm identity and probes, with checked compiler-generated runtime assets. |
| `engine-rust/src/ownership/` | Process-tree ownership, readiness, output capture, claims and cleanup evidence. |
| `engine-rust/src/network/` | Proxy checks, extension acquisition, diagnostics and bounded network transports. |
| `dashboard/` | Rust components and state compiled to WASM, plus native and real-browser tests. |
| `desktop/src-tauri/` | Window, owned engine lifecycle, signed-update verification and activation. |
| `tools/xtask/` | Rust CI, vendor provisioning, artifact receipts, packaging and release validation. |

## Browser launch

A profile chooses a coherent identity, backend, persistent browser directory and
proxy configuration. Validation checks the relationships between OS, browser,
locale, timezone, screen, hardware and network settings before launch.

The application acquires the profile's durable session claim, prepares admitted
options and starts `numera-browser-runtime` inside an owned process tree. The
worker translates these options into the vendor protocol. It starts an explicitly
provisioned Node driver and opens a persistent context. Each backend retains its
own compatible SDK and browser resources.

Playwright and Patchright use context options, initialization scripts and CDP
identity metadata on existing and new pages. Chromium controls the timing of
Client Hints after `Accept-CH`; Safari identities suppress Chromium hints.
Camoufox uses its own Firefox identity, Bayesian model, locale/GeoIP preparation,
WebGL data and packaged font capabilities. Cloak retains its vendor license,
exclusive seed and backend-specific launch policy. A configured driver alone
never establishes that the corresponding browser is installed.

The application and worker exchange bounded, versioned JSON frames. Navigation,
routes, cookies, mouse input and evaluation retain their typed semantics. BigInt
and 64-bit arrays retain integer values; cyclic, non-finite or oversized values
are refused. The worker acknowledges closure after context and driver cleanup.
Vendor errors are sanitized before crossing this boundary.

The browser engine and unmodified vendor Node SDK are third-party dependencies.
The repository does not reimplement Chromium, Firefox, Node or the vendor
JavaScript protocol server in Rust. Browser-injected script and WASM glue are
generated runtime assets. [Provisioning](../tools/xtask/README.md) explicitly
acquires reviewed vendor bytes; ordinary Cargo builds do not install browsers.

Browser identity and probe logic is authored in `browser-page-rust`. Locked Rust
and wasm-bindgen builds produce WASM, which pinned Binaryen translates into the
checked JavaScript factory embedded by the native engine. This avoids compiling
WASM inside pages whose CSP forbids it. The generated artifact has source and
compiler receipts and a required regeneration check. Strict-CSP browser tests
verify identity before the first page script while ordinary `eval`, `Function`
and WASM compilation remain blocked. The SDK's 1 MiB frame limit is unchanged.

Minimal platform bindings connect the Rust components to their WebViews. The
Tauri updater's initialization binding publishes its endpoint/token only to the
expected top-level origin; updater authorization, requests and state are Rust.
That scope guard remains at the native/WebView boundary.

## Ownership and shutdown

Windows uses nested Job objects with kill-on-close ownership. Unix uses owned
process groups and platform-specific observations; a leader's exit alone does
not prove that its descendants have exited. Linux observations distinguish live
members from zombies. macOS observations use the platform's native process
information rather than inventing Linux `/proc` results.

Durable claims include tree identity and session ownership. An unreadable claim,
unknown tree kind or uncertain observation remains owned. Status, stop, deletion,
background jobs and updater admission use that same rule. The project never
silently falls back to killing only a launcher or clears a claim merely because
a process lookup failed.

Preparation may run concurrently; ownership acquisition and claim registration
are serialized only at the actual spawn boundary. Stdout and stderr use bounded
readers whose handles remain retained until joined. Cleanup settles separately
reported units, proves tree emptiness, drains output and joins I/O within its
budget. Failure retains the evidence and pending handles for a later retry.
Server shutdown includes finite probes, background work, sessions and retained
adapter I/O. Unknown cleanup is an error, even after a successful exit code.

## Persistence and vault

Data-root precedence is an explicit path, `NUMERA_DATA_DIR`, a writable portable
`data/`, the home directory's `.numera`, then the legacy `.persona` location.
First-run import copies an older installation; it never moves or overwrites it.

Mutations share thread ownership and operating-system file locks. One deadline
covers both lock layers. Writes use a staged, durable replacement; readers retain
compatible views of older records without rewriting them. Archive import checks
paths, links, bounds and ownership before committing data.

The vault encrypts stored proxy credentials rather than all browser files.
Descriptor validation, journals and transition recovery prevent a partially
completed enable or rotation from being treated as complete. A matching salt
alone does not prove that the stored records belong to the new generation.
Failures retain their structured cause codes and recoverable state.

## HTTP, dashboard and desktop

The HTTP API binds to loopback by default. Non-loopback access requires explicit
authentication policy. Bearer-token validation and CORS wrap API and dashboard
composition; CORS is not an authentication mechanism. Requests have bounded
bodies and strict field admission. Browser-backed probes reserve work before
spawn and remain owned through cleanup.

The dashboard uses the same API from a browser or Tauri WebView. An unavailable
API is distinguished from an authenticated live connection. The Rust development
server proxies `/api` while serving a local frontend build.

The desktop starts its own engine on a free loopback port, verifies readiness and
retains native ownership. Closing the window requests graceful shutdown, then
settles the complete owned tree. Portable updates verify signed metadata and
artifact hashes, preserve mutable data and defer activation while work is active
or its ownership is uncertain. [Update details](updates.md) describe the trust and
rollback boundaries.

## Builds and verification

[Native CI](../.github/workflows/native-ci.yml) compiles the real engine and worker
independently on Linux, Windows and macOS before test gates. Full-product jobs
also build the Rust/WASM dashboard, Tauri installers and portable server archives.
Each package records its exact Git source, generation, component identities,
resource inventory, notices and checksums. Unsigned CI artifacts are not releases.

The original suite inventory is frozen in `rust-test-baseline.json`: 3,418 source
cases and 195 dashboard cases in 26 suites. The 294 preexisting native cases are
also retained. Port maps identify individual Rust cases, original test kinds and
platform applicability. Fixed characterization data records observed historical
behavior; live process, browser, fault and mutation assertions still execute real
Rust production paths. A fixture inventory or a compilation result is not proof
that those acceptance checks passed.
