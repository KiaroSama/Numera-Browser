# Plan 008: One proxy validator, identity and conversion everywhere

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the next
> step. If anything in "STOP conditions" occurs, stop and report - do not
> improvise. When done, update this plan's status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 798326a..HEAD -- engine/numera/proxypool.py engine/numera/server/stores.py engine/numera/server/routes_proxies.py engine/numera/cli/pool.py`
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

Proxy handling has five entry points and they disagree with each other. The
consequences are concrete: an HTTP and a SOCKS5 entry with the same host, port
and credentials are one proxy to the deduper and two to the usage counter; an
edit can persist a port of `70000`; an unsupported scheme in pasted text is
silently rewritten to HTTP before the validator that would have rejected it ever
runs; and assigning a pool proxy from the CLI downgrades HTTPS to HTTP, so the
profile claims one protocol and the browser speaks another.

`normalize()`, `identity()` and `server_url()` already exist and are correct.
This plan is about deleting the four copies that route around them.

## Current state

- `engine/numera/server/stores.py:279-286` - `ProxyPool._key` omits type:

  ```python
  return (p["host"], str(p["port"]), p.get("user") or "", p.get("pass") or "")
  ```

  while `engine/numera/proxypool.py:438-450` - `identity()` includes it:

  ```python
  return (str(entry.get("type", "HTTP")).upper(), str(entry.get("host", "")), ...)
  ```

- `engine/numera/server/routes_proxies.py:47-59` - `edit_proxy` filters keys to
  an `allowed` set and writes the patch straight through; `normalize()` is never
  called, so type/port/country/host are unvalidated on edit.
- `engine/numera/proxypool.py:98` - `parse_line` maps an unknown scheme to HTTP:

  ```python
  "type": _SCHEME_TYPE.get(scheme, "HTTP"),
  ```

  so `normalize_many` downstream sees a valid-looking HTTP entry and cannot
  reject the input.
- `engine/numera/cli/pool.py:40` - `pool add` uses `pp.parse_many(text)`
  directly, skipping `normalize_many`.
- `engine/numera/cli/pool.py:173-179` - `pool assign` has its own scheme map
  with `"HTTPS": "http"` and no IPv6 bracketing, versus the canonical
  `server_url()` at `proxypool.py:423-435`.

## Scope

**In scope**: `engine/numera/proxypool.py`, `engine/numera/server/stores.py`,
`engine/numera/server/routes_proxies.py`, `engine/numera/cli/pool.py`,
`engine/tests/test_proxy_one_source.py` (create).

**Out of scope**: the relay's wire protocols, and the `rotate` SSRF check (plan
004 owns that).

## Steps

### Step 1: One identity

Delete `ProxyPool._key` and dedupe on `proxypool.identity()`. Note this is a
deliberate behaviour change: two entries differing only by type are now two
proxies, which is the correct reading and what the usage counter already
believed.

### Step 2: Edit goes through the validator

Have `PUT /api/proxies/{id}` normalise the merged record and return a 4xx naming
the field on failure, exactly as `POST /api/proxies` already does.

### Step 3: Unknown text schemes are rejected, not rewritten

Stop `parse_line` from defaulting an unrecognised scheme to HTTP. Preserve what
was parsed so `normalize()` can reject it with a message naming the scheme.
Keep the tolerant behaviour for input that genuinely has no scheme (bare
`host:port` is HTTP by convention and must keep working).

### Step 4: Both CLI paths use the canonical helpers

`pool add` runs its parsed entries through `normalize_many`. `pool assign`
replaces its local map with `server_url()`.

**Verify after each step**: `engine/tests/test_proxy_one_source.py` plus the
existing `engine/tests/test_proxy_check_socks.py` and `test_proxypool*.py`.

## Test plan

New `engine/tests/test_proxy_one_source.py`:

- Protocol is part of identity: adding the same host/port/creds as HTTP and then
  as SOCKS5 yields two entries, and the usage count agrees.
- `PUT` with `port: 70000`, an unsupported type, and a malformed country each
  return 4xx and change nothing on disk.
- Pasting text with an unsupported scheme is rejected, naming the scheme; bare
  `host:port` still imports as HTTP.
- Round-trip: for each of HTTP/HTTPS/SOCKS4/SOCKS5, a pool entry assigned to a
  profile via the CLI produces the same `Proxy.server` as the API path -
  including an IPv6 host, which must stay bracketed.

## Done criteria

- [ ] `grep -n "_key" engine/numera/server/stores.py` shows the ProxyPool copy is gone
- [ ] `grep -n "scheme_of" engine/numera/cli/pool.py` returns nothing
- [ ] New tests pass; each confirmed able to fail
- [ ] Existing proxy suites pass unchanged
- [ ] Offline suite passes

## STOP conditions

- Making type part of dedupe breaks an existing import test that expects a
  same-address different-type pair to collapse - report it; that test encodes
  the old, wrong identity.

## Maintenance notes

- There is now exactly one definition each of "is this proxy valid"
  (`normalize`), "are these the same proxy" (`identity`) and "what URL does the
  engine use" (`server_url`). A sixth entry point must call them, not copy them.

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

