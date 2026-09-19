# Validation context follow-up (N04)

Apply the maintainer's repository/global rules and hooks before running integration gates or merging. Do not disable hooks or weaken assertions.

Baseline: `576ecc85e7703da14d324de0f78386a8e9db9ec0` (0.36.0).

## Defect and correction

The new explicit `kind` parameter is validated by string equality in `shape`, but `profile_problems` dispatched with `is`. A JSON-decoded `"dashboard patch"` therefore took complete-object validation, and a valid decoded engine record took attribute access instead of mapping lookup. With the canonical `ENGINE_RECORD` constant, a malformed top-level value raised `AttributeError` before a domain refusal could be produced.

Dispatch now uses equality. Each declared context checks its top-level container before selecting an accessor. Mapping subclasses remain accepted. Required-field policies, fractional memory compatibility, secret intent and legacy read repair are unchanged.

## Executed evidence

Command: `python -m pytest engine/tests/test_validation_context_regression.py -q --tb=short`.

On the hash-verified unchanged modules: 7 failed, 37 passed. On the patched modules: 44 passed, no skips/errors. Python 3.13.5 on Linux. Production converter, dataclasses, policies and validator were used without function substitution; the local sparse module checkout omitted package initializers. These are component/conversion results, not a full-store/API/CLI or native-build certification. The checked-in tests use normal package imports and are collected by the real project suites in CI.

## Acceptance before merge

Run this regression file and existing write-contract, import, batch, secret-intent and API suites through the maintainer's guarded runner. Preserve the successful missing/null fingerprint and 0.25/0.5 memory controls. Inspect actual candidate CI results; do not substitute historical green jobs or release readiness for this scoped fix. No version bump, merge, closure or release is performed by this change.
