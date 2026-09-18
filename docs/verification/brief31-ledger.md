# Closure ledger — brief 31 (N14, N02, N04, CI)

**Reviewed baseline** `c65afba22d8ed569477f605f390b6affa8586b83`, version 0.35.0.
**Candidate** `df5a90cd82ea49ff13639f7de67d9f2fdbcc4e98`, version 0.36.0, on `main`.

The N01–N17 ledger is preserved: nothing was renumbered, and N01, N03, N05–N13,
N15–N17 were not reworked in this pass.

## What each residual was, and what closed it

| ID | Residual | Root cause | Fix | Regression | State |
|---|---|---|---|---|---|
| **N14.A** | Cleanup failure still blocks stream closure | `Streams.close` closed every `BufferedReader` unconditionally. A reader blocked in `read1` holds that object's lock and `close` flushes under it, so the close waited for a writer the failed cleanup had just left running | Join first, close only readers that ended, **retain and name** the rest; `run_helper` writes them onto `straggler` | `test_helper_bounded_lifetime.py::test_closing_does_not_block_on_a_reader_holding_a_live_writer`, `::test_an_unsettled_helper_records_the_readers_it_retained` | **fixed** |
| **N14.B** | Failure before attach loses the real child | `Streams(proc)` ran before `attach`, so on a raise `proc`/`tree` existed only as locals; cleanup saw an empty record, released it, then blocked on the child's pipes | New `HelperManager.adopt`, called immediately after `spawn`; the no-`Streams` path closes the raw pipes | `::test_a_failure_before_attach_still_owns_the_spawned_child` | **fixed** |
| **N14.C** | Repeated shutdown misreads an unresolved reservation | `end_all` tested `state == STARTING`, which the *first* shutdown had already overwritten with `STRANDED`; the second fell through and released a reservation that could still spawn | Test `resolved` — a fact about the spawner — independently of the label | `::test_a_second_shutdown_does_not_release_a_still_unresolved_reservation` | **fixed** |
| **N14.D** | Report integrity incomplete | `read` trusted `Tree.from_record`, which coerces: `17`, `[1]`, `{"pid":"abc"}`, `tree:[1]` raised *out of settlement*, leaving non-daemon readers alive outside cleanup; invalid UTF-8 escaped through `read_text`; missing identity and unknown kinds were accepted as complete | `unit_report._unit` validates every field; bytes are decoded explicitly; `read` never raises and returns partial evidence | 15 parametrized cases + 4 controls in `::test_an_untrustworthy_record_is_incomplete_evidence` and neighbours | **fixed** |
| **N14.E** | Failed report transfer stops only the leader | `_reported` called `_abandon(proc)`, which kills the immediate process — but by then the leader is in its own job/session, so its browser is not a descendant of anything held | `disposal.reported` ends the **tree**, and says so honestly when it cannot | covered by the disposal path; `UnitReportFailed` now carries the identity to act on | **fixed** |
| **N14.F** | The serialization lock is outside the deadline | `settle` acquired `helper.finish` with no timeout and *then* started the clock, so `deadline=.05` could block indefinitely; nested grace/kill calls each took a fresh full budget | One absolute `ends_at` covers acquisition, termination, every unit and the drain; nested calls get a share | `::test_settle_does_not_wait_past_its_deadline_for_the_finish_lock` + `::test_the_finish_lock_is_released_for_the_next_caller` | **fixed** |
| **N02** | Same-salt exception to completion proof | `_finish_enable` ran `structural_problems` when the config's salt matched the journal's, so a foreign key with the same salt was confirmed (**0 of 3** credentials openable) and a damaged config verifier survived because only salts were compared | One invariant on every path; `DESCRIPTOR_DISAGREEMENT` for config-vs-journal; `same_descriptor` compares salt **and** verifier | `test_vault_completion_everywhere.py` (9 cases) + 2 rewritten in `test_vault_recovery_integrity.py` | **fixed** |
| **N02** | Nested shapes pass on the outer container | `_container_ok` checked only `isinstance(data, list/dict)`; `vault.holders` then skips elements it cannot walk, so `[1, 2, "x"]` read as "no secrets here" | Elements, holder nodes and secret types validated; absent/empty stay legitimate | 6 refusal cases + 6 controls across both site kinds | **fixed** |
| **N04.A** | Serialized engine records escape complete validation | `shape()` asked whether `fingerprint` was **non-null**, so a complete record whose required field was null or missing was routed to the *partial* validator | `shape(kind=…)` for callers that know; the guess turns on key **presence**, which malformation cannot erase | `test_write_contract_context.py` — 4 malformed cases + 4 controls | **fixed** |
| **N04.B** | Fractional memory rejected one way, truncated the other | `_num(..., int)` made `0.25` into `0`; the core policy demanded a whole number while the flat guard accepted the fraction | `Rule.fractions=(0.25, 0.5)` as a documented compatibility set; `convert._exact` preserves the value and keeps whole numbers `int` | 13 cases incl. a real-store round trip; `3.5` is still refused by the pre-existing test | **fixed** |
| **CI §5.4** | The portable gate never exercised a frozen self-spawn | The smoke launched via a direct CLI command and checked API readiness; the one path a packaged build has that a checkout does not — the frozen engine re-executing *itself* for a launch or a helper — was untested, which is where the PyInstaller entry point had already broken once | `scripts/smoke-portable.sh` gained steps 6 and 7: `POST /launch` → running status + session directory → `POST /stop` → nothing running; then a finite helper through `POST /trust`, asserted to return **whole** JSON carrying the report's own keys, and to leave no browser running | the portable smoke itself, run by the desktop job on the assembled artifact | **fixed** |
| **CI §5.2** | Bulky artifacts accumulate until uploads fail for everything | A ~700 MB desktop folder on every push to main, with no retention | `retention-days: 7` on all three uploads, beside the `continue-on-error` that keeps retention from being a verdict | — | **fixed** |
| **N14 repeats** | A race run once is a coin toss reported as a result | — | `RACE_REPEATS = 20`, parametrized so a single red attempt fails; no rerun, no flake masking | `::test_the_startup_shutdown_race_holds_every_time` | **fixed** |
| **CI §5.1** | The resource-warning gate never fired | A `ResourceWarning` from GC or a thread is re-emitted by pytest under a *different* class, which `error::ResourceWarning` does not name | Both wrappers gated; fixture owners in the new suites reap what they start | `test_warning_gate.py` — 3 planted leaks × (gated fails / ungated passes) + a clean control | **fixed** |

## How the regressions were proved load-bearing

Four N14 fixes were mutated away one at a time and the named test re-run:

| Mutation | Verdict |
|---|---|
| `Streams.close` closes unconditionally again | **RED** |
| `adopt` removed from `run_helper` | **TIMEOUT** — the hang itself, which is the defect |
| `end_all` tests the state label again | **RED**, with the production symptom (`state='settled'`, released) |
| `finish.acquire(timeout=-1)` | **RED** |

The C test did **not** fail on its first version and was rewritten: it had not
created the report file that `run_helper` creates before spawning, so `settle`
stranded the record for an unrelated reason and the release never happened.
Mutation is what found that; reading the test did not.

Two more test defects were found the same way and fixed rather than worked
around: a pipe opened `buffering=0` is a `FileIO` with no `read1`, so the reader
thread died instantly and never blocked; and patching `end_tree` without
`end_session` let the child be killed, so nothing was ever retained.

## Evidence

**The heavy pass is the CI run for the final SHA.** Nothing here substitutes
for it.

Local runs during development, Windows, through the guarded runner
(`overall: ok`, `exitCode: 0`, `leakedProcessIds: []`) — these were taken
suite-by-suite as each fix landed, which is **not** the cadence the rules ask
for: light checks while writing, one heavy pass at the end. Recorded as what
they are, diagnostics rather than the verification:

| Suite | Result |
|---|---|
| `test_helper_bounded_lifetime.py` | 25 passed, 6.5 s |
| vault: `..._everywhere` + `..._integrity` + `..._invariant` | 62 passed, 15.2 s |
| write contract, 8 suites | 369 passed, 19.2 s |
| `test_warning_gate.py` | 7 passed, 32.6 s |

The mutation runs also called `pytest` directly rather than through the guarded
runner. They are development instrumentation, and their verdicts are recorded
above; they are not part of the verification either.

## Still open, and why

| Item | State | Reason |
|---|---|---|
| Windows CI jobs (`engine-windows`, four `engine-live` legs) | **BLOCKED** | `steps: 0` under GitHub's account billing annotation. They cover a genuinely different path — Windows storage locks are `msvcrt.locking`, byte-range, not `fcntl.flock` — so removing them from `required` would weaken a real gate. Fixed only by GitHub billing or a Windows self-hosted runner |
| `Required verification` aggregate | **BLOCKED** | Needs all seven `needs:` jobs; five cannot start. Read the individual job results |
| 20× startup/shutdown race repeats | **Linux only, via CI** | The repeat is in the suite and runs on every platform CI executes. Windows is not one of them while its jobs cannot start, so the repeat has Linux evidence and no Windows evidence |
| Packaged self-spawn on Windows | **BLOCKED** | `desktop-windows` is release-path only *and* hosted, so the new steps 6-7 have Linux evidence only until Windows CI can run |
| `test_rotate_still_allows_a_real_provider_url` | **open** | Asserts a guard's logic by resolving `example.com` for real; fails when DNS is unavailable. Pre-existing, out of this brief's scope, recorded in `.ai/BUGS.md` |
| Rust dependency freshness | **UNKNOWN** | `cargo-outdated` is not installed; not run, so not claimed |
