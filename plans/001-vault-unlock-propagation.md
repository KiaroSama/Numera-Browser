# Plan 001: Make the vault unlock reach every subprocess and fail closed when locked

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/cli engine/numera/server engine/numera/store.py engine/numera/proxypool.py`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

The global `--password` flag is advertised as the way to unlock the vault, and
for the two long-running surfaces it does nothing. `numera --password X serve`
unlocks a `ProfileStore` and then throws it away: the server builds its own
locked one. Every browser the server launches therefore gets no proxy password,
and the profile silently browses without its proxy - the single worst failure
mode for an anti-detect tool, because it looks like it worked.

The second half is the mirror image. A locked read returns `None` for a secret
*by design* (handing back ciphertext would let the launcher authenticate with an
encrypted blob). But several operations treat that `None` as a real empty
password: a backup claims to contain the proxy credential and does not, a proxy
health check tests the wrong credential and records a false failure, and pool
assignment writes `pass=None` over a profile. Those operations must refuse while
locked, not quietly produce a wrong artifact.

## Current state

- `engine/numera/cli/setup.py:427-431` - `cmd_serve(args, store)` receives an
  already-unlocked `store` and discards it:

  ```python
  def cmd_serve(args, store: ProfileStore) -> int:
      from ..server import serve
      serve(host=args.host, port=args.port, data_dir=args.data_dir,
            verbose=getattr(args, "verbose", False))
      return 0
  ```

- `engine/numera/cli/setup.py:434-437` - `cmd_mcp` does the same, passing only
  `str(store.root)`.
- `engine/numera/server/__init__.py:98` - `def create_app(data_dir=None, start_scheduler=True)`
  builds `engine_store = ProfileStore(data_dir)` at line 105 with no password.
- `engine/numera/server/routes_browser.py:74-77` - `_run_cli` spawns children
  with no `env=`, so they inherit nothing:

  ```python
  proc = subprocess.run(
      [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root), *cli_args],
      capture_output=True, text=True,
  )
  ```

- `engine/numera/store.py:196-202` - a locked read sets the secret to `None`
  deliberately. That is correct and must not change.

## Scope

**In scope**:
- `engine/numera/cli/setup.py`, `engine/numera/cli/pool.py`
- `engine/numera/server/__init__.py`, `engine/numera/server/routes_browser.py`,
  `engine/numera/server/routes_proxies.py`
- `engine/numera/mcp.py`
- `engine/numera/store.py`, `engine/numera/proxypool.py`, `engine/numera/vault.py`
- `engine/tests/test_vault_propagation.py` (create)

**Out of scope**:
- The locked-read-returns-None rule in `store.py:_apply_secrets`. It is load
  bearing; three prior audits confirmed it. Do not "fix" it by returning
  ciphertext.
- `dashboard/**`, `.github/**` - other work is in flight there.

## Steps

### Step 1: Let the password travel by parameter

Add `password: Optional[str] = None` to `create_app()` and `serve()` in
`engine/numera/server/__init__.py`, and pass it into the `ProfileStore(...)`
construction. Do the same for `mcp.serve()`. Then have `cmd_serve` and `cmd_mcp`
forward `store._password`.

**Verify**: `.venv/Scripts/python.exe -c "import inspect,sys; sys.path.insert(0,'engine'); from numera.server import create_app, serve; print('password' in inspect.signature(create_app).parameters, 'password' in inspect.signature(serve).parameters)"` -> `True True`

### Step 2: Let it travel to children by environment, never argv

Every subprocess the server spawns must inherit `NUMERA_PASSWORD` when the
parent is unlocked. Build the env once, in one helper, and use it for
`_run_cli`, launch, attach and warmup. **The password must never appear in
argv** - argv is world-readable in the process table on both platforms.

**Verify**: `grep -rn "password" engine/numera/server/routes_browser.py` shows
env use and no argv use.

### Step 3: Add a raw-secret check and refuse locked secret-dependent work

Add a helper (e.g. `vault.has_stored_secret(engine, raw_value)`) that answers
"does a secret exist on disk here" without decrypting. Use it so these refuse
with a clear message while locked rather than emitting a wrong artifact:
profile `export`/`backup`, proxy-pool export, proxy `check`/`rotate`, and pool
`assign`. Non-secret listing and edits stay available while locked.

**Verify**: the new tests in step 4 pass.

### Step 4: Regressions

Create `engine/tests/test_vault_propagation.py`.

## Test plan

Model on `engine/tests/test_vault_covers_the_root.py` (same fixtures: build a
root with an encrypted proxy password, then assert behaviour).

- `numera --password X serve` reaches a `ProfileStore` that is unlocked - assert
  through `create_app(data_dir, password=...)` that the engine store is unlocked.
- A child command inherits the unlock via env, and **the password is not in
  argv** (assert on the captured argv list).
- With the vault locked: `export`, `backup`, pool export, `check`, `rotate` and
  `assign` each refuse rather than emitting a blank/None credential.
- With the vault unlocked those same operations succeed.

## Done criteria

- [ ] Offline engine suite passes
- [ ] `grep -rn "NUMERA_PASSWORD" engine/numera/server/` shows env propagation
- [ ] No occurrence of the password in any argv list built in `server/`
- [ ] New tests exist and pass; each was confirmed able to fail
- [ ] `plans/README.md` row updated

## STOP conditions

- The locked read in `store.py` no longer returns `None` (someone changed the
  rule this plan is built on).
- A refusal you add breaks an existing passing test that asserts the operation
  works while locked - report which, do not weaken the test.

## Maintenance notes

- Any new subprocess spawn in the server must use the same env helper. A new
  spawn that forgets it reintroduces exactly this bug silently.
- The password must stay out of argv forever; a reviewer should grep for it.

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

