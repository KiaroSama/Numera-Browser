# Rust rewrite integration

The integration branch starts at `3bf1d839fd33e42a10b982bd8607a7052ea7c4cd`
from `migration/native-runtime-checkpoint-20261012` (PR #56). It includes that
branch's full history. Main is not changed and this integration must not be
merged until the complete rewrite and its acceptance checks pass.

## Current boundary

The native CLI, HTTP and MCP application code already exists in `engine-rust`.
The selected product still uses Python, the browser SDK adapter still runs
Python, and the dashboard still uses TypeScript/React. The desktop shell and
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
Cargo's successful artifact record identifies the executable; the tool executes
its `--build-info` and verifies the compiled source, generation, product and
version before packaging. Each platform uploads the executable ZIP, its matching
tracked Git source ZIP and SHA-256 checksums. Outputs are new directories; an
existing build cannot be overwritten. Package tests exercise real ZIP contents,
permissions, checksums, tracked-source scope and failed-build cleanup.

These are acceptance candidates. They are not complete desktop distributions,
do not include the browser adapter or dashboard, and do not establish a
Python-free runtime. Their embedded notices and metadata state those limits.
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
This review did not replace the Python adapter or certify the other SDK wrappers.

## Branch consolidation

All 18 previously open PRs are closed with a link to integration PR #76. The
complete #56 checkpoint and 14 dependency heads are ancestors of this branch;
three incompatible or incomplete upgrades were declined with explicit reasons.
`rust-branch-disposition.json` records every head SHA and disposition. Branch
references remain recoverable and main was not merged or updated.
