# Plan 007: Finish delete/session and folder concurrency semantics

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/store.py engine/numera/folders.py`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

Two leftovers from the last atomicity pass.

Profile metadata deletion is rollback-aware now, but the session directory is
still removed with `ignore_errors=True`. On Windows a running Chromium holds its
files, so the delete quietly fails and the API reports success while
`user-data/<id>` is still there - the profile is gone from every list but its
browsing session survives on disk. For an anti-detect tool that is both a
correctness and a privacy problem.

Folder operations rewrite the folder list and every profile in the folder, and
they now roll back on failure - but they take no lock. Two concurrent folder
edits each read the list, each write their own version, and one silently loses.

## Current state

- `engine/numera/store.py:316`:

  ```python
  shutil.rmtree(ud, ignore_errors=True)
  ```

  inside `delete_session`, which `DashboardStore.delete` calls last, after both
  metadata records are gone.

- `engine/numera/folders.py:77` - `_persist(root, names)` reads
  `dashboard-config.json` via `_read_meta` and writes it via `_write_meta` with
  no lock around the pair. `create` (`:88`), `rename` (`:104`) and `delete`
  (`:118`) all end in `_persist`, and `rename`/`delete` additionally rewrite
  every profile in the folder inside a `write_batch()`.

## Scope

**In scope**: `engine/numera/store.py` (`delete_session` and its caller
contract), `engine/numera/folders.py`, `engine/tests/test_failure_atomicity.py`
(extend - it already holds the delete and folder tests).

**Out of scope**: the metadata-delete rollback that already works, and
`paths.write_batch` itself.

## Steps

### Step 1: Never report a session removed when it was not

Replace the silent `ignore_errors=True` with a recoverable strategy. The
straightforward one that works on Windows: atomically rename the session
directory to a tombstone first (a rename fails loudly if the directory is held),
then delete the tombstone; if the rename fails, refuse *before* the metadata is
committed so the profile is not left orphaned. Whichever you choose, a failure
must surface - never a success report over a surviving session.

**Verify**: `test_a_session_that_cannot_be_removed_is_reported`.

### Step 2: One lock around the folder transaction

Give the folder operations a shared lock keyed on the data root (use
`paths._lock_for` on the metadata file, which is the same mechanism the other
stores use), taken around the whole read-modify-write - the folder list *and*
the profile rewrites - so two concurrent operations serialise instead of
overwriting each other. Keep the existing `write_batch()` rollback inside it.

**Verify**: `test_concurrent_folder_changes_both_survive`.

## Test plan

Extend `engine/tests/test_failure_atomicity.py` (it already has
`test_the_session_is_removed_only_after_both_records` and the folder rollback
tests - match their style):

- Inject a failure into the session rename/delete; assert the call reports
  failure and does not claim success, and that nothing is left in a half state.
- Two threads performing independent folder operations (create "a" / create
  "b", or rename one folder while creating another): assert both results are
  present afterwards. Model the threading on
  `test_concurrent_schedule_writes_do_not_lose_each_other` in the same file.

## Done criteria

- [ ] `grep -n "ignore_errors=True" engine/numera/store.py` no longer matches
      the session-delete path (the restore-staging uses are fine)
- [ ] New tests pass; both confirmed able to fail
- [ ] Offline suite passes

## STOP conditions

- Making the session delete strict causes existing delete tests to fail because
  a temp directory is genuinely locked on this machine - report; do not revert
  to `ignore_errors`.

## Maintenance notes

- The folder lock and the schedule/config locks all come from
  `paths._lock_for`. Locking two of them in different orders in one operation
  would deadlock - if a future feature needs both, define the order once.

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

