# Plan 005: Only a real engine choice may change the global default

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/server/routes_profiles.py`
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

The global default engine is what every future profile is created with. Today it
can be changed by edits that say nothing about engines at all: renaming a
profile, adding a tag, or bulk-editing notes across a selection. The last
profile touched wins. A user who sets their default to `camoufox`, then renames
an old `playwright` profile, silently gets `playwright` for everything they
create afterwards - with no UI anywhere saying so.

## Current state

`engine/numera/server/routes_profiles.py`:

- `:185` - `_remember_engine(profile)` sets the global default from
  `profile["engine"]` whenever that is truthy.
- `:218-222` - **create** gets this right: it records `chose_engine` *before*
  normalisation and only remembers on an explicit choice.

  ```python
  chose_engine = bool(profile.get("engine"))
  _validate_profile(profile)
  _apply_default_extensions(profile)
  if chose_engine:
      _remember_engine(profile)          # an explicit choice, worth remembering
  ```

- `:233` - **update** calls `_remember_engine(profile)` unconditionally, for
  every `PUT`, whatever the patch contained.
- `:283-288` - **batch** takes `last_engine = p.get("engine")` *after* the loop
  has auto-filled missing engines from the current default, so in a mixed batch
  an auto-filled final entry overrides an explicit choice made earlier.
- `:327` and `:383` - **bulk** calls `_remember_engine(d)` per touched profile
  even when the patch has no `engine` key.

## Scope

**In scope**: `engine/numera/server/routes_profiles.py`,
`engine/tests/test_default_engine.py` (create).

**Out of scope**: `engine_store.set_default_engine` itself and the config
plumbing - the storage is fine, only the decision to call it is wrong.

## Steps

### Step 1: Decide explicitness before normalisation, everywhere

Follow the pattern create already uses. Capture whether *this request* named an
engine, before any auto-fill runs, and only remember when it did.

- **Update**: remember only when the request body carried an `engine` **and** it
  differs from the stored profile's engine.
- **Bulk**: remember only when the patch itself carried an `engine`.
- **Batch**: capture the explicit engine per entry before auto-fill; if any
  entry named one, the last *explicit* one wins - never an auto-filled value.

**Verify**: the tests in step 2.

### Step 2: Regressions

Create `engine/tests/test_default_engine.py`.

## Test plan

Modelled on `engine/tests/test_store_correctness.py` (same `c` TestClient
fixture and `stored()` helper).

- Set the default to `camoufox`. `PUT` a name-only change to a profile whose
  engine is `playwright`. Assert the default is still `camoufox`.
- Same for a tag-only bulk edit across profiles with a different engine.
- A `PUT` that *does* name a new engine changes the default.
- A mixed batch - entry 1 explicitly `camoufox`, entry 2 with no engine - leaves
  the default at `camoufox`, not at whatever entry 2 was auto-filled with.

## Done criteria

- [ ] Offline suite passes
- [ ] Four new tests pass; each confirmed able to fail
- [ ] `grep -n "_remember_engine" engine/numera/server/routes_profiles.py` shows
      no unconditional call on the update or bulk paths
- [ ] `plans/README.md` row updated

## STOP conditions

- An existing test asserts that an ordinary update updates the default - report
  it; that test encodes the bug and needs replacing, not the fix weakening.

## Maintenance notes

- Any new write route must decide explicitness before normalisation. The auto-
  fill runs first, so reading `profile["engine"]` after it can never tell you
  what the caller asked for.

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

