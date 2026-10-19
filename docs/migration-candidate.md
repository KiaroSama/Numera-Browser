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

## Sequential replay and source rollback

The existing native entry acceptance step also runs a sequential original/native
scenario once on Windows and Linux, not once per browser. It uses the immutable
original Python CLI and the current native executable over one fresh synthetic
store: source creation, native edits and vault enable, source decryption and
edits, both backup/restore directions, then source key rotation and native writes.
The original runtime must reopen the final state. Complete profile comparisons
permit only `updated_at` to change; identity, creation time, fingerprint and the
synthetic proxy password must survive. Locked exports must refuse without
changing protected store/session bytes. Both stored credential copies must remain
encrypted; unknown fields and exact synthetic session bytes must survive restore.

This scenario reuses the entry harness's per-command 15-second process ownership
and existing 180-second wall/90-second idle owner. Only fixed operation/verdict
metadata is logged. Hosted batch `39082712186` passed the complete sequence on
Windows and Linux, with both outer owners reporting exit 0 and clean trees.
A small binary session marker is not a real browser database, a crash-recovery
drill or complete migration rollback acceptance.

## Representative release performance

The existing native job now prepares an optimized, current Cargo artifact for
four foreground CLI scenarios: seeded creation, a 32-profile list, an unlocked
vault read and backup. One warm-up pair precedes seven alternating measured
pairs. Identical synthetic input stores are copied outside the measured interval;
returned profiles, both stored copies and backup contents must agree too.

Each fresh observer records elapsed monotonic nanoseconds and an OS lifetime
memory peak. Linux reports its sole child's maximum resident set; Windows uses
a job assigned at process creation and reports peak committed bytes. These are
different metrics and are compared only between runtimes on the same host, never
aggregated across operating systems. All individual numeric samples, source/build
identity and the binary hash are retained in the hosted performance artifact.

Disjoint lower native ranges establish no regression for a measured scenario;
disjoint higher ranges report regression. Overlapping ranges are inconclusive,
not accepted under an arbitrary percentage allowance. Failed or inconclusive
measurement blocks the native job. The first hosted run built both optimized
binaries but refused creation because its snapshot mistook two new empty record
locks for unrelated data changes. The correction allows only those exact new
empty locks; existing locks and all other protected bytes remain checked.

An encrypted read initially rewrote protected bytes. Ordinary read reconciliation
now checks for actual source-equivalent disagreement before materializing a
record; explicit launch coherence and credential-conflict refusal are unchanged.
Both Windows and Linux proved that the same 26-case original-source comparison
fails on the historical implementation, then passes after fixed-byte restoration.
It checks results, records and each file's own byte-preservation verdict.

Windows and Linux completed all four scenarios with seven measured pairs each:
both time and memory ranges showed no regression within each host. The retained
artifacts bind the samples to source `348bac2`, hosted generation `39106678127`
and each optimized binary's hash. This is foreground CLI coverage only, not
API/browser or complete migration performance acceptance.

Creation explicitly sets the default engine, so its settings write is checked
against the complete expected JSON document separately from unrelated byte
preservation. The original Windows text writer uses CRLF while the native writer
uses LF; neither format is silently applied to unrelated files. Persistent controls
reject lost, extra, mistyped or unexpected settings. The replacement Windows run
passed these checks through earlier creation samples, then refused the final
native creation observation at its single immediate tree-empty query. Leader exit
and drained output do not prove the job's descendants or accounting are settled.

The observer now uses existing observation-only tree polling within the remaining
original 15-second deadline. Expiry and persistent active/unknown state still fail;
successful natural settlement is included in elapsed time before peak accounting.
Emergency cleanup cannot manufacture an accepted sample. The job/process handles,
peak metric and comparison thresholds are unchanged. Deterministic controls cover
transient occupation, persistent active/unknown state and expired/late observations,
including a required assertion failure on exact historical observer bytes. These
controls and both complete benchmark tiers passed in the replacement hosted run;
the physical cause of the earlier occupied or unknown job observation remains
unestablished. No Windows API repair is claimed.

## Integer seed compatibility

The verified integer-seed correction preserves complete signed seed values during
pure generation, model serialization, dashboard numeric conversion and init-script
configuration. MT19937 initialization consumes every 32-bit magnitude word rather
than truncating at `i64`; no new random generator or dependency is introduced.

This does not widen write admission. The original single-profile create refuses
seeds outside `0..2^53` before persistence, while bulk creation uses its input seed
only to choose bounded per-profile seeds. The differential test passed
26 command outcomes and 14 complete identities, including a pure seed longer than
624 words, against the frozen original. It also checks eight expected create
refusals, both stored copies, settings preservation and collision output. Hosted
batch `39114556651` proved the historical bulk admission assertion failure,
restored fixed source bytes and passed the current test on Windows (0.74 seconds)
and Linux (0.47 seconds). Other model/coercion domains remain incomplete.

## Occurrence-level reconciliation

The capability ledger now records 161 individually inspected implementation/test
references with explicit remaining gaps: 906 rows are pending, one is implemented
but not verified, and none is verified. All 907 frozen occurrence IDs, source
hashes and the source commit are retained. A file reference or a passing broad job
cannot establish complete parity or authorize default selection.

This review found two missing pure helpers. The native collision module now
provides the original four-field summary, counting profile occurrences rather
than distinct IDs. Vault policy now distinguishes supported version prefixes from
successful token authentication. General envelope recognition uses the existing
pinned Unicode decimal catalog instead of ASCII-only digits, preserving opaque
future envelopes rather than treating them as plaintext. Decryption, KDF rounds
and error policy are unchanged.

Ten original-derived collision cases and 832 string-envelope observations passed
on hosted Windows and Linux in batch `39125230921`. They cover stable grouping
ties, repeated occurrences, Unicode decimal ranges and neighboring characters,
malformed prefixes and locked-envelope preservation. Both systems first observed
the exact historical ASCII-only implementation fail the named Unicode assertion,
then passed the current tests after fixed-byte restoration. Dynamic/nonstring model
inputs, numeric collision equality and broader vault compatibility remain incomplete.

A subsequent core-validation review found that packed screen dimensions still used
Rust's direct integer parser, although HTTP validation and conversion already used
the compatible decimal-text helper. The core branch now reuses that helper without
changing bounds, diagnostic wording or the shared parser. A 46-input frozen-original
comparison covers core and HTTP verdicts for Unicode/underscore/sign grammar,
boundaries and invalid values. Hosted batch `39144820589` proved the exact
historical assertion failure, fixed-byte restoration and current test success on
both systems. This closes the earlier missing Windows fixed-pass evidence;
broader screen/model compatibility remains incomplete.

The next folder correction preserves Python boundary trimming for U+001C–U+001F
in native create, rename, delete and reassignment. One original-source observer
compares 96 operation cases, returned lists/refusals, parsed metadata/profile state
and each file's byte-preservation verdict. Exact historical failure and current
acceptance passed on both systems in batch `39144820589`; locks, rollback and
serialization are unchanged.

Folder metadata and profile readers now reject invalid UTF-8 before parsing JSON,
rather than treating undecodable bytes as empty metadata or a skippable profile.
Forty original-derived operation cases compare admission, resulting records and
exact input-byte preservation on refusal. Valid UTF-8 malformed-JSON fallback is
unchanged. Historical failure and current verification passed on both systems
in batch `39144820589`.

Nonempty settings mutations now also reject invalid UTF-8 before JSON fallback,
rather than overwriting the damaged file with a fresh object. Eighteen
original-derived cases and the exact historical failure/restoration proof passed
on Windows and Linux in batch `39148402244`. Read/cache/direct-setter behavior
is unchanged and its known differences remain open.

The shared extension-path validator now also preserves Python boundary trimming,
including U+001C–U+001F. Seventy-two original-derived validation cases passed on
Windows and Linux in batch `39151673690`, after exact historical assertion failure
and fixed-byte restoration. Home expansion, dynamic conversion and other parsers
are unchanged.

Extension manifest and localized-name reads now propagate invalid UTF-8 through
the existing `describe`/`installed` results, rather than silently returning fallback
data. Ordinary read errors and valid UTF-8 JSON syntax fallback remain unchanged.
Twenty-four original-derived cases passed on Windows and Linux in batch
`39154868069`, including exact historical admission failure and fixed-byte
restoration. They compare complete results, refusal categories and fixture bytes.
The test rebuilds its entire input path from native components, avoiding mixed
Windows separators without normalizing returned data or weakening comparison.
Containment is unchanged.

Icon ranking now uses normalized signed decimal magnitudes instead of `i128`.
This preserves large values, Unicode decimal digits, underscores, stable numeric
ties and the frozen interpreter's default 4,300-digit limit without a new numeric
dependency. One hundred thirty-six original-derived public-result observations
passed on both systems in batch `39158418936`, after the exact historical ranking
assertion failed and fixed bytes were restored. Custom interpreter limits and
broader manifest/path domains remain incomplete.

First-run summaries now include the original `source` and `destination` fields.
Eight immutable-original observations passed on Windows and Linux in batch
`39162383046`, comparing complete direct/automatic import results, repeat decisions,
binary copy bytes, destination-wins behavior and existing marker preservation in
Unicode/spaced paths. Existing four path-independent fixtures are unchanged.
Both systems first proved the exact historical complete-summary assertion failure,
then restored fixed bytes before current acceptance.

The additional settings/folder references document existing implementations and
known gaps, including configuration caching, direct setter contracts, malformed
values, contention and rollback. All 26 settings/folder rows remain pending.
Sixteen additional extension references also remain pending: private/inlined
helpers are nearest equivalents, not new public APIs or complete parity. Thirty-six
path/archive/first-run references bring the mapped total to 125; all 36 remain
pending for concrete input, serialization, concurrency, recovery and error gaps.
The data-root fixture pre-expands its tilde case, so it does not prove home
expansion. Native containment, transport bounds, private errors and staged rollback
are retained; missing acceptance remains explicit in the ledger. Twenty-four
additional data-lock references bring the total to 149 and remain pending: explicit
native acquisition budgets are not the original per-acquisition timeout policy,
thread-bound guards do not supply a public ownership predicate, and registry lease
checks are not the original standalone cached-lease probe.

The shared atomic JSON writer now preserves the original ASCII Unicode escapes
and Windows structural CRLF, reusing the existing UTF-16 escaping helper. Twelve
original-derived byte comparisons passed on Windows and Linux in batch `39169080131`,
covering ASCII/control/BMP/astral strings, Unicode keys, nesting, order and empty
documents. Both systems first proved the exact historical byte assertion failure,
then restored fixed bytes before current acceptance. Serialization still precedes
publication; exclusive creation, sync, rename/retry and exact byte rollback are unchanged.
General numeric/nonfinite/surrogate domains and direct export/ZIP formatting remain
separate incomplete contracts.

The earlier cross-runtime session-path scenario uses `DataLock`. A separate direct
native `SessionLease` acquire/drop scenario passed on Windows and Linux in batch
`39204028426`: both contention directions, explicit refusal categories, continued
exclusion after a refused contender and successful reacquisition after release.
The existing data-lock scenario also passed. This adds lifecycle acceptance, not a
production repair; crash, duplicate-handle and full fault domains remain incomplete.

Nine cookie-format/SDK references and three cookie CLI references bring the mapped
total to 161; all twelve remain pending. Dynamic values, numeric spelling, malformed
formats, SDK failures, contention and broader browser acceptance remain incomplete.
The cookie CLI file writer now translates every LF on Windows before unchanged atomic
publication, matching the original text-mode output. Returned format strings and HTTP
output are unchanged. Existing CLI scenarios now compare actual original file bytes,
including Unicode and embedded controls, rather than normalizing CRLF away. Batch
`39210495203` proved the historical-byte failure and restoration on Windows, then
passed the current cookie cases on both systems. Linux has no invented historical failure.

Shared profile-twin reads now validate UTF-8 before JSON fallback and propagate an
`InvalidData` refusal instead of replacing undecodable state as if absent. Existing
read-I/O and valid-UTF-8 malformed-JSON fallback remain unchanged. A single frozen-source
observer compares 48 damaged-text read/repair/write cases and four syntax-fallback
controls through public operations, including exact preserved bytes. The exact four-module
historical proof and fixed-current acceptance are prepared but not yet executed.
Standalone dashboard readers and broader dynamic/error domains remain outside this repair.

Historical source swaps invalidated the earlier fixed prebuild, causing duplicate
compilation inside the test deadline. The build phase now fetches locked inputs,
compiles the first exact historical target, proves every named historical failure
with fixed-byte restoration, then compiles all current targets without running them.
Cold compilation stays inside the existing 900-second wall/300-second idle build
owner; each historical assertion keeps its 180-second wall/90-second idle owner.
The complete current test phase retains all discovered targets, two workers and
600-second wall/120-second idle limits. Cargo progress is projected without raw
build-script/environment/path records. No timeout is increased, proof skipped or
heartbeat added. Batch `39158418936` passed phase-order/failure-cleanup controls
and all fourteen historical proofs on both systems. Build/proof and current-test
owners took 180.168/177.586 seconds on Linux and 382.926/208.296 seconds on Windows,
respectively, with clean trees. These observations are not repeated comparable
speedup measurements or proof of a completely cold cache.

## Acceptance status

Source `5c5d7b8` ran in hosted batch
[39210495203](https://github.com/KiaroSama/Numera-Browser/actions/runs/39210495203).
All source platforms, eight relocated Windows/Linux cases and current native suites
passed; strict lint and 16 Linux/17 Windows historical failure/restoration proofs passed.
Windows native acceptance passed. Linux creation latency was inconclusive: native
8.378–174.537 ms overlapped original 164.734–172.131 ms across seven alternating pairs.
Other CLI time ranges and all peak-memory ranges passed their unchanged criterion;
this is neither a proven regression nor performance acceptance. macOS passed 2,834
with 13 explicit limits, not proof of its earlier reader snapshot's cause. CodeQL's
same 20 annotations and required verification failed; distribution/promotion/publication skipped.
Windows Camoufox now passes actual CLI/MCP cookie import/list/export and HTTP/auth/dashboard/browser restart,
idempotence and stop, with 24 complete, clean ownership units. Its earlier
`CamoufoxNotInstalled` context-open failure is resolved by preserving the effective
pre-isolation cache root. Both native tiers previously proved the historical
metadata bytecode failure, restored fixed bytes and passed the current tests.
CodeQL and required verification still failed; the integrated batch is not green.

See [migration status](migration.md). CodeQL, the 907-row capability ledger, complete
application coverage, representative replay, repeated comparative time/peak-memory
measurements, exercised rollback, native updater/signature acceptance and release
gates remain open. Python selection and its oracle fixtures are unchanged.
