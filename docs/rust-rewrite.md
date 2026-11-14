# Rust rewrite and verification

[PR #76](https://github.com/KiaroSama/Numera-Browser/pull/76) continues the complete
history of native checkpoint `3bf1d839fd33e42a10b982bd8607a7052ea7c4cd`
from `migration/native-runtime-checkpoint-20261012`. It remains open and unmerged;
`main` has not been advanced by this work.

## Implementation

Rust implements the native CLI, MCP, Axum API, compatible storage/vault behavior,
owned process lifecycle, browser preparation, Rust/WASM dashboard, Tauri desktop
integration and build/verification tools. The native product binaries are
`numera-engine` and `numera-browser-runtime`. The older acceptance candidate remains
a separate supplemental build rather than evidence of a complete distribution.

The dashboard retains **195 cases in 26 suites**: 71 native cases and 124
WebAssembly cases executed in Chromium. Application browser preparation supports
Playwright, Patchright, Camoufox and Cloak. Native workers use explicitly acquired
vendor Node transports and vendor browser engines. Those external dependencies
are not rewritten Chromium/Firefox/Node implementations; generated browser
scripts and WASM bindings remain runtime assets.

Browser-realm identity and probes also have Rust sources in
[`browser-page-rust`](../browser-page-rust/README.md). The embedded JavaScript
factory is compiler output, with source/tool hashes and deterministic regeneration
checks. Actual browser comparisons preserve identity, probe failures and strict
CSP behavior. No page CSP policy is relaxed to initialize this runtime.

[Current upstream review](upstream-review-20261114.json) distinguishes stable
artifacts from source-only changes. Driver/browser generations remain separate.
Camoufox includes native fingerprint-model, identity, geometry, locale, GeoIP,
font and WebGL preparation. CPU features require declared support in the selected
browser's properties; an older executable is not credited with unreleased engine
features. Cloak retains license, acquisition consent and exclusive seed behavior;
its proprietary binary is excluded from public bundles.

## Maintained suites

`rust-test-baseline.json` freezes the original collection, its identities and test
kinds. The counts are **3,418 backend/tooling source cases**, **195 dashboard
cases**, and **294 preexisting native cases on Linux**, including a separately
executed live-browser case. The original platform-specific native inventories are
295 on macOS and 286 on Windows; their applicability is preserved.

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

[Native CI](../.github/workflows/native-ci.yml) starts independent Linux, Windows
and macOS product builds on pushes and pull requests. The real application and
worker compile before test gates; a failed test job does not cancel compilation
on another platform. Full-product builds include WASM, Tauri installers and
portable server archives.

`cargo xtask provision-resources` explicitly acquires pinned vendor transports,
matching browsers, GeoIP data, notices and GPL/MPL corresponding source.
`prepare-resources` inventories already acquired bytes without downloading them.
Neither ordinary Cargo compilation nor browser launch silently acquires a vendor
SDK or proprietary browser.

`cargo xtask build-product` requires a clean checkout and an exact source SHA and
build generation. It verifies Cargo's actual artifact records and each component's
compiled identity, then packages the dashboard, engine, worker, resources,
notices, exact tracked source and checksums. Existing output directories are
refused. See [build tooling](../tools/xtask/README.md).

Windows executable version resources and Authenticode signing are implemented in
Rust using explicit certificate input, ephemeral key import, pinned certificate
verification and tampered-file rejection. CI uses a public synthetic fixture to
exercise that path; it does not authorize owner signing or establish public
publisher trust. Default CI distributions are unsigned review artifacts.

## Current acceptance boundary

Local Linux evidence includes real Playwright and Patchright browser acceptance,
and the released Camoufox158 acceptance with native preparation. The managed test
environment requires an isolated writable browser home; its sandbox exception is
limited to that local test and is not added to the product. Full licensed Cloak
acceptance remains a required CI tier.

Source-suite ports and interpreter retirement are one integration. Per-suite maps
record named execution evidence. Required acceptance includes ownership checks,
complete product packaging, actual Windows signing-fixture execution and the
full CI result for the exact current PR revision. An ancestor's green candidate
matrix does not establish current product acceptance.

Repository CodeQL default setup is a separate administration setting. It still
expects the main branch's Python configuration, so a Rust-only pull request can
report a missing Python configuration even when Rust extraction succeeds.
Maintain that distinction during the language transition: the advanced Rust
workflow retains all security-extended queries and full SARIF evidence, and its
independent verdict must not be replaced with a local successful process exit.
Resolving the repository configuration and reviewing reported findings remain
part of final CI acceptance.
The [initial individual CodeQL review](verification/codeql-review-20261114.json)
and [fresh scan review](verification/codeql-review-ci5-20261114.json) record exact
source identities, confirmed defects and the remaining test-data or FFI-model
findings. The fresh scan no longer reports the 152 logging sinks or four proved
salt-admission defects; these reviews are not a passing independent verdict.

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
