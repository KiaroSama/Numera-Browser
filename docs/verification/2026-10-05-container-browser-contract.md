# Container Playwright compatibility — implemented correction

**First follow the maintainer's actual Rules, Spec Kit process and applicable hooks. Do not disable checks or weaken assertions. The submitting assistant must not merge or close this PR.**

Baseline: `db310ab6c9b31a2ac7fa3c97e37e97bb6944ded3` (`0.36.0`).

## Cause and implementation

The Dockerfile selected the `v1.62.0-noble` Playwright image, but installed the application's `launch` extra with only `playwright>=1.40`. A fresh resolver could therefore select a client needing browser revisions absent from the image. The container CI checked HTTP health, authentication, profile storage and cryptography, but never launched the image's browser.

The Dockerfile now installs the application and `playwright==${NUMERA_CONTAINER_PLAYWRIGHT_VERSION}` in one resolver transaction and runs `pip check`. A contract test requires that pin to equal the literal `FROM` tag. Keeping `FROM` literal preserves the existing Docker dependency updater; source installations retain a flexible release range (updated to the current compatible floor), while the desktop extra explicitly respects Camoufox's current `<1.63` ceiling. No independently overridable version build argument is added. Image, container pin, existing CI pin and desktop floor must be reviewed together on a version update.

`scripts/smoke_container_browser.py` checks the installed version, requires the configured non-root Linux user, and selects an executable resolving inside `/ms-playwright`. It launches that exact executable, renders a synthetic local page, checks DOM text and JavaScript, then closes page, context, browser and driver. Every close is attempted even when another fails; a cleanup error or still-connected browser is failure. Logging supports DEBUG/INFO/WARNING/ERROR with UTC timestamps and does not dump credentials or the environment. Each CLI run writes UTF-8 diagnostics under `scripts/logs` relative to its launcher (stdin in the image uses `/app/logs`), named `smoke_container_browser_YYYY-MM-DD_HH-mm-ss_UTC.log`; exclusive creation adds a suffix on collision. `--debug` adds phase diagnostics. Unknown library exception text is omitted, controlled diagnostics pass through the engine's redaction filter, and run-owned handlers close on success or failure. Logging initialization failure is reported on stderr with console-only fallback. Logs are ephemeral in CI's removed container; locally they remain for support until the owner removes them.

The existing manual container CI job feeds the script through stdin to the built image. It uses the image's configured user, `--network none`, no host mounts/browser cache, a shared-memory allocation, and a bounded outer timeout. Interrupted-container removal is a separate always-run step; it cannot turn a timeout or smoke failure into success. Existing jobs, triggers, permission settings, API checks and required-job aggregation are preserved.

## Executed verification

- Same new definition/resolver checks on the unchanged baseline: **4 failed, 1 passed**, pytest exit **1**. The unconstrained synthetic-wheel control actually selected `1.63.0` rather than the image's `1.62.0`.
- Candidate: **32 passed**, zero failures/errors/skips, pytest exit **0**. Tests include offline pip resolution and conflict refusal; version mismatch; wrong user; absent/non-executable/outside-image binaries; page/JavaScript failures; individual cleanup failures; and an actual CLI failure exit.
- The baseline Dockerfile and CI workflow were verified against their Git blob hashes. YAML comparison verifies unchanged jobs and triggers and preserved existing container steps. Added shell commands passed `bash -n`; changed Python files compiled.

These are local Python 3.13.5 contract/unit tests. Playwright objects in the smoke unit tests are explicitly controlled doubles; the pip wheels are synthetic metadata fixtures. **No real Docker image or browser was executed in this local environment, and the full repository suite/private hooks were not executed.** Do not treat these results or historical main CI success as the new candidate's native/container approval.

## Reviewer acceptance

Run the new test file through the normal full checkout and Rules-prescribed hooks. Dispatch the existing manual CI once for the reviewed final branch, with the approved runner lifecycle. Require the new real-image Chromium step and all required jobs to pass on that exact SHA. Preserve the negative controls and verify a cold image build, not just a reused dependency layer. Check existing `test_docker.py` and the normal engine suite. Repair any integration defect before merging rather than marking missing native evidence passed.

Inventory and disposition all repository PRs according to the maintainer's Rules. Review each accepted correction; merge when verified, or integrate the corrected equivalent and then close its superseded PR with evidence. Mere closure is not completion. Only the owner/authorized reviewing agent performs those actions.

## Sources and design references

- https://playwright.dev/python/docs/docker — browser image contents and matching client versions.
- https://playwright.dev/python/docs/browsers — release-specific browser revisions.
- https://pip.pypa.io/en/stable/reference/requirement-specifiers/ — exact package requirements.
- https://github.com/dependabot/dependabot-core/issues/4597 — reason to avoid relying on unverified ARG parsing; the literal tag is retained.
- https://docs.docker.com/reference/dockerfile/ — image stages and environment behavior.
- https://github.github.com/spec-kit/ — use only through the owner's prescribed process.

Optional follow-up for owner evaluation: a compact image/client/browser-revision manifest and redacted final-SHA test summary. These do not expand this correction into a storage, lifecycle or dependency-framework rewrite.
