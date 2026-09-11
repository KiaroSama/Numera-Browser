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
| `bundled.py` | Resolves the portable manifest to each backend's own executable, addon and GeoIP resources, keeping Playwright and Patchright caches separate |
| `firstrun.py` | Copies an older install's profiles, pool, config and browser sessions into a new root once. Copy, never move; never overwrites |
| `logs.py` | Per-run log file, UTC. Credentials are redacted by a filter on the *handler*, so a library logging under its own name cannot bypass it |
| `relay.py` | Loopback proxy that terminates upstream authentication, so engines that cannot authenticate a proxy still use one |
| `sessions.py` | The durable session claim: which tree owns a profile's browser, kept on disk so it survives the launcher, and identified per session so two profiles open in one process do not hold each other. Every browser open goes through `drivers.open_session`, which takes it, and every status, stop and delete surface consults it. Three read states, not two: no claim, a claim to evaluate, and one this build cannot evaluate - unparseable, or naming a tree kind from an older version or another platform - which counts as owned, because it may be the only record of a browser whose launcher died |
| `driver_capture.py` | Per-thread driver-spawn interception. Preparation can overlap across profiles; only creation of the exact OS tree and its durable claim callback are serialized. `proctree` retains the public capture imports |
| `proctree.py` | One session and everything it starts, owned as a unit — a job object on Windows, its own session and process group on POSIX. A spawned session is born into one; an in-process session gets one at its *driver's* spawn, which is the last moment it can be decided, because everything that driver starts inherits it — and a session that cannot be given one is refused rather than downgraded. Every "is it stopped?" answer goes through it, because the process Numera holds a handle to is a launcher and the browser holding the profile directory is three processes further down |
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
- **cloak** — CloakBrowser's patched Chromium, launched through the same mandatory
  driver-tree capture contract. Binary and proxy preparation do not hold the
  process-wide capture mutex.

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
Numera Browser/
  numera.exe      the Tauri shell
  engine/         the frozen Python engine (PyInstaller onedir)
  browsers/       per-engine binaries, addons, GeoIP data and manifest.json
  data/           profiles, sessions, proxies, logs - written in place
```

`browsers/` is the one that is easy to forget. `pip install playwright`
installs the *package*; the browser is a separate download into a per-user
cache, which a machine that has never run Playwright does not have. Without it
the app starts, serves the dashboard and fails at the first launch with
"Executable doesn't exist". `scripts/bundle-browsers.py` downloads into separate
vendor caches and assembles a manifest with relative executable paths and hashes.
`bundled.launch_options()` selects the backend's binary explicitly; equal Chromium
revision labels never cause Patchright to use Playwright's stock binary.
Camoufox receives its packaged addons and GeoIP paths. Explicit user browser-path
overrides remain supported. Older flat Playwright bundles retain their fallback.
`scripts/smoke-portable.ps1` verifies an assembled folder can open a browser.

All four Python backends are frozen. The standard release includes three browser
binaries and lets the Cloak wrapper download directly from its vendor. The
Windows `-IncludeCloakBinary` option additionally packages the official unmodified
Cloak binary for local/internal use, subject to its separate license.

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

The profile editor stays open after a rejected save, including conflict, locked
vault, validation and network failures. Pending saves disable editing and
duplicate submissions. A confirmed write updates local state from the server's
response and refreshes profiles/config before closing. If that refresh fails,
the write remains committed and is not offered as an unsaved retry. Password
intent is local to the editor and is cleared between successful edit sessions.

`ApiError` preserves the HTTP status and domain `kind`: a `409 StaleProfile`
triggers an authoritative reload, while `409 ProfileDiverged` retains its separate
storage-conflict meaning. `profileEdits.js` compares the loaded base and current
draft to form a dirty-field patch, retaining `_rev`. It rebases those edits onto
the latest profile; arrays are atomic fields and proxy members can be independent.
Overlapping changes require explicit "Keep my edit" or "Use latest" choices.
Draft controls pause until those choices are resolved. Secret values stay hidden
in conflict previews, and password preserve/clear/replace and proxy removal intent
survive rebasing. An unavailable refresh keeps the draft and explains the retry.
When a nested proxy edit conflicts with a remotely removed proxy, keeping the
whole local proxy explicitly restores its known password. Dashboard reads use
empty text for a known passwordless proxy and null for a withheld credential;
the latter cannot be silently restored as passwordless. Choosing the latest
version accepts the removal without replaying any credential.

Camoufox's manual context protocol pairs every successful backend enter with
exactly one exit, including failure of the ownership check before yielding the
browser. A cleanup failure preserves the original failure in
`StartupRecoveryFailed` and retains the session claim; successful cleanup releases
only through the existing ownership/emptiness policy.

A Dashboard record that omits a proxy does not authorize deletion of a proxy
held only by its engine twin. Both listing-time derivation and Dashboard saves
refuse that loss, including passwordless endpoints. Explicit removal remains
available when the Dashboard shows the proxy; encrypted credentials still
require the existing vault-unlock policy.

```
 Dashboard (React)                     Engine API (FastAPI)              Engine core
 ─────────────────                     ────────────────────             ───────────
 on load → GET /api/health ───────────▶ health()
   ok?  → live mode                     GET  /api/profiles ────────────▶ DashboardStore
   no   → demo mode (sample data)       POST /api/profiles                (JSON per profile)
                                        PUT/DELETE /api/profiles/{id}
 click ▶ → POST /…/{id}/launch ────────▶ materialize_current() ────────▶ launcher.launch
                                          + subprocess: numera launch     (real Chromium)
                                          wait for the child to own it
 click ■ → POST /…/{id}/stop ──────────▶ terminate the launch process
                                          (409 if another process owns it)
```

Key points:

- **Two profile shapes.** The dashboard's profile is richer than the engine's
  (folders, per-signal spoof modes, media counts). The API stores the dashboard
  JSON as-is and only converts to an engine `Fingerprint`/`Profile` at launch
  time (`to_engine_profile`), so nothing in the UI is lost on a round-trip.
- **Launching is a subprocess.** `/launch` runs `numera launch <id>` in its own
  process rather than driving Playwright's sync API from inside the async server.
  The server does not call it the owner until the child confirms it holds the
  profile's session lease and has revalidated the profile and its proxy
  credential (`numera/handshake.py`); a child that never gets there is
  terminated and the request answers with the reason - `409` busy, `423` locked
  vault, `404` deleted. After that the profile is "running" while the process is
  alive, and `/stop` ends it. What it ends is the whole tree, not the
  launcher: a `numera launch` starts Playwright, which starts a Node driver,
  which starts Chromium, and none of those are the process the server has a
  handle to. Each session is spawned into an OS-level tree (`numera/proctree.py`)
  so emptiness is something the OS answers rather than something the engine
  infers - and nothing releases the registry entry, the session lease or the
  profile until that tree is proved empty. The server records what each child *is* -
  a launch, an attach, a manual warm-up or a scheduled one, with the exact
  schedule occurrence - so closing an ordinary launch is never written into a
  schedule's result, and `/stop` answers `409` rather than claiming it stopped
  a browser another process owns.
- **The engine copy is derived at write time.** The engine record a launch or a
  probe opens is re-derived from the dashboard record inside the transaction
  that writes it (`repo.materialize_current`), never from a snapshot read
  before the browser ran.
- **Coherence** stays in the dashboard (its meter mirrors `validate()`) for
  instant feedback; the engine re-validates the real fingerprint at launch.
- **Graceful fallback.** If the API is offline the dashboard runs on in-memory
  sample data (demo mode), so it still works standalone.

## Bundling a browser engine

Browser binaries are downloaded at build time. Vendor resources and notices
remain with their binaries; the project license does not replace their terms.

| Engine | Licence | Redistribution |
|---|---|---|
| `fingerprint-chromium` | BSD-3-Clause | permissive, redistributable as-is |
| `patchright` | Apache-2.0 | permissive, redistributable as-is |
| `camoufox` | [MPL-2.0](https://github.com/daijro/camoufox/blob/main/LICENSE) | retain notices and make the corresponding [upstream source](https://github.com/daijro/camoufox) available under MPL-2.0 |
| `cloakbrowser` | MIT wrapper; [proprietary binary](https://github.com/CloakHQ/CloakBrowser/blob/main/BINARY-LICENSE.md) | local/internal unmodified use allowed; third-party binary redistribution requires a separate license |

GeoIP resources include GeoLite Data created by MaxMind, available from
[MaxMind](https://www.maxmind.com), under its
[GeoLite terms](https://www.maxmind.com/en/geolite/eula) and
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).
Retain attribution and applicable terms with distributed data, and refresh old
databases as required by those terms. IP geolocation is approximate.

This lived in `LICENSE` until it was trimmed to licence terms only. It is
engineering guidance about what may ship, not a term of the licence itself.
