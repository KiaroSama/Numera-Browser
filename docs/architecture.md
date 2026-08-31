# Architecture

Numera Browser has three parts: a Python **engine** that does the real work, a React
**dashboard** that drives it — served by the engine, not separately — and a
Tauri **desktop shell** that wraps both into a standalone window. This doc
explains how they fit together.

## The coherence principle

Every design decision follows from one idea: a fingerprint is only convincing
when its parts agree. Anti-bot systems cross-check values, so the failure mode
isn't an unusual value — it's a *contradiction* (a Mac UA with a Windows GPU, a
timezone that doesn't match the language, a phone with a desktop screen).

So instead of randomising fields independently, the engine picks a device
**archetype** first, then draws every value from that archetype's realistic pool.
A `validate()` function encodes the same cross-checks and can gate any fingerprint
before it's used.

## Engine flow

```
pick OS ──▶ archetype (devices.py) ──▶ Fingerprint ──▶ validate()
                                             │
                        ┌────────────────────┴────────────────────┐
                        ▼                                          ▼
              context-level options                     page-level init script
              (launcher.py → Playwright)                (stealth.py)
              UA, viewport, screen, locale,             navigator.platform, WebGL
              timezone, proxy, Client-Hints             vendor/renderer, cores,
                                                        deviceMemory, webdriver=false,
                                                        seeded canvas noise
                        └────────────────────┬────────────────────┘
                                             ▼
                          Chromium persistent context per profile
                          (cookies + localStorage persist)
```

### Modules

| Module | Role |
|---|---|
| `devices.py` | Curated OS/GPU/screen/hardware/locale building blocks |
| `fingerprint.py` | `generate()` assembles a coherent fingerprint; `validate()` checks it; `sec_ch_ua()` builds Client-Hints |
| `models.py` | `Fingerprint`, `Proxy`, `Profile` dataclasses + (de)serialization |
| `store.py` | `ProfileStore` — one JSON file per profile, plus a `user-data/<id>` dir per profile for the browser session. Where that lives is decided by `paths.py`, not here |
| `paths.py` | The one place that decides the data directory. Order: explicit argument, `NUMERA_DATA_DIR`, a **writable** `data/` beside the app, `~/.numera`, then the pre-rename `~/.persona` |
| `firstrun.py` | Copies an older install's profiles, pool, config and browser sessions into a new root once. Copy, never move; never overwrites |
| `logs.py` | Per-run log file, UTC. Credentials are redacted by a filter on the *handler*, so a library logging under its own name cannot bypass it |
| `relay.py` | Loopback proxy that terminates upstream authentication, so engines that cannot authenticate a proxy still use one |
| `stealth.py` | Builds the per-fingerprint JS patch injected via `add_init_script` |
| `launcher.py` | Thin dispatcher — resolves `profile.engine` and hands off to a driver |
| `drivers.py` | Pluggable launch backends (playwright / patchright / camoufox / cloak), plus what has to happen to the profile *directory* before Chromium opens: clearing the stores the profile does not carry over, seeding its bookmarks, resolving the signals marked "based on IP", and assembling the command line |
| `server/` | The HTTP API, a package: `stores.py` (the two JSON stores), `convert.py` (dashboard shape <-> engine `Profile`), `spa.py` (serving the built dashboard), and one `routes_*.py` per group. Every route used to be a closure over `create_app`'s locals; each module now takes a `_Ctx` carrying them, so the bodies moved unchanged |
| `cli/` | Command-line interface over the store + launcher. A package, split by responsibility: `out.py` (terminal output and `--json` mode), `profiles.py` (store-only commands), `browser.py` (anything that opens a real browser), `pool.py` (the proxy pool), `setup.py` (config, extensions, folders, vault, schedules, servers) and `parser.py` (the argparse tree). `__init__.py` re-exports `main` and `build_parser`, so `numera.cli` is imported exactly as before |

### Pluggable launch engines

Numera Browser is the *manager*; a **driver** renders an identity in a real browser.
`launcher.launch()` looks up `profile.engine` in the `drivers.py` registry and
delegates. Built-ins:

- **playwright** — stock Chromium + the `stealth.py` init script. Always available.
  Its ceiling is the JS-injection ceiling: no control over TLS/JA3 or the CDP
  transport, so it clears basic bot checks but not Cloudflare/DataDome-grade ones.
- **patchright** — a drop-in, API-compatible *patched* Playwright that closes
  `webdriver`/CDP leaks at the runtime level.
- **camoufox** — a patched Firefox with C++-level fingerprint spoofing and its own
  coherent fingerprint generator.

Drivers share the same signature and either return launch handles
(`keep_open=True`) or block until the window closes (`keep_open=False`). Register
a new one with `@drivers.register("name", requires="pip-module")`.

### Why two levels of spoofing?

Playwright sets some values at the browser-context level (user-agent, viewport,
locale, timezone, proxy). But a few high-signal values live inside the page and
must be patched there (navigator.platform, WebGL strings, hardwareConcurrency,
the `webdriver` flag, canvas noise). The stealth script handles the second set,
and the launcher makes sure both levels describe the *same* device.

### User-Agent Client Hints: a third level

Client Hints need their own mechanism, because neither of the other two gets
them right. They are headers, so the page-level script cannot reach them; and
Chromium generates them itself, from the machine it is really running on, so a
Windows 11 profile on a Windows 10 host announced `Sec-CH-UA-Platform-Version:
"10.0"` to every site that asked.

Pinning them as `extra_http_headers` fixes the values and breaks the timing. A
real Chrome sends three hints unprompted - `Sec-CH-UA`, `-Mobile`, `-Platform` -
and the high-entropy rest only to an origin that has asked with `Accept-CH`. A
header goes on every request, so the profile volunteered its architecture and OS
build to sites that never asked: unusual enough to be worth detecting on its own.

The launcher instead hands Chromium the values as structured metadata over CDP
(`Emulation.setUserAgentOverride` with `userAgentMetadata`) and lets Chromium's
own `Accept-CH` logic decide when to send them. The same block also answers
`navigator.userAgentData.getHighEntropyValues()`, so the page and the wire come
from one source rather than two that can drift. The low-entropy trio stays
pinned as a header as well - the values are identical, and it keeps the trio
right if the CDP call ever fails. `_apply_ua_metadata` treats a failure on an
existing page as fatal for exactly that reason.

**Safari has no Client Hints at all**, so an iOS profile must send none - a
header a real iPhone never sends is a contradiction, not a weak signal. Nothing
declarative removes them: `route.continue_(headers=...)`,
`--disable-features=UserAgentClientHint,AcceptCHFrame,CriticalClientHint` and
`--blink-settings=clientHintsEnabled=false` were each measured against a local
server and changed nothing, and Playwright's route API rebuilds the header set
from `request.headers()`, so it cannot drop what Chromium appends further down
the stack.

Raw CDP can. `Fetch.continueRequest` *replaces* the header set outright, so a
name simply left out never reaches the network - measured, with the server
receiving no `sec-ch-ua` before or after it sends `Accept-CH`. `navigator.
userAgentData` is deleted in the page by the same rule, so the wire and the
page agree. `fingerprint.sends_client_hints()` is that rule, in one place.

The cost is a CDP round trip per request, so the stripping is only switched on
for a profile that needs it. `Fetch` is enabled for the request stage alone:
auth challenges are left alone so proxy credentials keep working, and no
response body is buffered through it. Every code path reaches
`continueRequest`, including the error path - with `Fetch` enabled, a request
that is never continued hangs the page for good.

### What a portable build contains

```
Numera/
  numera.exe      the Tauri shell
  engine/         the frozen Python engine (PyInstaller onedir)
  browsers/       Chromium, shipped with the app
  data/           profiles, sessions, proxies, logs - written in place
```

`browsers/` is the one that is easy to forget. `pip install playwright`
installs the *package*; the browser is a separate download into a per-user
cache, which a machine that has never run Playwright does not have. Without it
the app starts, serves the dashboard and fails at the first launch with
"Executable doesn't exist". `paths.use_bundled_browsers()` points Playwright at
this directory unless `PLAYWRIGHT_BROWSERS_PATH` is already set, and
`scripts/smoke-portable.ps1` proves an assembled folder can actually open a
browser rather than merely containing the files.

The 272MB `chromium_headless_shell` is deliberately **not** shipped: the
launcher passes `channel="chromium"` for headless runs precisely to avoid it,
because the shell cannot load extensions and announces itself as
HeadlessChrome over CDP.

## Storage layout

Resolved by `paths.py`. A **portable** install keeps this beside the executable
so the folder can be copied to another machine; an installed one falls back to
the home directory. The writability check matters: unpacked under Program Files
a data directory beside the binary is read-only, and without the check every
later write would fail far from the cause.

```
<data root>/            # data/ beside the app, or ~/.numera
├── logs/
│   └── numera_<UTC>.log
├── config.json
├── proxies.json
├── profiles/
│   ├── <id>.json          # profile metadata + fingerprint + proxy
│   └── …
├── extensions/
│   ├── <32-char id>/      # unpacked from the Web Store, not per profile
│   └── …
└── user-data/
    ├── <id>/              # Chromium user-data dir (cookies, localStorage)
    └── …
```

`extensions/` sits under the data root rather than beside the repo so a store
extension travels with a portable install; a profile then references the folder
by path, the same way it references one you unpacked yourself.

One JSON file per profile keeps things transparent, diff-friendly and easy to
back up or sync.

## Connecting the dashboard to the engine

This is implemented. `engine/numera/server/` exposes the engine over HTTP
(`numera serve`, default `http://127.0.0.1:8787`), and the dashboard talks to it
through `dashboard/src/api.js`.

**One process, one port.** The engine also *serves the built dashboard itself*,
so `numera serve` is the whole application — there is no second dev server and
nothing for a user to start by hand. Two consequences follow, and both have
already caused bugs:

- `api.js` uses a **same-origin** base URL. A hardcoded `http://localhost:8787`
  is a compile-time constant that is wrong whenever the port is chosen at
  runtime, and it failed quietly — the app rendered and fell back to demo data.
- The SPA catch-all route must refuse `/api/*`. Without that guard a mistyped
  endpoint returns the HTML page with status 200 and the dashboard tries to
  parse it as JSON.

Three surfaces, one engine:

    numera serve            the API, and the dashboard mounted on it
    desktop/                a Tauri window that starts the engine as a sidecar
                            on a port it picks at runtime
    numera <command>        the CLI, for headless and scripted use

The dashboard's own files split by responsibility, and the dependencies point
one way — `App` → views → primitives → leaf, with no cycles:

    App.jsx         state, routing, and the error boundary. Nothing else.
    views/          one file per view (Profiles, Extensions, Engines, Editor,
                    Health, Proxies, Automation, Folders, Settings, plus the
                    bulk modals and the command palette)
    ui.jsx          primitives more than one view needs
    deviceData.js   device archetypes and the coherence check — the mirror of
                    numera/devices.py and numera/fingerprint.py
    demoData.js     sample profiles for demo mode, kept away from the real
                    tables so fabricated rows cannot reach live mode
    toast.jsx       the toast store and its renderer
    theme.js        palettes, CSS custom properties, the stylesheet
    api.js          the only place that knows about HTTP

A component used by exactly one view lives in that view's file; `ui.jsx` is for
the genuinely shared ones.

```
 Dashboard (React)                     Engine API (FastAPI)              Engine core
 ─────────────────                     ────────────────────             ───────────
 on load → GET /api/health ───────────▶ health()
   ok?  → live mode                     GET  /api/profiles ────────────▶ DashboardStore
   no   → demo mode (sample data)       POST /api/profiles                (JSON per profile)
                                        PUT/DELETE /api/profiles/{id}
 click ▶ → POST /…/{id}/launch ────────▶ to_engine_profile() ──────────▶ launcher.launch
                                          + subprocess: numera launch     (real Chromium)
 click ■ → POST /…/{id}/stop ──────────▶ terminate the launch process
```

Key points:

- **Two profile shapes.** The dashboard's profile is richer than the engine's
  (folders, per-signal spoof modes, media counts). The API stores the dashboard
  JSON as-is and only converts to an engine `Fingerprint`/`Profile` at launch
  time (`to_engine_profile`), so nothing in the UI is lost on a round-trip.
- **Launching is a subprocess.** `/launch` runs `numera launch <id>` in its own
  process rather than driving Playwright's sync API from inside the async server.
  A profile is "running" while that process is alive; `/stop` terminates it.
- **Coherence** stays in the dashboard (its meter mirrors `validate()`) for
  instant feedback; the engine re-validates the real fingerprint at launch.
- **Graceful fallback.** If the API is offline the dashboard runs on in-memory
  sample data (demo mode), so it still works standalone.
