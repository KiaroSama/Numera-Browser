# Rust and TypeScript migration

The accepted boundary is [ADR0001](adr/0001-parity-gated-rust-migration.md).
The existing Python application remains the default and rollback implementation.
`engine-rust` is currently a native domain/persistence library, **not a replacement
CLI/server**. A passing library test does not certify a finished product migration.

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
  assignment and live proxy operations remain pending.
- Extension manifest/localized labels, contained icon selection, ID diagnostics,
  CRX2/CRX3 extraction and staged install/reuse/replacement/removal primitives.
  Failed replacement keeps the prior installation; installed-list HTTP includes
  known store and manually configured resources through the same native description.
  Package downloading is supplied at a narrow adapter seam. Live download,
  icon-serving, hostile-resource budgets, remaining
  compatibility contracts and real browser activation are pending.
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
  resource preparation, IP resolution, relay and launch-service integration remain pending.
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
  and format fallback, reusing the existing identity string encoder. Complete
  parser/normalization and public service wiring remain pending.
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

## Verification

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
[migration-random-notices.txt](migration-random-notices.txt); the project's LICENSE
has not been changed.
