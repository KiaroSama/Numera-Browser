# Native executable acceptance candidate

The Python application remains the selected product and rollback implementation.
The native candidate is an internal test intermediate, not a signed release or an
installed replacement. Its existence does not complete the migration.

## Explicit construction

Cargo declares `numera-native-candidate` behind the non-default `native-candidate`
feature. Ordinary library/default builds do not enable it. The existing example
and the named binary share the same actual CLI/HTTP/MCP entry; neither forwards
application commands to Python. Only browser SDK operations use the fixed adapter.

The manual full verification batch builds this target explicitly with locked
inputs. The builder consumes Cargo's current JSON `compiler-artifact` and
`build-finished` records instead of trusting a previous executable left on disk.
The candidate's `--build-info` reports the embedded owner, version, source SHA and
hosted generation. Missing or malformed compile-time identity refuses that command;
later environment changes cannot relabel it. This identity is not a signature.

All candidate compilation and execution currently run on GitHub-hosted Windows
and Linux only. No local installation, new default selection or public candidate
archive is introduced by this checkpoint.

## Relocated application verification

Each of the existing four-browser Windows/Linux legs retains its source and native
library SDK cases, then stages the actual executable with three SDK-only modules,
the same-source built dashboard and required notices. A fresh hashed inventory is
read back before and after relocation. The test moves the layout outside checkout
ancestry into a path with spaces and executes from an unrelated working directory.
Application home, data and logs are synthetic isolated fixtures.

The SDK interpreter and previously installed browser caches remain explicit
external prerequisites. Windows Camoufox binds its effective pre-isolation local
application-data root through platformdirs' documented override; synthetic
application home/data remain isolated. No browser is copied or downloaded.
This is not a self-contained Python-free distribution, full browser-archive
acceptance, Docker integration or desktop runtime selection.

The external application scenario checks:

- CLI creation, readback, refusal, persistent-cookie import/list/export and actual
  MCP child execution using the relocated executable.
- HTTP authentication and byte-exact delivery of the staged dashboard/assets.
- Real browser cookie readback, repeated launch idempotence, busy refusal, stop,
  restart persistence and empty-import refusal without clearing the jar.
- Native server parent-death shutdown with an open browser: successful exit,
  absent claim, OS-observed empty SDK tree and a closed listener before emergency
  cleanup can count as acceptance.

Every process has bounded output/deadlines and a versioned receipt handed to the
outer timeout owner for separately grouped application and SDK descendants. Finite
CLI/MCP commands must prove those trees empty before fallback cleanup. Emergency
cleanup runs on failure, but cannot turn a failed
lifecycle assertion into a pass. File logs use the existing sanitized UTF-8 UTC
per-run migration logger and isolated native/adapter log directories; no real
profiles, credentials or command payloads belong in those logs. Failed acceptance
projects only bounded, allowlisted lifecycle events and exception types into the
existing CI log before temporary cleanup; raw SDK messages and command output are
not uploaded.

## Acceptance status

Source `7c88397` ran in hosted batch
[39059706094](https://github.com/KiaroSama/Numera-Browser/actions/runs/39059706094).
All four relocated Linux cases and Windows Playwright, Patchright and Cloak passed.
Windows Camoufox failed its first CLI cookie import; its source/library cases
passed. Safe retained diagnostics show import succeeded, then context open raised
`CamoufoxNotInstalled`; the ownership report was complete and clean. The prepared
Windows cache-root binding needs hosted verification against that same scenario;
it is not yet a proven repair. Both native tiers previously proved the historical
metadata bytecode failure, restored fixed bytes and passed the current tests.
CodeQL and required verification still failed; the integrated batch is not green.

See [migration status](migration.md). CodeQL, the 907-row capability ledger, complete
application coverage, representative replay, repeated comparative time/peak-memory
measurements, exercised rollback, native updater/signature acceptance and release
gates remain open. Python selection and its oracle fixtures are unchanged.
