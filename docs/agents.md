# Driving Numera without a GUI

Numera has two front doors and they are not the same size.

* The **HTTP API** (`numera serve`) is what the dashboard talks to. Anything the
  GUI can do, an HTTP client can do, because the GUI has no other way of doing
  it either.
* The **CLI** (`numera …`) is what a script or an agent on a headless server
  wants: no server to start, no port to pick, an exit code that means something.

`engine/tests/test_agent_parity.py` holds the two against each other and fails
when they drift. This page is the human version of that file.

---

## Structured output

Every command takes `--json`. It prints **one JSON document on stdout and
nothing else** — the prose a person reads is suppressed, not merely coloured,
so a caller never has to parse around it.

```console
$ numera list --json
{"total": 2, "profiles": [{"id": "5c517ec3d2cb", "name": "alpha", ...}]}

$ numera list
ID            NAME                  OS       LOCALE   PROXY
5c517ec3d2cb  alpha                 windows  tr-TR    -
```

The human output is still the default. Nothing that reads the plain form breaks.

Two conventions worth knowing:

* **Exit codes carry the verdict, with two exceptions.** `check`, `proxy test`,
  `tls`, `dns` and `collisions` exit non-zero when the thing they inspected is
  *unhealthy* — the payload is still on stdout, so
  `numera check p --json; echo $?` is a one-liner health gate. `trust` and
  `align` exit **0 under `--json` whatever the grade** (read `score` from the
  payload); the API shells out to those two and treats a non-zero exit as a
  server error, so a merely-imperfect profile must not look like a broken
  command. Their human form still reports the verdict in `$?`.
* **`--json` never prompts.** A destructive command without `-y` fails with
  `{"ok": false, "error": "this needs confirmation — pass -y …"}` instead of
  waiting on a keyboard nobody is at. Same for `vault enable` without
  `--password` / `NUMERA_PASSWORD`.

`list` reports a summary row per profile; `show` and `export` are the
full-fidelity paths and include the proxy password, so keep them off shared
logs.

## Job handles for the slow work

Launching a browser, warming a profile and attaching DevTools all block for as
long as the session is open. `--detach` runs any command in the background and
prints a handle instead:

```console
$ numera warmup alpha --minutes 30 --headless --detach --json
{"id": "a22ad4e3ef19", "command": "warmup", "profile": "5c517ec3d2cb",
 "pid": 49276, "log": ".../jobs/a22ad4e3ef19.log", "started": 1788031243.29}

$ numera jobs show a22ad4e3ef19 --json
{"id": "a22ad4e3ef19", "state": "running", "running": true, "tail": ["  browsing https://…"]}
```

`state` is one of `running`, `starting`, `done`, `failed`, `stopped` or `lost`
(the process went away without recording an outcome — killed, or crashed hard).
A finished job also carries `exit`, the command's real exit code.

| | |
|---|---|
| `numera jobs list` | every job and its state |
| `numera jobs show <id> [--lines N]` | one job, with the tail of its output |
| `numera jobs stop <id>` | terminate it **and everything it started** |
| `numera jobs clear` | forget the finished ones |
| `numera stop <ref>` | stop every job belonging to one profile |

Records live in `<data-dir>/jobs/`, one JSON file plus one log per job, so a
later process can see work an earlier one started. `jobs stop` kills the whole
process tree (`taskkill /T` on Windows, the process group elsewhere) — a
detached `numera launch` owns a Chromium that would otherwise outlive it.

The HTTP API has its own equivalent and it is not this one: `POST /launch`
returns immediately, the profile then reads `"status": "running"` in
`GET /api/profiles`, and `POST /stop` ends it. That registry lives in the
server's memory, which is why the CLI needed one on disk.

## A worked example

```bash
numera bulk-create --count 20 --prefix Acct --tag batch1 --json
numera pool add proxies.txt --json
numera pool assign --tag batch1 --json          # round-robin, locale follows country

for id in $(numera list --tag batch1 --json | jq -r '.profiles[].id'); do
  numera check "$id" --json || echo "$id is incoherent"
  numera warmup "$id" --minutes 20 --headless --detach --json
done

numera jobs list --json | jq '[.jobs[] | select(.running)] | length'
numera health --json | jq '.summary'
```

## HTTP instead

```bash
numera serve --port 8787 --detach --json        # a job handle, like anything else
curl -s localhost:8787/api/profiles | jq
```

Set `NUMERA_API_TOKEN` before serving to require `Authorization: Bearer <token>`
on every request; it also narrows CORS, which is the point of it — a wildcard
origin plus a token is a token any web page can spend. `NUMERA_ALLOWED_ORIGINS`
sets the list explicitly.

FastAPI publishes a schema at `/openapi.json` and a browsable form at `/docs`.
Read [the caveats below](#what-the-openapi-schema-does-not-tell-you) before
generating a client from it.

## MCP

`numera mcp` speaks MCP over stdio. Point a client at it:

```json
{
  "mcpServers": {
    "numera": {
      "command": "numera",
      "args": ["--data-dir", "/srv/numera", "mcp"]
    }
  }
}
```

Every command becomes a tool — `create`, `list`, `pool_assign`, `jobs_show`,
`cookies_export`, and so on — and each tool's schema is **derived from the
argparse parser at run time**, not written out beside it. Adding a CLI command
adds a tool; renaming a flag renames its argument. There is no second list to
fall out of date.

Two things a caller should know:

* Every tool takes `detach: true`. A call is bounded at 120 seconds, so anything
  that opens a browser (`launch`, `warmup`, `attach`, `trust`, `align`, the
  cookie and probe tools) should be detached and polled with `jobs_show`.
* A non-zero exit is a *verdict*, not a failure. `check` on an incoherent
  profile returns a normal result whose payload lists the issues, with the code
  in `_meta.exitCode`. `isError` is reserved for a tool that could not run at
  all.

`serve` and `mcp` are deliberately not tools — they never return.

No SDK is involved: MCP's stdio transport is newline-delimited JSON-RPC 2.0,
which the standard library already speaks.

---

## Known gaps and closed ones

These are asserted, not aspirational: the parity test fails if the list changes
in either direction.

### Folders and the shared store — both closed

Two gaps used to live here. Both are fixed, and how they were fixed is worth
knowing because it decides where you look when something goes wrong.

**Folders** now have `numera folder list|create|rename|delete`, with `--json`.
The rules — a rename moves the folder's profiles with it, a delete reassigns
them to `Unfiled` rather than destroying them — live in `numera/folders.py`, and
both the HTTP routes and the CLI call it. That is deliberate: two copies of
behaviour this subtle would drift, and the drift would be silent, with the GUI
and the CLI simply disagreeing about where a profile lives. Folders are stored
in `<data-dir>/dashboard-config.json`, the same file the dashboard already used,
so nothing had to be migrated.

**The two stores are reconciled.** `DashboardStore` mirrors every write into the
engine store, and `GET /api/profiles` adopts anything the CLI created. So:

| | writes | visible to |
|---|---|---|
| `numera create` | `<data-dir>/profiles/<id>.json` | both, adopted on the next list |
| `POST /api/profiles` | `<data-dir>/dashboard/<id>.json` **and** the engine store | both, immediately |

The mirroring is in the store rather than in each of the five API write paths,
so a sixth write path cannot forget it. The conversion is best-effort: a profile
shape the dashboard can hold but `to_engine_profile()` cannot convert still
saves, because refusing the edit would fail for a reason the user cannot see.

`test_a_cli_created_profile_is_visible_to_the_dashboard` was a strict xfail while
this was open; it passes outright now, and the strictness is what announced the
fix rather than letting it go unnoticed.

**Web Store extensions install headlessly too.** `numera ext install <url>`
takes the same link the Extensions page takes, so an agent is not restricted to
folders a human unpacked by hand. Chromium cannot install from the store once
automation is enabled and `--load-extension` only accepts an unpacked directory,
so the link is resolved to a 32-character id, fetched as a `.crx` from Google's
update endpoint and unpacked into `<data-dir>/extensions/<id>/` — which means it
travels with a portable install instead of pointing at one machine.

Ids are exactly 32 letters from `a`–`p`. An id that is nearly right is diagnosed
rather than reported as missing: the error names the offending letter, or the
actual length, because "not found" would send a caller to check whether the
extension still exists when the real problem is a truncated copy-paste.

### `numera health` reports no trust grade

The dashboard's Health page shows a *cached* grade, written into the dashboard
profile by the API when a trust or align run finishes. Nothing caches one on the
engine side, so `numera health` reports `"trust": null`. Run
`numera trust <ref> --json` for a live grade — it opens a real browser, which is
why the page caches it in the first place.

## What the OpenAPI schema does not tell you

`/openapi.json` lists all 30 API paths correctly — the SPA catch-all is
excluded, the methods are right, nothing is missing. What it does not have is
any *content*:

* **Every request body is `{"type": "object", "additionalProperties": true}`.**
  The routes take `body: dict`, so the schema cannot name a single field. From
  the schema alone there is no way to know that `POST /api/profiles/{pid}/launch`
  accepts `{"headless": bool}`, or what a profile object contains.
* **Every 200 response is `{}`.** The handlers have no return annotation, so
  generated clients get `Any` everywhere.
* **The failure modes are undocumented.** The `409` from launching a profile
  whose session is in use, the `404`s, the `400` from an unparseable proxy list —
  none appear. Only the automatic `422` does.
* **Bearer auth is invisible.** `NUMERA_API_TOKEN` adds a middleware that
  answers `401`; no security scheme or `401` response is declared, so a
  generated client will not send the header.

Treat the schema as a route index, not a contract. Fixing it means adding
Pydantic models and `response_model=` in `server.py`.
