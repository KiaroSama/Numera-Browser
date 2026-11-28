# Native application builds

From the repository root, run
`cargo run --manifest-path tools/xtask/Cargo.toml --locked -- build-product <new-output> --resources <provisioned-directory>`
to build the native server, Rust/WASM dashboard, browser worker and Tauri desktop
for the current host. Complete distributions include the compiled dashboard,
generated browser assets and reviewed vendor SDKs.

Use `--server-only` to omit the desktop build and its SDK requirement, or
`--bundles none` to package the portable desktop and server without installers.
The Windows release uses MSI; Linux uses AppImage and DEB; macOS ARM64 uses
app ZIP and DMG. Build each target on its matching native CI runner. Linux and
Windows support x64 and arm64; macOS supports arm64. `--bundles nsis` remains
available for Windows source builds with a resource set below the NSIS size
limit; complete browser bundles use MSI by default.

## Provision the build host

- Install Rust **1.100.0**, the `wasm32-unknown-unknown` target and
  `wasm-bindgen-cli` **0.2.129** before building.
- For desktop builds, install `tauri-cli` **2.12.5** with Cargo. Linux requires
  WebKitGTK 4.1/GTK3 development packages and AppImage/deb prerequisites.
  Windows MSI builds require MSVC and the Tauri WiX prerequisites. Tauri may
  acquire verified installer build tools while building a requested bundle.
  macOS ARM64 builds require Xcode Command Line Tools and the system WebKit.
- Provision vendor browser/SDK artifacts and their licenses/corresponding source
  explicitly. No browser, SDK, Cloak binary, license or source archive is acquired
  by this command. The separate build-host preparation step owns acquisition and
  upstream authenticity verification.
- Supply canonical `GITHUB_SHA` (40 lowercase hexadecimal characters, equal to
  Git HEAD) and a positive canonical `GITHUB_RUN_ID`. The checkout must be clean,
  including untracked build inputs. Generated outputs belong under ignored
  `build/`, `dist/` and target directories. A local build can set these two values
  explicitly; they are identity inputs, never release authorization.

Workflows set `PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD=1` for builds. From the repository
root in Bash, set it explicitly and provision the reviewed pins before building:

```bash
export PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD=1
export GITHUB_SHA="$(git rev-parse HEAD)"
export GITHUB_RUN_ID=1  # Local generation; not release authorization.
cargo run --manifest-path tools/xtask/Cargo.toml --locked -- provision-resources build/vendor-resources
cargo run --manifest-path tools/xtask/Cargo.toml --locked -- build-product build/native-product --resources build/vendor-resources
```

In PowerShell 7, use `$env:PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD = '1'`,
`$env:GITHUB_SHA = git rev-parse HEAD` and `$env:GITHUB_RUN_ID = '1'` in place
of the Bash exports.

## Provisioned resource contract

The input directory must have this layout, containing full vendor runtime files:

```text
drivers/
  playwright/{node,package/cli.js,package/package.json,...}
  patchright/{node,package/cli.js,package/package.json,...}
  camoufox/{node,package/cli.js,package/package.json,...}
  cloak/{node,package/cli.js,package/package.json,...}
browsers/
  manifest.json
  playwright/...
  patchright/...
  camoufox/...
notices/...
sources/third-party/...
```

Windows uses `node.exe`. Native SDK protocol versions must match the adapter:
Playwright 1.64.x/1.65.x, Patchright 1.64.x, Camoufox's Playwright driver 1.62.x,
and Cloak's Playwright driver 1.64.x/1.65.x. The Cloak **driver** contains the vendor
Playwright SDK, without the separately licensed Cloak browser. Cloak browser
redistribution is rejected; configure its vendor binary after installation.

`browsers/manifest.json` uses the runtime's existing format: `playwright`,
`patchright` and `camoufox` each have an `executable` path relative to `browsers/`
and `sha256`; Camoufox additionally retains `version` and `addons` (an array of
relative addon directories). Addons retain their own manifest, licenses and
source offer. A `cloak` browser entry or directory is refused.

Provide a JSON **array** of source/license offers outside the resource directory.
Every offer has `component`, exact `version`, `license`, HTTPS `source_url`, and
optional `archive` (an inventoried path under `sources/third-party/`). Required
component names are `node`, `playwright`, `patchright`, `camoufox`, `chromium`,
and `firefox`. Bundled addons also require `ublock-origin`. Provision exact
Camoufox patch source and uBlock source archives, including their build
instructions; these two components require `archive`. Retain matching Firefox
complete-source instructions and offers alongside the Camoufox patches. Other
components may also include exact source archives.

Example offer shape (replace version, URLs and archive with the exact acquired
artifact's corresponding source):

```json
{
  "component": "camoufox",
  "version": "EXACT-VENDOR-VERSION-AND-BUILD",
  "license": "MPL-2.0",
  "source_url": "https://github.com/daijro/camoufox/tree/EXACT-COMMIT",
  "archive": "sources/third-party/camoufox-EXACT-COMMIT.tar.gz"
}
```

`prepare-resources` inventories existing bytes into `RESOURCE-PROVENANCE.json`,
records native platform/architecture, checks the layout and offers, and refuses
an existing receipt. `build-product` requires this receipt and verifies every
path, file size and SHA-256; unlisted files, missing files, mismatched hashes,
unsafe paths, symbolic links, foreign platforms and absent source/license
offers fail before compilation. Copied bytes are verified again. Provisioned
resource receipts are operator assertions; they do not authenticate upstream
artifacts or automatically establish redistribution rights.

## Artifacts and lifecycle

A full output contains `Numera Browser/` with the desktop executable and `engine/`.
The engine directory includes both native binaries, the `web/` dashboard, vendor
resources and notices, plus a provenance receipt and file inventory. The desktop
ZIP has the shell and `engine/`; the full server ZIP exposes the engine contents
at the `Numera Browser/` root. The build also produces corresponding source,
`distribution.json`, `SHA256SUMS` and any requested installers.

The release workflow's separate `cli-package` command packages the two native
executables, license and CLI instructions without the dashboard or vendor resources.

Run `numera-engine serve --host 127.0.0.1 --port 8787` in a server installation.
Its dashboard is served at `/`. Set `NUMERA_DATA_DIR` to select persistent data.
The desktop selects a free loopback port, waits for `/api/ready`, and owns the
native engine's process lifetime. Portable data stays beside the shell;
installed bundles use the per-user directory. `NUMERA_ENGINE` explicitly
overrides the native engine path. There is no implicit Python/venv fallback.

These outputs are **unsigned**. No `BUILD.sig` or authenticated update/release
manifest is fabricated. Ambient signing/notarization credentials are removed
from the Tauri build command. Full acceptance, native signing, release manifest
signing and publishing remain separate authorized release workflow operations.
The existing `build-candidate` command remains available independently.

## Verification

```sh
cargo fmt --manifest-path tools/xtask/Cargo.toml --all --check
cargo clippy --manifest-path tools/xtask/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path tools/xtask/Cargo.toml --locked
cargo test --manifest-path tools/xtask/lifecycle-tests/Cargo.toml --locked
```

The lifecycle harness exercises engine selection, loopback readiness, port and
data behavior, and platform process guards without WebKit. It uses the production
source with test stubs for Tauri and the update journal. Native window, installer
and signed updater checks run separately.
