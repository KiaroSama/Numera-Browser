# Phase 1 — Quickstart: reproducing the evidence

Every claim in this feature can be re-obtained from these commands. They are written so a
later session does not have to trust this conversation.

## Prerequisites

- Windows workstation, repository at `G:\Program Files\Numera Browser`, virtualenv at `.venv`.
- Every run goes through the project's guarded runner, which owns the process tree and bounds
  wall and idle time. Raw `pytest` is refused by the repository's own test guard.

The runner is invoked as:

```text
pwsh -NoLogo -NoProfile -File ".claude\hooks\Hook-Maker\Test-Run-Guard\scripts\Run-Tests-Guarded.ps1"
  -FilePath ".venv/Scripts/python.exe"
  -ArgumentsJson '["-m","pytest", <files>, "-q","--timeout=<seconds>"]'
  -TimeoutSeconds <wall> -IdleTimeoutSeconds <idle> -HeartbeatSeconds 30
  -RunId <unique> -ProjectFingerprint <project> -ResultPath <state json>
```

Redirect its output to a file and read the tail; the runner keeps printing heartbeats, so a
pipe into `tail` can truncate the summary line.

## Reproduce the failing baselines

Each baseline is the new test file placed on unrepaired `main`.

**N04 (#18)** — expect **7 failed, 37 passed**:

```text
git checkout -B baseline18 origin/main
git checkout origin/fix/20260919-validation-context -- engine/tests/test_validation_context_regression.py
<guarded runner> engine/tests/test_validation_context_regression.py
```

**N02 (#19)** — expect **36 failed, 26 passed** (the brief predicted 33; the real checkout
fails more, not fewer):

```text
git checkout -B baseline19 origin/main
git checkout origin/fix/20260919-active-descriptor -- engine/tests/test_vault_active_descriptor.py
<guarded runner> engine/tests/test_vault_active_descriptor.py
```

**N14 (#20)** — the defect is in the PR's own new suite, so its baseline is the merged branch
itself: `test_failed_report_after_a_descendant_exists_ends_the_whole_tree` fails before the
repair and passes after it.

## Verify each repair

Merge the PR onto `main` in a scratch branch, then run what it touches.

| PR | Suites | Expected |
|---|---|---|
| #18 | the eight `test_write_contract_*`, `test_validation_context_regression`, `test_store_boundary_validation`, `test_cli_import_write_contract` | 404 passed |
| #19 | all 15 `test_vault*` plus `test_locked_cli_and_vault` | 242 passed |
| #20 | `test_helper_*`, `test_driver_capture_concurrency` | all passed, after the repair |

## Reproduce the timing measurement behind FR-008

The research decision rests on one measurement. To re-take it, force the ownership report to
fail after a real grandchild exists, then sample the grandchild's liveness immediately and
poll:

1. `proctree.spawn` a helper that itself spawns a child and writes its pid and token out.
2. Replace `proctree.report_unit` with one that raises `UnitReportFailed`.
3. Call `disposal.reported(proc, tree)` and catch the error.
4. Read the error text: `"may still be running"` present means `end_tree` reported failure;
   absent means it reported success.
5. Sample `proctree.alive(pid, token)` at once, then poll until it turns false, recording the
   elapsed time.

Observed here: the original error (so `end_tree` succeeded), `alive_tree` already false, the
grandchild answering `True` for **10–11 ms** before turning false.

## Whole-feature check

After all three merges, one run over every suite the three touched, then the platform gate
once. Automatic CI is switched off at the owner's request, so that gate is started
deliberately and stopped again rather than left listening.
