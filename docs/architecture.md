# Architecture

Numera has three parts: a Python **engine** that does the real work, a React
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
| `drivers.py` | Pluggable launch backends (playwright / patchright / camoufox) |
| `cli.py` | Command-line interface over the store + launcher |

### Pluggable launch engines

Numera is the *manager*; a **driver** renders an identity in a real browser.
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

This is implemented. `engine/numera/server.py` exposes the engine over HTTP
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
