# Dashboard numeric policy before conversion

**First obey all maintainer repository/workspace/global rules and applicable hooks. Never weaken an assertion, disable a hook, or label an unexecuted gate as passed.**

Baseline: `bca9f07f4cb3e872f817e1e8b4547fa5feed3298`. The validation-context and valid fractional-memory corrections from merged PR #18 remain intact. This is a different, reproduced boundary problem, not a request to redo that PR.

## Defect and implemented correction

The complete engine model requires whole counts, but the flat dashboard contract called `number_problem` without that policy. The server's range checks also admitted fractions. Conversion then applied `int`: for example, `cores=1.5` became `hardware_concurrency=1`. A later validator cannot detect information already lost. The affected count aliases are `cores`, `mediaVideo`, `mediaAudio`, `mediaSpeakers`, `maxTouchPoints` and `seed`. Memory must preserve the explicitly supported 0.25/0.5 values but reject other non-integral values such as 3.5; devicePixelRatio remains a genuinely fractional field.

The canonical numeric validator now derives default whole-number/fraction exceptions from the existing model field policy. The server conversion adapter calls that validator BEFORE delegating to the core converter. This adapter is shared by profile create/update/batch/native-import conversion and DashboardStore's engine twin; its existing callers translate a refusal into a field-specific error before the paired write batch. There is no second table of allowed numeric values. Missing/null values retain existing defaulting semantics, and integer-valued floats remain valid.

The core `numera.convert` mapping is deliberately unchanged: its tolerant legacy read behavior is not a public write validator. This patch does not retroactively repair old files, redefine a third-party exporter's schema, or assert that an abstract standalone DashboardStore with no engine mirror validates every possible field. Read-only overviews already catch an unconvertible record; stricter server conversion must not turn those views into an unhandled traceback.

An associated diagnostic defect is corrected at the shared validator: rendering an out-of-range Python integer with thousands of digits raised ValueError instead of WriteRefused. Bounded diagnostics do not require its decimal representation. Secret fields remain unquoted.

## Executed checks and remaining integration gate

`engine/tests/test_dashboard_numeric_policy.py`: **79 cases**. Unchanged baseline: **62 failed, 17 passed**. Candidate: **79 passed**, no errors/skips. It checks every numeric alias, malformed values, both flat validation and the actual server adapter, valid fractional controls, defaults, policy coverage, huge Python integers and unchanged tolerant core reads.

Combined local candidate with the independent data-lock PR: **273 passed**, no failures/errors/skips, pytest exit 0. This consists of the 170 prior merged regressions, 24 lock cases and 79 numeric cases; iterations and reruns are not extra unique cases. Complete real paths/config/vault/helper/conversion modules were used, but the sparse local snapshot omits package initializers. These are not full HTTP/native-build results.

`engine/tests/test_dashboard_numeric_boundaries.py` adds **53 integration cases**: five real HTTP write routes, direct single/batch mirrored-store writes, and equality of supported values in both persisted copies. This file was syntax-checked but **not executed locally** because the complete app checkout was unavailable. It must run in the ordinary full checkout with `[api,dev]`; its cases are not included in the 273 passes. It imports actual application/store code, not substitute route fixtures. The new files are collected by the existing mandatory Linux/Windows Python CI jobs.

## Acceptance

Run both new files plus the existing profile-schema, write-contract, native/vendor import, bulk, transactional and legacy-read suites using approved hooks. Every rejected HTTP/direct-store request must leave both profile directories and config/metadata bytes unchanged; every allowed memory/ratio value must survive both copies and listing. Check partial update omissions and minimal-profile creation. Preserve #18's equality/shape semantics and the existing secret-intent rules. Correct a real integration failure on this PR; do not delete a case, bypass the adapter or globally loosen integer policies.

Inspect the candidate's own CI, not only main's prior green run. A queued/unavailable gate remains pending. Only the owner's reviewing agent may integrate verified code and then mark this PR done; the submitting assistant must not merge or close it.

## Official references

- Python numeric conversion: https://docs.python.org/3/library/functions.html#int
- Python integer-string conversion limit: https://docs.python.org/3/library/stdtypes.html#integer-string-conversion-length-limitation
- pytest parametrized regression matrices: https://docs.pytest.org/en/stable/how-to/parametrize.html
- FastAPI TestClient and application testing: https://fastapi.tiangolo.com/tutorial/testing/
