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
[`vite.config.js`](vite.config.js) or set a `VITE_API_URL` env var, which the
client uses directly and so bypasses the proxy. The API client lives in
[`src/api.js`](src/api.js).

## Tech

Saving a stale profile keeps your draft and reloads the latest revision. Edits to
different fields are combined; choose **Keep my edit** or **Use latest** for fields
both writers changed, then save again. Editing pauses while those choices are
pending. Proxy password intent is preserved, and conflict previews hide secrets.
A failed refresh keeps the draft with an explicit retry message. Other storage
conflicts retain their own error and are not automatically rebased.

Plain React 19 + Vite. Icons from `lucide-react`. Styling is a single scoped
`<style>` block — no CSS framework dependency.

The source splits by responsibility, and the dependencies point one way —
`App` → views → primitives → leaf, with no cycles:

| | |
|---|---|
| `App.jsx` | state, routing and the error boundary. Nothing else. |
| `views/` | one file per view, plus the bulk modals and the command palette |
| `ui.jsx` | primitives more than one view needs |
| `deviceData.js` | device archetypes and the coherence check — the mirror of `numera/devices.py` and `numera/fingerprint.py` |
| `demoData.js` | sample rows for demo mode. Only `App.jsx` may import it, so a view cannot render a fabricated profile by accident |
| `toast.jsx` | the toast store and its renderer |
| `theme.js` | palettes, CSS custom properties, the stylesheet |
| `api.js` | the only place that knows about HTTP |

A component used by exactly one view lives in that view's file; `ui.jsx` is for
the genuinely shared ones. `engine/tests/test_dashboard_imports.py` enforces the
boundaries — and catches the import mistakes Vite builds without complaint, since
esbuild resolves module specifiers but not free variables: a JSX tag with no
import compiles cleanly and throws at render.
