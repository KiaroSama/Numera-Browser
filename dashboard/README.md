# Numera Browser - Dashboard

The web front-end for [Numera Browser](../README.md) — an operations console for
managing browser profiles, styled after commercial anti-detect browsers but
self-hosted.

> This is the UI. For the engine that actually generates fingerprints and
> launches browsers, see [`../engine`](../engine).

## Run it

```bash
cd dashboard
npm install
npm run dev        # opens http://localhost:5173
```

Start `numera serve` in another terminal and the two connect: the dev server
forwards `/api` to `127.0.0.1:8787`, because Vite serves this app on 5173 while
the engine listens on 8787. That forwarding is development-only - in production
the engine serves the built bundle itself, so the client asks its own origin and
needs no proxy.

Build for production, and run the tests:

```bash
npm run build      # outputs to dist/
npm run preview
npm test           # vitest, in test/
npm run lint       # eslint
```

## What's inside

- **Profile grid** — live running/stopped status, environment, proxy, last active,
  and per-row launch / edit / clone / delete.
- **Fingerprint editor** — a tabbed drawer covering OS, browser, user-agent,
  screen, CPU/RAM, WebGL, and the spoofing modes for canvas, WebRTC, audio and
  fonts, plus timezone, locale, geolocation and media devices.
- **Launch engine selector** — pick the stealth backend per profile
  (`playwright` / `patchright` / `camoufox`); Numera Browser manages, the engine renders.
- **Live coherence meter** — the signature feature. As you edit a fingerprint it
  re-checks consistency in real time (mirroring the engine's `validate()`), so a
  Windows profile wearing an Apple GPU is flagged instantly.
- **Proxy manager, bulk create, folders, tags, search, multi-select.**

## Live mode vs demo mode

On load the dashboard pings the engine API (`numera serve`, default
`http://localhost:8787`). The sidebar footer shows which mode you're in:

- **Engine online** — the API is reachable. Profiles are read from and saved to
  disk, and the launch (▶) button opens a **real Chromium window**.
- **Demo mode** — the API is offline, so the UI runs on in-memory sample data.
  Perfect for exploring the project; nothing is persisted and launch is simulated.

To go live, start the engine API in another terminal:

```bash
cd ../engine
pip install -e ".[api,launch]"
playwright install chromium
numera serve
```

If the engine is on another port or host, either change the proxy target in
[`vite.config.ts`](vite.config.ts) or set a `VITE_API_URL` env var, which the
client uses directly and so bypasses the proxy. The API client lives in
[`src/api.ts`](src/api.ts).

## Tech

Saving a stale profile keeps your draft and reloads the latest revision. Edits to
different fields are combined; choose **Keep my edit** or **Use latest** for fields
both writers changed, then save again. Editing pauses while those choices are
pending. Proxy password intent is preserved, and conflict previews hide secrets.
If another writer removed the proxy, **Keep my edit** restores the whole local
proxy with its known password (including an intentionally empty password).
An unavailable password blocks that choice; unlock and reload before restoring
the proxy, or choose **Use latest** to accept the removal.
A failed refresh keeps the draft with an explicit retry message. Other storage
conflicts retain their own error and are not automatically rebased.

React 19 with strict TypeScript and Vite 8.3.4. `npm run typecheck` checks source
and tests; `npm run build` type-checks before bundling. Icons from `lucide-react`.
The manual **Dashboard verification** workflow runs the locked install, dependency
consistency and advisory checks, tests, strict types, lint and build on a hosted
runner. Full CI calls that same workflow; it does not duplicate the suite.
Styling uses the existing scoped theme, inline styles and handwritten `ps-`
classes; the updater's small stylesheet reuses those tokens. No CSS framework
is introduced.

## Desktop update interface

A signed portable shell supplies a per-run loopback capability to the main
WebView at its exact engine origin. `updates.ts` validates bounded status data;
`useUpdates.ts` owns one serial status poller shared by Settings and Engines.
Native code, not React polling, checks the release feed after readiness and daily.
Download and install/restart have separate exact-offer consent dialogs. Busy work
defers installation. Source/server/demo pages cannot self-update; they use their
own package/deployment route. Engines also provides separate Cloak acquisition
consent; an installed SDK alone does not prove its browser is ready.

The implementation is under hosted verification. The rendered UI fixture uses a
synthetic bridge; actual native worker acceptance is a separate check. See
[update protocol and limits](../docs/updates.md).

The source splits by responsibility, and the dependencies point one way —
`App` → views → primitives → leaf, with no cycles:

| | |
|---|---|
| `App.tsx` | state, routing and the error boundary. Nothing else. |
| `views/` | one file per view, plus the bulk modals and the command palette |
| `ui.tsx` | primitives more than one view needs |
| `deviceData.ts` | device archetypes and the coherence check — the mirror of `numera/devices.py` and `numera/fingerprint.py` |
| `demoData.ts` | sample rows for demo mode. Only `App.tsx` may import it, so a view cannot render a fabricated profile by accident |
| `toast.tsx` | the toast store and its renderer |
| `theme.ts` | palettes, CSS custom properties, the stylesheet |
| `api.ts` | the only place that knows about HTTP |

A component used by exactly one view lives in that view's file; `ui.tsx` is for
the genuinely shared ones. `engine/tests/test_dashboard_imports.py` enforces the
boundaries — and catches the import mistakes Vite builds without complaint, since
esbuild resolves module specifiers but not free variables: a JSX tag with no
import compiles cleanly and throws at render.
