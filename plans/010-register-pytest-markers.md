# Plan 010: Register pytest markers so the offline/live split cannot drift

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/pyproject.toml engine/tests`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

The whole test workflow depends on one marker. Offline runs use
`-m "not slow"`; the live browser suite uses `-m slow`. That marker is not
registered, so pytest warns on every run and - more importantly - a typo like
`@pytest.mark.slwo` is silently accepted as a brand new marker. A live browser
test mis-marked that way would be pulled into the offline suite, where it would
either fail confusingly or, worse, be skipped by its own guard and quietly stop
protecting anything.

Only three of the seven `*_live.py` files actually carry the marker, so a local
`-m slow` run already misses four of them. CI happens to catch them by globbing
`tests/test_*_live.py`, which is why nobody noticed.

## Current state

`engine/pyproject.toml`:

```toml
[tool.pytest.ini_options]
testpaths = ["tests"]
```

No `markers`, no `--strict-markers`, no `filterwarnings`. `@pytest.mark.slow`
appears in `test_client_hints_live.py:26`, `test_dead_controls_live.py` (6
occurrences) and `test_launch_extras_live.py:22`. `test_extensions_live.py`,
`test_parity_live.py`, `test_relay_live.py` and `test_webrtc_live.py` have no
marker.

## Scope

**In scope**: `engine/pyproject.toml`, and the `*_live.py` files that need a
consistent marker.

**Out of scope**: test bodies, the CI glob, the guarded runner.

## Steps

### Step 1: Register the marker and make unknown ones an error

Add to `[tool.pytest.ini_options]`:

```toml
markers = ["slow: needs a real browser; excluded from the offline run"]
addopts = "--strict-markers"
```

### Step 2: Mark every live file consistently

Add `pytestmark = pytest.mark.slow` to the four `*_live.py` files that lack it,
so `-m slow` and `-m "not slow"` sort all seven the same way CI's glob does.

**Verify**: `-m "not slow"` collects zero `*_live.py` tests, and `-m slow`
collects them all.

## Test plan

No new test file; the verification is the collection itself:

- Guarded runner with `["-m","pytest","engine/tests","-q","--no-header","-m","not slow","--collect-only"]`
  -> no `_live.py` test ids in the output.
- Same with `-m slow` -> all seven live files represented.
- No `PytestUnknownMarkWarning` in either run.

## Done criteria

- [ ] Offline suite passes with no `PytestUnknownMarkWarning`
- [ ] Live suite passes and collects all seven `*_live.py` files
- [ ] `grep -n "strict-markers" engine/pyproject.toml` matches

## STOP conditions

- `--strict-markers` surfaces an existing typo'd marker elsewhere in the suite.
  That is the point of the change - fix the typo, but report it, because it
  means a test has not been running the way its author believed.

## Maintenance notes

- With `--strict-markers` on, a new marker must be declared in `pyproject.toml`
  before use. That is the intended friction.

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

