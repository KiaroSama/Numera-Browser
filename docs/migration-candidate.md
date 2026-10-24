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

The prepared expansion retains that sequence and adds settings writes/reopen and
all-or-nothing invalid-extension refusal; folder rename chains, retired-name
recreation and reassignment; and proxy-pool import/deduplication, metadata edits,
archive/restore, locked refusal, export, credential clearing, encrypted reimport and removal.
The frozen CLI shares the nonempty pool `--pass` destination with the master key;
that ambiguous edit must refuse without changing bytes, not pretend to replace a
credential. Removal/reimport exercises the supported keyed path instead.
It uses the actual returned pool IDs on the same store, not normalized random IDs.
Both runtimes must agree on complete read/refusal payloads, exact protected bytes,
unknown fields and encrypted-at-rest secrets across vault enable and key rotation.
The existing per-command and outer budgets are unchanged. Batch
[39327015878](https://github.com/KiaroSama/Numera-Browser/actions/runs/39327015878)
passed this expanded sequence on Windows and Linux. It does not establish complete
crash/selector/browser-database rollback or acceptance of every source occurrence.

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

The capability ledger now records 460 individually inspected implementation/test
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
controls through public operations, including exact preserved bytes. Batch `39217826831`
proved the exact four-module historical failure, restored fixed bytes and passed all
52 current cases on Windows and Linux.
Standalone dashboard `get`, `list` and `get_meta` now have a separate strict-text
repair. They retain their individual missing-file, I/O and valid-UTF-8 JSON-syntax
policies, ordering and decryption. Eighteen frozen-source observations cover invalid
text, valid Unicode, malformed JSON and missing files, with healthy listing neighbors
and exact byte preservation. Batch `39223897493` proved the historical refusal and
fixed-current success on both systems; broader dynamic/error-domain acceptance is incomplete.

Thirty-nine repository and 23 dashboard-store references brought the mapped total to
223; sixteen converter references now bring it to 239, all newly mapped rows pending.
References identify the nearest implementation
and test, not complete acceptance: full dynamic and failure-domain coverage remains pending.
Optional engine-free dashboard construction, legacy mirror behavior, dynamic records,
faults and concurrency remain explicit gaps. ID-validated path joins do not establish
physical or symlink containment.

Public native `ProfileRepo::divergence` now reports the complete ordered field list,
including `proxy.password` before `proxy`, without repairing either stored copy.
It reuses credential-aware and Python-compatible comparisons, excludes identity,
timestamps and viewport-only differences, and retains root/recovery/text guards.
One frozen-original observer compares 47 cases covering full lists, numeric and Unicode
ordering, proxy normalization, encrypted states, malformed text/JSON and missing records,
with exact input-byte preservation. Batch `39236792303` passed them on Windows and Linux;
the ordinary reader still stops after its first disagreement. No historical runtime
failure is invented for an API that did not previously exist.

Public native `ProfileRepo::edit` now resolves and invokes its callback inside the root
transaction, saves either its returned replacement or mutated current profile, and
returns one saved profile. Missing targets do not invoke the callback; callback errors
propagate before its changes are published. Existing validation, name, credential,
recovery and atomic-write safeguards remain, as do current CLI/batch behavior.
A 28-case frozen-original observer compares callback-entry views/counts, complete results,
refusals and both stored copies/config, with exact preservation where the source writes
nothing. It includes ID/name/missing references, in-place/replacement callbacks, retained
dashboard metadata and locked/unlocked credential decisions. Batch `39246782529` passed
all 28 cases on Windows and Linux; arbitrary dynamic callbacks, concurrent races and
complete fault coverage remain open.

Converter text boundaries now match the original Unicode line splitting and trimming.
String fields split at the original separators while array items remain whole; URL comma
handling and numeric conversion are unchanged. One frozen-original observer compares 69
complete profiles and dashboard roundtrips, including all source whitespace boundaries,
nonseparator controls and nontext inputs. Batch `39253919503` proved the exact historical
failure, restored fixed bytes and passed all 69 cases on Windows and Linux.

Reverse proxy conversion now trims complete server boundaries, matches explicit schemes
case-insensitively and recognizes `socks`/`socks5h` as SOCKS5. Empty original servers return
null, while nonempty whitespace-only servers retain the source's empty-host object.
Host/port spelling, bracketed/bare IPv6, unknown-scheme HTTP fallback and credentials are
preserved without network URL canonicalization. A 112-case frozen-original public
`to_dashboard` observer compares complete results and unchanged input state. Batch
`39261214144` proved the exact `e04e4fc` historical failure, restored fixed bytes and
passed all 112 cases on Windows and Linux.

Integer conversion now separates numeric truncation from integer-text parsing: decimal
and exponent strings fall back to the field default, while signed Unicode decimal digits,
single digit underscores and permitted boundary whitespace are accepted. Seed magnitude
is retained without a float roundtrip or saturating cast; integer text keeps the original
4,300-digit limit. An 80-case frozen-original public conversion observer compares full
profiles, dashboard roundtrips and input preservation. Batch `39267690547` proved the
exact `67f6ac3` historical failure, restored fixed bytes and passed all 80 cases on both
systems. The sixteen pending converter references were consumed in the same batch.
Counts beyond the typed model's range remain incomplete, not arbitrary-model parity.

Fractional memory and pixel-ratio text now reuse the existing Unicode decimal/underscore
parser. Finite whole memory values retain their full integer value instead of saturating
at i64; fractional values, zero/default and iOS behavior remain distinct. An 82-case
frozen-original observer compares full profiles and dashboard roundtrips, exact integer
memory and finite float bits, with unchanged inputs. Batch `39272776764` proved the exact
`ba29315` historical failure, restored fixed bytes and passed all 82 cases on Windows and
Linux. Models, validation and the shared parser itself are unchanged.

Public `conversion::dashboard_coherence` now composes tolerant conversion and identity
validation in the native core. Recoverable errors become a single validation issue; the
existing HTTP route reuses the helper without changing authentication or body admission.
A 30-case frozen-original observer compares complete ordered issues, HTTP responses and
input preservation. Batch `39278753551` passed all 30 cases on Windows and Linux. No
historical runtime failure is invented for an API that did not previously exist; all
four older converter proofs passed with their exact historical HTTP-route companions.

Profile-list activity labels now accept boolean `true` as timestamp one, preserve false
values as `never`, and use the original sequential divisions and decimal integer labels.
Malformed truthy values still refuse through the existing HTTP error path. The
24-case frozen-original HTTP comparison checks complete responses and exact stored-byte
preservation, including future, fractional and wide timestamps. Each original observation
uses both actual native request clock endpoints and must agree before comparison; there
are no sleeps, retries or label normalization. Batch `39297993608` proved the exact
`199eb08` historical status failure, restored fixed bytes and passed on Windows/Linux.
No new core API, route, dependency or timeout is introduced.

Native settings now expose direct selected-value, engine, headless and string-list extension
setters. Extension validation checks only new entries inside the root transaction. Whole-map
replacement requires ownership of that root by the current thread; another thread or root
does not authorize it. Existing CLI/HTTP validation is unchanged. A 71-case frozen
source comparison covers return values, refusal categories, stored state, unchanged bytes
and six ownership states; batch `39312135957` passed it on both systems. Settings caching,
arbitrary dynamic input and complete fault/concurrency coverage remain open. The earlier
malformed-settings proof uses its exact pre-addition test companion to reach the original assertion.

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

## Pooled proxy compatibility batch

Pool text handling preserves Python whitespace and line boundaries. Port
normalization reuses the existing Unicode decimal/underscore parser, retains the
4,300-digit limit, and compares canonical magnitude before the unchanged 1–65535
range check. Stored pool text is decoded strictly before the existing JSON-syntax
fallback, so malformed encoding refuses instead of replacing the file. Direct
empty updates retain the matched-row result/save; an empty batch still returns
before reading. Lock, atomic-write, secret and network-URL policies are unchanged.

Source observers add 50 full parse/render/batch cases, 47 normalization cases with
matching single-entry batch verdicts, and 100 stored-file operations. Three exact
historical source assertions failed as required, fixed bytes were restored, and
current cases passed on hosted Windows/Linux in batch `39336995688`. The 23 new
nearest references raised the ledger to 262; four direct profile-storage references
brought it to 266. Another 54 vault-domain references bring it to 320; all new rows remain pending.
Dynamic values, network, fault and concurrency domains remain incomplete.

## Configured data-lock deadlines

The source centralizes `NUMERA_LOCK_TIMEOUT` beside `DataLock` and
routes 73 ordinary acquisitions across 25 modules through it, including updater
admission and direct settings. Each acquisition reads the policy separately;
root/file order, reentrancy and the shared local/OS deadline are unchanged.
Invalid, negative, nonfinite or platform-oversized values retain the original
30-second fallback. Explicit lifecycle budgets and immediate session-lease refusal
remain separate. Timeout messages cross settings/repository wrappers unchanged;
other I/O diagnostics keep their context.

Prepared acceptance compares 20 original timeout-policy values, seven public
store/admission operations, and actual source/native CLI root/file contention.
Only a completed refusal received while the acknowledged frozen owner still holds
the lock counts; setup errors, capture timeouts and late results do not. Protected
bytes must remain identical, and the released operations must succeed. The exact
old pool caller must fail the named runtime assertion while current lock APIs stay
available. Hosted batch `39348825678` passed the 20 policy values, seven direct
operations and required historical assertion/restoration on both platforms. Its first
CLI scenario failed because frozen read-only settings do not take the root lock;
that scenario now uses pool listing. Corrected four-case CLI acceptance passed both
platforms in batch `39355362784`. Fork, poison, fault and complete cross-domain
concurrency acceptance remain open.

## Direct profile storage operations

Prepared APIs expose name-only lookup separately from ID-first resolution, validated
session-directory creation, and standalone session deletion. Deletion takes the immediate
session lease, refuses occupied or unverifiable claims, and detaches before reclaiming
outside the lease. Both profile copies and claim/configuration bytes remain untouched.
Name lookup reuses reconciliation rather than a second implementation. Combined profile
deletion shares the guarded detach operation.

Prepared acceptance compares 14 original name-lookup cases and 19 original path/session
cases, plus actual original-held lease refusal and post-release deletion. Existing
historical lookup proofs retain pinned test companions so a later API's absence cannot
masquerade as a runtime regression. All three new tests passed on Windows and Linux in
batch `39355362784`; full dynamic, fault and concurrency coverage remains open.

## Profile credential boundaries

Prepared public conflict guards share the existing credential decision used by repository
and dashboard writes. Non-preserve intents return before stored-copy reads. Equal or
empty credentials remain writable; readable conflicts refuse without choosing a copy,
while opaque locked pairs remain preserved. Prepared acceptance compares 101 independent
original public-guard observations, including non-text stored credentials, and exact unchanged input bytes.

The withheld-credential check now validates UTF-8 before JSON-syntax fallback, and profile
export reuses the existing atomic ASCII/platform-newline JSON serializer. Fourteen direct
withheld checks and thirteen export-byte cases preserve existing destinations on
missing/locked/wrong-key refusal and repeat successful exports. Stored integer timestamps
retain their exact value rather than passing through the typed floating-point read view.
Other dynamic model and numeric-spelling domains remain incomplete. Two pinned historical
archive controls must fail their named semantic assertions before restored current success.
Batch `39365718715` passed all 101 conflict, 14 withheld-text and 13 export-byte
observations on Windows and Linux. Both systems proved the named historical
archive failures and restored fixed bytes before current success. Pinned compatible
archive companions resolve the earlier old-lookup compilation failure without
weakening semantic-failure or fixed-byte restoration requirements.

## Failed-helper cancellation

The earlier macOS incomplete-report case recorded a 0.424-second collection against
its 0.300-second budget, leaving no cancellation/join time. The worker exited without
supervisor rescue; a permanent leak was not established. The first repair passed its
controls but cancelled complete-report readers before genuine EOF, breaking later
settlement and error classification. The replacement skips positive waiting only for
incomplete reports; complete reports retain the original bounded drain. Three real-pipe
historical budget proofs and two complete-report real-EOF controls cover that distinction.
Windows PID observation now retains process identity and uses only the remaining original
helper-plus-cleanup deadline, with four deterministic transient/persistent/expired controls.
Registry settlement must precede observation; termination cannot manufacture success.
Existing worker, terminal, late-EOF and read-error assertions/deadlines are unchanged.
Batch `39372271823` passed all source platforms, including the original failed terminal
cases, Windows driver observation, three worker variants and five cancellation controls.

## Public vault store lifecycle

Public `ProfileRepo` methods expose descriptor, enabled/unlocked state, enable,
password change and resume through existing root transactions and durable recovery.
Successful enable/rotation updates the same instance only after commit-and-prove;
refusals leave its password unchanged. Enabled distinguishes null from falsey values.
Four immutable-original observers cover 47 status/enable/rotation/resume cases and 15
unresolved-journal refusals across five public operations. They check all three secret
sites, authenticated readback, unknown metadata, exact refusal bytes, journals and binary
session preservation. All four tests passed on Windows and Linux in batch `39372271823`;
malformed dynamic descriptors, config caching, concurrency and complete fault coverage
remain open. Cryptography is unchanged.

## Crypto text admission

Prepared native decoding preserves the frozen interpreter's ASCII salt admission,
ignored nonalphabet characters, intermediate/excess padding and trailing bits. Version
selection counts colons; payload slicing counts Unicode characters, matching the source
rather than assuming a valid prefix. Permissively decoded tokens are canonicalized before
unchanged Fernet authentication; modified ciphertext must still fail verification.
An existing-target observer compares 145 original decrypt/password-check outcomes,
including exact refusal text, and reopens four new native tokens through the original.
Batch `39377575857` proved the pinned historical semantic failure, fixed-byte restoration
and current success on Windows and Linux. No KDF rounds, cipher, MAC, randomness, journals
or default changed; nonstring/backend/cache-fault and complete Unicode error domains remain
incomplete. The Windows test exceeded 60 seconds; prepared fixture ordering groups identical
raw password/salt keys while preserving every input, original output order and assertion.
Linux batch `39402856263` still reported over 60 seconds; no measured improvement or timeout/cache change is claimed.

## Recovery descriptor boundaries

Prepared journal selection retains truthy malformed `new` values for unreadable classification
instead of substituting legacy fields. Descriptor text uses Python-compatible boundary trimming;
strict Base64 rejects alphabet junk and bad padding but accepts unused trailing bits. Authentication
still separately proves the verifier and every stored secret. An existing-target immutable-original
observer compares 145 descriptor/selection/classification results and exact retained journal bytes.
Both platforms proved its pinned historical semantic failure and fixed-byte restoration in batch
`39411399239`; current acceptance then passed on both platforms. Recovery ownership and prior fixtures are unchanged.

## Authenticated recovery results

Prepared completion diagnostics count distinct full paths, deduplicating only displayed
basenames. A malformed container still refuses, but independently readable ciphertext
also contributes its wrong-generation cause. A committed rotation accepts extra descriptor
metadata only after usable salt/verifier identity, authenticated data, durable bytes and
persisted readback are proved; matching salt alone never suffices.

Recovery errors now retain an explicit kind and ordered code/detail problems alongside
existing codes, role and message. Unreadable active settings, including a directory at
`config.json`, retain the same structured cause and leave evidence untouched. Existing
observers add 36 completion cases, five config failures repeated twice, ten committed
rotation cases repeated twice, and 15 typed error projections. Exact historical completion,
config-cause and metadata assertions failed as required on both platforms in batch `39397136419`,
with fixed-byte restoration. Batch `39411399239` passed all new current cases on both platforms;
callback-induced readback, crash, complete dynamic-value and concurrency domains remain incomplete.

## Settings instance cache and historical build reuse

The prepared per-instance settings cache keys successful object reads by exact modification
time and file length. Returned maps are detached; stat failure clears the cache, while
read/parse/nonobject failure retains the prior successful entry. One original/native observer
covers 11 sequential steps, including same-key replacement, 100-ns timestamp changes,
size changes, deletion/recreation and independent instances, without sleeps or file mutation
by reads. Batch `39411399239` proved the exact pre-cache semantic failure, byte restoration
and current success on both platforms. Strict UTF-8 error propagation remains open.

Repository instances retain one settings reader and reuse it through
its dashboard/pool storage consumers. An existing-target observer retains 48 sequential
source/native observations and four warmed-cache late-journal refusals. Three main profile
observations explicitly retain a stronger native guard: when an absent descriptor makes the
original return ciphertext, native returns null. These are documented differences, not
full parity or skipped cases. Independent instances, precise invalidation and unchanged
input bytes remain required. Mutation snapshots still use fresh readers; no global cache
or measured application speedup is introduced. Batch `39419005226` passed these cases on both platforms.

The pinned cache action disables incremental compilation for cross-job caching. Historical
proofs repeatedly replace source in one build tree instead. Their runner now temporarily
enables incremental compilation, restoring the incoming setting even on failure before the
fixed-tree build. Every historical source hash, semantic assertion, byte restoration,
sequential ownership and existing deadline remains required. Batch `39411399239` completed
Windows build/proofs in 570.762 seconds versus the prior 900.052-second timeout, with one
additional proof. This resolves that run's timeout, not a repeated comparable speedup claim.

## Settings text and filename selection

Existing fallible settings APIs (`default_extensions` and an empty `set_many`) now
use a checked cached read. Invalid UTF-8 propagates without altering bytes or the prior
successful cache; ordinary I/O, JSON syntax and nonobject fallbacks remain unchanged.
Twenty sequential original/native observations include a same-key cache hit over invalid
bytes and cache recovery after a changed-key decoding refusal. The infallible `read`,
default-engine/headless getters and their remaining consumers still need strict-error
propagation; this bounded correction does not certify that domain.

Storage and vault enumeration now share a filename predicate for the original platform
`*.json` selection: literal `.json`, leading-dot names, and Windows mixed-case/Unicode
long-s suffixes. Existing sorting, file-kind, ID, link and error policies are retained.
Fifty original/native observations cover folder listing/rename, dashboard listing,
first-run profile counts and completion diagnostics with exact input-byte verdicts.
General glob faults, races, non-Unicode names and every repository/HTTP consumer are not
independently certified. Batch `39439030544` proved both historical semantic failures,
restored fixed bytes and passed the current observations on Windows and Linux.

## Prepared storage settings error propagation

Repository status, credential reads, reconciliation and dashboard/pool/archive consumers
now propagate typed settings decoding errors instead of treating invalid UTF-8 as an
absent vault. Existing fallible cached snapshots and fresh mutation snapshots are reused.
Source-specific short circuits remain: engine profile reads always inspect settings;
plain dashboard/pool reads do not, and withheld materialization retains stored credentials
without reading unrelated settings. Pool deletion decrypts before filtering its rows.
One observer in the existing repository target compares 189 immutable-original outcomes,
complete success values, persisted records and exact changed/unchanged input bytes.
A coherent seven-source historical group and compatible earlier companions retain named
semantic-failure proof. Batch `39460478283` exposed a late fixture-clock binding; replacement
`39470005037` proved historical failure/restoration and all 189 current cases on both systems.
The earlier Windows lock timeout also passed, without establishing its cause.
Core read/default-engine/headless APIs return typed errors; CLI/HTTP callers retain lazy
fallback and original error order. Batch `39486565418` passed all 33 core, 56 CLI and 42 HTTP
original-derived observations on both systems. Compatible exact historical callers retained
selected defects and proved the named CLI admission failure before fixed-byte restoration.

## Acceptance status

Latest source `4e570f1` failed batch [39637385281](https://github.com/KiaroSama/Numera-Browser/actions/runs/39637385281):
26 jobs passed; three failed, macOS was cancelled and three publication stages were skipped.
Linux completed 55 historical proofs/current acceptance; Windows completed 56 proofs in 739.967 seconds,
then separate fixed compilation reached the unchanged 300-second idle limit before current tests.
The exact prior-importer companion clears the old pool swap's compiler failure while preserving its defect.
Current contracts passed, including 106 importer, 270 shared write-contract and 123 identity comparisons,
ten complete indexed batch-refusal comparisons and exact protected-byte preservation.
Earlier 268 ASCII validation cases, 24 core/HTTP legacy views and 16 stored-name refusals remain
covered. Windows released-vault lock acceptance stopped at 5,032 ms against its unchanged 5,000 ms
idle limit, with a live leader after descriptor creation started; settlement was clean and no journal
or config descriptor was published. Current vault parity took 106.74 seconds on Windows and
84.17 seconds on Linux in `965ba02`. The next development optimization made repeated historical
application rebuilds too expensive before current tests ran. Historical commands now use application
opt-level0, retaining current opt-level1 crypto/assertions/overflow checks and release settings. Separate
required history/fixed-build commands each receive 900 wall/300 idle seconds under the same 45-minute
job ceiling. Aggregate command allowance increases; no measured speedup is claimed.
Linux's four CLI time/memory log verdicts report no regression; Windows performance did not run. Earlier `4297b1f` Linux list
native12.670–411.152ms versus original155.006–161.573ms and vault-read native67.177–327.772ms versus
original289.129–296.725ms remain recorded as inconclusive; no cause is established by a later passing run.
No sample is trimmed or criterion relaxed. The latest `1397538` batch failed only CodeQL and required verification;
native tiers and all eight SDK paths passed, but this does not complete migration acceptance.
Linux/Camoufox provisioning passed without a causal repair or deadline change. Fifteen deterministic cleanup-failure controls and the exact prior
runner's negative control passed on Windows (16 tests, 0.21 seconds). Operation/error-code/file-kind
diagnostics retain the original exception and failed exit, without retry or permission changes;
this does not establish the previous filesystem failure's cause.
The shared write-contract boundary distinguishes raw JSON from a typed Profile, infers fingerprint
key presence and retains explicit shapes, custom context and every ordered refusal; 270 observations
passed on both systems. Importer repairs reuse existing numeric grammar and retain the first valid
Unicode-decimal Chrome token and arbitrary-integer envelope guidance. Their 89 profile plus 17 envelope
comparisons and two exact historical controls passed on both systems. All 1,011 claim descriptions and
historical failure/restoration controls passed on both systems. All 54 strict job-text observations and
the exact historical failure/restoration passed on both systems. Both systems passed 38 importer
tag/display, four complete-refusal and 83 bulk-selection observations plus the released-vault lock test.
They passed 126 DNS cases, 40 complete-option fixtures, 99 proxy-echo transport cases and 103 timezone
proxy/trust grade pairs after historical failure/restoration. The 92 alignment-version comparisons
also passed both systems after named historical failure/restoration; all 24 new references stay pending.
Nontext-ID present-profile outcomes and full scheduler acceptance remain incomplete. All 26
scheduler mappings remain pending; no capability status is promoted by these narrow fixes.
Native nonfinite refusal remains an explicit stronger difference, not full original parity.
Prior source `01be6df` ran in batch
[39355362784](https://github.com/KiaroSama/Numera-Browser/actions/runs/39355362784).
All 28 nondependent jobs passed, including the corrected four CLI contention cases and
new direct storage acceptance on both platforms. All four foreground CLI time/memory
comparisons reported no regression. CodeQL/required verification failed; publication
stages were skipped. The previous source `0de3fab` ran in hosted batch
[39336995688](https://github.com/KiaroSama/Numera-Browser/actions/runs/39336995688).
All 28 nondependent jobs passed: source platforms, both native tiers, eight relocated
cases and dashboard checks. Expanded shared-store replay, bounded process observation,
current native suites, strict lint and exact historical-test restoration passed.
Both platforms reported no regression in all four foreground CLI latency/memory scenarios.
Earlier Linux vault-read overlap remains recorded: native 71.302–339.860 ms versus
original 294.654–304.417 ms on `e04e4fc`; this and the earlier creation overlap still
have no proved cause. Passing later measurements do not diagnose or erase those results.
Samples and acceptance criteria are unchanged. macOS passed, not proof of its earlier
reader snapshot's cause. Windows partition 0 passed all four process variants and four
observer controls, including persistent survivors and sub-10-ms budgets, without increasing
the original envelope. CodeQL and required verification failed; publication stages were skipped.
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
