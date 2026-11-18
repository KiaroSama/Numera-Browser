# Complete Rust rewrite implementation plan

> **For agentic workers:** Use superpowers:executing-plans task by task. Independent browser preparation, dashboard and source-test ports use superpowers:dispatching-parallel-agents with separate file ownership.

**Goal:** Finish the existing native rewrite, replace authored Python and TypeScript application/test/build code with Rust, preserve capabilities and reported test cases, and obtain green cross-platform CI in one unmerged PR.

**Architecture:** Continue the integrated native checkpoint in `engine-rust`. A Rust SDK worker owns the vendor driver transport; Rust WASM replaces React in the dashboard. Rust build and verification tools package and test the real application.

**Tech stack:** Rust 2024, Cargo, existing native core/Tauri, WASM web APIs, provisioned vendor browser drivers.

**Spec:** The user's complete-rewrite requirements, recorded in `docs/rust-rewrite.md`; these supersede ADR 0001's temporary Python/React retention.

## Global constraints

- All changes belong to PR #76; do not merge or change main.
- Preserve 3,418 individually reported source test cases and 195 dashboard cases across 26 suites, their assertions, types and platform applicability.
- Every push and PR builds Linux, Windows and macOS independently; final acceptance includes full distributions and required checks.
- No Python forwarding facade, count-only tests, weakened lifecycle assertions, relabeled coverage or language-statistics concealment.
- Preserve exact-offer acquisition consent, secret handling, persistent data and actual process ownership.

## Review focus

- Failed launch and parent death must settle actual browser/driver descendants.
- Persistent profiles must retain identity, cookies, seeds and encrypted credentials.
- Browser updates must respect protocol/version-specific behavior and licenses.
- Dashboard edits must preserve revisions, password intent and explicit acquisition offers.
- Missing/tampered resources and platform-specific paths must fail with usable, sanitized errors.

## Remaining deliverables

- [x] Integrate the reference branch and dispose of the other PRs with recorded provenance.
- [x] Add automatic three-platform Rust candidate builds and Rust packaging checks.
- [x] Replace Playwright/Patchright SDK execution and verify real launches without Python.
- [ ] Complete Camoufox: pinned model, source-equivalent catalogs, coherent geometry/WebGL, locale/GeoIP, startup prefs, owned launch and real persistence/cleanup acceptance. Implement in `engine-rust/src/browser/sdk/`, extending existing SDK/browser-policy tests.
- [ ] Complete Cloak preparation in `sdk/cloak.rs`; preserve source seed publication, options, license denial and cleanup. Integrate through the same worker and verify real supported launches.
- [ ] Replace `dashboard/src` with Rust WASM, preserving existing CSS/assets, API behavior and all 195 test cases. Build `dashboard/dist` and connect desktop/server packaging.
- [ ] Port each original test with an explicit source-ID mapping and meaningful native assertions. Begin with the independent model/identity/validation domain; retain originals until their replacements are verified.
- [ ] Replace remaining build/CI/desktop entry helpers and cut the product over to the native executable. Remove obsolete source implementations and redundant scaffolding after replacement checks pass.
- [ ] Update README, setup/architecture/release docs, notices, badges and topics to the actual final code. Review the complete diff, run required local checks, push, fix final hosted CI and keep PR open.

Each deliverable is checked against its source behavior and failure cases before
commit. Candidate-build success is recorded separately from complete migration
acceptance. The user has already requested execution through completion; this
plan does not introduce another approval stop.
