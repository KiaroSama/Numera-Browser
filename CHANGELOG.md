# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.37.0] - 2026-11-26

- Build MSI, AppImage, DEB and portable desktop packages on the matching 64-bit
  Windows or Linux host. Publish a separate CLI-only archive for every target.
- Verify browser functionality, relocation and process ownership in CI. Record
  hosted timing and memory comparisons as diagnostic artifacts. Live Cloak runs
  when the vendor license is available; offline tests run regardless.
- Add the MCP HTTP bridge, shared Playwright/CDP commands, per-profile MAC and
  device names, proxy country flags, JSON and Netscape cookie export, and local
  workflows with actions across selected windows.
- Use the Rust/WASM Glimm dashboard and expose app and browser-core update
  preferences. Add `BrowserEngineProvider` as the extension point for future
  providers, including Numera Core Browser.
- Publish release packages through the protected `numera-signing` environment.
  The workflow verifies Authenticode, detached archive signatures and signed
  update metadata before publication.

- Select the Rust CLI, MCP, Axum API and native browser worker as the application
  runtime with Rust 1.100.0. Build the web dashboard from Rust/WASM and keep
  its 195 cases. Port all 3,418 backend/tooling source test identities and preserve the
  native inventories applicable to x86-64 Linux and Windows. Compile the real
  application and worker in CI for both platforms on every push and pull request.
- Port the current Camoufox font, media-device and speech-voice catalogs and
  seeded selection algorithms to Rust. Check their original data digests and
  360 frozen upstream draws in the existing browser-policy test. Preserve both
  branches of the source random-sampling algorithm and existing test identities.
- Preserve SDK evaluation integers, typed arrays, shared references and URL
  tuples; refuse non-finite/cyclic/non-JSON results instead of coercing them to
  null or text. Bound response encoding and reference expansion, retain ASCII
  wire escaping and source numeric formatting, and acknowledge encoding failure
  through the existing SDK error envelope. Existing tests gain 38 frozen source
  value/encoding cases and real-browser value assertions without new test IDs.
- Port Camoufox's pinned fpgen model sampling to Rust, with archive/member/data
  integrity checks, bounded Bayesian sampling and streaming value decoding.
  Extend the existing browser-policy checks with 18 frozen upstream posteriors,
  generation and model-tampering checks, preserving reported test counts. Native
  Camoufox preparation is exercised by policy and real-browser acceptance.
- Add a Rust SDK subprocess for Playwright/Patchright with bounded protocol I/O,
  cancellable stdin, persistent profiles, page identity policy, routes, mouse
  input, evaluation, full cookie objects and owned driver cleanup. Native builds
  package both executables after checking their independent Cargo records and
  matching compiled identities. Real-browser acceptance passes for Playwright
  1.64/1.65 and Patchright 1.64.1 without Python; vendor Node/SDK resources remain
  external. The native application is now selected by default; complete product
  distribution acceptance remains tied to current CI.
- Review CloakBrowser 0.7.2's Playwright compatibility cap and current Camoufox
  fingerprint-model/geometry changes. Update repository topics through the
  authorized saved GitHub profile, retaining language topics until cutover.
- Continue the native checkpoint on a separate Rust rewrite integration branch.
  Record the source test baseline, review current upstream browser releases and
  add automatic Linux, Windows and macOS native candidate builds. A Rust build
  tool verifies Cargo artifact provenance and compiled identity, then packages
  executable/source ZIPs with notices and SHA-256 checksums. These candidates
  do not complete runtime selection, test migration or desktop distributions.
- Pass the source archive destination to Git as an open file handle, preserving
  native Windows paths without Git's incompatible verbatim-path parsing.
- Give portable ZIP entries an explicit Unix creator format so Windows builds
  preserve executable and notice permissions when archives move between systems.
- Copy first-run files through bounded reads on Linux so filesystem copy offload
  cannot skip source access-time updates required by the original copy policy.
  Existing destination bytes and copytree metadata snapshots remain preserved.

- Begin a parity-gated native Rust domain library with source-generated identity,
  model, validation, conversion and vault fixtures; keep the current Python
  application selected until the complete migration is verified.
- Refresh Vite to 8.3.4, Lucide React to 1.55.0 and Node types to 26.6.5; share
  the bounded dashboard verification between manual maintenance and full CI.
  Native Windows/Linux Rust parity and source-fixture drift gates remain required.
  The library is not yet a replacement CLI or HTTP server.

- Couple the container's installed Playwright client to its bundled browser image;
  require an offline Chromium render and orderly cleanup, with sanitized run logs.
- Refresh dashboard, API, browser-wrapper and desktop dependencies, resolve the
  brace-expansion advisory, and align Node and Rust prerequisites with the current stack.

Three reviewed correctness fixes went in after 0.36.0, and the round of repairs
that reviewing them found. The shape they share: a check that read a label
instead of the thing the label described.

- Choose a profile's validation context by the value the caller declared, and
  refuse a declared container whose contents do not match that context as a
  domain error rather than routing it to the validator that permits the gap.
- Validate the **active persisted** vault descriptor on every recovery path,
  and read back what actually reached `config.json` instead of trusting the
  write that claimed to put it there.
- End a helper's processes and streams under one deadline, and when a straggler
  cannot be accounted for, retain it and say so by name - including when a
  bounded settle gives up because another caller held the lock for the whole
  budget.
- Say which category a rejected ownership record fell into as well as what was
  wrong with it, so a caller matching on the category still recognises the
  refusal a person now has enough detail to debug.
- Import a vendor profile whose timezone arrives nested under `id`. Everything
  else unreadable is still refused by name.
- Announce a field's label once. A labelled group around a labelled control
  gave assistive technology two names for one field; the group is named only
  when no control inside it took the name.
- Mark what the engine generates rather than presenting it as something the
  operator entered, and offer each country once.
- Refuse a fractional count in a flat profile edit - `cores`, `seed`, camera,
  microphone, speaker and touch counts, and memory outside 0.25/0.5 - instead of
  accepting it and storing a truncated number. The API, the batch and import
  routes and the dashboard store all read the rule from the model's field policy.
- Refuse an integer too long to print with the ordinary write refusal rather
  than a conversion error raised while formatting the message.
- Treat only "no such process" as proof that a process group is gone; any other
  probe error keeps the browser owned instead of reporting it stopped.
- Tie a data lock to the thread that holds it, so another thread can no longer
  pass as the owner of a transaction and replace the whole config underneath
  it; one `NUMERA_LOCK_TIMEOUT` deadline now covers waiting on another thread
  and another process, and an invalid value falls back to the default.
- Dependencies: Tauri 2.11.6 (security fixes), lucide-react 1.47.0, and dev tooling (vitest 5.0.1, eslint 10.11.0,
  jsdom 30.1.1).
- Report what a health action actually did instead of only that it ran.
- Reach every section from the command palette, and drop the sidebar entry that
  had no screen behind it.
- Let a registered self-hosted Windows runner take the Windows jobs, so the
  platform gate is a deliberate run rather than a permanently red check.
- Split `proctree` into the tree itself, the Windows job-object layer
  (`winjob`) and the POSIX group and terminal layer (`posix_session`). Behaviour
  is unchanged and every name is still reached through `proctree`; the file was
  1016 lines, which is past the point where anything can be added to it safely.

## Older releases

- [0.36.0 through 0.27.0](docs/changelog/0.36.0-to-0.27.0.md)
- [0.26.0 through 0.19.1](docs/changelog/0.26.0-to-0.19.1.md)
- [0.19.0 through 0.13.0](docs/changelog/0.19.0-to-0.13.0.md)
- [0.12.0 through 0.5.0](docs/changelog/0.12.0-to-0.5.0.md)
- [0.4.0 through 0.1.0](docs/changelog/0.4.0-to-0.1.0.md)
