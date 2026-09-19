# Feature Specification: Verified integration of three correctness repairs

**Feature Directory**: `specs/001-pr-integration-repair`
**Created**: 2026-09-19
**Status**: Draft
**Input**: Integrate PRs #18 (N04), #19 (N02) and #20 (N14) into `main` after independent
verification, and repair the one integration defect that verification found.

## Clarifications

### Session 2026-09-19

No question was put to the owner: the ambiguity scan found nothing whose answer would change
the work. Three decisions that could have been questions are recorded here instead, with what
settled each.

- Q: How is a verified repair integrated — merged through the hosting platform, or applied
  manually and its request closed? → A: merged directly when its change is correct and its
  gates pass; manual integration only where direct merging is unsuitable, and then the
  superseded request is closed against the replacement commit. Settled by the review brief.
- Q: What counts as "gates pass" while automatic continuous integration is switched off at the
  owner's request? → A: the agent runs the suites itself, deliberately, and runs the platform
  gate once at the end. Settled by the owner in this session.
- Q: How long may a check wait for an ended process to stop answering? → A: a bounded poll,
  with a deadline of the same order the product already uses to prove a tree empty (seconds,
  not milliseconds). Chosen as the default rather than asked: it is the existing convention in
  this codebase, and the measured interval is roughly 10 ms, three orders of magnitude inside
  it. Recorded in Assumptions.

## User Scenarios & Testing *(mandatory)*

The user here is the owner of a Numera Browser installation. None of these three repairs
adds a control they click; each removes a way the product could tell them something untrue
about their own profiles and processes. The scenarios are written from what the owner
experiences, because that is what decides whether a repair is worth shipping.

### User Story 1 - An import of somebody else's export is accepted or refused, never misread (Priority: P1)

The owner exports profiles from another anti-detect tool and imports them. Today a profile
whose declared shape reaches the validator as an equal-but-distinct label can be validated
by the wrong reader: a complete record is treated as a partial patch, so a missing required
field passes, and a malformed top-level container escapes as an internal error instead of a
refusal naming the field.

**Why this priority**: it decides whether a stored profile is what the file said. A record
accepted under the wrong contract is wrong from its first launch and nothing later flags it.

**Independent Test**: import files whose context label arrives by value rather than identity,
and files whose declared container does not match the context they claim. Every one is either
stored correctly or refused with the field named. Nothing is written by a refused import.

**Acceptance Scenarios**:

1. **Given** an import whose context label is equal to the canonical one but not the same
   object, **When** the import is validated, **Then** it is read by the same rules as the
   canonical label.
2. **Given** an import that declares an engine-record context but supplies a scalar, a list
   or a null, **When** it is validated, **Then** it is refused, the refusal names the field,
   and no profile is written.
3. **Given** a legitimate partial patch, **When** it is validated, **Then** its omissions are
   still permitted.

### User Story 2 - A vault recovery finishes only when the vault actually opens (Priority: P1)

The owner's machine loses power during `vault enable` or a master-password rotation. On the
next start, recovery completes and reports success. Today that report can rest on a marker,
a matching salt or a callback that returned without error — none of which prove the
descriptor the next opener will read can actually decrypt the data.

**Why this priority**: a recovery that reports success it cannot back leaves the owner with
a vault that opens later, in a state where the journal is already gone.

**Independent Test**: cross enable and rotation with ordinary, resumed and cleanup-only
paths against absent, new, old, divergent and malformed active descriptors, healthy and
foreign ciphertext, correct, wrong and missing passwords, and a save callback that lies.
Every accepted completion is checked by reopening the store from disk.

**Acceptance Scenarios**:

1. **Given** a recovery that completes, **When** it reports success, **Then** the descriptor
   persisted on disk is the verified target and the data answers to it.
2. **Given** a save step that returns successfully without writing what it was asked to
   write, **When** completion is checked, **Then** the recovery is refused and its evidence
   is retained.
3. **Given** a rotation whose active descriptor is neither an allowed starting generation nor
   the journal's target, **When** recovery runs, **Then** it refuses without discarding
   either record.
4. **Given** the recovery record cannot be deleted after the data and config phases finished,
   **When** cleanup fails, **Then** the owner is told recovery must be retried rather than
   the failure being swallowed.

### User Story 3 - Stopping a profile stops everything it started (Priority: P1)

The owner stops a profile, or the engine shuts down while helpers are running. Today a
helper's readers, its output, its ownership record and its descendants are released on
separate paths with separate budgets, so a shutdown can return while a descendant still
holds a pipe, and a lost ownership record can read as "nothing was ever started".

**Why this priority**: an unstopped browser is the failure this product exists to prevent,
and an unreadable ownership record is what makes it unfindable afterwards.

**Independent Test**: run the real helper, route and shutdown paths with ordinary, non-zero
and timeout exits; trailing, unterminated, Unicode and large output; missing, malformed,
truncated and wrongly-typed receipts; failed starts, adopts and attaches; repeated and
overlapping settlement; and a report failure after a real descendant exists. Inspect the OS,
the readers, the registry and the report before teardown.

**Acceptance Scenarios**:

1. **Given** a helper whose ownership report cannot be written after a descendant exists,
   **When** disposal runs, **Then** the whole tree is ended, not only its leader.
2. **Given** several helpers shutting down together, **When** one declared deadline covers
   them, **Then** the deadline is not multiplied by nesting.
3. **Given** an ownership record that is damaged in part, **When** it is read, **Then** the
   readable units survive and an erased file is distinguished from a record of zero units.
4. **Given** cancellation of a reader, **When** output is reported, **Then** partial output
   is never upgraded to complete output.

### Edge Cases

- A process that the operating system has terminated can still answer a liveness question
  for a short interval afterwards. Any check that a process ended MUST allow for that
  interval rather than sampling once.
- A refusal raised while reading configuration must not be swallowed by the handler that
  exists to catch unreadable configuration.
- An import file may be a Numera export written before the format carried a version; it is
  refused by name rather than reinterpreted by vendor heuristics.
- A vendor value of the wrong type for a field that has no coherent generated default is
  refused; the whole file is refused with it, so half an import never lands on disk.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Validation MUST select a reader by the value of the declared context, not by
  the identity of the label object.
- **FR-002**: A declared context whose container does not match MUST produce a refusal that
  names the field, and MUST NOT be reported as an internal error.
- **FR-003**: Every vault recovery path MUST validate the active persisted descriptor, and
  compare complete descriptors rather than the salt alone.
- **FR-004**: A recovery MUST read back the persisted configuration after saving and refuse a
  result that is absent, unchanged or different from the verified target.
- **FR-005**: A failure to dispose of the recovery record MUST surface as an actionable
  recovery failure.
- **FR-006**: Helper settlement MUST place process termination, output collection, reader
  cancellation, report removal and registry release under one owner and one budget.
- **FR-007**: A failed ownership report MUST end the whole verified unit, and a failure to do
  so MUST be reported with the identity that can still reach it.
- **FR-008**: An assertion that a process has ended MUST be satisfied within a bounded wait
  rather than a single sample taken immediately after the request to end it.
- **FR-009**: Every scenario retained from the existing suites MUST remain, by name, after
  any rewrite of those suites.
- **FR-010**: Each repair MUST be shown failing on the unrepaired code before it is accepted.

### Key Entities

- **Declared context**: the label a caller supplies to say which shape it is handing over —
  an engine object, an engine record, or a dashboard patch.
- **Active descriptor**: the vault generation recorded in the configuration that the next
  opener will read; distinct from the generation a recovery journal is moving toward.
- **Unit**: one helper process together with everything it owns — descendants, output
  readers, its ownership record and its registry entry.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An import whose declared context arrives by value behaves identically to the
  canonical one, in 100% of the shapes the suite covers.
- **SC-002**: No refused import writes any record: the store holds zero new profiles after
  every refusal case.
- **SC-003**: Every accepted vault recovery is confirmed by reopening the store from disk,
  with no accepted completion resting on a callback's return.
- **SC-004**: After a helper unit is settled, no process, reader, pipe, registry entry or
  report belonging to it remains; verified before teardown.
- **SC-005**: Each of the three repairs has a test that fails on the code before it and
  passes after, demonstrated by running both.
- **SC-006**: The three repairs together leave every previously passing suite passing.

## Assumptions

- The three implementations under review are the open pull requests #18, #19 and #20; they
  are reused rather than reimplemented, and their production files do not overlap.
- Verification runs on this Windows workstation and, where an external runner is available,
  in CI. Where a platform cannot be exercised, that is recorded rather than inferred.
- The existing refusal contract for unrenderable vendor values is deliberate and stays: the
  scope here is reading a nesting that was simply unknown, not softening a refusal.
- `main` is the integration target and is not protected, so a verified repair is merged
  directly rather than through a protected-branch process.
- A check that a process has ended polls until a deadline of seconds rather than sampling
  once. The measured interval between "the owning job proves empty" and "the process stops
  answering a liveness question" is about 10 ms on this Windows workstation, so a deadline of
  the order the product already uses to prove a tree empty leaves ample margin without
  turning a real survivor into a passing test.
