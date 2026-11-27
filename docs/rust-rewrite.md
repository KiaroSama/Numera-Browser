# Rust rewrite and verification

[PR #76](https://github.com/KiaroSama/Numera-Browser/pull/76) merged the Rust
rewrite into `main`. It continues native checkpoint
`3bf1d839fd33e42a10b982bd8607a7052ea7c4cd`. Numera Browser 0.37.0 uses Rust 1.100.0.

## Implementation

Rust implements the native CLI, MCP, Axum API, compatible storage/vault behavior,
owned process lifecycle, browser preparation, the WASM web dashboard, Tauri
desktop integration and build/verification tools. The native product binaries are
`numera-engine` and `numera-browser-runtime`. The older acceptance candidate remains
a separate supplemental build rather than evidence of a complete distribution.

The dashboard retains **195 cases in 26 suites**: 71 native cases and 124
WebAssembly cases executed in Chromium. Application browser preparation supports
Playwright, Patchright, Camoufox and Cloak. The native browser worker uses
explicitly acquired vendor Node transports and browser executables.

Browser-realm identity and probes also have Rust sources in
[`browser-page-rust`](../browser-page-rust/README.md). The embedded JavaScript
factory is compiler output, with source/tool hashes and deterministic regeneration
checks. Its receipt records the Rust version and exact generated output.
The earlier [compiler review](verification/browser-page-printer-20261114.json)
records strict CSP, Unicode memory growth and 480 native SDK frames against
the compiler used in that review.

The [November 14 upstream review](upstream-review-20261114.json) records the
vendor resource pins accepted at that checkpoint. Check the [engine lockfile](../engine-rust/Cargo.lock)
and [desktop lockfile](../desktop/src-tauri/Cargo.lock) for the current dependency versions.

The native worker uses `playwright-rs` with separately provisioned vendor
transports. Build workflows set `PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD=1` in the
environment; set it explicitly for a source build.

Provisioning uses the independently pinned [Patchright 1.64.3 transport](https://registry.npmjs.org/patchright-core/1.64.3)
with the existing Node carriers and matching Chromium generation. Playwright
and Cloak remain on Playwright 1.65.1. Camoufox's newer beta is a prerelease,
so the production pin remains on the GitHub-selected 158.0.1-beta.41 release.
The [Scrapfly model-2/2026 release](https://github.com/scrapfly/fingerprint-generator/releases/tag/model-2/2026)
still matches the model shipped in this tree.
Camoufox includes native fingerprint-model, identity, geometry, locale, GeoIP,
font and WebGL preparation. CPU features require declared support in the selected
browser's properties; an older executable is not credited with unreleased engine
features. Cloak retains license, acquisition consent and exclusive seed behavior;
its proprietary binary is excluded from public bundles.

Process ownership retains original kernel handles until cleanup is proved. The
retained Windows owner requires observed descendants and the original
leader to have exited, together with a fresh complete empty Job observation,
within the existing deadline. A failed or unknown cleanup keeps its owner and
claim for retry. These are scoped guarantees for the reviewed native owner;
fresh hosted Windows execution remains part of acceptance.

## Maintained suites

`rust-test-baseline.json` freezes the original collection, its identities and test
kinds. The counts are **3,418 backend/tooling source cases**, **195 dashboard
cases**, and **294 preexisting native cases on Linux**, including a separately
executed live-browser case. The current Windows inventory is 286. The
295-case macOS inventory remains available as historical baseline data.

Each `source-*-test-map.json` maps an individual source case to an individually
reported Rust case, its original kind and platform. Mapping is not an execution
result. Parameter matrices retain individual cases; adding a fixture loop does
not create additional counted tests. Negative controls, real subprocesses,
concurrency, filesystem failures and cleanup assertions remain executable.

Historical observations use immutable source commit
`13b6d9c6973281fe123353eaec05b29d65781a2d`, with file hashes and separate manifests.
A verified two-row path-normalization change uses the selected `3bf1d8` checkpoint
and is identified separately. Frozen JSON contains inputs and observations rather
than interpreter programs. Dynamic browser/process assertions use native Rust
fixtures, independent OS observations and actual production code.

The 907-symbol `migration-coverage.json` inventory is historical source evidence.
Its old pending rows are not retroactively labeled accepted. Current Rust
occurrence inventories use the Rust AST and identify duplicate symbol occurrences
separately. They complement behavioral tests rather than establish feature parity
by symbol count.

The [current capability review](rust-capability-review.json) covers the exact
907-ID union in six independently reviewed families. Each family records actual
Rust replacements, asserting tests, typed or composed adaptations and remaining
platform evidence limits. The review also records the defects found and fixed
during the rewrite; it does not substitute mapping counts for test execution.

## Build and resource provenance

[Native CI](../.github/workflows/native-ci.yml) builds the application and browser
worker independently on Windows and Linux x64 and arm64. Full-product builds
include the Rust/WASM dashboard, Tauri installers and portable archives.

From the repository root, run `cargo run --manifest-path tools/xtask/Cargo.toml --locked --`
followed by the build-tool command and arguments. `provision-resources` fetches
pinned vendor transports, matching browsers, GeoIP data, notices and GPL/MPL
corresponding source. `prepare-resources` inventories existing bytes without
network acquisition. Browser launch does not implicitly acquire a proprietary browser.

`build-product` requires a clean checkout, exact source SHA, build generation and
new output directory. It checks Cargo artifact paths and component identities
before packaging the dashboard, engine, worker, resources, source and notices.
See [build tooling](../tools/xtask/README.md).

Windows executable version resources and Authenticode signing are implemented in
Rust using explicit certificate input, ephemeral key import, pinned certificate
verification and tampered-file rejection. CI uses a public synthetic fixture to
exercise that path; it does not authorize owner signing or establish public
publisher trust. Default CI distributions are unsigned review artifacts.

## Cutover evidence and release acceptance

The [November 14 validation record](verification/functional-ci8-20261114.json)
separates hosted results from local Playwright, Patchright and Camoufox checks.
Its local sandbox exception applied only to the test environment. The
[initial CodeQL review](verification/codeql-review-20261114.json),
[full result review](verification/codeql-review-ci5-20261114.json) and
[scan comparison](verification/codeql-review-ci7-20261114.json) describe findings
at their recorded source revisions. They are historical evidence, not a current
CI verdict.

Use the workflows for the exact release commit to check acceptance. The package,
process ownership, browser, signing and CodeQL checks must pass before publishing.
See [maintainer publication](updates.md#maintainer-publication).

Performance compares the genuine `3bf1d8` native application checkpoint with the
current native product, using the same current Rust SDK transport and resources.
The checkpoint has exactly two reviewed transport bridges; receipts pin their
original Git blobs, before/after hashes, patch hash and SDK hash. This comparison
is labeled native-checkpoint-to-native-product. It is not a Python performance or
rollback measurement, and a candidate is never compared with itself.

[Branch dispositions](rust-branch-disposition.json) record integrated and declined
prior work. Documentation and topic cleanup follow the actual language cutover.
No release, merge or `main` update is authorized by a passing test or generated
artifact.
