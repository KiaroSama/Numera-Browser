# Plan 012: Cover the Editor derivations and the bulk generator; add ESLint

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- .`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/009-posix-dashboard-and-ci.md
- **Category**: tests
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

The dashboard is 4,400 lines with five tests, and the untested part is the
feature the product exists for. `Editor.jsx` derives a whole coherent identity
from two dropdowns - pick an OS and the user agent, screen, cores, memory and
GPU string all move together; pick a locale and the timezone follows. If one of
those derivations breaks, the app ships a Windows user agent with a macOS GPU,
which is exactly the contradiction an anti-detect tool exists to avoid, and
nothing fails.

The bulk generator is the other one: it produces up to 10,000 profiles from
spread arithmetic, and its `engine: engine || null` contract matters because the
server treats `null` as "you decide" but rejects `""`.

Do this **after plan 009**, which makes CI actually run `npm test`. Before that,
new frontend tests gate nothing.

## Current state

- `dashboard/test/` holds `auth-lockout.test.jsx` and `dev-proxy.test.js` - five
  tests, the structural pattern to follow (they render against a fake engine).
- `dashboard/src/views/Editor.jsx:208` - `changeOS` derives `userAgent`,
  `screen`, `cores`, `memory`, `webglRenderer`.
- `dashboard/src/views/Editor.jsx:209` - `changeLocale` derives `timezone`.
- `dashboard/src/views/Editor.jsx:~395` - the WebGL vendor is derived from the
  renderer on save; `:~701` - `disabled={!p.name}` gates
  `onSave({ ...p, _new: false })`.
- `dashboard/src/views/bulk.jsx:~108` - `BulkModal.build()`: OS spread
  `oses[i % oses.length]`, country spread
  `countries[Math.floor(i / oses.length) % countries.length]`, name padding
  `Math.max(2, String(start + n - 1).length)`, and `engine: engine || null`.
  `SyncModal` in the same file builds a patch that deliberately omits blank
  fields.
- No JS linter exists anywhere in the repo.

## Scope

**In scope**: `dashboard/test/**` (new files), `dashboard/package.json` (scripts
and devDependencies), `dashboard/eslint.config.js` (create), and minimal edits
in `dashboard/src/` only where a lint error is real or a pure function needs
extracting to be testable.

**Out of scope**: `dashboard/README.md`, the `version` field, the existing
`vite.config.js` proxy and test blocks, and everything outside `dashboard/`.

## Steps

### Step 1: Editor derivation tests

Render `Editor` with a fixture profile, drive the OS and locale controls, assert
the derived fields change together and coherently. Assert save is disabled with
an empty name and that the `onSave` payload carries `_new: false`. If a
derivation is awkward to reach through the DOM, extracting it to a small pure
helper is acceptable - keep behaviour identical and the diff minimal.

### Step 2: Bulk generator tests

Cover the spread arithmetic, the zero-padding, the `null`-engine contract, and
`SyncModal`'s omit-blank behaviour.

### Step 3: ESLint

Add a flat config with recommended JS plus React hooks rules, a `lint` script,
and the devDependencies it needs. Fix the real errors it surfaces. Do not
mass-disable rules to reach zero; if one rule is genuinely wrong here, disable
that rule in config with a one-line reason.

## Test plan

- Model new tests on `dashboard/test/auth-lockout.test.jsx`.
- **Every new test must be proven able to fail**: break the derivation or the
  spread, watch it go red, restore.

## Done criteria

- [ ] `cd dashboard && npm run lint` exits 0
- [ ] `npm test` passes, with the new tests included
- [ ] `npm run build` exits 0
- [ ] Each new test was confirmed able to fail, and how is reported

## STOP conditions

- ESLint surfaces more than a handful of real errors in `src/` - report the list
  rather than making a large unreviewed sweep inside a test-focused change.

## Maintenance notes

- These tests only protect anything once CI runs `npm test` (plan 009). If that
  step is ever removed, they silently stop gating.

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

