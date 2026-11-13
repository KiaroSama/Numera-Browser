# Rust application migration

The selected application is Rust: `numera-engine` provides the CLI, MCP and Axum
HTTP API, and `numera-browser-runtime` owns browser SDK operations. The dashboard
is Rust compiled to WebAssembly; the desktop shell uses Rust and Tauri.

[PR #76](https://github.com/KiaroSama/Numera-Browser/pull/76) continues native
checkpoint `3bf1d839fd33e42a10b982bd8607a7052ea7c4cd`. It remains unmerged.
The earlier Python-default and React/TypeScript plan is superseded by the owner's
request for a complete Rust rewrite. There is no interpreter fallback in the
selected runtime.

Use [setup](setup.md) for current build and launch commands,
[architecture](architecture.md) for component boundaries, and
[rewrite verification](rust-rewrite.md) for source provenance, retained test
identities, platform conditions, upstream review and outstanding acceptance.

CI independently compiles the actual application and browser worker on Linux,
Windows and macOS. Full installers, source archives, real browser acceptance,
security checks and exact test inventories remain required. A compiled candidate,
a historical green run or a test map alone does not establish current completion.

The supplemental [native candidate](migration-candidate.md) is retained for
explicit regression and checkpoint comparisons. It does not select the installed
product. Historical migration logs and their original evidence remain available
in [the reference checkpoint](https://github.com/KiaroSama/Numera-Browser/blob/3bf1d839fd33e42a10b982bd8607a7052ea7c4cd/docs/migration.md).
