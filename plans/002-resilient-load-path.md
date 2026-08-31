# Plan 002: Load a profile without crashing on one bad record

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/store.py engine/numera/models.py engine/numera/importers.py`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

Three separate places turn one malformed record into a total failure of the
operation around it. All three are the same shape - a strict parse with no
per-record tolerance - and the codebase already solved this pattern once, in
`vault.decrypt_for_read`, which swallows a decrypt failure and returns `None`.
The engine store diverged from its own sibling.

The practical trigger is ordinary: restore a backup taken on another machine, or
rotate the vault, and one profile's ciphertext no longer matches the current key.
`GET /api/profiles` then 500s and the entire dashboard is empty - not one broken
row, the whole list.

## Current state

- `engine/numera/store.py:202` - decrypt is unguarded:

  ```python
  node[b] = crypto.decrypt(pw, v["salt"], val) if (pw and self.unlocked()) else None
  ```

  `crypto.decrypt` raises `VaultLocked` on a wrong key (`crypto.py:80-81`), and
  `self.unlocked()` only checks the *current* vault verifier, not whether this
  record was encrypted with it.

- `engine/numera/store.py:283-291` - `list()` catches only
  `(json.JSONDecodeError, KeyError)`, so `VaultLocked` escapes:

  ```python
  except (json.JSONDecodeError, KeyError):
      continue  # skip corrupt files rather than crash
  ```

- `engine/numera/store.py:263-271` - `get()` catches nothing around the decrypt.

- `engine/numera/models.py:98-100` - `Proxy.from_dict` is the asymmetric one:

  ```python
  @classmethod
  def from_dict(cls, d: dict) -> "Proxy":
      return cls(**d)
  ```

  while `Fingerprint.from_dict` (`models.py:72-75`) filters to known fields
  precisely so an added field does not break older readers.

- `engine/numera/importers.py:137-140` - bare `int()` on export-supplied values:

  ```python
  hardware_concurrency=int(_first(d, "navigator.hardwareConcurrency", ...)),
  device_memory=int(_first(d, "navigator.deviceMemory", ...)),
  ```

  `POST /api/profiles/import` (`routes_profiles.py:369-385`) loops without a
  per-item guard and saves as it goes, so a `ValueError` half-way leaves the
  import partly applied.

## Scope

**In scope**: `engine/numera/store.py`, `engine/numera/models.py`,
`engine/numera/importers.py`, `engine/numera/server/routes_profiles.py` (import
loop only), `engine/tests/test_resilient_load.py` (create).

**Out of scope**: the vault rule itself; anything in `vault.py`.

## Steps

### Step 1: A bad secret degrades to None, it does not raise

Wrap the decrypt in `_apply_secrets` dec-mode so a failure yields `None`,
mirroring `vault.decrypt_for_read`. Add `crypto.VaultLocked` to the `list()`
catch so one unreadable record is skipped rather than ending the listing.

**Verify**: new test `test_one_undecryptable_profile_does_not_hide_the_others`.

### Step 2: `Proxy.from_dict` filters unknown keys

Match `Fingerprint.from_dict` exactly.

**Verify**: `test_a_proxy_with_an_unknown_field_still_loads`.

### Step 3: The importer tolerates a non-numeric count

Use a tolerant int coercion falling back to the base default, matching the
existing `_parse_resolution` guard at `importers.py:74-79`. Guard the import
route's per-profile loop so one bad entry is reported without aborting the batch
or leaving it half-applied.

**Verify**: `test_an_import_with_a_bad_core_count_does_not_500`.

## Test plan

New file `engine/tests/test_resilient_load.py`, modelled on
`engine/tests/test_vault_covers_the_root.py`:

- Encrypt a profile under vault A, switch to vault B, assert `list()` returns
  the other profiles and does not raise.
- A profile whose stored `proxy` has an extra key loads.
- An import payload with `"cores": "8 cores"` returns a 4xx or imports with the
  default, and leaves no half-written batch.

## Done criteria

- [ ] Offline suite passes
- [ ] Three new tests exist, pass, and were each confirmed able to fail
- [ ] `plans/README.md` row updated

## STOP conditions

- Making the decrypt tolerant causes a *locked* profile to return ciphertext
  anywhere - that is the opposite failure and is worse. Stop and report.

## Maintenance notes

- `Proxy` and `Fingerprint` now both tolerate unknown keys. Any third
  `from_dict` added later must do the same or it becomes the next version of
  this bug.

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

