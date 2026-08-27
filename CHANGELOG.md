# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

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

### Removed

- `CODE_OF_CONDUCT.md` and `CONTRIBUTING.md`.

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
