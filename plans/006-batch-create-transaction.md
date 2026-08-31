# Plan 006: Make batch create all-or-nothing on storage failure

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/server/routes_profiles.py engine/numera/server/stores.py`
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

Batch create validates the whole payload before writing, which was the last
fix - so a malformed entry writes nothing. But the writes themselves are still a
plain sequential loop, so a *storage* failure part-way (disk full, permission
error, a failing engine mirror) leaves the earlier profiles on disk while the
request reports failure. The caller retries, and now there are duplicates. The
requirement was true all-or-nothing.

The repo already has the tool for this: `paths.write_batch()` restores every file
it wrote if anything inside the block raises. It is used by `folders` and by the
vault migration.

## Current state

`engine/numera/server/routes_profiles.py` - after the validation pass:

```python
ids = []
last_engine = None
for p in incoming:
    last_engine = p.get("engine")
    ids.append(dash.save(p)["id"])
```

Each `dash.save` writes the dashboard copy and mirrors an engine copy
(`server/stores.py` - `save()` already rolls back its *own* two files if the
mirror fails, but nothing rolls back across iterations).

## Scope

**In scope**: `engine/numera/server/routes_profiles.py` (the batch route),
`engine/numera/server/stores.py` (add a batch-save entry point),
`engine/tests/test_write_contracts.py` (extend - it already holds the batch
tests).

**Out of scope**: the validation pass (already correct), single create, bulk
update.

## Steps

### Step 1: A store-level batch save that rolls back both copies

Add a method on `DashboardStore` that saves many profiles as one transaction,
covering the dashboard file *and* the engine mirror for every entry. Build it on
`paths.write_batch()` so the rollback story is the one the repo already uses;
the engine mirror needs its files registered with the same batch.

**Verify**: step 2's injected-failure test.

### Step 2: The route uses it

Replace the loop with the batch call. Keep the existing return shape
(`{"created": N, "ids": [...]}`) - clients depend on it.

**Verify**: `test_a_batch_that_fails_on_the_third_write_stores_none_of_them`.

## Test plan

Extend `engine/tests/test_write_contracts.py` (it already has
`test_a_minimal_batch_entry_is_normalised_on_disk` and friends - match their
style):

- Monkeypatch the writer to raise on the Nth call. Assert the response is a 5xx
  or a clear error, **and** that zero batch profiles exist on disk in both
  `dashboard/` and `profiles/`.
- Keep and re-run the existing malformed-entry and unique-seed tests unchanged -
  they must not regress.

## Done criteria

- [ ] `engine/tests/test_write_contracts.py` passes, including the existing
      batch tests and the new rollback test
- [ ] The rollback test was confirmed able to fail (remove the transaction and
      watch profiles survive)
- [ ] Offline suite passes

## STOP conditions

- Rolling back the engine mirror turns out to need changes inside
  `ProfileStore.save` - that is a wider blast radius than this plan; stop and
  report rather than refactoring the engine store.

## Maintenance notes

- Batch create and the vault migration now both rely on `paths.write_batch()`.
  A change to that helper affects both; its tests live in
  `engine/tests/test_failure_atomicity.py`.

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

