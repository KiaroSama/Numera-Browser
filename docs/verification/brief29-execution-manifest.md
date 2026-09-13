# Execution manifest — 0.34.0 (N14, N02, N04)

**Baseline:** `d40415b648f854178b88eb8aa88551e9e6ea5979` (0.33.0).
**Candidate:** the single commit on `fix/brief29-remediation` that carries this
file; `git log -1` on that branch is the SHA every row below was produced on.

This file is documentation and executes nothing. It is written after the final
pass and committed with it, so the tree the gates ran on differs from the
committed tree only by this file and the two ledgers beside it.

## Toolchain

| | |
|---|---|
| OS | Windows 11 26200 |
| Python | 3.13.15 · pytest 9.1.1 · pytest-timeout (global ceiling 120s) |
| Node | 24.18.0 · npm 12.0.2 · vitest 5.0.0 · vite 8.3.0 |
| Rust | rustc 1.98.0 · cargo 1.98.0 |
| Git | 2.55.0.windows.1 |
| Browsers | project bundle at `build/verification-brief24/windows-final/Numera Browser/browsers/playwright` (chromium-1234 + chromium_headless_shell-1234), plus the installed camoufox |

## Final gates, on the candidate tree

Every run went through `.claude/hooks/Hook-Maker/Test-Run-Guard/scripts/Run-Tests-Guarded.ps1`
with `-ResultPath`, so each has a receipt; a receipt counts only at
`overall: ok`, `exitCode: 0`, `terminated: false`, `leakedProcessIds: []`.

| Gate | Command | Result | Wall | Receipt |
|---|---|---|---|---|
| Engine | `python -m pytest engine/tests -q --tb=short` | **2112 passed, 16 skipped** | 565s | ok / 0 / no leaks |
| Dashboard | `npm test` | **17 files, 109 tests passed** | 84s | ok / 0 / no leaks |
| Lint | `npm run lint` | clean | ~20s | ok / 0 |
| Build | `npm run build` | 1869 modules, 431.20 kB | <1s | ok / 0 |
| Rust | `cargo test --manifest-path desktop/src-tauri/Cargo.toml` | **15 passed** | ~45s | exit 0; see note |

**The Rust receipt's `leakedProcessIds: [63744]`** is the known cold-compile
artefact: the pid was verified gone (`Get-Process -Id 63744` → absent) and no
`cargo`/`rustc`/`rustdoc` process survived. Reported rather than suppressed.

**The live browser matrix really ran** — the failure mode where all eight
`*_live.py` modules skip at module level and the suite still reports a clean
pass. Confirmed from the durations, not the item count:
`test_webrtc_live.py::test_altered_hides_public_addresses` 20.00s,
`test_session_tree_live.py::...[camoufox]` 14.75s,
`test_relay_live.py::test_socks5_with_credentials_works_in_a_real_browser` 12.02s.

### Counts by area, from the focused runs during the work

| Suite group | Tests |
|---|---|
| helper lifecycle (5 files incl. 2 new) | 71 |
| vault (9 files incl. 1 new) | 133 |
| write contract / field policy (8 files incl. 3 new) | 356 |
| vault + helper + proctree + session ownership | 213 |

These overlap the engine total; they are listed because each was the evidence
for one finding before the integrated pass existed.

## Skips

16 skipped in the engine suite, unchanged from the 0.33.0 baseline: optional
backends not installed on this host and platform-gated POSIX cases. None is a
skip this change set introduced, and none masks a required assertion.

## CI: not run, and why

`global-test-rules.md` runs the heavy pass in GitHub CI and permits a local run
only when a live query shows CI cannot execute. Verified at
**2026-09-13T11:29:35Z**, on the day of this work rather than from memory:

    gh api repos/KiaroSama/Numera-Browser/actions/permissions
      -> {"enabled":true,"allowed_actions":"all"}       # not a repository-level disable
    gh run list --limit 1                                # run 34724953396, sha d40415b6
    jobs with steps: 0 of 5
    gh api .../check-runs/<job>/annotations
      -> "The job was not started because recent account payments have failed or
          your spending limit needs to be increased."

No job executed, so this is not a code or test failure and no commit can clear
it; the blocker is account billing and only the repository owner can lift it.
The runs the brief named (CI `34724953224`, Build `34724953396`) were re-read
rather than assumed — 0 of 22 jobs across them carry any step, and the missing
job log is consistent with a job that never started. **Nothing here is
CI-verified, and no row above claims to be.**

## Failures found by the integrated pass, and fixed

The engine suite ran five times before it was green. Each round found something
real; recording them because the cost of rediscovering them is another 10-26
minute pass.

| # | What failed | Diagnosis | Fix |
|---|---|---|---|
| 1 | `test_helper_lifecycle::test_an_unexpected_exception_still_ends_the_helper` — 3 failures in 5, in isolation | `proc.poll()` sampled with zero tolerance; Windows reports STILL_ACTIVE ~0.1s after `TerminateJobObject`. `alive_tree` was False every time | assert the strong oracle first, read `poll()` with a bounded wait. 0/8 after |
| 2 | `test_startup_handshake::test_a_probe_does_not_materialise_a_stale_snapshot` exceeded the 120s pytest ceiling | measured 19.7s warm / 33.9s cold / 82s cold-under-load; 19.7s matches the 0.33.0 baseline exactly, so **not a regression** — no margin | `@pytest.mark.timeout(330)`, above `HELPER_TIMEOUT` so the product's deadline fires first |
| 3 | `test_helper_lifecycle::test_shutdown_ends_a_helper_that_is_still_running` | `settle()` was not idempotent: `end_all` racing `run_helper`'s own settle read the already-cleared report as *lost evidence* and stranded a finished helper | `already SETTLED` check first inside the lock; release moved under it |
| 4 | two `BufferedReader`s left open (only visible under `-W error::ResourceWarning`) | an exception between spawn and wait bypassed every close | try/except around the whole post-spawn region that settles and closes |
| 5 | `test_session_tree_live[camoufox]` rendezvous timeout at 90s during a 26-minute run | contention: passed in isolation immediately after | none needed; recorded as environmental |

## Evidence locations

Receipts and the JUnit report are guarded-runner output under this session's
scratch directory and are not committed (they carry absolute local paths). The
durable evidence is: this manifest, `brief29-closure-ledger.md` and
`brief29-closure-ledger.json` beside it, and the regression modules themselves,
each of which fails on the pre-fix behaviour.
