# Numera Browser - Engine

The Python core of [Numera Browser](../README.md): coherent fingerprint generation,
profile storage, and a Playwright-based launcher, with a full CLI.

> This is the backend. For the visual dashboard see [`../dashboard`](../dashboard).

## Install

```bash
cd engine
pip install -e .

# To launch real browsers (optional):
pip install -e ".[launch]"
playwright install chromium

# To serve the HTTP API for the dashboard (optional):
pip install -e ".[api]"
```

## CLI

```bash
numera create acct-01 --os windows --locale en-US --tag client-x
numera create acct-02 --mobile --proxy "http://user:pass@host:8080" --country PK
numera bulk-create --count 500 --os windows --os macos \
        --locale en-US --locale de-DE --prefix Batch --tag bulk   # many at once
numera list
numera show acct-01
numera check acct-01          # validate fingerprint coherence
numera engines                # list available launch engines
numera default-engine cloak   # set the engine new profiles use (CloakBrowser)
numera launch acct-01
numera export acct-01 acct-01.json
numera import acct-01.json
numera backup acct-01 acct-01.zip        # profile + whole session, one portable file
numera restore acct-01.zip               # bring it back (add --new-id to clone it)
numera collisions                        # any two profiles sharing a fingerprint?
numera import-from gologin-export.json   # migrate from another anti-detect tool
numera serve                  # run the HTTP API the dashboard talks to
```

`export`/`import` move a profile's *config* (a JSON file). `backup`/`restore`
move the whole identity — the config **and** its browser session (cookies,
localStorage) — zipped together, so a warmed-up account survives a move to
another machine. Treat a backup as sensitive: the archive holds the proxy
password in plaintext.

`collisions` catches an easy self-inflicted wound: two profiles that render the
*same* fingerprint (same WebGL, screen, hardware, user-agent) look like one
device to a site and can be linked, so a ban on one can take the others with it.
Regenerate one side of each pair (`numera regen <name>`).

### Migrating from GoLogin / AdsPower / Multilogin

```bash
numera import-from their-export.json
```

Point it at a profile export (a single object or a list) from GoLogin, AdsPower
or Multilogin. The mapping is tolerant — those tools rename fields between
releases, so Numera Browser looks for each value under any of the names it's known by,
fills gaps from a coherent generated base, and runs `validate()` on the result.
Anything that didn't line up is flagged for review; run `numera check <name>`
to see the details. Treat an import as a reviewed starting point, not a
guaranteed one-to-one copy. (Lands in the engine store / CLI; dashboard-side
import isn't wired yet.)

### Inspecting a profile

```bash
numera trust acct-01            # score it the way a fingerprinting script would
numera align acct-01            # auto-adjust: match the profile to what the browser shows
numera proxy test acct-01       # exit IP, country, timezone match + WebRTC leaks
numera dns acct-01              # do DNS lookups exit through the proxy, or leak to your ISP?
numera tls acct-01              # the TLS/JA3 handshake — the one layer JS can't touch
```

`dns` is the companion to `proxy test`: even with the proxy hiding your IP, if
name lookups go to your own ISP's resolver the sites you visit are still visible
at your real location. It hands a third-party test service a batch of unique
subdomains to resolve, reads back which resolvers actually did the work, and
fails if any sits outside the proxy's country. (It's also folded into `proxy
test`.) Needs internet.

`align` is the fix for a low trust score. The checker fails whenever the
*declared* identity and the *presented* one disagree — which is unavoidable with
an engine that renders the host's real hardware (CloakBrowser patches at the C++
level, so it shows this machine's GPU/CPU, not Numera Browser's). `align` opens the
profile, reads back platform, CPU, memory, GPU, screen, fonts, media and
languages, and rewrites the fingerprint to those values — regenerating the OS
identity (user-agent, platform) if the machine is a different OS than the profile
claimed. Afterwards nothing contradicts and the grade jumps to A. (On the
`playwright`/`patchright` engines the injected script already spoofs the hardware
to the declared identity, so there's usually nothing to align.)

`tls` reads the profile's own TLS/HTTP2 handshake back from an echo service and
reports its **JA3** and **JA4** fingerprints. This is the layer the whole
"JS injection isn't enough" argument is really about: a server sees the TLS
ClientHello *before* the browser has a document, so no page script — yours or an
anti-detect browser's — can change it. Run it on two engines and compare the
JA3/JA4 side by side to see, rather than argue, what each engine actually sends.
(Chromium engines send Chrome's real handshake because they *are* Chromium; the
check confirms nothing downstream turned it into a tell.)

`trust` opens the profile and runs the checks a real fingerprinter runs —
`navigator.webdriver`, plugins, UA vs platform, WebGL, timezone, font probing,
media devices, and whether canvas reads the same twice. It grades the *browser*,
not the config; `numera check` grades the config. Add `--json` to either for
machine-readable output.

`proxy test` needs internet: it routes a real request through the profile and
reports where it came out, then asks WebRTC whether it would give away an
address the proxy was meant to hide. It works without a proxy too — that's your
baseline.

### The proxy pool

```bash
numera pool add proxies.txt        # or - to read stdin
numera pool list
numera pool test <id> <id>         # several at once, or --all for the whole pool
numera pool edit <id> --host 1.2.3.4 --port 1080 --label "IT 1"
numera pool remove <id> <id>       # several at once
numera pool export --file out.txt  # importable lines; omit --file for stdout
numera pool assign --all           # round-robin across profiles, aligning locale to country
```

The same `<data-dir>/proxies.json` the dashboard's Proxies page uses, so a change
from either side is visible to both.

**SOCKS proxies are health-checked for real.** urllib has no SOCKS transport, so
`pool test` used to answer "test it via a profile's proxy test" for anything
SOCKS — which, for a pool that is mostly SOCKS, made the check look broken. It
now borrows the same loopback relay the browser uses, so the result is the real
exit IP. A wrong password and an unreachable host are reported differently,
because they need different fixes.

SOCKS4 included. It used to be declined with "the protocol has no authentication
to test", which was the wrong reason for the wrong thing - a health check needs a
connection and an exit IP, and neither involves authentication. The real gap was
that the relay spoke only SOCKS5; it speaks SOCKS4a now. What SOCKS4 genuinely
cannot do is carry a *password*: the protocol has a user-id field and no password
field, so a SOCKS4 proxy given one is refused with that reason instead.

`pool export` is the exact inverse of the import parser — a round-trip test holds
the two together, so an exported file re-imports to the same proxies, credentials
and country codes included. Treat the file as sensitive: it holds passwords in
plaintext.

Editing a proxy's address resets its health status. The stored verdict described
the old address, and reporting "OK" about somewhere that was never tested is the
same class of lie as a control that reports instead of acting.

### Folders

```bash
numera folder list                              # every folder, including ones only a profile references
numera folder create "Facebook Ads"
numera folder rename Amazon "Amazon EU"         # its profiles move with it
numera folder delete "Amazon EU"                # profiles are reassigned to Unfiled, never deleted
numera folder delete Old --reassign-to Archive  # ...or somewhere you choose
```

Folders are shared with the dashboard: the same list, the same file. Add
`--json` to any of these for machine-readable output.

### Extensions, warm-up, automation

```bash
numera ext install https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh
numera ext installed                   # what's been fetched from the store
numera ext add acct-01 ./ublock        # unpacked folder (the one with manifest.json)
numera ext list acct-01
numera ext default-add ./ublock        # every NEW profile starts with it
numera ext default-list
numera apply --all --extension ./ublock   # ...and push it onto existing ones
numera warmup acct-01 --minutes 10               # browse normally so the session isn't brand-new
numera warmup acct-01 --preset crypto            # warm up on that vertical's sites
numera schedule add acct-01 --every 12 --preset ads   # recurring, unattended warm-up
numera schedule list                             # what's scheduled and when it next runs
numera schedule run-due                          # run everything due now (for cron/Task Scheduler)
numera attach acct-01 --port 9222      # open it for Selenium / Puppeteer / Playwright
```

Chromium loads extensions per browser instance and every profile *is* its own
instance, so there is no browser-wide "install once" to hook. `ext default-add`
is the closest honest equivalent: a set that gets copied onto each profile as it
is created (CLI, dashboard and `bulk-create` alike). Profiles that already exist
predate the set, so `apply --extension` is what pushes it onto them; passing
`--extension ""` clears the list.

Extensions load in headless mode too. Playwright's default headless binary is
the *headless shell*, which cannot load extensions at all — Numera Browser launches the
full Chromium in `--headless=new` instead, which also stops the browser
advertising itself as `HeadlessChrome` over CDP.

`attach` launches the profile with the DevTools protocol listening and prints
ready-to-paste snippets. Your automation drives *that* browser, so the identity,
proxy and session stay exactly as Numera Browser set them up — a browser started by
Selenium itself would have none of them.

`warmup --preset` browses a per-vertical site set (`ads`, `ecommerce`, `crypto`,
`social`, `news`, or the default `general`) so the session's history looks like
a real user of that world, not a random walk. `schedule` reruns it unattended on
an interval: when `numera serve` is running it fires the due warm-ups itself
from a background thread; without the server, put `numera schedule run-due` on
cron or Windows Task Scheduler and it runs whatever's due. This keeps a session
that you log into once from going "cold" — no fresh cookies, an about-to-expire
token — which is itself a signal on your next visit.

### WebRTC

WebRTC gathers ICE candidates over UDP, which an HTTP proxy never carries, so
the browser can hand a page a public address that is not the proxy's exit — the
one signal that links a proxied session straight back to you. Chromium's
`--force-webrtc-ip-handling-policy` is set on every launch but does not reliably
prevent it, so the page layer finishes the job:

| Mode | Behaviour |
|---|---|
| `altered` *(default)* | `RTCPeerConnection` keeps working, but candidates carrying a public address are dropped from the ICE events *and* the SDP. Private and mDNS (`.local`) candidates are kept — a real browser behind NAT offers those, so stripping them would be its own tell. |
| `disabled` | `RTCPeerConnection` is removed entirely. Effective, but a page can notice the API is missing. |
| `real` | Left untouched. Only for a profile where WebRTC has to work. |

Set it per profile from the dashboard's **Fingerprint → WebRTC** picker (which
used to be cosmetic), or in the fingerprint itself:

```python
from dataclasses import replace
fp = replace(generate(os="windows"), webrtc="disabled")
```

Being honest about the limit: this is a JavaScript-level patch, so a
sufficiently determined fingerprinter can detect that `RTCPeerConnection` has
been wrapped. It stops the address leak, which is the part that actually
identifies you.

### Changing many profiles at once

```bash
numera apply --tag client-a --engine cloak        # move a whole set to CloakBrowser
numera apply --all --locale de-DE                 # locale, timezone and languages together
numera apply acct-01 acct-02 --add-tag q3 --regen # fresh fingerprints, same accounts
numera apply --tag old --proxy "" -y              # "" means go direct
```

Only the flags you pass are changed — everything else is left alone, so the same
command is safe to run again later for one more tweak.

### Encrypting secrets at rest

By default the store is plaintext JSON — easy to read and back up, but a proxy
password sitting in a file is a risk. Turn on the vault to encrypt just the
secrets (the proxy password), leaving everything else readable:

```bash
pip install -e ".[secure]"                 # one-time: the crypto backend
numera vault enable                        # prompts for a master password
numera vault status                        # off / locked / unlocked
```

Enabling re-writes existing proxy passwords as `enc:v2:…` tokens
(`enc:v1:` tokens written by an older build keep opening; the version travels
in the prefix because it is part of what the key derives from)
(AES via Fernet, key derived with PBKDF2-HMAC-SHA256). After that, give the
password when you need the proxy — set `NUMERA_PASSWORD` or pass `--password`:

```bash
NUMERA_PASSWORD=… numera launch acct-01   # subprocess inherits it; proxy works
numera list                                # works without it — only the secret is hidden
```

Names, OS, tags and the rest stay visible while locked, so you can still manage
profiles; only the proxy password is withheld until you unlock. Operations that
*need* that password refuse rather than run without it - profile export and
backup, proxy test, rotate, assign and pool export - because an artifact that
silently omits a credential is worse than one you were told you cannot make
yet. **Keep the
password — there's no recovery.**

It covers **every** secret under the data directory, not just the engine's own
profiles: the dashboard's copy of each profile and the shared proxy pool hold
the same passwords and are migrated in the same step. That is what
`vault.SITES` lists, and adding a store to the product means adding a line
there. It used to cover `profiles/*.json` alone, so `vault enable` reported
success while `dashboard/*.json` and `proxies.json` still held the passwords in
the clear.

The migration is all or nothing. If anything fails - encrypting, writing, or
saving the config - every touched file is restored and the vault stays off,
because a half-migrated store is the one state nothing can read.

### Cookies

Move a logged-in session between machines, or seed a fresh profile with one you
already have:

```bash
numera cookies list   acct-01                   # what's in the jar, by domain
numera cookies export acct-01 session.json      # Playwright storage state
numera cookies export acct-01 cookies.txt       # Netscape format (curl/wget)
numera cookies import acct-01 session.json      # merge into the profile
numera cookies import acct-01 session.json --clear   # replace instead
```

Import understands whatever you have on hand: Cookie-Editor / EditThisCookie
JSON, Playwright storage state, and Netscape `cookies.txt`. Export picks the
format from the file extension (`.txt` → Netscape) unless you pass `--format`.

Two things the browser decides, not Numera Browser: **session cookies aren't stored** (
Chromium keeps them in memory only, so they don't survive an export), and
Chromium **caps cookie lifetime at ~400 days**, so far-future expiry dates come
back shortened. Close the profile before moving cookies — a session can't be
open twice.

## HTTP API

`numera serve` (needs `pip install -e ".[api]"`) starts a small FastAPI app the
dashboard uses to manage and launch profiles for real. It listens on
`http://127.0.0.1:8787` by default (`--host` / `--port` to change).

**Authentication.** Unauthenticated by default, which is fine while it is bound
to localhost — but it can create profiles, launch browsers and export cookies,
so anything reachable from elsewhere needs a token:

```bash
export NUMERA_API_TOKEN="a-long-random-string"
numera serve
curl -H "Authorization: Bearer $NUMERA_API_TOKEN" http://127.0.0.1:8787/api/config
```

With a token set, requests without it get `401`, and CORS stops defaulting to
`*` (set `NUMERA_ALLOWED_ORIGINS` to the origins that genuinely need it —
`*` plus a token is a token any web page could spend). `X-Numera-Token` works
in place of the bearer header.

| Method | Path | Does |
|---|---|---|
| GET | `/api/health` | Liveness check the dashboard pings on load |
| GET | `/api/profiles` | List profiles (with live run status) |
| POST | `/api/profiles` | Create a profile |
| PUT | `/api/profiles/{id}` | Update a profile. A **patch**: fields you omit keep their stored values, so `{"name": "x"}` renames and changes nothing else |
| DELETE | `/api/profiles/{id}` | Delete a profile + its session |
| POST | `/api/profiles/{id}/launch` | Open the profile in a real Chromium window (`{headless}` for a display-less host) |
| POST | `/api/profiles/{id}/stop` | Close a running profile |
| GET | `/api/profiles/{id}/cookies` | Read the profile's cookie jar |
| POST | `/api/profiles/{id}/cookies` | Import cookies (`text` or `cookies`, plus `clear`) |
| POST | `/api/profiles/{id}/proxy-test` | Exit IP, country and WebRTC leak check |
| POST | `/api/profiles/{id}/trust` | Trust score + the individual checks (grade is cached for Health) |
| POST | `/api/profiles/{id}/align` | Auto-adjust to what the browser presents; before→after |
| POST | `/api/profiles/{id}/tls` | TLS/JA3/JA4 handshake fingerprint |
| POST | `/api/profiles/{id}/dns` | DNS leak check — which resolvers see the lookups |
| POST | `/api/profiles/{id}/warmup` | Start a background warm-up (`minutes`, `preset`) |
| POST | `/api/profiles/{id}/attach` | Open with a CDP endpoint (optional `{headless}`); returns `port`, `wsUrl`, snippets |
| GET | `/api/profiles/health` | One row per profile: coherence, proxy, cached trust, schedule |
| GET | `/api/profiles/collisions` | Groups of profiles that share a fingerprint |
| POST | `/api/profiles/bulk` | Apply one `patch` to many `ids` |
| POST | `/api/profiles/batch` | Save many generated `profiles` at once (bulk create) |
| POST | `/api/profiles/delete` | Delete many `ids` in one call; reports `removed`, `missing`, `failed` |
| POST | `/api/profiles/import` | Import GoLogin/AdsPower/Multilogin export `text` |
| GET | `/api/proxies` | List the proxy pool, each entry with how many profiles use it |
| POST | `/api/proxies` | Import proxies from `text` (or a `proxies` list) |
| PUT | `/api/proxies/{id}` | Change one in place. A changed address resets its health status - the old verdict describes somewhere else |
| DELETE | `/api/proxies/{id}` | Remove a proxy from the pool |
| POST | `/api/proxies/delete` | Remove several (`{"ids": [...]}`) |
| POST | `/api/proxies/{id}/test` | Health-check one proxy (exit IP/country) |
| POST | `/api/proxies/test` | Health-check several, concurrently. Omit `ids` for the whole pool |
| POST | `/api/proxies/export` | Render proxies back to importable lines |
| POST | `/api/proxies/assign` | Assign the pool round-robin across profiles |
| GET | `/api/schedules` | List warm-up schedules + when each next runs |
| POST | `/api/schedules` | Schedule a recurring warm-up for a profile |
| PUT | `/api/schedules/{id}` | Change interval / preset / enabled |
| DELETE | `/api/schedules/{id}` | Remove a schedule |
| POST | `/api/fingerprint/validate` | Coherence check a fingerprint |
| POST | `/api/extensions/install` | Fetch and unpack a Chrome Web Store extension (`{"url": "..."}` — a link or a bare 32-character id) |
| GET | `/api/extensions/installed` | Every extension in the default set, with the name and icon read from its manifest — store installs and hand-unpacked folders alike |
| GET | `/api/extensions/icon` | That extension's own icon file (`?path=<folder>`). Only folders the engine already knows are served; anything else 404s |
| GET | `/api/config` | Default engine, default extensions, headless-launch flag, folders, data dir |
| PUT | `/api/config` | Change any of them (`default_engine`, `default_extensions`, `headless_launch`) |

## Python API

```python
from numera import ProfileStore, generate, Profile, Proxy

store = ProfileStore()
profile = Profile(
    name="acct-01",
    fingerprint=generate(os="windows", locale="en-US"),
    proxy=Proxy(server="http://host:8080", country="US"),
)
store.save(profile)
```

`save()` and `delete()` write **both** copies of a profile — the engine's and
the dashboard's — so a script and the GUI never disagree about one, and they
share the same one-name-one-profile rule as the CLI and the API: a duplicate
name raises `NameTaken`. Looking a profile up by a name two profiles already
share raises `AmbiguousProfile` naming both ids, rather than silently picking
one; look up by id to act on either.

## Modules

| Module | Responsibility |
|---|---|
| `devices.py` | Curated, internally-consistent hardware/OS building blocks |
| `fingerprint.py` | Assembles coherent fingerprints; `validate()` proves it |
| `models.py` | `Fingerprint`, `Proxy`, `Profile` data models |
| `store.py` | JSON-per-profile persistence + user-data dirs |
| `repo.py` | The one path a profile changes: both copies, one transaction, names and secrets decided under the lock that writes them |
| `archive.py` | Moving a profile between the store and a file: `export`/`import`, and `backup`/`restore` of the whole session |
| `cookies.py` | Cookie import/export across the formats people actually have |
| `crypto.py` | Encrypt-at-rest for secrets (optional `cryptography` backend) |
| `importers.py` | Tolerant import from GoLogin / AdsPower / Multilogin exports |
| `probe/` | Proxy/DNS/leak/TLS tests, the trust report, and the auto-aligner (`align`) - one module per question: `network`, `dns`, `trust`, `align` |
| `collision.py` | Finds profiles that share a fingerprint (`numera collisions`) |
| `bulk.py` | Multi-profile synchroniser + bulk generator (`generate_profiles`) |
| `proxypool.py` | Parse pasted proxy lists (any layout) + health-check them |
| `warmup.py` | Human-paced browsing (per-vertical presets) that gives a fresh profile a past |
| `scheduler.py` | Recurring unattended warm-up bookkeeping (`numera schedule`) |
| `automation.py` | CDP attach so Selenium/Puppeteer/Playwright can drive a profile |
| `stealth.py` | Generates the page-level JS patch for a fingerprint |
| `launcher.py` | Thin dispatcher: picks the profile's engine and hands off |
| `drivers.py` | Pluggable launch backends (cloak / camoufox / patchright / playwright), the registry, and the gate every browser open goes through |
| `launch_options.py` | What a profile *means* to a browser before a backend is chosen: flags, proxy, DoH, geolocation, bookmarks, extensions |
| `handshake.py` | The startup channel a spawned session uses to tell its parent a browser is open on the profile, and the terminate-escalate-reap helper that ends one |
| `server/` | The HTTP API: stores, dashboard<->engine conversion, SPA mounting, and one module per route group. `numera.server` still exports `create_app`, `serve` and the converters |
| `cli/` | Command-line interface: one module per command group, plus `parser.py` for the argparse tree and `out.py` for terminal output. `numera.cli` still exports `main` and `build_parser` |

## Launch engines (pluggable)

Numera Browser *manages* identities; a **driver** actually opens the browser. Pick one
per profile with `--engine` (or in the dashboard), and see which are installed
with `numera engines`.

| Engine | What it is | Install |
|---|---|---|
| `cloak` | **Recommended.** CloakBrowser — a patched Chromium binary (C++-level fingerprint/TLS/CDP patches). Beats Cloudflare/DataDome-grade detection. | `pip install cloakbrowser` |
| `camoufox` | **Recommended.** Patched Firefox with C++-level fingerprint spoofing and its own coherent fingerprint. | `pip install "camoufox[geoip]" && camoufox fetch` |
| `patchright` | Drop-in **patched** Playwright — fixes `webdriver`/CDP leaks at runtime. | `pip install patchright && patchright install chromium` |
| `playwright` | Stock Chromium + Numera Browser's injected stealth script. Built-in, always available. | included in `[launch]` |

```bash
numera engines                              # list engines + install status
numera create acct-01 --engine patchright   # choose a backend
numera launch acct-01                        # uses the profile's engine
numera launch acct-01 --engine camoufox      # override for one run
```

Add your own backend by registering a function in `drivers.py`.

> **Reality check.** `playwright` injects JavaScript into stock Chromium, so it
> can't change the TLS/JA3 fingerprint or hide the CDP transport — it passes basic
> bot tests but not Cloudflare/DataDome/reCAPTCHA-v3. For that, use a
> runtime-patched engine (`patchright`, `camoufox`) plus a residential proxy and a
> warmed-up session.

## Tests

```bash
pip install -e ".[dev]"
python -m pytest
```
