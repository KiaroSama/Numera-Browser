# Rust and TypeScript migration

The accepted boundary is [ADR0001](adr/0001-parity-gated-rust-migration.md).
The existing Python application remains the default and rollback implementation.
`engine-rust` includes native domain/persistence services and an acceptance-only
CLI/server example, **not an installed replacement**. Passing library or narrow
entrypoint tests does not certify a finished product migration.

## Implemented native seams

- Fingerprint/Profile/Proxy typed serialization and explicit proxy-password intent.
- Integer-seeded identity generation with source-compatible MT19937 selection and
  the original device catalog; original Client Hints and coherence diagnostics.
  Native init-script configuration and the unchanged original page template are
  implemented; ten exact source-derived config/full-script cases passed hosted
  Windows/Linux. Real script application through the SDK transport is being checked;
  full browser and legacy encoding/number compatibility remain separate gates.
- Uncoerced engine-record/dashboard-patch field validation from the authoritative
  field-policy catalog, with source-generated refusal cases.
- Bidirectional profile shape conversion and absent/null/empty secret intent.
  The HTTP fingerprint-validation route uses tolerant conversion and the native
  identity validator; complete malformed/legacy/browser parity remains pending.
- Data-root precedence and isolated portable write probes; first-run copy/merge
  primitives that preserve destination files and retry failed imports without
  marking them complete. Executable/environment wiring remains pending.
- Native reentrant data locks, immediate session lease refusal, atomic file
  replacement and byte-preserving batch rollback primitives.
- Fernet v1/v2 envelopes, secret-write policy, authenticated completion checks,
  secret-file migration, ordinary enable and interrupted-enable recovery primitives.
  Ordinary rotation retains the old generation on refusal; interrupted rotation
  authenticates both generations before resuming. Complete runtime-open integration,
  crash/concurrency acceptance and remaining error contracts are unfinished.
- Dual-store adoption/materialization and save primitives, including exact-byte
  rollback when a later batch write fails; batch/transaction-time patch, tolerant
  lookup and lease/claim-aware deletion primitives. Full repository contracts remain pending.
- Credential-aware JSON profile export and dual-store import primitives; locked
  exports refuse before touching the destination. Streaming ZIP backup and staged
  restore replace sessions under a lease and roll back failed metadata commits.
  Real browser archives, large/hostile archive bounds, remaining error vocabulary,
  symlink compatibility and byte-format compatibility remain pending.
- Rich dashboard reads/save/batch/transaction-time patch primitives preserve raw
  fields, revision checks, explicit credential intent and atomic engine twins.
  Metadata reads/replacement and delegated lease-aware profile deletion are present;
  metadata remains outside the profile listing. Broader legacy, concurrent-writer
  and HTTP integration contracts remain pending.
- Folder list/create/rename/delete and retired-name resolution, with preserved
  profile/metadata fields. Actual HTTP create/rename/delete handlers reuse the
  same transactions; damaged-legacy and complete error contracts remain pending.
- Proxy paste/export round trips, structured-field validation and pool
  read/import/update/delete primitives with locked-secret preservation. Complete
  HTTP list/create/edit/delete/export handlers reuse those stores, preserving
  credential identity, clear/preserve intent, locked-vault refusal and health
  invalidation. URL/network trust rules, bulk
  HTTP archive and atomic round-robin assignment now reuse these stores,
  preserving explicit archived selection, unreadable-password refusal and
  transaction-time profile readback. Original HTTP/store fixtures are prepared,
  not executed. Native proxy checking now uses explicit authenticated transports,
  remote SOCKS DNS and the original echo/UA/result policy; HTTP checks run at most
  eight tasks and commit health once, while CLI checks retain sequential verdicts.
  Single checks preserve missing/locked refusal before network access. Native
  rotation now validates all destination addresses, pins each direct control-plane
  hop, revalidates redirects, applies cooldown and checks whether the exit moved.
  CLI and single/bulk HTTP handlers preserve their distinct attempt/health updates;
  original refusal/store and two-address loopback fixtures are prepared, unexecuted.
  The native loopback relay now supplies browser HTTP/SOCKS credentials and
  SOCKS4 USERID checks, retaining its runtime and connection tasks through SDK
  settlement. Original wire, admitted-idle shutdown and framed-SDK lifetime
  fixtures are prepared, not executed. Precise legacy/network errors, failed
  cleanup retention, real DNS/TLS/rotation checks and full acceptance remain pending.
- Extension manifest/localized labels, contained icon selection, ID diagnostics,
  CRX2/CRX3 extraction and staged install/reuse/replacement/removal primitives.
  Failed replacement keeps the prior installation; installed-list HTTP includes
  known store and manually configured resources through the same native description.
  Native async TLS download now supplies the fixed Web Store request to the
  staged installer, with total/read/connect deadlines, a streamed output ceiling
  and CRX magic checks. CLI installation is wired; loopback transport fixtures
  are prepared but unexecuted. Actual TLS/proxy compatibility, live download,
  icon-serving, remaining contracts and real browser activation are pending.
- Named token persistence, sticky auth-policy adoption, duplicate-name creation,
  lockout-aware revocation and constant-time digest verification primitives.
  Per-request auth and literal-host bind refusal decisions are present, with auth
  exercised through actual Axum request middleware. CORS response contracts and
  readiness/health and token list/create/revoke routes have in-memory HTTP parity
  cases. Shared body extraction covers source-generated shape, media-type and
  malformed-JSON diagnostics and valid UTF-8/16/32 wire autodetection. Complete
  surrogate/number/nesting/body-budget parity is unfinished. Typed token errors distinguish conflict from storage failure.
  Implemented diagnostic/token/folder/schedule/settings/proxy/installed-extension
  routes now compose with one auth/CORS
  boundary and startup legacy-token adoption. Losing that token file does not reopen
  protection. Complete remaining routes, SPA, listener and shutdown/error contracts
  are pending; this router still is not a replacement server.
- Schedule occurrence bookkeeping, single-occurrence claims/release/outcome recording,
  create/update validation and protected scheduler-owned fields. Actual HTTP
  list/create/update/delete handlers preserve refusal order and decorated rows.
  Warm-up dispatch, lifecycle integration, remaining numeric/error contracts and concurrent
  platform acceptance remain pending.
- Native process birth identities, Windows named jobs and bounded owned-tree
  cleanup, durable claim uncertainty checks and lease-retaining session admission.
  File-based startup readiness preserves original failure kinds and exit ordering,
  with owned-tree cleanup on failure; the narrow Linux integration check passed.
  A per-server registry now retains ownership, reaps only empty trees and attributes
  schedule completion to its exact occurrence; the limited lifecycle check passed
  hosted Windows/Linux. Profile status and deletion now share that actual registry;
  deletion stops only locally owned sessions and preserves foreign ownership refusals.
  The deletion HTTP and owned-session integration cases passed hosted Windows/Linux.
  SDK-only Python transport now waits inertly for native activation, exchanges
  bounded versioned UTF-8 frames and exposes sanitized SDK metadata/open/close.
  Native transport owns its pipes and session tree; protocol/error/EOF checks passed
  hosted Windows/Linux. The separate manual browser SDK tier opens real persistent
  contexts for four backends on both platforms. The first live run passed seven
  legs; Windows Camoufox stalled during NumPy binary import before browser entry.
  An isolated hosted regression passed after sequencing frame reads so no outstanding
  pipe read overlaps SDK initialization (10.53 seconds, same 90-second deadline).
  Duplicate-descriptor isolation alone did not fix it. Whole-tree cleanup passed;
  no ownership guarantee was relaxed. The subsequent matrix passed five legs and
  exposed SDK stdout contamination on Linux Camoufox plus isolated-world inspection
  on both Patchright hosts. Protocol output is now reserved independently of SDK
  stdout, and Patchright observation reads the main page world. Reverification is
  passed in all eight hosted SDK legs; these narrow results are not full
  backend/application acceptance. Forty source-derived effective-option cases
  capture the SDK call boundary with network/resources isolated explicitly. Native
  backend option construction passed all forty cases on hosted Windows/Linux;
  Native packaged-resource selection now preserves per-backend executables,
  explicit overrides, Camoufox addon/version options and writable GeoIP seeding.
  Its original-option, file-preservation and framed-runtime cases are prepared,
  unexecuted. Automatic executable-entry discovery, actual SDK enum receipt,
  platform packaging and complete launch-service acceptance remain pending.
  A native post-lease admission seam now rereads coherent current state and refuses
  missing, divergent or unreadable-credential profiles before any SDK activation;
  five source-derived cases passed hosted Windows/Linux. The admitted session lease
  now moves into the SDK owner without reacquisition; all eight hosted real-browser
  regression legs passed. Native clear-before-seed resource preparation passed nine
  original file/bookmark cases on hosted Windows/Linux. It requires the admitted
  profile lease before helper birth and refuses linked/reparse resource paths;
  that containment regression and integration are being verified. Real database,
  concurrent-path replacement and full failure-path acceptance remain pending.
  Native IP-marked signal resolution from an observed country is implemented;
  33 original pure outcomes passed hosted Windows/Linux. Actual country lookup and
  relay services remain pending. Native CDP hint payload/header policy is implemented;
  ten original current/new-page command cases passed hosted Windows/Linux. The SDK
  adapter executes the native commands and retains supplied header-interception
  policy; actual CDP application passed the eight-leg hosted SDK checkpoint.
  Five source-derived OS identities now pass page/wire checks on both
  Playwright/Patchright hosts, including first-request/Accept-CH timing, iOS
  suppression, script-resource completion and new-page configuration. The hosted
  eight-leg checkpoint on `bf29681` settled every owned tree. New-page navigation
  waits for its SDK callback to finish CDP configuration. The Windows fixture
  cleanup stall was isolated to the accept/wait boundary and resolved by moving
  one nonblocking listener directly into its worker instead of duplicating it;
  the precise underlying socket-mode change was not captured. Secure-origin and
  responsive-viewport fixture corrections preserve actual page assertions.
  Temporary worker instrumentation is removed; final regression verification and
  complete failure coverage remain pending.
  The server registry now has an actual SDK transport owner variant, with
  post-lease current-profile activation and retained transport handoff; its
  protocol and real-context ownership regressions passed hosted Windows/Linux.
  Independent review found an acknowledged-close/normal-exit race. Shared cleanup
  now gives the adapter a bounded normal-exit grace period before forced tree
  settlement; the synchronized regression, prior protocol/logging cases and all
  eight real-context legs passed on `ad77c24`. Complete activation-failure,
  concurrent-shutdown and natural-browser-exit pipe coverage remains pending.
  Native admitted launch composition now reuses IP-marked identity, resource
  preparation, effective options, init-script/CDP policy and SDK transport. Its
  caller must supply independently observed country, working/home directories,
  relay and packaged binary data. Composition passed all eight hosted SDK legs
  on `3dea706`, with native refusal/parity/strict lint checks on both hosts.
  Camoufox retains its original locale rather than applying the Chromium-only
  country resolver; an independent SDK-frame regression checks that distinction.
  Startup navigation preserves original ordering/new-tab decisions and
  per-URL failure continuation through the SDK transport. Four independent source
  cases and actual invalid-then-valid startup passed the hosted Windows/Linux
  checkpoint on `7cca1bc`, with all eight real SDK legs and native strict lint.
  Full launch-service integration, hard-parent-death and backend acceptance are pending.
  Native cookie write policy now preserves empty/no-clear admission avoidance and
  clear-before-add ordering, with SDK-only read/add/clear operations. Six original
  policy observations passed hosted Windows/Linux on `1767757`, alongside native
  parity and strict lint. Real isolated cookie roundtrips passed seven matrix legs;
  Windows Camoufox initially could not fetch its paired browser, then passed one
  isolated confirmation on that same commit. The failed matrix is not reported as
  wholly green. Cookie format normalization, persisted restart and public HTTP/CLI
  integration remain pending. Independent review found that a locally oversized
  replacement frame could be refused after clearing; complete add-frame preflight
  now precedes activation and clearing; hosted Windows/Linux regression passed
  on `1dfc484`. Native Netscape export for normalized rows now preserves the
  original header, order, flags, Unicode bytes and expiry truncation. An exact
  source-observation test compares exported bytes and malformed-row refusals;
  JSON storage-state export also preserves source indentation, ASCII escaping
  and format fallback, reusing the existing identity string encoder. Native
  Netscape parsing preserves tab/whitespace fallback, HttpOnly/comment handling,
  original digit rules and text line boundaries through source-observation cases.
  Finite JSON normalization now covers storage-state/extension records and ISO
  expiry using native calendar/timezone primitives. A deployment-selected runtime
  composes native launch policy with the fixed SDK transport; cookie import/export
  and launch handlers reuse it through the authenticated router. These additions
  are unverified pending hosted execution. HTTP import rejects empty usable jars
  before clearing, matching the original CLI boundary. Finite operations join the
  registry's shutdown/admission synchronization without becoming user launch
  sessions; unsuccessful cleanup remains retained for a later sweep. New original
  command-handler observations and synchronized shutdown tests cover those policies.
  Actual SDK tests also exercise persisted-cookie reopen, idempotent HTTP launch,
  busy refusal and stop, but their extended acceptance has not run yet.
  Complete non-finite/surrogate/legacy/DST compatibility, CLI service wiring,
  full listener and lifecycle acceptance remain pending.
  Native extension path resolution reuses existing home expansion and canonical
  Windows/UNC spelling, preserving directory filtering, order and duplicates.
  Isolated original flag comparison passed hosted Windows/Linux on `dee4c5b`.
  Launch composition now resolves the current admitted profile's extension entries;
  its original-derived SDK-frame regression and related native strict lint passed
  hosted Windows/Linux on `d8853d4`. Named-user home
  lookup, concurrent path replacement and full application service wiring remain pending.
  Independent review found Windows drive-relative entries could use ambient CWD;
  same-drive entries now anchor to the supplied CWD, while cross-drive entries
  without an explicit drive context refuse. Original-derived Windows comparison
  passed on `1dfc484`, with related native parity and strict lint on both hosts.
  Repeated Camoufox release-fetch failures are isolated to CI preparation; that
  step now uses a scoped read-only workflow token and verifies the actual binary
  before browser entry. Isolated Windows Camoufox preparation and real-context
  verification passed on `5ed531c`; the earlier seven-success/one-failure matrix
  remains reported as failed. Complete candidate acceptance is still pending.
  The HTTP stop route now reuses actual native registry ownership, preserves
  missing/foreign-session refusals and returns the original pre-stop record.
  Original HTTP observations plus an owned SDK-transport stop regression passed
  hosted Windows/Linux on `5a1a718`. Actual browser tests now stop through this
  authenticated HTTP route; all eight hosted real-browser legs passed on `c44fcbc`.
  Source observation now has a separate inert hello and finite fixture deadline;
  native parity and strict lint passed both hosts on `e106e2c`, which changes only
  that source-fixture test. Full launch/stop failure and concurrent lifecycle remain pending.

- Native clap command definitions and automation reflection are now prepared for
  the original command tree, including shared flags, numeric/default/choice rules
  and MCP schemas derived from that same registry. Profile create/list/show/edit,
  coherence/collision verdicts and archive service seams reuse native stores rather
  than forwarding Python application commands. New original-handler/parser tests
  are pending hosted execution; exact help/error/human output, all command handlers,
  detached jobs and executable MCP/server lifecycle remain incomplete.
  A native output owner now separates human narration from one structured document,
  using the existing ASCII JSON formatter and TTY-gated colors. Profile, settings,
  folder/vault and durable-job renderers have original-handler byte fixtures.
  Probe startup/result narration now runs through the actual native service, and
  warmup/run-due progress is emitted at its execution boundary rather than invented
  from final payloads. Token/schedule display reuses the existing local-time library.
  Public presentation now composes native pool, extension, cookie, fleet and mutation
  output without a generic JSON fallback. Launch narration uses the admitted profile;
  attachment publishes its single structured endpoint while ownership is still held.
  Omitted CDP browser titles retain the original human fallback without changing JSON.
  A native HTTP lifetime is drafted with secure bind checks, readiness, the original
  scheduler cadence and retained shutdown retries; its loopback/failure fixtures are
  prepared for hosted execution. Static dashboard discovery/fallback now has a native
  draft backed by the maintained file service and fourteen original-response cases,
  including missing assets, unknown API paths, HEAD policy and text charset fidelity.
  It is not a selectable replacement server: executable signals, detached supervision,
  full static/auth verification and acceptance remain open. The fallback is now mounted
  before the shared CORS/auth layers; four original-auth cases cover protected unknown
  API paths alongside public SPA/readiness. These checks are prepared, not executed.
  These fixtures are prepared, not compiled or executed; file-output rendering tests
  do not certify archive behavior, and interactive input deadlines, other command
  families and executable output/signal acceptance remain pending.
  Native settings, folder/token/schedule, extension, vault, pool and cookie CLI
  service seams now compose through one shared application dispatcher. Native
  DNS/TLS and combined proxy probes now use finite browser ownership: Rust
  selects the original scripts, endpoints, retry order, grading and command exit;
  Python performs only SDK navigation/evaluation. The combined probe reads
  IP/WebRTC/DNS/TLS in one context, preserving the original private-address
  heuristic and timezone aliases rather than substituting rotation policy.
  Initial IP evaluation failure stops later probes; raw SDK error text remains
  suppressed for credential safety, so exact exception-message parity is unfinished.
  HTTP proxy/TLS/DNS routes reuse those same services,
  preserving auth, missing/busy admission and unhealthy-result HTTP 200 behavior.
  Original grade/retry/HTTP observations and actual CLI/HTTP protocol fixtures
  are prepared, unexecuted. Invalid navigation deadlines
  refuse before SDK calls; tolerated SDK errors do not hide protocol serialization
  failures. Offline trust now selects its in-memory HTTPS page and original
  observation script natively, with backend-aware grading and distinct human/JSON
  exit rules. HTTP trust caches only grade/score/time onto the current profile.
  Alignment observes the original font candidates and patches only its owned
  fingerprint fields onto a fresh transaction-time profile after browser cleanup;
  unrelated edits are retained. Original grades and concurrent-edit/frame fixtures
  are prepared, unexecuted. Real service/CORS, stalled-evaluation cleanup, exact
  legacy errors, complete alignment and full backend acceptance remain pending.
  Native CLI DevTools attachment publishes connection details through an explicit
  output owner before waiting for browser closure. Natural SDK exit finishes the
  existing owner without sending a second close request. HTTP attachment stores
  endpoint metadata beside its exact registry owner; admission, readiness and
  failure cleanup share one per-profile decision, preventing an older request from
  stopping a replacement attachment. Original HTTP refusals/success/idempotence,
  actual native loopback CDP/stop and controlled readiness-failure fixtures are
  prepared, unexecuted. Actual SDK attachment, concurrent shutdown, failed
  settlement and executable signal/output acceptance remain pending.
  Native warmup control now owns the original preset destinations, timing,
  navigation, mouse-action selection and same-site link scripts. Its duration
  and preset validator is shared with schedules. The SDK only passes selected
  mouse operations and navigation deadlines; finite sessions strip startup URLs
  without changing stored profiles. Original validation and virtual-time runs,
  native service/claim cleanup and fixed-SDK mouse/deadline fixtures are prepared,
  unexecuted. HTTP manual warmup now registers an owned native control worker;
  stop deadlines retain unfinished workers/adapters for retry rather than losing
  their ownership. CLI run-due and a caller-driven server scheduler tick are
  drafted, sharing occurrence claims and native warmup. Scheduled admission,
  atomic outcome bookkeeping and failure cleanup hold one profile decision;
  unsettled startup retains the occurrence claim. Controlled worker/claim,
  quarantine retry, HTTP refusal/start/stop and bookkeeping-failure fixtures are
  prepared, unexecuted. Full failure injection, automatic server lifetime,
  cancellation/progress, wider branch coverage and actual backend acceptance
  remain incomplete. Pool
  standard-input import shares the same service through an explicit input owner;
  unrelated commands never consume stdin. Shared
  CLI store opening resolves explicit/inherited unlock and supplies both keys
  for interrupted-rotation resume before exposing services; its generated
  recovery-fixture regression is prepared, not executed. Bulk
  generation/apply and vendor imports reuse native identity and atomic dual-store
  services; fresh proxy-password choices survive storage reads. Original-handler
  observations cover structured payloads, refusal order and persisted twins.
  Durable job identity/outcome merging, bounded UTF-8 log reads, finished-job
  pruning and recorded-tree stopping are prepared with source and isolated OS
  fixtures. Unknown process observations remain owned; cleanup refuses a log
  belonging to another job. Detached spawning, legacy Windows tree stopping and
  full failure/recovery acceptance remain unfinished. MCP dispatch also reaches
  the shared native application and reuses the existing JSON formatter; non-zero
  command verdicts remain structured results rather than transport errors. This is still library
  code, not proof of a running MCP server or a completed CLI. A bounded sequential
  stdio loop now reuses that dispatch and checks frame size before buffering;
  tool subprocess capture uses one nonblocking pipe owner with wall, idle,
  output and cleanup ceilings. Failed cleanup retains its tree owner. Inherited
  vault unlock stays in the environment, never public argv. Original protocol
  observations now traverse the loop; these additions have not run in hosted CI.
  POSIX escaped-writer fixtures check capture deadlines and separately stop their
  own escaped tree; they do not prove universal descendant containment. A complete
  native executable and detached-job supervisor are still required. The integrated native
  dependency batch selects clap 4.6.7, Chrono 0.4.45, serde_json 1.0.152 and ZIP 9.0.1;
  upstream exact transitive pins remain respected. Hosted compatibility is pending. No library
  registry entry is evidence that its command is runnable in the finished product.

All expected fixture outcomes are produced by the retained implementation,
using non-sensitive inputs, fixed source clocks for conversion and deterministic
crypto fixture IVs **only in the fixture generator**. Production encryption uses
OS randomness. The generated catalogs and fixture provenance are retained with
source tests, and CI checks their regeneration for drift.

## Coverage and unfinished work

[The source capability ledger](migration-coverage.json) inventories the frozen
original source and keeps its required symbols pending until their complete target
and test evidence is reconciled. It intentionally refuses release/default switch.
The inventory counts symbols, not user features or completed test cases.

Remaining application work includes complete dual-store operations and vault
recovery; complete native browser/session/helper/job ownership; all browser adapters
and operations; full CLI/HTTP/MCP surfaces; portable/runtime packaging; real
Windows/Linux four-engine acceptance; representative replay, benchmarks with no
measurable regression, and exercised rollback. Do not hide these obligations by
removing source tests or declaring a Python-forwarding facade a Rust engine.

All original dashboard source and test modules are now TypeScript/TSX, with
strict compilation and typed, checked API boundaries. Independent wire-contract
regressions and a narrow demo editor render have passed; complete live interface,
viewport and final CI acceptance remain pending. TypeScript6.0.3 is retained
because the current ESLint parser supports versions below6.1, not TypeScript7.
Existing React, theme tokens, handwritten CSS and Tauri are retained.
The October 10 maintenance refresh selects Vite8.3.4, Lucide React1.55.0 and
Node types26.6.5 without changing the interface or compiler compatibility hold.
Dashboard tests, strict types, lint, build and dependency checks share one manually
dispatchable hosted workflow, also called by full CI. A dashboard-only checkpoint
does not certify native product acceptance or self-hosted runner job compatibility.

## Hosted-only distribution update — October 14, 2026

All further application tests, checks and builds run on GitHub-hosted runners.
Earlier local checkpoints below remain historical and cannot verify the newly
edited tree. Full CI discovers the maintained source/native/dashboard tiers,
runs source and native SDK checks in the same eight Windows/Linux browser legs,
and gates Windows x64, Linux x64 and macOS ARM64 retained-Python distributions.
macOS source/package checks do not certify unsupported native process ownership.

The new detached MCP path shares the durable native supervisor instead of
creating an unnecessary transient command-capture tree. It preserves parser
versus application verdicts, cancellation ownership and idle supervisor reaping.
Actual handoff-cancellation and no-zombie regressions are prepared for hosted
execution; the current source has not been compiled or runtime-verified.

Only a successful exact-SHA trusted `main` CI run with every applicable required
check can publish a `build-<SHA>` prerelease. Archives include matching editable
source, full project/third-party notices and checksums; restricted Cloak binaries
are excluded. The new workflows and their downloads are not yet verified. This
distribution work changes neither default selection nor the remaining capability,
real-browser, replay, performance and rollback acceptance gates.

## Verification

### Local Windows checkpoint — October 13, 2026

After hosted jobs were refused before runner startup, the owner authorized local
verification. The initial profile bucket failed twelve tests in ten targets. All
observed failures were repaired: the final bucket passed 80 tests across 42 targets,
including one added pipe-framing regression. A separate discovery-driven pass passed
76 tests across the other 71 non-live native targets. Source-oracle regeneration and
inventory drift checks, strict all-target Clippy and formatting also passed.

Corrections preserve stored profile identity under Windows case-insensitive lookup,
source integer/float JSON distinctions, persisted-language readback, path identity and
exact record counts. Fixture sockets explicitly enter blocking mode before bounded
reads. The SDK adapter now uses one main-thread input owner rather than leaving a
blocked daemon reader during Python shutdown; the reproduced Windows exit crash and
packaged-option failures passed their regression checks without weakening assertions.

After entry integration and review corrections, the discovery-driven final Windows
batch passed 160 tests across the library and 115 non-live integration targets, with
zero failed or ignored cases. Strict all-target Clippy and formatting passed on that
tree. These results describe uncommitted changes, not the unchanged published candidate. Tests used reduced debug information with
assertions unchanged; documented cold compilation idle timeouts were build failures,
not test verdicts. Every bounded command owner reported its tree empty.

An acceptance-only `native_candidate` example now composes the real Rust entrypoint.
Fifteen JSON CLI scenarios, three human output scenarios and MCP ping/tool execution
matched the original runtime on Windows/Linux using isolated stores. Twenty-one actual
HTTP read/authentication/error and documentation cases also matched status, bodies
and selected headers on both platforms; listeners and process trees settled.
Documentation assets are generated from the pinned original application, carry the
FastAPI MIT notice and are served natively without application forwarding. A Router
regression also compares allowed methods and content lengths for twelve documentation
requests. Swagger loaded the schema in a browser preview and executed readiness with
status 200; ReDoc rendering remains unverified. Real invisible ConPTY/PTY
checks exercised vault enable/change/resume, Unicode password input without echo,
Ctrl-C cancellation and damaged-journal refusal before prompts. Recovery checks
decrypted every source-fixture secret and preserved unrelated config; Linux checks
also inspected terminal echo restoration after success and cancellation. The example is not installed or selected
as the product. Deployment discovery reaches ordinary browser opens; SDK metadata and
redirected UTF-8 input/cancellation have narrow Windows checks.

Linux detached execution now records trusted nested native/SDK spawn transfers before
activation, gates the command until its durable identity is published, and retains
incomplete receipts rather than declaring them empty. Actual detached list/MCP/server
start, poll, stop and prune checks passed, including a separately-owned SDK protocol
stand-in after normal stop and a hard-killed leader. MCP signal cancellation also
passed with stdin kept open and a nonresponsive SDK stand-in; the parent settles its
nested receipts rather than only terminating the command group. These checks preserve
the original trusted-spawn contract, not universal containment of hostile arbitrary
process-group changes. They do not substitute for real four-engine acceptance.

Windows detached startup explicitly refuses under a non-breakaway outer job. That
refusal is verified; successful cooperative handoff, non-password console/signal
handling and the complete parent-death matrix remain pending. Native server parent
identity disappearance has scoped Windows/Linux evidence; the original Windows
PID-only path is inapplicable because the desktop shell owns it through a job object. Kill-on-close ownership is not disabled.

The real four-engine matrix, complete native CLI/MCP/server packaging,
replay, performance and rollback remain required. Python stays selected; these results
authorize neither cutover nor retirement. Earlier **prepared/unexecuted** descriptions
in the seam inventory are historical and superseded only by actual scoped evidence.

### Earlier hosted evidence

The owner-requested October 8 checkpoint uses manually dispatched GitHub-hosted
Windows/Linux jobs, without local or self-hosted test execution. It verifies the
current incomplete implementation, not final candidate acceptance or retirement.
The standalone profile-read router now includes engine-only adoption, view-only
repairs and uncertain-claim status. Its expanded regression passed the hosted checkpoint. Application composition
now includes profile read/create/update routes with an explicit caller-supplied
session host and backend availability. Create/update reuse the shared field
policy and atomic dashboard/engine/config writes; updates merge raw stored state
inside the transaction and derive credential intent only from the request.
The initial 14 source-generated write outcomes and 612 field-refusal cases passed a
narrow hosted Linux check. Expanded writes check accepted Unicode/underscore
screen dimensions against both stored copies; vendor import regressions check
that invalid proxy fields are refused rather than discarded. Health and collision overview handlers reuse native
conversion, identity validation and lease/claim status; 26 source-generated
read-only outcomes passed hosted Linux verification. Batch creation and bulk
patch handlers use the same validators and all-or-nothing store transactions;
21 independent source outcomes passed hosted Linux verification. Native-format
HTTP imports and seeded vendor conversion have source-generated cases awaiting
verification; randomized vendor HTTP acceptance and full legacy coercion remain
pending. No full application acceptance is implied. Typed storage
refusals distinguish name conflict, stale revision and unconvertible records
without parsing error-message prefixes.

CI discovers all native parity test targets through Cargo on Windows and Linux,
runs formatting/strict Clippy, regenerates source oracles, checks original runtime
selection, and includes the strict frontend compiler gate. Current fixtures cover
model/proxy35, seeded generation112, identity135, write-validation1159,
conversion29, cipher17 and journal23 cases plus a native storage primitive test.
These synthetic/pure-seam cases do not replace real browser/data/recovery tests.

No default switch occurs before every required source capability, actual platform,
fixture-drift, security, replay, performance and rollback gate passes. The
explicitly selected non-default candidate is checked first; only afterward does
the default selector change get its affected selector/rollback check. Retain the
old runtime through those checks. The owner has approved removing obsolete product
versions and disposable temp/caches **only after** all acceptance and selector/rollback
checks pass, leaving one current usable product. That condition has not been met;
no original runtime has been deleted. Current SDK-only adapters, user assets,
required fixtures, LICENSE and Git history are not obsolete product copies.

## Migration tooling diagnostics

Tools under `scripts/migration/` write a new UTF-8 run log under `scripts/logs/`
relative to the repository, not the caller's CWD. Names use
`<tool>_YYYY-MM-DD_HH-mm-ss_UTC[.n].log`; entries use
`[timestamp UTC] [LEVEL] [COMPONENT] Message`. INFO records lifecycle/verdicts,
WARNING/ERROR report failures, and handler-level redaction protects credential-shaped
values. Logging initialization failure falls back explicitly to stderr; handlers
close on every path. Logs are local diagnostics, not bundled or committed; remove
old run logs deliberately after diagnosis, and sanitize any approved support export.
The original engine's logging policy remains unchanged.

Algorithm redistribution notices are in
[migration-random-notices.txt](migration-random-notices.txt). The maintainer-selected
[Noncommercial Network Share-Alike License 1.0](../LICENSE) now applies to authorized
project material; independent dependency licenses and earlier valid grants remain
in force. See [licensing scope and notices](licensing.md).
