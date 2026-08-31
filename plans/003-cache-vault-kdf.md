# Plan 003: Cache the vault key derivation

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/crypto.py`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

Every single encrypt and decrypt re-runs 200,000 rounds of PBKDF2-HMAC-SHA256 to
derive a key that is identical every time - the vault has one salt, fixed when it
was created. Listing profiles decrypts every profile; listing the proxy pool
decrypts every proxy; and `_save_entries` re-encrypts every entry that has a
password on every write. So a single proxy "test all" on a pool of N does on the
order of N-squared key derivations at tens of milliseconds each, and the 3-second
dashboard poll pays it repeatedly.

This is one cached function. It is the highest ratio of win to risk in the audit.

## Current state

`engine/numera/crypto.py:29` and `:57-61`:

```python
PBKDF2_ROUNDS = 200_000
...
def _fernet(password: str, salt_b64: str):
    from cryptography.fernet import Fernet
    salt = base64.b64decode(salt_b64)
    key = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt, PBKDF2_ROUNDS)
    return Fernet(base64.urlsafe_b64encode(key))
```

Callers: `crypto.encrypt` (`:68`), `crypto.decrypt` (`:74`), reached from
`vault.encrypt_for_write` / `vault.decrypt_for_read`, and from
`ProxyPool.list()` (`server/stores.py:249`) and `DashboardStore.list()`
(`server/stores.py:50`), which decrypt every record.

## Scope

**In scope**: `engine/numera/crypto.py`, `engine/tests/test_crypto_cache.py` (create).

**Out of scope**: the iteration count itself (see maintenance notes), the
`enc:v1:` format, and every caller - this must be a drop-in change.

## Steps

### Step 1: Derive once per (password, salt)

Extract the derivation into its own function and memoise it with a bounded cache
(`functools.lru_cache(maxsize=8)` is enough - a process has one vault). Build the
`Fernet` from the cached key. The password is already resident in memory as
`engine._password`, so caching the derived key adds no new exposure; say that in
a comment so a future reader does not "fix" it.

**Verify**: `test_the_key_is_derived_once_per_password_and_salt` - patch
`hashlib.pbkdf2_hmac` with a counter, encrypt and decrypt several times with the
same password/salt, assert the counter is 1.

### Step 2: Prove nothing else changed

**Verify**: existing vault suites still pass unchanged:
`engine/tests/test_vault_covers_the_root.py`, `test_vault_dashboard.py`.

## Test plan

New `engine/tests/test_crypto_cache.py`:

- One derivation for many operations with the same password+salt.
- A *different* password or salt derives again (the cache is keyed, not global).
- Round-trip still works: `decrypt(encrypt(x)) == x`.

## Done criteria

- [ ] `test_crypto_cache.py` passes; the counter test was confirmed able to fail
      (remove the cache and watch it go red)
- [ ] `engine/tests/test_vault_covers_the_root.py` and `test_vault_dashboard.py`
      pass unchanged
- [ ] Offline suite passes

## STOP conditions

- Any existing vault test fails. The cache must be invisible to behaviour.

## Maintenance notes

- **Deliberately deferred**: raising `PBKDF2_ROUNDS` from 200k toward current
  OWASP guidance (~600k). It is real hardening, but existing vaults are tied to
  the current count, so it needs a versioned KDF - the `enc:v1:` prefix
  (`crypto.py:32`) is the natural place to carry the version. Do that as its own
  change, with a re-derive on unlock, not as part of this cache.
- If a future feature holds several vaults open at once, raise `maxsize`.

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

