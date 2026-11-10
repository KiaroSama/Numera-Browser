# Rust rewrite integration

The integration branch starts at `3bf1d839fd33e42a10b982bd8607a7052ea7c4cd`
from `migration/native-runtime-checkpoint-20261012` (PR #56). It includes that
branch's full history. Main is not changed and this integration must not be
merged until the complete rewrite and its acceptance checks pass.

## Current boundary

The native CLI, HTTP and MCP application code already exists in `engine-rust`.
The selected product still uses Python and the dashboard still uses
TypeScript/React. A Rust SDK worker now implements the Playwright/Patchright
protocol; Camoufox and Cloak still use the retained Python path. The desktop shell and
the existing CI orchestration also contain non-Rust code. These dependencies
must be replaced; changing language badges or excluding files from language
statistics does not complete that work.

`docs/rust-test-baseline.json` records the 3,418 Python test identities collected
on Linux and all 195 dashboard assertions in 26 suites at the reference head.
Collection is not an execution result. OS applicability, live-browser scenarios,
negative controls, fault injection and subprocess ownership checks must survive
the port. A Rust test containing a loop of many fixtures does not automatically
replace the same number of independently reported source tests. The existing
source and Rust suites are retained until a case-by-case replacement is verified.

The original 907-row capability ledger remains in `migration-coverage.json`.
Its pending entries have not been relabeled as accepted.

## Automatic native builds

`rust-candidate.yml` runs on every push and pull request, with independent Linux,
Windows and macOS jobs. Compilation has no dependency on test or coverage jobs;
matrix failures do not cancel another platform. These checks supplement the full
verification workflow, which still has outstanding migration acceptance gates.

The build tool is Rust:

```text
cargo xtask build-candidate build/rust-candidate
```

It requires a clean checkout and the exact `GITHUB_SHA` and `GITHUB_RUN_ID`.
Cargo's successful artifact records identify both native executables; the tool
executes their `--build-info` and verifies the compiled source, generation,
product, version and browser-worker component before packaging. Each platform uploads the executable ZIP, its matching
tracked Git source ZIP and SHA-256 checksums. Outputs are new directories; an
existing build cannot be overwritten. Package tests exercise real ZIP contents,
permissions, checksums, tracked-source scope and failed-build cleanup.

These are acceptance candidates. They are not complete desktop distributions,
include the Rust browser worker but exclude vendor SDK/browser resources and the
dashboard. They do not establish a complete Rust-only application. Their embedded notices and metadata state those limits.
No signing input, publishing step, main update or merge is involved.

## Linux first-run copy correction

The unchanged five-path access-time regression failed on the integration tree:
Rust's file-copy offload left source access times unchanged on this filesystem,
while the frozen Python implementation performed reads. Linux first-run imports
now copy through a bounded 64 KiB buffer. Direct copies still read metadata after
copying, and new copytree subtrees retain their pre-read metadata snapshots.
Source bytes, existing destinations, permissions, extended attributes and failure
cleanup remain covered by the same 11 first-run tests. Large-import throughput
has not been benchmarked for this correction.

## Acceptance still required

1. Replace the Python SDK adapter and remove application interpreter dependencies
   while retaining all four supported browser backends and owned-tree cleanup.
2. Port the dashboard and desktop integration to Rust and preserve their user
   features, rendered interaction tests and updater behavior.
3. Port every source test and CI helper, retaining case counts, test types,
   assertions, platform applicability and failure checks.
4. Integrate compatible upstream browser updates with browser, persistence,
   proxy and lifecycle regressions; see `upstream-review-20261110.json`.
5. Verify full application distributions on Linux, Windows and macOS. Passing
   these candidate compilation jobs alone is insufficient.
6. Update all documentation and repository topics to the final implementation,
   and obtain green full CI on
   the final head. Keep the PR unmerged.

## Rust SDK compatibility review

`playwright-rs` 0.20.1 was inspected as an adapter replacement. Its persistent
context launcher adds CI-specific Chromium arguments on Windows, including
`--disable-web-security` and a site-isolation override. These are absent from
Numera's admitted launch options. The binding must not be adopted through that
entry point without an equivalent controlled transport and regression coverage.
The reviewed packaged source is
<https://docs.rs/crate/playwright-rs/0.20.1/source/src/protocol/browser_type.rs>.
The Rust worker uses the public protocol connection directly, with its own child
ownership and launch parameters. It does not call that launcher, install the
binding's terminal/signal handlers, or detach the Node driver from Numera's owner.

### Native SDK worker

```text
cargo build --manifest-path engine-rust/Cargo.toml --locked --features native-browser --bin numera-browser-runtime
```

`NUMERA_BROWSER_RUNTIME` explicitly selects that executable. Candidate packages
also discover it beside the application executable. During migration an explicit
legacy `.py` adapter can still be selected; the product has not switched runtimes.

The worker retains the bounded version-1 JSON protocol, cancellable stdin owner,
operation-state checks, persistent contexts, initialization scripts, CDP identity
policy on existing/new pages, navigation, routes, evaluation, mouse input and
cookie operations. Cookie objects keep vendor fields including partition keys.
Close is acknowledged after context and driver cleanup. Vendor error payloads,
credentials and evaluated source are never returned or logged as diagnostics.

SDK acquisition remains explicit. `NUMERA_PLAYWRIGHT_DRIVER` and
`NUMERA_PATCHRIGHT_DRIVER` point to separate vendor driver directories, each
containing `node` (`node.exe` on Windows), `package/cli.js` and
`package/package.json`. Alternatively `NUMERA_BROWSER_DRIVERS` contains directories
named `playwright` and `patchright` with that layout. The vendor driver is Node
code, not a Rust implementation; it is an external SDK dependency and is not
hidden in this repository or included in candidate ZIPs. Browser revisions must
match their own driver. Cargo cannot download drivers as a build side effect.

On local Linux, the existing live acceptance test passed with Playwright 1.64.0,
Playwright 1.65.0 and Patchright 1.64.1. Each run exercised the existing five
identity profiles, initial/new-page wire hints, cookie persistence and isolation,
HTTP launch/stop and controlled foreground CLI closure. The oracle interpreter
was deliberately set to a nonexistent path; these operations did not invoke
Python. Existing protocol-inertness/cleanup coverage also passed against the Rust
worker. Test identities and assertion counts were retained.

Cloak's upstream wrapper 0.7.2 (2026-11-10) caps Playwright below 1.65. The native
driver admission keeps that backend-specific cap. Specialized Cloak/Camoufox
preparation is not admitted by the worker yet: their metadata remains unavailable
rather than claiming that a driver alone constitutes a working backend. The
current Camoufox source also changed fingerprint-model pinning, identity and
geometry rules; adopting an older Rust facade without porting those rules would
not incorporate its current behavior.

The Camoufox fingerprint-model component is now native Rust. It verifies the
exact `model-2/2026` archive, all three compressed members and the decompressed
value-data digest. A bounded beam preserves the upstream Bayesian posterior;
streaming value lookups avoid loading the full 202 MiB value file into memory.
The existing two browser-policy tests retain their identities and assertion
coverage, and additionally verify 18 frozen source posteriors across Firefox on
Windows/macOS/Linux, actual fingerprint generation and refusal of modified or
truncated archives. CI runs this target with `native-browser` enabled, without
Python. The pinned archive is a source-test fixture; candidate runtime ZIPs do
not include it. This component does not yet admit Camoufox context launch.

Repository topics `cargo`, `cross-platform` and `mcp` were saved through the
user-authorized TinyFish Default profile and verified separately through GitHub's
API. Existing language topics remain until their code is replaced.

## Branch consolidation

All 18 previously open PRs are closed with a link to integration PR #76. The
complete #56 checkpoint and 14 dependency heads are ancestors of this branch;
three incompatible or incomplete upgrades were declined with explicit reasons.
`rust-branch-disposition.json` records every head SHA and disposition. Branch
references remain recoverable and main was not merged or updated.
