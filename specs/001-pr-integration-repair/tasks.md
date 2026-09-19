---
description: "Task list for verified integration of three correctness repairs"
---

# Tasks: Verified integration of three correctness repairs

**Feature**: `specs/001-pr-integration-repair`
**Plan**: [plan.md](./plan.md) | **Spec**: [spec.md](./spec.md)

**Tests**: no new test *suites* are written here. The three pull requests bring their own,
and the one test change is a repair to an assertion that verification proved wrong. Baselines
are reproduced rather than invented.

## Phase 1: Setup

- [X] T001 Fetch and pin the three heads under review with `git fetch origin --prune`, recording each head SHA against the review brief
- [X] T002 Confirm each branch merges cleanly onto `origin/main` in a scratch branch, and record how far each is behind

## Phase 2: Foundational (blocking)

These gate every story: they are the measurements the constitution check left open and the
evidence that the repairs are real rather than asserted.

- [X] T003 Measure every production file the three PRs touch against the 800-line ceiling, before and after, in `engine/numera/`
- [x] T004 Resolve the ceiling finding for `engine/numera/vault_recovery.py` (794 → 855): extract the two new validators into a new responsibility-named module and wire them in, keeping the file under the ceiling
- [x] T005 Record the `engine/numera/proctree.py` ceiling deviation (981 → 1016, already over before this work) in `.ai/DECISIONS.md` with the measurement, the reason extraction is not the right cut here, and the real remedy as separate work
- [X] T006 Verify the rewritten `engine/tests/test_helper_ownership.py` retains every test by name against `origin/main`

## Phase 3: User Story 1 — imports are accepted or refused, never misread (P1)

**Goal**: a declared validation context selects its reader by value, and a container that does
not match its declared context is refused by field.

**Independent test**: reproduce the failing baseline on `main`, then show it green merged.

- [X] T007 [US1] Reproduce the N04 baseline: apply only `engine/tests/test_validation_context_regression.py` to `origin/main` and record the failure count
- [X] T008 [US1] Read the production change in `engine/numera/write_contract.py` and confirm the context constants are strings, so identity comparison was a real defect
- [X] T009 [US1] Confirm no production caller declares an engine-object context, so the new container guard tightens only the declared path
- [X] T010 [US1] Run the write-contract, validation, store-boundary and CLI-import suites with #18 merged onto `main`
- [x] T011 [US1] Merge #18 into `main` through the authorized workflow and confirm the merge commit contains the tested change

## Phase 4: User Story 2 — a vault recovery finishes only when the vault opens (P1)

**Goal**: every recovery path validates the active persisted descriptor and reads back what
reached `config.json`.

**Independent test**: reproduce the failing baseline on `main`, then show all vault suites
green merged.

- [X] T012 [US2] Reproduce the N02 baseline: apply only `engine/tests/test_vault_active_descriptor.py` to `origin/main` and record the failure count
- [X] T013 [US2] Confirm the refusal raised inside the configuration-reading guard cannot be swallowed by that guard's own `except` clause, by checking the vault exception hierarchy
- [X] T014 [US2] Run all fifteen vault suites plus the locked-CLI suite with #19 merged onto `main`
- [x] T015 [US2] Re-run the vault suites after T004's extraction, since T004 changes a file this story owns
- [x] T016 [US2] Merge #19 into `main` through the authorized workflow and confirm the merge commit contains the tested change

## Phase 5: User Story 3 — stopping a profile stops everything it started (P1)

**Goal**: helper settlement covers the whole unit under one deadline, and a failed ownership
report ends the whole tree.

**Independent test**: the helper suites pass on native Windows, including the descendant case
that exposed the defect.

- [X] T017 [US3] Run every helper suite with #20 merged onto `main` and isolate the single failure
- [X] T018 [US3] Establish whether the surviving grandchild is a production defect or an assertion artefact, by measuring which error `disposal.reported` raised and how long the pid keeps answering
- [X] T019 [US3] Confirm the rationale deleted from `engine/numera/server/helper_streams.py` survives the rewrite (decode errors policy, newline translation)
- [x] T020 [US3] Repair the unbounded assertion in `engine/tests/test_helper_lifecycle_matrix.py` so it polls to a deadline instead of sampling once, on branch `fix/20260919-helper-terminal-lifecycle`
- [x] T021 [US3] Re-run the helper suites with the repair and confirm the previously failing case passes
- [x] T022 [US3] Push the repair to #20's branch
- [x] T023 [US3] Merge #20 into `main` through the authorized workflow and confirm the merge commit contains both the tested change and the repair

## Phase 6: Polish & cross-cutting

- [x] T024 Run the combined suite over everything the three PRs touched, on the final `main`
- [x] T025 Confirm no process, worker, port or temporary artifact survived any run in this feature
- [x] T026 Start the platform gate deliberately, read its result for the exact final SHA, then stop it again — it is not left listening
- [x] T027 Update `.ai/memory.md` and `.ai/TESTING_NOTES.md` with the baselines, the measured liveness interval and the ceiling findings
- [x] T028 Confirm all three pull requests are closed by their merges, none by hand

## Dependencies

```text
T001 → T002 → T003 ─┬─► T004 ──► T015 ──► T016
                    └─► T005
T006 ──► T017

US1 (T007→T011)  independent
US2 (T012→T016)  independent, but T015 waits on T004
US3 (T017→T023)  independent

All stories ──► T024 → T025 → T026 → T027 → T028
```

## Parallel opportunities

- T007, T012 and T017 are three separate baselines on three separate branches: independent.
- T004 and T005 touch different files and may proceed together.
- T011, T016 and T023 are merges and are deliberately **serial**, so a failure after a merge
  names the merge that caused it.

## Implementation strategy

The MVP is User Story 1 alone: it is the smallest change, it merges cleanly, and it needs no
repair. Landing it first proves the integration route works before the larger two use it.

## Phase 8: Convergence

- [X] T029 CRITICAL Split engine/numera/proctree.py by responsibility so no file exceeds the ceiling, keeping behaviour and every public name reachable through proctree, per Constitution V (contradicts)
- [X] T030 Re-run the platform gate to a recorded run-level conclusion on the exact final SHA, then stop the runner, per SC-006 (partial)
- [X] T031 Add the post-0.36.0 entry to CHANGELOG.md covering the three merged repairs and the fixes found reviewing them, per Constitution "Change Discipline" (missing)
- [X] T032 Rebuild the code graph so it covers engine/numera/vault_active_descriptor.py and dashboard/src/views/ProfileSummary.jsx, per plan: navigation before broad browsing (missing)

### Convergence dependencies

```text
T029 ──► T031 ──► T030
T032 (independent of the other three; run it after T029 so it indexes the split)
```
