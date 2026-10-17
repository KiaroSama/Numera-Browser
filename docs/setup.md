# Installation and setup

Choose one route: a portable desktop release, a source checkout, or a server/container deployment. These are different installations; the portable updater does not rewrite source checkouts or containers.

## Portable desktop

1. Open [GitHub Releases](https://github.com/KiaroSama/Numera-Browser/releases). Select a completed build and its matching platform asset: Windows x64, Linux x64, or macOS ARM64. Do not use an ARM64 archive on an Intel Mac.
2. Download the archive and its published checksums/signature metadata. Verify before extraction; see [updates and verification](updates.md). If the release has no matching asset, it is not a supported ready-build route for that machine.
3. Extract the **whole** `Numera Browser` folder into a writable location. Keep the engine, browser resources, notices and manifests beside the shell; copying only the executable does not install the application.
4. Start `numera.exe` on Windows or `numera` on Linux/macOS. The shell starts its own engine on a free loopback port and opens the dashboard. Close the window to shut down its owned engine.

Python and Node are not prerequisites for a completed portable bundle. The operating system still supplies the WebView and native libraries: WebView2 on Windows, WebKitGTK 4.1/GTK 3 on Linux, and WKWebView on macOS. Browser binaries also need their platform's system libraries. The CI build platform does not guarantee every Linux distribution or older OS release is compatible.

Native publisher signing uses the project's private-trust certificate. It is **self-signed**, not a publicly trusted Windows publisher certificate or Apple Developer ID/notarization. SmartScreen and Gatekeeper can still warn or block the app. Do not install an unknown root certificate or disable system security to make it run. Obtain an appropriately trusted build if local policy requires it.

Playwright, Patchright and Camoufox resources are bundled as compatible SDK/browser generations. CloakBrowser's proprietary browser binary is **not redistributed**: its acquisition is a separate end-user, consent-gated vendor operation, subject to its license and version availability. An installed SDK is not proof that its browser binary is ready.

### First profile

1. Confirm the footer says **Engine online**. **Demo mode** contains sample data, not a connection to your real profiles.
2. Open **Settings** and choose the default browser engine. Create a profile with a name and the settings required by your authorized test.
3. Launch it from **Profiles**. Close or stop the session before editing resources that require exclusive profile ownership.
4. Open **Settings → Software updates** to check, review a target, approve its download and separately approve installation/restart. See [update safety](updates.md).

`playwright` is the initial default. Explicit engine choices on profile creation or engine-changing edits can update the persisted default; an unrelated notes edit does not. Use Settings or `numera default-engine` to set it deliberately.

### Data and backups

The portable shell resolves data beside its installation when writable, with explicit configuration taking precedence. **Settings → Storage** shows the actual root: use that location, not a guessed folder. Preserve the entire data root for a full backup; profile exports contain configuration, not necessarily browser sessions. `numera backup`/`restore` handle an individual profile and its session.

Stop owned sessions before copying live browser storage. Protect backups like credentials: cookies, proxy credentials and API-token records can grant access. Do not put them into Git, release artifacts or public support attachments. Updates preserve mutable data and external browser overrides; application rollback is not a guarantee that a newer browser's database can be opened by an older browser.

## From source

Requirements: Python **3.10+**; a supported Node branch satisfying [dashboard/package.json](../dashboard/package.json) (currently `^22.22.2 || ^24.15.0 || >=26.0.0`); and the browser/system prerequisites for the selected backend. Rust is unnecessary for the retained Python server. Desktop shell compilation requires Rust **1.90+** and Tauri's platform prerequisites.

From the repository root:

```bash
git clone https://github.com/KiaroSama/Numera-Browser.git
```

```bash
cd Numera-Browser
```

```bash
python -m venv .venv
```

Activate on Windows PowerShell:

```powershell
.\.venv\Scripts\Activate.ps1
```

Or on Linux/macOS:

```bash
. .venv/bin/activate
```

Install the API and Playwright backend into that environment:

```bash
python -m pip install -e "./engine[api,launch]"
```

```bash
python -m playwright install chromium
```

For another backend, follow its own prerequisites in [engine/README.md](../engine/README.md); installing Playwright's Chromium does not install Patchright's separate Chromium or Camoufox's Firefox.

### Dashboard and engine

Install the locked frontend dependencies:

```bash
npm --prefix dashboard ci
```

Build the dashboard so the source engine can serve `dashboard/dist`:

```bash
npm --prefix dashboard run build
```

Start the retained engine; it stays on loopback by default:

```bash
python -m numera serve --port 8787
```

Open <http://127.0.0.1:8787>. For frontend development, leave that engine running and start Vite in another terminal:

```bash
npm --prefix dashboard run dev
```

Vite uses <http://localhost:5173> and proxies `/api` to loopback port 8787. A standalone dashboard without the API enters demo mode. A locked API prompts for its token instead of presenting demo profiles as live data.

### CLI examples

```bash
python -m numera create qa-profile --os windows --locale en-US --engine playwright
```

```bash
python -m numera list --json
```

```bash
python -m numera launch qa-profile
```

```bash
python -m numera --help
```

Use only profiles/accounts and targets you are authorized to operate. Profile consistency checks are diagnostics, not proof of anonymity or permission to bypass a site's controls.

## Server and Docker

The same retained engine can run without the desktop shell. Wheel builds do not bundle browser binaries. Select required extras at installation, install the matching browser, and configure headless launching on a machine without a display.

[Docker Compose](../docker-compose.yml) builds the application with Playwright's matched runtime, persists `/data` in the `numera-data` volume, reserves browser shared memory, and publishes **only** `127.0.0.1:8787`. Before starting, set `NUMERA_API_TOKEN` in your private local environment or ignored `.env`; generate it with an approved secret manager and never paste the value into an issue, command transcript or repository.

```bash
docker compose up -d --build
```

```bash
docker compose logs --tail 100
```

Do not delete the named volume to upgrade the container. Source/container updates use the reviewed checkout/image deployment process, not the portable updater.

## Configuration and access

| Setting | Purpose |
|---|---|
| `--data-dir` / `NUMERA_DATA_DIR` | Explicit data-root selection; confirm the resolved path in Storage. |
| `NUMERA_API_TOKEN` | Bearer authentication. A non-loopback bind requires authentication. Named revocable tokens can also be managed in Settings. |
| `NUMERA_ALLOWED_ORIGINS` | Explicit browser origins allowed to call the API; avoid wildcard exposure. |
| `PLAYWRIGHT_BROWSERS_PATH` and backend-specific overrides | User-managed browser resources; outside managed replacement. |
| `--verbose` | Additional runtime diagnostics; see the engine's logging documentation. |

Keep the API on localhost. For remote access use an authenticated tunnel or a reverse proxy you control; a bearer token is not a reason to publish a browser-management API directly to the internet. Keep CDP endpoints private too.

## Logs and troubleshooting

The retained engine writes UTF-8 run logs under the actual data root's `logs/`. Check [engine logging](../engine/README.md) for exact controls. Logs use UTC timestamps and severity/component fields; file logging is diagnostic, console output is concise. Review/redact a log before attaching it to an issue. The retained engine prunes its oldest run logs to a bounded retained set; preserve a diagnostic copy privately when investigating an unresolved failure.

| Symptom | Check |
|---|---|
| Demo mode | Start the API, verify its port, then reload. |
| `numera` not found | Activate the environment or use `python -m numera`. |
| Browser executable missing | Install the selected SDK's matching browser, or use a complete portable bundle. |
| Update unavailable | Check the signed portable identity, supported platform, network, permissions and current update status. Do not bypass signature checks. |
| Busy/deferred update | Finish active sessions/jobs and review again; uncertain ownership must not be force-cleared. |

## Building distributions

Maintainers use the manually initiated [CI workflow](../.github/workflows/ci.yml) for one exact reviewed SHA. It runs blocking validation and native builds, then permits signed publication on trusted `main`. The workflow is the reproducible release path; a source checkout alone is not a signed downloadable build. See [updates](updates.md), [licensing](licensing.md) and [migration status](migration.md). Local build scripts remain source tools, not a claim that an unsigned output is a completed release.
