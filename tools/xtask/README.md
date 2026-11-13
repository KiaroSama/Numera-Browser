# Native application builds

`cargo xtask build-product <new-output> --resources <provisioned-directory>`
builds the native server, Rust/WASM dashboard, native browser adapter and Tauri
desktop on the current host. It invokes Rust tooling, without Python, npm or
application JavaScript sources. Generated `wasm-bindgen` bindings and unmodified
vendor SDK JavaScript are runtime dependencies.

Use `--server-only` to omit the desktop SDK requirement, or `--bundles none` to
produce portable server and desktop archives without installers. The native
default installer formats are Linux `appimage,deb`, Windows `msi,nsis`, and macOS
`app,dmg`. `--bundles` can select a subset; cross-host installer targets are
rejected. Build each supported platform on its own native CI runner.

## Provision the build host

- Install the repository's Rust toolchain, `wasm32-unknown-unknown` target and
  `wasm-bindgen-cli` **0.2.129** before building.
- For desktop builds, install a compatible **2.x** `tauri-cli` with Cargo. Linux
  requires WebKitGTK 4.1/GTK3 development packages and AppImage/deb prerequisites;
  Windows requires MSVC and the Tauri WiX/NSIS prerequisites; macOS requires Xcode
  command line tools and the native DMG tools. Tauri may acquire its normal
  verified installer build tools as part of an explicitly requested bundle build.
- Provision vendor browser/SDK artifacts and their licenses/corresponding source
  explicitly. No browser, SDK, Cloak binary, license or source archive is acquired
  by this command. The separate build-host preparation step owns acquisition and
  upstream authenticity verification.
- Supply canonical `GITHUB_SHA` (40 lowercase hexadecimal characters, equal to
  Git HEAD) and a positive canonical `GITHUB_RUN_ID`. The checkout must be clean,
  including untracked build inputs. Generated outputs belong under ignored
  `build/`, `dist/` and target directories. A local build can set these two values
  explicitly; they are identity inputs, never release authorization.

Example, with the reviewed pins in `provision-pins.json`:

```sh
cargo xtask provision-resources /opt/numera-resources
cargo xtask build-product build/native-product --resources /opt/numera-resources
```

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

The output contains a `Numera Browser/` directory with a desktop `numera`
executable (or `numera.exe`) and `engine/` containing both native Rust binaries,
`web/` dashboard, vendor resources, notices, matching tracked source, an unsigned
`PROVENANCE.json` receipt and `FILES.json` hashes. The server zip exposes the
contents of `engine/` under its `Numera Browser/` root. The desktop zip contains
the shell and `engine/`. The matching source zip, installers, `distribution.json`
and `SHA256SUMS` accompany these archives.

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

The lifecycle harness includes the production source and original lifecycle
tests, with inert Tauri Manager and update-journal type stubs. It checks native
engine selection, loopback readiness, port/data behavior and platform process
guards without WebKit. It does not substitute for native Tauri window, installer
or authenticated updater acceptance on the three desktop hosts.
