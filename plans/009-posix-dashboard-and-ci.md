# Plan 009: Ship the dashboard in Linux/macOS artifacts; run dashboard tests in CI; fix vault docs

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- .github/workflows scripts/smoke-portable.sh SECURITY.md engine/README.md README.md`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

The Linux and macOS release artifacts ship no user interface. The workflow
copies the built dashboard into `engine/numera/web` and then freezes with
`--collect-submodules numera`, which collects Python *modules* - it does not
collect package data, so `numera/web` is dropped. Windows gets it right with an
explicit `--add-data`. And the POSIX smoke test never requests `/`, so an
artifact passes its own gate while serving an API and a blank page.

Two smaller things ride along: CI builds the dashboard but never runs its test
suite, so the regressions guarding the auth-lockout and dev-proxy fixes cannot
gate anything; and the vault docs contradict themselves after the 0.12.0 change.

## Current state

- `.github/workflows/build-desktop.yml:114-116`:

  ```yaml
  cp -r dashboard/dist engine/numera/web
  pyinstaller --noconfirm --name numera-engine --distpath desktop/dist/.build \
              --collect-all uvicorn --collect-submodules numera \
              engine/numera/__main__.py
  ```

  versus `scripts/build-desktop.ps1`, which passes
  `--add-data "$repo\dashboard\dist;numera/web"`. On POSIX the separator is `:`.
- `scripts/smoke-portable.sh` checks the bundled browser and `/api/ready`, never `/`.
- `.github/workflows/ci.yml` `dashboard:` job runs `npm ci` then `npm run build`;
  `dashboard/package.json` has `"test": "vitest run"` and nothing calls it.
- `SECURITY.md` states the vault covers the dashboard store and the pool, then
  later still says the dashboard's own profile store is not covered.
- `engine/README.md` still says the dashboard profile store is not encrypted.
- `README.md:172` pins `numera_engine-0.3.0-py3-none-any.whl`; the version is
  0.12.0, so the documented install line fails.

## Scope

**In scope**: `.github/workflows/build-desktop.yml`, `.github/workflows/ci.yml`,
`scripts/smoke-portable.sh`, `SECURITY.md`, `engine/README.md`, `README.md`.

**Out of scope**: `scripts/build-desktop.ps1` (Windows already correct),
`engine/**`, `dashboard/**`.

## Steps

### Step 1: Include the package data in the POSIX freeze

Add an explicit `--add-data` (POSIX `:` separator) or `--collect-data numera`
for `numera/web`.

### Step 2: Make the smoke test prove the UI exists

Extend `scripts/smoke-portable.sh` to request `/`, assert the response is the
real dashboard HTML (read `engine/numera/server/spa.py` to pick a meaningful
marker - the app root element or title, not just a 200), then parse one asset
path out of that HTML and assert it is fetchable. Keep every existing check,
especially the real bundled-browser launch. Keep waits bounded - no blind sleep.

### Step 3: Run the dashboard tests in CI

Add a `npm test` step to the `dashboard:` job after `npm ci`.

### Step 4: Make the docs true

Fix the `SECURITY.md` self-contradiction and the `engine/README.md` claim, and
replace the pinned wheel version with the `<version>` placeholder style already
used by `scripts/build-server.ps1`. Update any portable-size or dependency claim
made false by bundling Chromium (the Windows portable folder is now ~590 MB).

## Test plan

This plan's verification is the artifact itself, and it cannot fully run here -
see STOP conditions.

- `bash -n scripts/smoke-portable.sh` parses.
- Both workflow files parse as YAML.
- Every CLI flag used in the smoke script exists (check `--help`; a previous
  pass shipped `create --name` and `serve --no-browser`, neither of which exist).

## Done criteria

- [ ] Both workflows parse; `bash -n scripts/smoke-portable.sh` exits 0
- [ ] `grep -n "add-data\|collect-data" .github/workflows/build-desktop.yml` matches
- [ ] `grep -n "npm test" .github/workflows/ci.yml` matches
- [ ] No remaining claim that the dashboard store is unencrypted
- [ ] `grep -n "0\.3\.0" README.md` returns nothing

## STOP conditions

- **Known and expected**: GitHub Actions has never executed on this repository
  (an account billing block), and a Linux/macOS artifact cannot be built on the
  Windows development host. Do not claim CI is green and do not claim the POSIX
  artifact was smoke-tested. State both as unverified.

## Maintenance notes

- The Windows and POSIX freezes now both carry the dashboard, by different
  flags. A change to one must be mirrored; the smoke script is the shared gate
  that catches it.

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

