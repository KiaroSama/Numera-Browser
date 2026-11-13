# Native Numera engine

Rust implements profile management, persistent sessions, storage, proxy tools,
vault recovery, CLI, MCP and the Axum API. `numera-engine` is the server/CLI binary;
`numera-browser-runtime` is its owned browser worker. The source tree's
`rust-toolchain.toml` pins the compiler and WASM target.

```sh
cargo build --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bins
cargo run --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser --bin numera-engine -- --help
```

In an extracted server bundle, run `numera-engine` directly (`.exe` on Windows).
In a desktop bundle the CLI is under `engine/`. Keep the worker, browser resources,
web assets, inventories and notices with the engine.

```sh
numera-engine create qa-profile --os windows --locale en-US --engine playwright
numera-engine list --json
numera-engine launch qa-profile
numera-engine engines
numera-engine default-engine playwright
numera-engine serve --host 127.0.0.1 --port 8787
```

Browser SDK and browser acquisition is explicit. See [setup](../docs/setup.md) and
[resource/package tooling](../tools/xtask/README.md). Browser workers use native
Rust preparation with unmodified vendor Node transports; no separately installed
Node is required by a complete bundle. Cloak's licensed browser is provisioned
separately and excluded from public distributions.

`--data-dir` and `NUMERA_DATA_DIR` select persistent storage. Otherwise the engine
uses a writable portable root and then the supported home-directory fallbacks.
API and CDP listeners should remain private. `NUMERA_API_TOKEN` configures bearer
authentication; a non-loopback server must meet the explicit authentication policy.

Logs use UTC timestamps and bounded retention under the resolved data root's
`logs/`, or the explicit `NUMERA_LOG_DIR`. Credentials are redacted before file
output. `--verbose` adds diagnostics. File logging failures use safe console
fallback where supported; missing diagnostics never prove successful cleanup.
Vault enable has separate bounded phase logs with fixed labels and a per-component
20-run pruning target. Active, linked or otherwise uncertain files are preserved.

The native suite contains the individually mapped source-test ports and all
preexisting native cases. Build test-support explicitly for the complete suite:

```sh
cargo test --manifest-path engine-rust/Cargo.toml --locked --features native-product,native-browser,test-support
```

The ignored live-browser case is a separate required CI tier. The CI runner
executes and checks counts/platform applicability; this command alone does not
establish complete four-backend acceptance. [Rewrite status](../docs/rust-rewrite.md)
records outstanding work. Keep [PR #76](https://github.com/KiaroSama/Numera-Browser/pull/76)
unmerged.
