# Implementation Plan: Verified integration of three correctness repairs

**Feature**: `specs/001-pr-integration-repair` | **Date**: 2026-09-19
**Spec**: [spec.md](./spec.md)

## Summary

Three open pull requests each repair one correctness defect in a different subsystem —
profile validation (#18), vault recovery (#19), helper lifecycle (#20). They are reviewed,
tested against a reproduced failing baseline, repaired where verification finds a defect,
and merged into `main`. Verification found exactly one defect, in #20's own new test rather
than in its production change; that repair is made on #20's branch before it merges.

## Technical Context

**Language/Version**: Python 3.13 (engine, verified locally), also run on 3.10 and 3.12 in CI
**Primary Dependencies**: pytest with `pytest-timeout`; no new dependency is introduced
**Storage**: the profile store and vault under the data root; `config.json` holds the active
vault descriptor
**Testing**: pytest through the project's guarded runner, which owns the process tree and
bounds wall and idle time
**Target Platform**: Windows 11 workstation (native, where this verification ran) and Linux
(self-hosted CI runner, owner-controlled)
**Project Type**: single repository, three components — Python engine, React dashboard,
Tauri shell. Only the engine is touched.
**Performance Goals**: not a performance feature. The one timing fact that matters is a
measured ~10 ms interval, recorded in the spec's Assumptions.
**Constraints**: automatic CI is switched off at the owner's request, so suites are run
deliberately by the agent; the platform gate runs once at the end. No force-push, no
protection change, no billing change.
**Scale/Scope**: 3 pull requests, 10 changed production files, 5 test files, ~2000 changed
lines.

## Constitution Check

| Principle | Gate | Status |
|---|---|---|
| I. Ownership Before Capability | #20 must place termination, output collection, cancellation, report removal and registry release under one owner and one budget | PASS — that is the substance of the change; verified by its suites |
| II. Fail Closed on Missing Evidence | no repair may soften an existing refusal | PASS with one correction: an earlier attempt in this repository added a fallback where the write contract requires a refusal; that attempt was reverted and the refusal is retained. #19 strengthens refusals rather than relaxing them. |
| III. Verify Against the World | each accepted completion must be checked against disk/OS state | PASS — #19 reads `config.json` back after saving; #20's suites inspect OS state before teardown |
| IV. Tests That Can Fail | each repair must be shown failing before it passes | PASS — baselines reproduced for #18 (7 failed) and #19 (36 failed); #20's own new suite failed on the defect this plan repairs |
| V. One File, One Responsibility | no file may pass 800 lines | CHECK REQUIRED — measure every file the three PRs touch before merging |

No unjustified violation. The one item needing measurement is recorded as a task rather than
assumed.

## Project Structure

### Documentation (this feature)

```text
specs/001-pr-integration-repair/
├── spec.md
├── plan.md              # this file
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
└── checklists/
    └── requirements.md
```

### Source Code (repository root)

```text
engine/numera/
├── write_contract.py           # #18 — context dispatch by value
├── vault_recovery.py           # #19 — active descriptor validation
├── disposal.py                 # #20 — report-or-end
├── driver_capture.py           # #20
├── proctree.py                 # #20 — tree ownership, end_tree, alive
├── unit_report.py              # #20 — ownership record
└── server/
    ├── helpers.py              # #20 — settlement under one budget
    └── helper_streams.py       # #20 — cancellable readers

engine/tests/
├── test_validation_context_regression.py   # #18, new
├── test_vault_active_descriptor.py         # #19, new
├── test_helper_completion_regression.py    # #20, new
├── test_helper_lifecycle_matrix.py         # #20, new — carries the defect to repair
└── test_helper_ownership.py                # #20, rewritten bodies, same 15 test names
```

**Structure Decision**: no new module is introduced. The repair is a change to one assertion
helper inside an existing test file, so no file-layout decision arises.

## Phase 0 — Research

See [research.md](./research.md). One question needed answering and it was answered by
measurement, not by reading: whether the surviving grandchild in #20's failing test is a
production defect or an artefact of how the test asks.

## Phase 1 — Design

- [data-model.md](./data-model.md) — the three entities the spec names, with the fields each
  repair actually reads.
- [quickstart.md](./quickstart.md) — the exact commands that reproduce each baseline and
  verify each repair, so the evidence can be re-obtained without this conversation.
- No `contracts/` directory: this feature exposes no new interface. The contracts it touches
  — the write contract's refusal set, the vault recovery outcome, the ownership record format
  — already exist and are held in place by existing suites rather than redefined here.

## Integration Order

1. **#18** first. Its production change is 15 lines in one file, it touches nothing the other
   two touch, and merging it first makes the later merges easier to attribute.
2. **#19** second. Independent of both others.
3. **#20** last, after its repair. It is the largest, the only one with a repair, and the one
   whose suites take longest, so it is the one most worth landing on a `main` that is already
   known good.

Each merge is followed by the suites for what it touched, so a failure names the merge that
caused it. After all three, one combined run over every affected suite, then the platform
gate.

## Complexity Tracking

| Item | Why it is not simpler | Rejected alternative |
|---|---|---|
| Three separate merges rather than one | Their production files do not overlap, so separate merges keep a failure attributable to one change | One integration branch: cheaper to run, but a failure would not say which repair caused it |
| A bounded poll in the test rather than a single sample | The OS does not guarantee a pid stops answering at the instant its job empties; the product's own `prove_empty` already polls | Asserting on `alive_tree` only: would pass, but would stop testing the thing the scenario is about — that the *descendant* is gone |
