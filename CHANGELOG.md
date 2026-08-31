# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.4.0] - 2026-08-31

### Added

- **The profile editor is a full page, not a 580px drawer**, with a two-column body and a
  **profile summary** panel listing every resolved value while you edit - so a contradiction is
  visible at the moment you create it rather than at the next trust check.
- **Six fields the engine had always read but the editor never offered**: client rects, WebGL
  image, media devices, WebGL vendor, speaker count and an editable font list. Every profile had
  been silently taking the default. `test_editor_covers_the_engine.py` now compares what
  `to_engine_profile` consumes against what the dashboard writes, so a field cannot again be
  readable by the engine and unreachable in the UI.
- **Bookmarks.** `Name | URL` per line, written into the profile's own Chromium bookmark file just
  before launch and **merged** - anything bookmarked inside the browser survives, and relaunching
  does not duplicate. An account with an empty bookmark bar reads as new.
- **Extra Chromium arguments**, appended to the flags Numera already sets. They land on the real
  command line; a live test asserts a flag changes what the running page reports.
- **DNS over HTTPS.** Chromium has no plain-nameserver flag, so a well-known resolver address is
  mapped to its DoH endpoint and a custom endpoint can be typed. An address that cannot be honoured
  is **refused at launch** rather than ignored - a leak you believe is closed is worse than a
  failure you can see.
- **Timezone, geolocation and languages can follow the proxy's real exit IP**, resolved at launch
  because the exit country is not knowable before then. A stored timezone is a guess made when the
  profile was created; a clock that disagrees with the IP is the cheapest tell there is.
- **What carries over between launches** is now per-profile: cookies, local storage, IndexedDB,
  open tabs, history, saved passwords, extension data and bookmarks. Unticked stores are cleared on
  the way *in*, so a session that ends in a crash still starts clean. The default is full
  persistence, which is what every existing profile already had.
- **The proxy pool page got the operations it was missing**: multi-select with select-all, bulk
  test, bulk export, bulk delete, an edit dialog, and a masked password with reveal and copy.
  Deleting asks first, and says how many profiles use the proxy.

### Fixed

- **The proxy Test button did nothing for SOCKS proxies** - most of a real pool. `urllib` has no
  SOCKS transport, so the check reported "test it via a profile" instead of testing. It borrows
  `relay.py`, which already speaks SOCKS5, and reports the exit IP and country like any other. A
  credentials failure is now distinguishable from a refused connection.
- **The dashboard 404'd from a source checkout** after `server.py` became a package: the path that
  finds `dist/` was still counting parents from the old depth. Every test passed and every page was
  missing.

### Changed

- `App.jsx` (2463 lines), `cli.py` (1725) and `server.py` (1466) are each now a package of focused
  modules, largest 414. No behaviour change: verified by declaration inventory, bundle size, and a
  before/after screenshot diff inside the same-build noise floor.
- **One version number.** The engine had reached 0.3.0 while the desktop app and the dashboard sat
  at 0.1.0 from the first commit. All five manifests now agree, and a test fails if they drift.

### Added

- **Install an extension from a Chrome Web Store link.** Paste the store URL on the Extensions page
  and the button reads **Install**; `numera ext install <url>` does the same headlessly, with
  `ext installed` and `ext uninstall` beside it. Chromium refuses to install from the store once
  automation is enabled and `--load-extension` only accepts an unpacked directory, so the `.crx` is
  fetched from Google's own update endpoint and unpacked into `<data-dir>/extensions/<id>/` — which
  means it travels with a portable install instead of pointing at one machine. Extension ids are
  exactly 32 letters from `a`–`p`, and a near-miss is diagnosed rather than reported as missing: the
  error names the offending letter, or the actual length, because a truncated copy-paste is the
  common case and "not found" sends you to the wrong place.
- The default-extension list shows each store extension's **name**, read from its manifest, above
  the folder path. A row that reads only a 32-character id says nothing about what is installed.

### Fixed

- **`build-desktop.ps1` deleted the data of the app it was replacing.** The assemble step wiped
  `build/windows/Numera` outright, and the app is *portable* — run it from there and it keeps
  profiles, browser sessions, proxies and logs in `data/` beside the executable. A rebuild therefore
  destroyed real profiles. `data/` is now carried across the rebuild and everything else is
  replaced. `test_build_preserves_data.py` extracts that block from the real script and runs it, so
  a future edit that drops the guard fails a test instead of a user's profiles.

- **The Extensions page accepted things that could never load.** A Chrome Web Store URL, a mistyped
  path or a folder without a `manifest.json` were all stored and reported as saved; the launcher
  then silently dropped them (`_extension_args` keeps only real directories) and nothing loaded,
  with no error anywhere. The API refuses them now, with a message that says what to do instead,
  and rejects the whole write rather than saving part of a list. Verified end to end in a real
  browser: a genuine unpacked extension goes page -> engine -> Chromium and its content script
  runs.
- **API errors lost the server's explanation.** `api.js` threw `PUT /api/config → 400` and
  discarded FastAPI's `detail`, so the dashboard could only ever show a status code. It shows the
  reason now.


- **The Canvas, Audio, Fonts, Geolocation and Do-Not-Track pickers did nothing** (B14). All five
  were rendered by the dashboard and never read by `to_engine_profile()`, so choosing "Real" or
  "Off" changed no browser behaviour at all - the same class as the WebRTC bug fixed in 0.3.0.
  Each is now a real `Fingerprint` field, carried in *both* directions, and enforced where it
  actually matters: canvas honours noise/block/real, audio and font masking can be switched off,
  geolocation distinguishes prompt from allow from block, and DNT reaches both the request header
  and `navigator.doNotTrack`. Proven in a live browser by `tests/test_dead_controls_live.py` -
  reading the mode back out of a dict is not evidence.
- **A fresh install defaulted to an engine it did not have** (B8). `cloak` was hardcoded as the
  default while the installer ships only Playwright, so the first launch click always failed. A
  new install now resolves to an engine that is actually present; an explicit user choice is still
  honoured as-is rather than silently substituted.
- **The engine default could be overwritten by a client's guess** (B7, engine side). Only an
  engine the caller genuinely chose is remembered now - one filled in from the current default is
  not a choice.
- **The API stored profile shapes the dashboard could not render** (B10). `POST`/`PUT` now reject
  a bad shape instead of persisting it and taking the UI down on the next load.
- **One malformed profile blanked the whole app** (B9). There was no error boundary, so a single
  render throw took the React tree down to a white page with no way back. Bad data now degrades to
  a readable panel: per-view (switching tabs clears it), around the profile editor, and once at the
  root as a last resort.
- **An unhandled promise rejection on every engine-offline load** (B11). `api` declared the key
  `health` twice - a safe `async health()` probe that returns false when the engine is down, and
  later `health: () => req(...)` for per-profile data. The second silently shadowed the first, so
  the call that was written to never throw was in fact the throwing one. Renamed to
  `profileHealth`; the probe is a probe again.
- **The profile editor could not use the proxy pool** (B12). Attaching a saved proxy meant retyping
  host, port, user and password by hand. There is a picker now, and manual entry still works.
- **Proxy import collapsed same-country entries** (B13). A label like `IT 1` was reduced to `IT`,
  making four Italian proxies indistinguishable. The country code still aligns the fingerprint; the
  rest is kept as the label.

### Added

- **A portable data root.** Numera now keeps profiles beside the application in `data/`, so the
  folder can be copied to another machine or a flash drive and the profiles travel with it.
  Resolution order is one decision in one place (`paths.py`): explicit `--data-dir`, then
  `NUMERA_DATA_DIR`, then a *writable* directory beside the app, then `~/.numera`, then the
  pre-rename `~/.persona`. The writability check matters: unpacked under Program Files, a data
  directory beside the binary is read-only, and without the check every later write would fail far
  from the cause.
- **First-run import.** An existing install's profiles, proxy pool, config and **browser sessions**
  are copied into the new root the first time it is used. It copies rather than moves, never
  overwrites anything already there, and marks itself done in the destination - so a fresh copy of
  the portable folder imports again, which is what you want when moving machines.
- **Run logging.** A new file per run under `<data-dir>/logs`, UTC to the second, oldest pruned.
  Credentials are redacted by a filter on the *handler*, so a library logging under its own name
  cannot bypass it - verified against the real 16-password proxy pool with zero leaks. `numera
  serve --verbose` raises the detail without weakening redaction.

### Added

- **`build/`, one directory per platform.** Source and artifacts are kept apart: everything under
  `build/` is produced by `scripts/`, never edited by hand, git-ignored, and published through
  GitHub Releases rather than carried in the repository. `build/windows/Numera/numera.exe` is the
  app; `build/server/` holds the wheel.
- **Linux and macOS desktop builds in CI.** `.github/workflows/build-desktop.yml` builds both, plus
  the server wheel, and attaches every platform to the Release for a `v*` tag. They cannot be built
  on Windows: a Tauri binary links against the host's web view.

- **`numera folder list|create|rename|delete`**, with `--json`. Folders were the last capability
  the GUI had and the CLI did not (B18), which made "headless parity" not quite true. The rules
  live in a shared `folders.py` that both the API routes and the CLI call — a rename moves the
  folder's profiles, a delete reassigns rather than destroys them — so the two surfaces cannot
  drift on behaviour subtle enough that the drift would be silent. Parity is now **39/39**.

- **A server / CLI build.** `scripts/build-server.ps1` produces a **platform-independent** wheel
  (`py3-none-any`), so the same file installs on Linux, macOS or Windows. No desktop shell, no
  bundled browser. Verified by installing it into a clean venv and confirming it serves both the
  API and the dashboard.
- **A Linux CI workflow** (`.github/workflows/build-linux.yml`) that builds both the server wheel
  and the Linux desktop app. The desktop half exists because a Tauri binary links against the
  host's WebKitGTK and cannot be cross-compiled from Windows — a Linux runner is the only way this
  project produces one at all.

- **GoLogin-level fingerprint surface.** Twelve more fields carried through *both* conversion
  directions and proven in a live browser: ClientRects and WebGL-image noise as separate switches
  from the vendor/renderer strings, a media-devices mode with speaker count, OS version, Android
  device model, `maxTouchPoints` and `devicePixelRatio`.
- **Android and iOS profile targets**, emulated at the CDP layer — mobile user agent, touch points,
  device pixel ratio, mobile viewport. This is what AdsPower does too, so it is parity rather than
  a shortfall, but the honest limit is recorded in the code: an iOS profile is not running WebKit.

- **A headless parity test.** It reads the API calls `dashboard/src/api.js` actually makes and the
  commands the CLI actually registers, then asserts every GUI capability has a non-GUI path.
  **34 of 37 do.** The three that do not — creating, renaming and deleting folders — are frozen in
  `KNOWN_GAPS` with the reason (folders exist only in the dashboard's profile shape;
  `to_engine_profile()` drops the field, so the engine store has nothing to hang one on. Tags are
  the CLI's equivalent). Closing a gap fails the test too, which is the point: the set cannot drift
  in either direction unnoticed. A separate assertion guards the readers themselves, since a regex
  that quietly matched nothing would make every other assertion pass while proving the opposite.
- **`--json` on the commands an agent would drive**, emitting only machine-readable output on
  stdout with the human text kept off it. The existing human output stays the default.
- **Detached jobs.** `--detach` on a long operation returns a handle immediately;
  `numera jobs list|show|stop|prune` polls and controls them. Verified end to end: spawn, poll a
  running job, stop it, and confirm the OS process is gone.
- **An MCP surface** (`engine/numera/mcp.py`) over the same operations, plus `docs/agents.md`.

- **An Extensions page.** The default set of unpacked Chromium extensions is now managed from the
  sidebar instead of only from the CLI. Adding a folder seeds every profile created afterwards;
  **Apply to all** pushes the set onto profiles that already exist, *merging* rather than replacing
  so a profile keeps whatever it already carried. The page states the limit that surprises people:
  Firefox-family engines cannot load Chromium extensions at all.

- **A portable desktop app.** `scripts/build-desktop.ps1` produces a 32.6 MB folder containing a
  Tauri shell and a frozen engine. Copy it anywhere and double-click: no Python, no Rust, no
  VC++ redistributable, no installer. Only WebView2 is required, which ships with Windows 11.
  The shell picks a free port at runtime, waits for the engine's health endpoint rather than
  sleeping, shows the engine's real stderr if it fails to start, and tears the engine down with
  the window - including when the shell is killed, which is what the job object is for. macOS and
  Linux need their own build hosts; Tauri cannot be cross-compiled from Windows.

- **A local proxy-authentication relay.** A profile whose proxy carries credentials is now
  launched through a small loopback relay that terminates the authentication and forwards
  upstream, so the browser only ever sees an unauthenticated `http://127.0.0.1:<port>`. That is
  the one thing every engine can do: fingerprint-chromium has no password support on
  `--proxy-server`, Firefox cannot do authenticated SOCKS, and Chromium refuses SOCKS
  authentication outright. Stdlib only, ephemeral port, bound to loopback, torn down with the
  browser session. HTTP `CONNECT` and SOCKS5 upstreams are both supported.

### Fixed

- **The CLI and the dashboard kept separate profile stores** (B17). A profile created headlessly
  was invisible in the GUI, and one created in the GUI was invisible to `numera list` until a
  launch happened to materialise it. `DashboardStore` now mirrors every write into the engine
  store, and the profile list adopts anything the CLI created. Mirroring lives in the store rather
  than in each of the five API write paths, so the next one added cannot miss it.

- **`device_scale_factor` ignored the profile and `has_touch` was never set.** The launcher
  hardcoded a 2-or-1 scale factor, so CSS resolution queries disagreed with
  `window.devicePixelRatio`; and without `has_touch`, `(pointer: coarse)` reported desktop while
  `maxTouchPoints` reported a phone. Two contradictions a site can simply read. Both now come from
  the profile.

- **The light theme left a dark page behind it.** `index.html` hardcoded the page background, so
  switching to light repainted the app but not the document beneath it. Both the boot background
  and the app root are tokens now, and follow the theme.

- **A credentialed SOCKS proxy could not be used at all.** Chromium's own refusal
  (`Browser does not support socks5 proxy authentication`) is real; the relay steps around it.
  Confirmed by reproducing the refusal with the relay bypassed and the success with it in place.
- **The proxy/leak test and trust report ran un-proxied for camoufox profiles.** The shared
  `session()` helper opened camoufox with no proxy, so those checks measured the host's own
  connection and then reported it clean - a check that passed by testing the wrong thing. It now
  takes the same proxy path as every other launch.
- `proxy_warnings()` no longer tells the user a SOCKS proxy's credentials are ignored; with the
  relay in front, they are not.
- **The dashboard silently showed fabricated profiles on any port but 8787.** `api.js` baked
  `http://localhost:8787` in at build time, so once the engine started choosing its port the app
  could not reach its own API - and instead of failing, it fell back to demo mode and rendered
  sample data. It is same-origin now, which has no port to get wrong.
- **The log redaction filter dropped any record with a numeric format.** It stringified every
  argument, so a `%d` then raised `TypeError` and `logging` discarded the line. Only strings are
  scrubbed now; a redaction filter that silently deletes log lines is worse than the leak it
  prevents.

### Changed

- **React 18 -> 19, react-dom 18 -> 19, lucide-react 0.383 -> 1.34.** Verified locally, since CI
  cannot run: the app builds, every page opens, the theme toggles, all 63 icons render non-empty
  and the console is clean. The two React packages were upgraded *together* — Dependabot proposed
  them as separate PRs, and taking one without the other leaves a broken pairing.
- **GitHub Actions bumped** to `checkout@v7`, `setup-python@v7`, `setup-node@v7`.
  `dependency-review-action` was left at v4: the proposed `@v5` tag does not exist upstream (only
  `v5.0.0` does), so merging it would have failed the workflow with "action not found".

- **One process, one port.** The engine now serves the built dashboard itself, so
  `numera serve` is the whole application at `http://localhost:8787` - no second dev server, no
  CORS, and one window instead of two. The SPA fallback explicitly refuses `/api/*`: without that
  guard a mistyped endpoint would answer 200 with the HTML page and the dashboard would try to
  parse it as JSON. `start.bat` also stopped guessing - it polls for the port instead of sleeping
  a fixed six seconds and hoping.
- A server or CLI-only install with no `dashboard/dist` still starts normally; the UI routes are
  simply absent.

### Changed - BREAKING

- **The project is now `Numera`.** It was `persona-studio` under MIT. Every user-facing name
  moved with it:

  | | Was | Is |
  |---|---|---|
  | Command | `persona` | `numera` |
  | Python package | `persona` | `numera` |
  | Distribution | `persona-engine` | `numera-engine` |
  | Environment | `PERSONA_API_TOKEN`, `PERSONA_PASSWORD`, `PERSONA_ALLOWED_ORIGINS` | `NUMERA_*` |
  | Data directory | `~/.persona` | `~/.numera` |
  | Launcher marker | `.persona_setup_done` | `.numera_setup_done` |

  Update any script, cron entry, systemd unit or shell profile that calls
  `persona` or exports a `PERSONA_*` variable.

- **Your existing data is not moved and not lost.** `ProfileStore` resolves
  `~/.numera`, and falls back to `~/.persona` when that is the only one present,
  so an existing install keeps reading its profiles, proxies and config in place.
  A real move to a portable application-local root is planned separately.

- **Licence: MIT -> proprietary, all rights reserved.** No redistribution, modification or
  reuse is permitted without written permission - this repository is closed source. The upstream
  MIT notice from persona-studio is still preserved verbatim under **Third-party notices** at the
  end of `LICENSE`, since MIT requires that regardless of the licence wrapped around it; that
  section also records the licence of each browser engine that may be bundled.

- **Python floor raised to 3.10** (was 3.9). patchright and camoufox both require 3.10, so the
  declared floor never matched the engines the project actually launches. `pip install` now
  refuses 3.9 with a clear message; the CI matrix is 3.10/3.12/3.13.

### Removed

- **`start.bat`.** The portable desktop app replaces it: `scripts/build-desktop.ps1` produces a
  folder you copy and double-click, with the engine frozen inside and no Python or Node needed on
  the target machine.


- `CODE_OF_CONDUCT.md` and `CONTRIBUTING.md`.
- `docs/launch.md` - a launch kit for a public open-source release (Show HN, r/selfhosted,
  awesome-lists, Product Hunt). Every channel in it depended on the repo being public, free and
  open source, which it no longer is.

### Documentation

- Every "open source" / GPL claim was swept from the README, badges, comparison table, the
  dashboard footer, the CLI description, `engine/numera/__init__.py`, `docs/social-preview.html`
  and `dashboard/README.md`.
- The `cloak` engine is documented honestly: it is still
  `ProfileStore.DEFAULT_ENGINE`, but **CloakBrowser requires a paid subscription
  and its licence forbids redistribution**, so it cannot be bundled. Camoufox is
  now the recommended engine for anyone who wants a free and open one.

## [0.3.0] - fork

### Fixed
- **WebRTC handed out a non-proxied public IP.** Chromium's
  `--force-webrtc-ip-handling-policy=disable_non_proxied_udp` is passed on every
  proxied launch but does not reliably stop ICE from offering an address that is
  not the proxy's — reproduced with a live proxy: HTTP exit `95.234.60.166` while
  WebRTC offered a different public address. Compounding it, the dashboard's
  "Altered / Disabled / Real" WebRTC picker was cosmetic: the engine had no
  `webrtc` field, so the choice was dropped in conversion and every profile
  behaved identically. The field now exists and reaches the page. `altered`
  (the default) drops candidates carrying a public address from both the ICE
  events and the SDP while leaving private/mDNS ones intact; `disabled` removes
  `RTCPeerConnection`; `real` leaves it alone. Profiles saved before the field
  existed load as `altered`.

### Added
- **`it-IT` and `nl-BE` locales**, with timezones, languages and coordinates, in
  both the engine and the dashboard's own table. A proxy exiting in Italy or
  Belgium had no locale to align to and could never pass the timezone check. A
  test now keeps the four country tables in step so a future locale cannot be
  added to one and forgotten in the others.

## [0.2.0] - fork

First release of the [KiaroSama](https://github.com/KiaroSama/Numera)
fork of [TechQaiser/persona-studio](https://github.com/TechQaiser/persona-studio).
Everything below is on top of upstream `0.1.0`.

### Fixed
- **Extensions now load in headless mode.** Playwright's default headless binary
  is `chromium_headless_shell`, which cannot load extensions at all, so
  `--load-extension` was silently ignored for every headless launch. Numera now
  launches the full Chromium in `--headless=new` (`channel="chromium"`), which
  loads extensions *and* stops the browser reporting itself as `HeadlessChrome`
  over CDP — a free stealth win, since that string was readable by anything
  attached to the DevTools endpoint.
- **`numera launch --headless` stays up.** It used to open the browser, hit the
  startup URLs and tear everything down immediately, because the "wait for the
  window to close" step only ran in headed mode. A headless session now blocks
  until the browser is stopped, which is what makes the dashboard's launch button
  usable on a machine with no display.
- **Sticky-session proxies survive import.** The pool deduped on `host:port`, so
  a vendor list that points every entry at one gateway and selects the exit node
  from the credentials collapsed into a single proxy. The dedupe key now includes
  the username and password, which is where that identity actually lives.
- **Proxy warnings instead of silent failures.** Chromium implements proxy
  authentication for HTTP(S) only. A SOCKS proxy carrying a username/password is
  now called out at launch and marked failed in the pool health check, rather
  than being accepted and quietly failing to connect.

### Added
- **Global extension set** — `numera ext default-add <folder>`,
  `ext default-list`, `ext default-remove`. Every profile created afterwards
  starts with it, from the CLI, the dashboard or `bulk-create`; exposed over the
  API as `default_extensions` in `GET`/`PUT /api/config`. Chromium has no
  browser-wide extension store to hook, so "global" means copied onto each
  profile at creation — `numera apply --all --extension <folder>` (also new)
  pushes it onto profiles that already exist.
- **Headless launching from the API** — `POST /api/profiles/{id}/launch` accepts
  `{"headless": true}`, and a persisted `headless_launch` config flag makes it
  the default, so the dashboard works unchanged on a headless server.
- **Optional API authentication** — set `NUMERA_API_TOKEN` and every request
  needs `Authorization: Bearer …` (or `X-Numera-Token`). With a token set, CORS
  stops defaulting to `*`; `NUMERA_ALLOWED_ORIGINS` sets it explicitly. Off by
  default, so a localhost install is unchanged.
- **CI and dependency automation** — the workflow now pins job timeouts,
  least-privilege permissions, an oldest/newest Python matrix, `npm ci` from the
  lockfile, and a browser-backed job that proves an extension really loads
  headless. Added Dependabot (pip / npm / actions) and a dependency-review gate.

## [Unreleased]

### Added
- **Folders manager** (dashboard **Folders** tab): create, rename and delete
  folders, each shown as a card with live profile / running / coherent counts and
  one-click open. Folders are persisted server-side (`GET /api/config` →
  `folders`, plus `POST /api/folders`, `…/folders/rename`, `…/folders/delete`), so
  an empty folder survives a reload; renaming or deleting one moves the profiles
  inside it (delete reassigns to *Unfiled*) rather than dropping anything. The
  editor, bulk-edit and sidebar dropdowns now read this live list.
- **Settings page** (dashboard **Settings** tab): theme toggle, the default launch
  engine for new profiles (persisted via `PUT /api/config`, with an "not
  installed" hint per engine), the on-disk storage location and a data summary
  (profiles / folders / proxies), and an about card. `GET /api/config` now also
  returns `root` and `version`.
- **Command palette** (dashboard, ⌘K / Ctrl-K): fuzzy-search across views,
  actions (new profile, toggle theme) and every profile (open / launch / stop),
  driven entirely from the keyboard (↑/↓/Enter, Esc to close).
- **Light / dark theme** (dashboard): a Sun/Moon toggle in the top bar switches
  the whole app between a new light theme and the original dark one; the choice
  is remembered in `localStorage`. The palette repaints inline styles, icons and
  the full stylesheet in one pass.
- **Toast notifications** (dashboard): success / error / info actions now surface
  as stacked, auto-dismissing toasts instead of blocking `alert()` popups.
- **Headless attach** (`numera attach --headless`, `POST …/attach {headless}`):
  attach a profile's DevTools endpoint without opening a visible window — for
  servers and CI. Exposed as a checkbox on the Automation page.

### Fixed
- **Auto-adjust capped at grade B** — `align` matched everything to the browser
  *except the timezone*, so on an engine that shows the host clock (e.g.
  CloakBrowser) the "timezone matches" check stayed failed forever, holding the
  grade at B (15/16). Align now follows the machine's real timezone and moves
  the locale to the coherent one for that zone (e.g. `Asia/Karachi` → `en-PK`),
  so a fully-aligned profile can reach A. Only moves the timezone when a locale
  models it, so coherence never regresses.
- Coherence's language check now compares the **base** language (`en`), not the
  region (`en-US` vs `en-PK`): an English browser in a Karachi timezone is a
  real, coherent user and shouldn't be flagged. A genuine mismatch (`de` vs
  `en`) is still caught.

### Changed
- Automation attach now reports success as a toast (with the port and whether it
  came up headless).

### Added (earlier this cycle)
- **Automation page** (dashboard, `POST /api/profiles/{id}/attach`): the engine
  already spoke CDP (`numera attach`) — now the dashboard drives it. One click
  opens a profile with a DevTools endpoint and hands back the connection details
  with ready-to-paste **Playwright / Puppeteer / Selenium** snippets (copy
  buttons, language tabs), so your script attaches to the *same* spoofed,
  warmed-up window instead of launching a fresh un-spoofed one. Detach = stop.
  Chromium engines only (cloak / patchright / playwright); the endpoint rejects
  Firefox-based camoufox with a clear message.
- **Fingerprint collision detector** (`numera collisions`, `collision.py`,
  `GET /api/profiles/collisions`): finds profiles that present the *same*
  fingerprint — identical WebGL, screen, hardware and user-agent read as one
  device wearing two hats, so a site can link the accounts and ban them
  together. Pure logic over the stored profiles (no browser), flagged on the
  dashboard **Health** page with a "Shared FP" count and a per-row badge, plus a
  CLI table. `identical` marks the worst case (same canvas/audio seed too).
- **Backup & restore** (`numera backup` / `numera restore`,
  `ProfileStore.backup`/`restore`): zip a profile **and its whole browser
  session** (cookies, localStorage — everything under user-data) into one
  portable file to archive it or move it to another machine. `restore --new-id`
  brings it in as a fresh copy instead of overwriting the original. (Unlike
  `export`/`import`, which move only the config.)
- **Bulk grid actions & tag filtering** (dashboard): the multi-select bar now
  also **warms up** and **exports** the selected profiles in one go (alongside
  the existing launch / edit-all / delete); the filter popover gained a **Tag**
  filter, and tags shown on a profile are clickable to filter by them.
- **Profile health overview** (`GET /api/profiles/health`, dashboard **Health**
  page): one row per profile grading it at a glance — coherence (computed
  instantly from the stored fingerprint), whether a proxy is set and its country,
  the last measured trust grade (cached from the last trust/align run, since a
  live check opens a real browser), any warm-up schedule, and last-active. Rows
  that need attention (incoherent, or a C/D trust grade) are flagged, with
  one-click **trust check** and **auto-adjust** per row.
- **Scheduled warm-up** (`numera schedule`, `scheduler.py`, `/api/schedules*`):
  recurring, unattended warm-up so a session you log into once doesn't go
  "cold". Add a schedule per profile (interval + vertical); when `numera serve`
  is running a background thread fires the due runs itself, and for people who
  don't run the server, `numera schedule run-due` runs whatever's due from cron
  / Windows Task Scheduler. Exposed in the dashboard's Warm-up section as a
  toggle with interval and vertical controls.
- **Warm-up presets per vertical** (`numera warmup --preset`, `warmup.PRESETS`):
  `ads`, `ecommerce`, `crypto`, `social`, `news` (or the default `general`) each
  browse a site set that matches the account's world, so its history reads like
  a real user of that vertical rather than a random walk. Chosen from a dropdown
  in the dashboard; passed through `POST /api/profiles/{id}/warmup`.
- **DNS leak checker** (`numera dns`, `probe.check_dns`,
  `/api/profiles/{id}/dns`): the companion to the proxy test — even with the
  proxy hiding your IP, DNS lookups that go to your own ISP's resolver still
  reveal the sites you visit at your real location. It resolves a batch of
  unique subdomains through the profile, reads back which resolvers were seen
  (and their countries) from a third-party test service, and fails if any sits
  outside the proxy's country. Folded into `proxy test` and surfaced as a **DNS
  leak test** button next to the connection test in the dashboard.
- **Proxy pool — import, test, assign** (`proxypool.py`, `/api/proxies*`):
  a managed pool the dashboard's Proxies page now drives for real. Import a
  pasted list or file in any common layout (`host:port`, `host:port:user:pass`,
  `user:pass@host:port`, `scheme://…`, optional trailing country code),
  health-check each one over HTTP(S) without a browser (exit IP/country/latency),
  and **assign them round-robin across profiles** — which also aligns each
  profile's locale/timezone to its proxy's country. Bulk-created profiles start
  proxy-less and get real proxies from here, instead of the old placeholder
  hosts.
- **Dashboard-side import** (`/api/profiles/import`, `to_dashboard_profile`):
  the GoLogin / AdsPower / Multilogin importer is now wired to an **Import**
  button in the dashboard — pick an export file, and each profile is mapped to a
  coherent identity, saved, and flagged for review if anything didn't line up.
  (Previously the importer lived in the CLI/engine store only.)
- **Auto-adjust — align a profile to reality** (`numera align`, `probe.py`):
  a one-click fix for a low trust grade. It opens the profile, reads back what
  the browser *actually* presents (platform, CPU, memory, GPU, screen, fonts,
  media, languages) and rewrites the fingerprint to match — regenerating the OS
  identity when the engine turns out to render a different OS than the profile
  claimed (e.g. CloakBrowser showing the host's real hardware). Afterwards the
  browser and the profile tell one story, so a detector sees no contradiction;
  the trust score jumps to A. Exposed as `POST /api/profiles/{id}/align` and an
  **Auto-adjust** button next to the trust check, which pulls the rewritten
  values straight back into the editor and shows the before→after grade.
- **Bulk create at scale** (`numera bulk-create`, `bulk.generate_profiles`,
  `POST /api/profiles/batch`): generate thousands of coherent profiles at once —
  up to 10,000 from the dashboard — spread across operating systems, countries/
  locales and Chrome versions, each with its own seed so no two share a
  fingerprint. The dashboard's bulk dialog gained OS/country/browser multi-pick,
  an engine choice, a randomize-hardware toggle and a per-country proxy switch,
  and posts in chunks; the profile grid is now **paginated** so a 10k list stays
  responsive.
- **Real GPUs and displays count as coherent.** `validate()` (and the dashboard
  meter) now accept any WebGL renderer whose graphics API fits the OS (Windows →
  Direct3D, macOS/Linux → desktop OpenGL) and any real-world screen size, not
  just the short curated lists — so a machine-aligned profile carrying its
  actual GPU/resolution isn't wrongly flagged. `navigator.languages` is matched
  on its primary language rather than an exact list.
- **Import from other anti-detect browsers** (`numera import-from`,
  `importers.py`): migrate profiles exported from GoLogin, AdsPower or
  Multilogin. Tolerant field mapping (each value looked up under its known
  aliases), gaps filled from a coherent generated base, and every imported
  profile run through `validate()` so mismatches are flagged for review rather
  than shipped silently.
- **Encrypt-at-rest for secrets** (`numera vault`, `crypto.py`): a master
  password encrypts proxy passwords in the store (Fernet/AES, key via
  PBKDF2-HMAC-SHA256), leaving the rest of each profile readable. Supply the
  password via `NUMERA_PASSWORD` or `--password` at use time so it never hits
  disk; locked stores still list/manage profiles but withhold the secret, and
  saving would-be-plaintext over an encrypted secret is refused. Optional
  `[secure]` extra; degrades gracefully when it isn't installed.
- **TLS/JA3 handshake checker** (`numera tls`, `probe.py`): reads the profile's
  own TLS/HTTP2 handshake back from an echo service and reports its JA3 and JA4
  fingerprints — the network layer JavaScript can never touch, since the server
  sees it before any page script runs. Checks it negotiates TLS 1.3 + HTTP/2 and
  sends GREASE like a real Chromium. Folded into `proxy test` too, plus
  `POST /api/profiles/{id}/tls` and a button in the dashboard.
- **Fingerprint trust checker** (`numera trust`, `probe.py`): opens the profile
  and runs the checks a fingerprinting script runs — webdriver, plugins, UA vs
  platform, WebGL, timezone, font probing, media devices, canvas stability — then
  scores the result. Grades the real browser, where `numera check` grades the
  config. Also `POST /api/profiles/{id}/trust` and a button in the dashboard.
- **Proxy tester + WebRTC leak checker** (`numera proxy test`): routes a real
  request through the profile, reports the exit IP/country/timezone and whether
  they agree with the identity, then checks whether WebRTC exposes an address the
  proxy was meant to hide. Wires up the dashboard's "Test connection" button.
- **Multi-profile synchroniser** (`numera apply`, `bulk.py`): one change applied
  to many profiles, selected by name, `--tag` or `--all`. Unmentioned fields are
  left alone; changing the locale moves timezone and languages with it. Exposed as
  `POST /api/profiles/bulk` and "Edit all" in the dashboard's selection bar.
- **Account warm-up** (`numera warmup`, `warmup.py`): browses ordinary sites at
  human pace so a fresh profile doesn't arrive with an empty history.
- **Local automation API** (`numera attach`, `automation.py`): launches the
  profile with the DevTools protocol open and prints connect snippets, so
  Selenium/Puppeteer/Playwright drive *that* browser — identity, proxy and
  session intact.
- **Extensions manager**: per-profile unpacked extensions (`numera ext
  add|list|remove`, `Profile.extensions`, dashboard field). Only the profile's own
  extensions load, so nothing carries between profiles.
- **More fingerprint signals**: per-OS installed fonts (answered by
  `document.fonts.check`), camera/microphone counts via `enumerateDevices` with
  seeded stable ids, sub-pixel ClientRects jitter, and seeded audio noise. Fonts
  and media counts are covered by `validate()` too.
- **Cookie import/export** (`numera/cookies.py`): move a logged-in session
  between machines or seed a fresh profile with one you already have. Reads
  Cookie-Editor / EditThisCookie JSON, Playwright storage state and Netscape
  `cookies.txt`; writes JSON or `cookies.txt`. Available as
  `numera cookies list|export|import`, `GET`/`POST /api/profiles/{id}/cookies`,
  and Import/Export buttons in the dashboard's **Advanced** tab.
- **One-click Windows launcher** (`start.bat`): double-click to install deps on
  first run, pick the default engine (CloakBrowser first), then start the engine
  API + dashboard and open the browser.
- **Persisted default engine** for new profiles (`config.json`): `numera
  default-engine [name]` command, `GET`/`PUT /api/config` endpoints, and the
  dashboard defaulting new profiles to it. CloakBrowser is the top-priority
  default, and the last engine you pick "wins" (becomes the next default).
- **HTTP API** (`numera serve`, `engine/numera/server.py`) wrapping the store,
  validator and launcher so the dashboard can manage and launch profiles for real.
- **`python -m numera`** entry point (`numera/__main__.py`).
- Dashboard **live/demo mode**: connects to the engine API when available
  (launching opens a real browser), falls back to sample data when it isn't.
- **Pluggable launch engines** (`numera/drivers.py`): choose a stealth backend
  per profile — `playwright` (built-in), `patchright`, `camoufox`, or `cloak`
  (CloakBrowser). New `numera engines` command, `--engine` flag, `Profile.engine`
  field, dashboard engine selector, and `GET /api/engines` endpoint.
- **Dashboard redesign**: new "ink violet" theme, Space Grotesk / Inter / JetBrains
  Mono typography, motion (load reveals, status pulses, drawer slide), and a
  dedicated **Engines** page recommending CloakBrowser & Camoufox with one-click
  install commands. Reduced-motion respected.

### Changed
- **Geolocation now agrees with the proxy.** Launches set
  `navigator.geolocation` to coordinates for the proxy's country (or the
  locale's, without a proxy) and pre-grant the permission, so a site that asks
  gets coordinates that match the exit IP instead of a US timezone with German
  GPS. New `devices.COUNTRY_GEO` / `LOCALE_TO_COUNTRY`.
- **WebRTC IP handling is enforced at launch.** With a proxy, non-proxied UDP is
  disabled (`disable_non_proxied_udp`) so WebRTC can't bypass it; without one,
  only the public interface is exposed so local IPs stay hidden.
- **Hardened the stealth script**: `navigator.webdriver` now reads `false` from the
  prototype (not an own property), `window.chrome`, realistic `navigator.plugins`/
  `mimeTypes` (native `PluginArray`), consistent `permissions.query`, and native
  `toString()` on all patched functions — passing BrowserScan's basic checks.
- `launcher.py` is now a thin dispatcher over `drivers.py`.

### Fixed
- **Canvas and audio noise are now stable across reads.** Both drew from a single
  advancing PRNG and mutated the caller's canvas/buffer in place, so reading the
  same canvas twice gave different bytes — itself a detection signal (CreepJS
  checks for exactly this). Each source now restarts from the profile seed and
  works on a copy.
- **`enumerateDevices` spoofing actually applies.** It was building device objects
  with `Object.assign` against getter-only `MediaDeviceInfo` properties, which
  threw and silently fell back to Chromium's real list. Found by the new trust
  checker.
- **Startup URLs are now opened** on launch (first in the main tab, the rest in
  new tabs) instead of always landing on `about:blank`.
- **Launch button no longer sticks on "running"** after you close the browser
  yourself — the dashboard polls run status in live mode and flips back to start.
- **"Last active" now updates** (server records when a session ends and shows a
  relative time) instead of always reading "never".
- **New profiles no longer overwrite an existing one** — the server assigns each
  profile a unique id, and the local id generator is collision-resistant.
- Suppressed Chromium's infobars: the "controlled by automated test software"
  bar (Playwright) and the cosmetic "--no-sandbox unsupported flag" bar
  (CloakBrowser, via `--test-type` — invisible to websites).
- `numera launch --headless` no longer hangs forever waiting for a window that
  never opens.

## [0.1.0] - 2025

### Added
- **Engine**: coherent fingerprint generator (device-archetype approach) with
  `validate()`; `Fingerprint`/`Proxy`/`Profile` models; JSON-per-profile store
  with import/export and persistent user-data dirs; stealth init-script
  generator; Playwright launcher with proxy support; full CLI.
- **Dashboard**: React web console with profile grid, tabbed fingerprint editor,
  live coherence meter, proxy manager, bulk create, folders, tags, search.
- Monorepo layout, CI, docs, and contribution guides.
