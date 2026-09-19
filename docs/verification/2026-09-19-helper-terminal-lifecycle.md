# Helper terminal lifecycle (N14)

**First obey all maintainer repository/workspace rules and applicable hooks. Do not disable a guard or weaken assertions to obtain a green result.**

Base: `039f63bfdcc03e1398429ebabcf8b65dcdc67e3c` (0.36.0). This is an implemented correction, not a request to investigate later. The submitting assistant will not merge or close the PR.

## Defects and correction

The earlier bounded `Streams.close` stopped waiting but left non-daemon readers blocked on live writers, so a worker could not exit after its helper reported failure. A partially started second reader could lose the first reader; helper settlement could remove its registry record before stream disposal; direct settlement could release an unresolved reservation. `end_all` refreshed its deadline for each helper, and nested process termination restarted grace windows. Malformed identity fields and invalid UTF-8 tails could lose usable ownership evidence. A failed driver-claim callback still ended only the immediate process.

Readers now perform cancellable nonblocking reads. POSIX uses nonblocking descriptors; Windows uses typed `SetNamedPipeHandleState(PIPE_NOWAIT)`/`ReadFile`, including supported Python versions without Windows `os.set_blocking`. EOF, cancellation and read failure remain separate: cancellation never becomes successful complete output. No daemon-thread, garbage-collection, discarded-output or production `DEVNULL` workaround is used. The managed stream object is recorded before starting pumps; partial startup unwinds owned resources.

Each helper now owns its streams and output as well as its processes. A single settlement owner covers termination, collection, cancellation, joins, report disposal and registry release. Missing output/evidence produces a retained truthful failure. Unresolved admission stays reserved across direct and repeated settlement, and `end_all` distributes one deadline across the snapshot. Absolute deadlines propagate into process-tree completion without changing the legacy defaults of unrelated callers.

Version-2 receipts distinguish initialized zero ownership from an erased file. A pending record precedes separately owned process creation; a completed unit resolves that exact intent. Interrupted transfers remain incomplete. Nonempty valid legacy records remain readable; invalid records do not erase earlier usable units. Unsupported/invalid process identities are not signalled. Failed ownership or claim transfer disposes of the verified whole tree, retaining structured identity if it cannot prove completion.

## Tests and evidence

New suites: `engine/tests/test_helper_completion_regression.py` (19 cases) and `engine/tests/test_helper_lifecycle_matrix.py` (27 cases). The existing 18 ownership cases are retained and updated for initialized zero-unit receipts, pre-spawn refusal and resolved fixtures. Output/OS/registry assertions occur before teardown.

Fresh unchanged-baseline run of the 19 regression cases, in disjoint batches: **18 failed, 1 passed**. Patched lifecycle suites: **64 passed**, including 60 deterministic start/shutdown interleavings (20 per pause point; not 60 additional pytest cases). The integrated local candidate with the validation and vault PRs: **170 passed**, zero failures/errors/skips, with ResourceWarning and both pytest thread/unraisable wrapper warnings treated as errors. No failed attempt was silently retried into the unique-case count; interrupted outer harness attempts remain recorded separately.

The local test snapshot contains exact selected production modules, but omits package initializers and uses a documented production file-write/batch slice for the separate vault suite. These results are not a full checkout, HTTP/CLI transaction, native Windows, real-backend or packaged-build certification. The committed tests use ordinary imports and the real repository modules. Native Windows pipe behavior and the full existing suite are mandatory reviewer gates, not inferred from Linux success.

## Reviewer acceptance

Run the two new files plus existing helper admission, deadline, lifecycle, ownership, terminal-state, driver-capture, process/session and route/shutdown suites using the maintainer's approved runner. Preserve the 20-repeat barriers and before-teardown assertions. Exercise both inherited writers, clean/nonzero/timeout exits, missing/corrupt receipt, failed prepare/start/adopt/attach, overlapping cleanup, unresolved admission, report unlink failure and failed claim/report after a real descendant exists.

A required test timeout or outer-supervisor rescue is failure. For intentionally lost evidence, bounded refusal with no local reader left behind is the expected result; later fixture cleanup is not proof the application stopped an unidentifiable process. Check genuine EOF output and retained diagnostic state independently. Do not replace domain checks with exact old prose, or a versioned initialized report with an unexplained empty file in fixtures.

Run supported Linux/Windows backend and portable gates on the candidate SHA. If existing integration fixtures reveal a legitimate compatibility issue, repair it on this PR without removing the scenario or weakening invariants. Review and merge only after those gates pass; unresolved infrastructure is BLOCKED, not DONE. If equivalent corrected code is integrated manually, verify it on the target before closing the superseded PR. The submitting assistant performs neither action.
