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

The capability ledger now records 47 individually inspected implementation/test
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
boundaries and invalid values. Hosted batch `39130990824` proved the exact
historical assertion failure and fixed-byte restoration on both systems; the
current test passed on Linux. Windows timed out during the following fixed-tree
compilation before current tests ran, so its fixed-pass evidence remains pending.

The next folder correction preserves Python boundary trimming for U+001C–U+001F
in native create, rename, delete and reassignment. One original-source observer
compares 96 operation cases, returned lists/refusals, parsed metadata/profile state
and each file's byte-preservation verdict. Exact historical failure and current
hosted acceptance are pending; locks, rollback and serialization are unchanged.

Historical source swaps can invalidate the earlier native prebuild. The full test
command now projects actual Cargo artifact completions and build success while
Cargo renders compiler diagnostics normally. Raw build-script/environment/path
records are discarded. Phase ordering, all discovered targets, two workers and
the 600-second wall/120-second idle limits remain unchanged; no heartbeat or
increased timeout masks inactivity. A single compilation unit silent for more
than 120 seconds still fails. Hosted proof of this correction remains pending.

## Acceptance status

Source `bb82941` ran in hosted batch
[39130990824](https://github.com/KiaroSama/Numera-Browser/actions/runs/39130990824).
All source platforms, Linux native and all eight relocated Windows/Linux cases
passed. Windows proved every historical assertion and restored fixed bytes, but
its next full compilation hit the idle limit before current tests or later replay
and performance stages. No compiler deadlock is established. The preceding
`22e0e59` batch passed both complete native tiers, replay and four CLI measurements;
that historical success does not verify the changed Windows tree.
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
