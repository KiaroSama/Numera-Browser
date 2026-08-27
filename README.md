<div align="center">

# Numera

### Self-hosted anti-detect browser & profile manager

Create browser profiles that each look like a real, separate device — a
*coherent* fingerprint, its own proxy, and a persistent session — then manage
and launch them from a clean web console or CLI.

A self-hosted alternative to GoLogin, Multilogin and AdsPower.

![python](https://img.shields.io/badge/python-3.9%2B-blue)
![react](https://img.shields.io/badge/react-18-61DAFB)
![license](https://img.shields.io/badge/license-proprietary-lightgrey)
![tests](https://img.shields.io/badge/tests-passing-brightgreen)
![status](https://img.shields.io/badge/status-beta-orange)

<br>

![Numera dashboard](docs/screenshot-dashboard.png)

</div>

---

## Why Numera?

Most fingerprint tools randomise each value on its own — and that's exactly what
gets them caught. Anti-bot systems don't read values in isolation; they
**cross-check** them. A macOS user-agent with an NVIDIA GPU, or a 4-core phone
claiming a 1440p screen, is an instant tell.

Numera's core idea is **coherence**. It picks a realistic device archetype
first, then draws *every* value (OS, user-agent, GPU, screen, CPU, RAM, timezone,
language) from that archetype's real-world pool. The result is an identity whose
parts all agree — and a live coherence check that proves it, both in the engine
(`validate()`) and in the dashboard's real-time meter.

That coherence-first design is the thing commercial tools don't surface, and it's
what makes Numera distinctive.

## Numera vs the commercial tools

|  | **Numera** | Multilogin | GoLogin | AdsPower |
|---|:---:|:---:|:---:|:---:|
| **Price** | **Free** | from ~$99/mo | from ~$49/mo | free tier + paid |
| **Self-hosted** — data never leaves your machine | ✅ | ❌ cloud | ❌ cloud | ❌ cloud |
| **Live coherence meter** (proves the fingerprint agrees with itself) | ✅ | ❌ | ❌ | ❌ |
| **Trust checker** — grades the *real* browser, not the config | ✅ | ❌ | ❌ | ❌ |
| **Patched-binary stealth engine** | ✅ Camoufox / Cloak\* | ✅ | ⚠️ | ✅ |
| **Proxy pool + leak/DNS/TLS tests** | ✅ | partial | partial | partial |
| **CLI + Python API** | ✅ | ❌ | limited | limited |
| **Cloud team sync** | ❌ *(local-first by design)* | ✅ | ✅ | ✅ |

<sub>Pricing and feature comparison are approximate and based on each vendor's public plans at the time of writing — check their sites for current details. Numera doesn't do hosted team sync on purpose (see the roadmap); everything else is local and free.</sub>

## What's in the box

Numera is a monorepo with two parts that work together:

```
Numera/
├── engine/        Python core — fingerprints, storage, launcher, CLI
├── dashboard/     React web console — the visual profile manager
└── start.bat      Windows one-click launcher (installs + starts both)
```

| | |
|---|---|
| 🧬 **Coherent fingerprints** | OS, UA, GPU, screen, hardware, timezone & language always agree |
| 🛡️ **Live coherence meter** | Real-time consistency score as you edit — nobody else shows this |
| 🌐 **Proxy-aware** | Set a proxy's country and locale/timezone auto-align to it |
| 🎛️ **Rich editor** | Canvas, WebRTC, audio, fonts, geolocation, media devices, DNT |
| 💾 **Persistent sessions** | Cookies & localStorage survive between launches |
| 🍪 **Cookie import/export** | Move a logged-in session in or out — Cookie-Editor JSON, Playwright state, or `cookies.txt` |
| 🔎 **Trust checker** | Grades the *real* browser against the checks a fingerprinter runs |
| ✨ **Auto-adjust** | One click aligns a profile to what its browser actually presents — grade jumps to A |
| 📡 **Proxy & leak test** | Exit IP, country and timezone match + WebRTC leak detection |
| 🌐 **DNS leak check** | Confirms name lookups exit through the proxy, not your real ISP's resolver |
| 🌐 **Proxy pool** | Import, health-check and assign proxies round-robin across profiles |
| 🔐 **TLS/JA3 check** | Reads the handshake fingerprint — the one layer JS injection can't reach |
| ❤️ **Health overview** | One page: coherence, proxy, trust grade and warm-up schedule per profile |
| 🔑 **Encrypted vault** | Proxy passwords encrypted at rest behind a master password |
| 🧩 **Extensions** | Per-profile unpacked extensions, isolated from every other profile |
| ✨ **Warm-up + schedule** | Human-paced browsing (per-vertical presets) so a fresh profile isn't obviously fresh — and a recurring schedule so it never goes cold |
| 🤖 **Automation attach** | Drive a live profile from Selenium, Puppeteer or Playwright over CDP |
| 🔁 **Bulk edit** | One change applied to hundreds of profiles, coherently |
| 📥 **Migrate in** | Import profiles from GoLogin, AdsPower or Multilogin exports |
| 📦 **Bulk create at scale** | Generate thousands of coherent profiles — mixed OS, country, browser, per-profile hardware |
| 🗂️ **Folders, tags, search** | Organize hundreds of accounts |
| 🖥️ **Real browser** | Launches a real browser, per-profile isolation |
| 🔌 **Pluggable engines** | Swap the stealth backend: CloakBrowser, Camoufox, Patchright, Playwright |
| 🔗 **Startup URLs** | Each profile opens the pages you want, in its own tabs |
| 🖱️ **One-click launcher** | `start.bat` installs everything and starts both halves on Windows |
| ⌨️ **CLI + Python API** | Script it or drive it by hand |

## What Numera adds

The gaps that show up running an anti-detect manager unattended on a server:

| | |
|---|---|
| 🧩 **Extensions work headless** | Playwright's default headless binary is the *headless shell*, which cannot load extensions at all and announces itself as `HeadlessChrome` over CDP. Numera now launches the full Chromium in `--headless=new`, so extensions load and the browser stops advertising that it is headless. |
| 🌍 **Global extension set** | `numera ext default-add <folder>` — every profile created afterwards starts with it, from the CLI, the dashboard or `bulk-create`. `numera apply --all --extension <folder>` pushes it onto profiles that already exist. |
| 🖥️ **Headless launching** | `numera launch --headless` now keeps the session up instead of closing the moment it opens, and `POST /api/profiles/{id}/launch` takes `{"headless": true}` — so the dashboard's ▶ button works on a box with no display. |
| 🔐 **Optional API auth** | Set `NUMERA_API_TOKEN` and every request needs `Authorization: Bearer …`. Off by default, so a localhost setup is unchanged. |
| ⚠️ **Honest proxy warnings** | Chromium only authenticates HTTP(S) proxies. A SOCKS proxy carrying a username/password is now flagged at launch and in the pool health check instead of silently failing to connect. |
| 🛡️ **WebRTC no longer leaks the real IP** | Chromium's `--force-webrtc-ip-handling-policy` does not reliably stop ICE from offering a public address that isn't the proxy's — verified: exit IP `95.x` while WebRTC handed out a different public IP. The dashboard's WebRTC picker was cosmetic (the engine had no such field), so the choice never reached the browser. Now it does: **Altered** drops public candidates from both the ICE events and the SDP while leaving private/mDNS ones alone, **Disabled** removes `RTCPeerConnection`, **Real** leaves it untouched. |
| 🗺️ **Italian and Belgian locales** | `it-IT` and `nl-BE`, with timezones, languages and coordinates — proxies exiting in Italy or Belgium previously had no locale to align to and always failed the timezone check. |
| 🔁 **Sticky-session proxy import** | The pool used to dedupe on `host:port`, which collapsed a 16-line sticky-session list — one gateway, credentials picking the exit node — into a single proxy. Credentials are part of the identity now. |

Details for each are in [engine/README.md](engine/README.md).

## Quick start

**Prerequisites:** [Python 3.9+](https://www.python.org/downloads/) and (for the
dashboard) [Node.js 18+](https://nodejs.org/). Check with `python --version` and
`node --version`.

```bash
git clone https://github.com/KiaroSama/Numera.git
cd Numera
```

### Easiest (Windows): one-click launcher

Double-click **`start.bat`** in the project folder. On first run it installs
everything, lets you pick the default browser engine, then starts the engine API
+ dashboard and opens the console in your browser. The menu offers CloakBrowser
first, but it needs a paid subscription — pick **Camoufox** if you want a free,
open engine of comparable strength. Two
terminal windows stay open while it runs — close them to stop. To re-run setup,
delete `.numera_setup_done`.

The engine you pick "wins": it becomes the default for every new profile you
create afterwards, from the batch menu, the CLI, or the dashboard. Change it any
time with `numera default-engine camoufox`.

Prefer to do it by hand, or on macOS/Linux? Follow the manual steps below —
the two parts are independent, so you can run the engine, the dashboard, or both.

### 1. Engine (backend)

```bash
cd engine
pip install -e ".[launch]"      # omit [launch] to skip Playwright
playwright install chromium     # downloads the browser (needed for `launch`)

numera create acct-01 --os windows --locale en-US
numera launch acct-01          # opens a Chromium window; close it to exit
```

> **Windows / PowerShell:** keep the quotes around `".[launch]"` exactly as
> shown — without them the shell mis-reads the brackets.
>
> **`numera` not recognized?** The install worked but its Scripts folder isn't
> on your PATH. Either reopen your terminal, or run it as a module instead:
> `python -m numera create acct-01 --os windows --locale en-US`.

### 2. Dashboard (frontend)

```bash
cd dashboard
npm install
npm run dev                      # then open http://localhost:5173
```

> The dashboard runs on its own with in-memory **sample data**, so you can
> explore it without the engine (the footer shows *Demo mode*).

### 3. Connect them (launch real browsers from the dashboard)

Start the engine's API, then reload the dashboard — its footer flips to
*Engine online* and the launch (▶) button now opens a **real Chromium window**,
with profiles saved to disk.

```bash
cd engine
pip install -e ".[api,launch]"
playwright install chromium
numera serve                    # HTTP API on http://localhost:8787
```

Leave `numera serve` running and start the dashboard (`npm run dev`) in a second
terminal. That's it — clicking ▶ launches the profile for real.

Full details live in each part's README:
[engine/README.md](engine/README.md) · [dashboard/README.md](dashboard/README.md).

### On a headless Linux server

No display, so every launch has to be headless and the API is the interface:

```bash
python3 -m venv .venv && . .venv/bin/activate
pip install -e "engine[api,launch]"
playwright install --with-deps chromium

export NUMERA_API_TOKEN="$(python3 -c 'import secrets;print(secrets.token_urlsafe(32))')"
numera serve                       # stays on 127.0.0.1; reach it over an SSH tunnel
```

Then turn on headless launching once, and the dashboard's ▶ button works from
there on:

```bash
curl -X PUT http://127.0.0.1:8787/api/config   -H "Authorization: Bearer $NUMERA_API_TOKEN"   -H "Content-Type: application/json"   -d '{"headless_launch": true}'
```

**Keep the API on localhost.** It creates profiles, launches browsers and
exports cookies; `NUMERA_API_TOKEN` is a second lock, not a licence to bind it
to `0.0.0.0` on the open internet. Put it behind an SSH tunnel or a reverse
proxy you control. `NUMERA_ALLOWED_ORIGINS` narrows CORS when a browser has to
reach it.

To drive a profile from your own automation, attach instead of launching — the
browser stays up and hands you a CDP endpoint:

```bash
numera attach acct-01 --port 9222 --headless
```

## How it works

```
             ┌──────────────┐
   pick OS → │  archetype   │   curated real-world combos (engine/numera/devices.py)
             └──────┬───────┘
                    │  every value drawn from this archetype's pool
                    ▼
             ┌──────────────┐        ┌──────────────┐
             │ Fingerprint  │───────▶│  validate()  │   coherence gate
             └──────┬───────┘        └──────────────┘
                    │
        ┌───────────┴────────────┐
        ▼                        ▼
  context options          stealth.js init script
  (UA, viewport, locale,   (navigator.platform, WebGL,
   timezone, proxy, CH)     hardwareConcurrency, webdriver, canvas noise)
        └───────────┬────────────┘
                    ▼
          Chromium (persistent context per profile)
```

See [docs/architecture.md](docs/architecture.md) for the full walkthrough,
including how to connect the dashboard to the engine.

## Manager + pluggable engines

Numera's job is to be the **manager** — coherent identities, proxies, sessions,
folders, and a dashboard. The actual stealth browser is a **pluggable engine** you
choose per profile:

| Engine | Strength | Notes |
|---|---|---|
| `cloak` | Patched Chromium binary | CloakBrowser — C++-level fingerprint/TLS/CDP patches; reaches Cloudflare/DataDome/reCAPTCHA-v3-grade evasion. **Requires a paid CloakBrowser subscription** and its licence forbids redistribution, so Numera cannot bundle it — you install it yourself. |
| `camoufox` ⭐ | Patched Firefox (C++-level) | **Recommended** — the strongest engine that is actually free and open. Brings its own coherent fingerprint. |
| `patchright` | Runtime-patched Playwright | Drop-in; fixes `webdriver`/CDP leaks the stock engine can't. |
| `playwright` | Built-in, always available | Stock Chromium + injected stealth script. Passes basic bot tests (BrowserScan, sannysoft); can't beat TLS/CDP-level detection. |

```bash
numera engines                          # see what's installed + which is default
numera create acct-01 --engine camoufox # pick a backend per profile
numera default-engine camoufox          # set the default for new profiles
```

`cloak` is still the built-in default (`ProfileStore.DEFAULT_ENGINE`), which is a
leftover from before its licence was checked — **if you have no CloakBrowser
subscription, switch to `camoufox` or `patchright` on first run.** The last engine
you choose anywhere becomes the default for the next profile you create.

\* Cloak is not bundled and is not free; Camoufox is both.

Being honest about limits: pure JavaScript injection (the `playwright` engine)
has a ceiling — for Cloudflare / DataDome / reCAPTCHA-v3-grade evasion, pair a
runtime-patched engine with a residential proxy and a warmed-up session. Numera
stays the same manager over all of them.

## Roadmap

**Built**

- [x] Coherent fingerprint engine + `validate()`
- [x] Profile storage, import/export, persistent sessions
- [x] Playwright launcher with proxy support
- [x] Full CLI
- [x] Web dashboard: profile grid, fingerprint editor, coherence meter, bulk create, proxies
- [x] HTTP API (`numera serve`) connecting the dashboard to the engine — launch real browsers from the UI
- [x] Pluggable launch engines (CloakBrowser, Camoufox, Patchright, Playwright)
- [x] Persisted default engine + one-click Windows launcher (`start.bat`)
- [x] Per-profile startup URLs
- [x] Cookie import/export (CLI, API, dashboard) — Cookie-Editor JSON,
      Playwright storage state, Netscape `cookies.txt`

- [x] Account warm-up — human-paced browsing that ages a fresh profile
- [x] Local automation API — `numera attach` opens CDP for Selenium/Puppeteer/Playwright,
      plus a dashboard **Automation** page (one-click attach + copy-paste connect snippets)
- [x] Multi-profile synchronizer — `numera apply`, plus "Edit all" in the dashboard
- [x] Proxy tester + WebRTC leak checker (`numera proxy test`)
- [x] Fingerprint trust checker (`numera trust`) — grades the real browser, not the config
- [x] Extensions manager, plus fonts, media devices, ClientRects & audio noise

- [x] TLS/JA3 handshake checker (`numera tls`) — the pre-JS network layer
- [x] Encrypted secrets at rest (`numera vault`) — proxy passwords, master-password
- [x] Geolocation aligned to the proxy country + WebRTC IP-handling enforced at launch

- [x] Import from GoLogin / AdsPower / Multilogin (`numera import-from`)

- [x] Auto-adjust (`numera align`) — align a profile to what its browser really
      shows so a low trust grade jumps to A
- [x] Bulk create at scale (`numera bulk-create`, dashboard) — thousands of
      coherent profiles across OS / country / browser, with a paginated grid
- [x] Proxy pool — import a list, health-check it, and assign round-robin across
      profiles (aligning each profile's locale to its proxy country)
- [x] Dashboard-side import (GoLogin / AdsPower / Multilogin) — an Import button
      that maps each export to a coherent profile

- [x] DNS leak checker (`numera dns`) — reads which resolvers actually saw the
      lookups and fails if any escaped the proxy's country
- [x] Warm-up presets per vertical (`--preset ads|ecommerce|crypto|social|news`)
- [x] Scheduled warm-up (`numera schedule`) — recurring, unattended; the server
      fires due runs from a background thread, or drive it from cron with
      `schedule run-due`
- [x] Profile health overview — one page grading coherence, proxy, cached trust
      and warm-up schedule across every profile, with one-click fix
- [x] Fingerprint collision detector (`numera collisions`, Health page) —
      catches profiles that share a fingerprint before a site links the accounts
- [x] Backup & restore (`numera backup` / `restore`) — a profile **and its
      session** zipped into one portable file, movable to another machine
- [x] Bulk grid actions (warm up / export the selection) + tag filtering
- [x] Folders manager (dashboard) — create / rename / delete folders with live
      counts; renaming or deleting one moves its profiles instead of dropping them
- [x] Settings page (dashboard) — theme, default launch engine, storage location
      and data summary, backed by `GET`/`PUT /api/config`

**Next**

- [ ] Cloud sync + team roles — deliberately unbuilt: it needs a hosted backend,
      accounts and a threat model of its own. Numera stays local-first until
      that's designed properly rather than bolted on.


## Legitimate use & responsibility

Numera is a tool for **legitimate multi-account management**: marketing agencies
handling many client ad accounts, QA and ad-verification testing, web-automation
development, and privacy research — the same use cases served openly by
commercial anti-detect browsers.

Do **not** use it for fraud, credential stuffing, spam, evading bans you've
earned, or anything that violates a site's terms of service or the law. You are
responsible for how you use it.

## Donate

If this project helps you, donations are appreciated.

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |

## License

Proprietary. **All rights reserved.** See [LICENSE](LICENSE). This repository is not open
source: no permission is granted to use, copy, modify or distribute it.
