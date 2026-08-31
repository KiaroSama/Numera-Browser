# Plan 004: Close the SSRF and bind-guard gaps

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/proxypool.py engine/numera/server/__init__.py`
> If any in-scope file changed since this plan was written, compare the "Current
> state" excerpts against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: security
- **Planned at**: commit `798326a`, 2026-09-01

## Why this matters

Two small holes in controls that already exist.

The proxy rotate endpoint fetches a user-supplied URL from the engine process
itself, deliberately not through the proxy. Only the scheme is checked, so an
authenticated caller can point it at loopback, a LAN host, or a cloud metadata
endpoint and read the outcome through the reflected error. On the default
loopback deployment the caller is already local so it matters little; it matters
in the token-gated remote mode the server explicitly supports and documents.

The bind guard exists to stop the fully-capable unauthenticated API being
exposed on a non-loopback interface. Its loopback set contains the empty string,
and an empty host binds *all* interfaces - so `--host ""` is precisely the case
the guard exists to prevent, waved through as if it were localhost.

## Current state

- `engine/numera/proxypool.py:351-352` - the only validation:

  ```python
  if not url.lower().startswith(("http://", "https://")):
      return {"ok": None, "rotated": False, ...}
  ```

  then `:369-371`:

  ```python
  req = urllib.request.Request(url, headers={"User-Agent": "numera-proxy-rotate/1.0"})
  with urllib.request.urlopen(req, timeout=timeout) as r:
      body = r.read(500).decode("utf-8", "replace").strip()
  ```

  `urlopen`'s default opener follows redirects, so a host allowlist alone would
  be bypassable by a 3xx. `rotateUrl` is client-writable via
  `PUT /api/proxies/{pid}` (`server/routes_proxies.py:56-58`).

- `engine/numera/server/__init__.py:264`:

  ```python
  LOOPBACK = {"127.0.0.1", "localhost", "::1", "0:0:0:0:0:0:0:1", ""}
  ```

  and `:279-280` returns `None` (allowed, no credential needed) for anything in
  that set. `--host 0.0.0.0` is correctly refused; `--host ""` is not.

## Scope

**In scope**: `engine/numera/proxypool.py` (the `rotate` fetch),
`engine/numera/server/__init__.py` (the `LOOPBACK` set / `serve` host
normalisation), `engine/tests/test_ssrf_and_bind.py` (create).

**Out of scope**: the relay's own loopback listener and the CDP attach port -
both are by design and documented in their module docstrings.

## Steps

### Step 1: Refuse private destinations, including after a redirect

Before fetching, resolve the hostname and reject loopback, private, link-local
(including `169.254.0.0/16`), unique-local and multicast addresses. Because
redirects are followed, the check must apply to *each* hop - the simplest
correct approach is a custom opener/redirect handler that re-validates, or
disabling redirects and validating explicitly. Keep the existing error shape so
the UI still renders the outcome.

**Verify**: `test_rotate_refuses_a_private_destination`.

### Step 2: An empty host is a bind-all, not a loopback

Remove `""` from `LOOPBACK`, and normalise an empty/unset host to `127.0.0.1` at
the `serve()` boundary so an explicitly empty host still reaches the guard as
what it is.

**Verify**: `test_an_empty_host_is_refused_without_a_credential`.

## Test plan

New `engine/tests/test_ssrf_and_bind.py`, modelled on the existing bind tests in
`engine/tests/test_auth_runtime.py` (which already parametrise hosts against
`server._refuse_insecure_bind`):

- `rotate` with a `rotateUrl` pointing at `127.0.0.1`, `169.254.169.254` and a
  private range each returns an error and performs no fetch.
- A public host is still allowed (do not hit the network - assert the
  validation verdict, not a live request).
- `_refuse_insecure_bind("", root)` returns a refusal message; the real loopback
  values still return `None`.

## Done criteria

- [ ] Offline suite passes
- [ ] `.venv/Scripts/python.exe -c "import sys;sys.path.insert(0,'engine');from numera.server import LOOPBACK;print('' in LOOPBACK)"` -> `False`
- [ ] New tests pass and were confirmed able to fail

## STOP conditions

- A legitimate provider rotate URL (public host) is refused by your validation -
  the check is too strict; report rather than loosening it blindly.

## Maintenance notes

- Any future feature that fetches a user-supplied URL from the engine process
  must reuse this validation. It is the only such fetch today.

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

