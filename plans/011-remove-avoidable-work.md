# Plan 011: Stop the avoidable re-reads, re-derivations and re-renders

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/server/stores.py engine/numera/server/routes_profiles.py engine/numera/server/routes_proxies.py dashboard/src/App.jsx`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/003-cache-vault-kdf.md
- **Category**: perf
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

Four independent pieces of repeated work, all on paths the dashboard hits every
few seconds. None changes behaviour; each removes work that was never needed.

Do this **after plan 003** (the key-derivation cache). Most of the cost the
proxy loop pays today is PBKDF2, so measuring this first would credit the wrong
change.

## Current state

- `engine/numera/server/stores.py:304` - `ProxyPool.get()` is
  `next((p for p in self.list() if p.get("id") == pid), None)`: a full file
  read, parse and decrypt per single-id lookup. Callers do it in a loop -
  `routes_proxies.py:113` (test), `:190` (rotate), `:221` (archive), `:229`
  (assign), and four sites in `cli/pool.py`.
- `engine/numera/server/stores.py:307-319` and `:262-270` - `update()` reads the
  file via `list()`, then `_save_entries()` reads it *again* via `_stored()` to
  rebuild the by-id map. Two full reads per single-field update.
- `engine/numera/server/routes_profiles.py:199-205` - `GET /api/profiles` calls
  `dash.list()`, then `engine_store.list()`, then possibly `dash.save(...)`, then
  `dash.list()` a second time for the response. The dashboard polls this every
  3 seconds.
- `engine/numera/server/routes_profiles.py:417` and `:425` -
  `profiles_health` converts every profile with `to_engine_profile` for
  collision detection, then `dashboard_coherence(d)` converts each one again
  internally (`server/convert.py:300`).
- `dashboard/src/App.jsx:96` and `:343` - `<style>{makeCss()}</style>` rebuilds a
  ~550-line stylesheet on every render. Its output is invariant: theme switching
  is done with a `data-theme` attribute, and `makeCss` (`theme.js:79`) takes no
  arguments.

## Scope

**In scope**: the five sites above, and
`engine/tests/test_perf_contracts.py` (create).

**Out of scope**: the locking semantics of `ProxyPool.update` - the
read-inside-the-lock behaviour was added to fix a lost-status bug and must be
preserved. Any bulk writer must hold the lock across the whole batch.

## Steps

### Step 1: Resolve ids from one snapshot

In the routes and CLI paths that loop, take `pool.list()` once, build a
`{id: entry}` map, and work from it instead of calling `pool.get()` per id.

### Step 2: One read per pool write

Add a bulk update that takes the lock once, applies all patches to one in-memory
list and writes once; have `_save_entries` avoid the second `_stored()` read when
the caller already has the stored map.

### Step 3: List profiles once per request

Return the list built during reconciliation instead of re-listing. Keep the
CLI-adoption scan (it exists so a headless-created profile becomes visible), but
do not run it twice per request.

### Step 4: Convert each profile once in health

Convert once and reuse the `Profile` for both the collision set and the
coherence check.

### Step 5: Build the stylesheet once

Hoist `makeCss()` to module scope in `dashboard/src/App.jsx` (or `useMemo` with
an empty dependency list) and render the constant.

## Test plan

New `engine/tests/test_perf_contracts.py` - assert the *contract*, not a
duration (timing tests are flaky):

- Count file reads: patch `Path.read_text` with a counter and assert
  `GET /api/profiles` reads each profile file once, and a pool "test all" of N
  proxies does not scale reads with N-squared.
- Assert `to_engine_profile` is called once per profile in `profiles_health`.
- Existing pool and profile suites pass unchanged - especially
  `engine/tests/test_failure_atomicity.py::test_concurrent_schedule_writes_do_not_lose_each_other`
  and the pool concurrency tests, which guard the locking this must not break.

For the dashboard, assert `makeCss` is invoked once across several renders.

## Done criteria

- [ ] Offline suite passes, including every existing concurrency test
- [ ] New read-count tests pass and were confirmed able to fail
- [ ] `cd dashboard && npm test && npm run build` pass
- [ ] Record before/after wall time for a pool "test all" and for
      `GET /api/profiles` with the vault enabled - "feels faster" is not evidence

## STOP conditions

- Any pool concurrency test fails. The lost-status bug these locks fixed is
  worse than the performance they cost; stop and report.

## Maintenance notes

- The bulk pool writer holds the lock for the whole batch. If a future caller
  needs to interleave reads, it must not take the same lock again from inside.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Engine tests (offline) | `pwsh -NoProfile -File ./.claude/hooks/Hook-Maker/Test-Run-Guard/scripts/Run-Tests-Guarded.ps1 -FilePath ".venv/Scripts/python.exe" -ArgumentsJson '["-m","pytest","engine/tests","-q","--no-header","-m","not slow"]' -TimeoutSeconds 800 -IdleTimeoutSeconds 250` | all pass |
| One test file | same runner, `["-m","pytest","engine/tests/<file>.py","-q","--no-header"]` | all pass |
| Live browser tests | same runner with `["-m","pytest","engine/tests","-q","--no-header","-m","slow"]` | all pass |
| Dashboard | `cd dashboard && npm test && npm run build` | both exit 0 |
| Rust | `cd desktop/src-tauri && cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test` | all exit 0 |

A bare `python -m pytest` is refused by a repo hook - always use the guarded
runner above. A full run (offline + live) takes about 12 minutes and needs a
1700s ceiling; 800s is enough for offline only.

## Repo conventions to match

- Comments and docstrings explain **why**, and name the concrete defect they
  prevent, in plain prose. Look at `engine/numera/vault.py` for the voice - e.g.
  "Dropped, not refused. `GET /api/profiles` returns `status` and `lastActive`
  ... Refusing that made the API reject its own output."
- Test names are sentences: `test_minting_the_first_token_protects_the_running_app`.
- No emoji. Use `-` not em-dashes.
- **Every new test must be able to fail.** After writing one, break the code it
  covers, confirm it goes red, then restore. A test that cannot fail is not a
  test - this repo has shipped three of those and each is recorded in
  `.ai/LESSON.md`.
- Atomic writes go through `paths.write_json`; single-file read-modify-write
  through `paths.mutate_json`; multi-file transactions through
  `paths.write_batch()`. Never `write_text` a JSON store directly.

