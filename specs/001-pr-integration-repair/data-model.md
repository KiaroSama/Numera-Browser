# Phase 1 — Data model

Three entities carry this feature. None is new; each is listed with the fields the repairs
actually read, because that is what the acceptance tests observe.

## Declared context

The label a caller hands to validation to say which shape it is supplying.

| Field | Type | Notes |
|---|---|---|
| value | one of `"engine object"`, `"engine record"`, `"dashboard patch"` | plain strings, so two equal labels may be distinct objects — the defect #18 repairs |

**Validation rules**

- A label outside the three is refused outright.
- Selection is by equality, never identity (FR-001).
- The declared container is checked before its reader is chosen: an engine record must be a
  mapping, an engine object must be a profile object, a dashboard patch must be a mapping
  (FR-002).

**Lifecycle**: supplied per call. Where absent, the shape is inferred from whether the
`fingerprint` key is *present* — presence, not truthiness, so a null or missing required
field stays the thing under test rather than the thing that picks the test.

## Active descriptor

The vault generation recorded in configuration — the one the next opener will use.

| Field | Type | Notes |
|---|---|---|
| salt | string | compared as part of the whole descriptor, never alone (FR-003) |
| verifier | string | must be non-empty and must accept the password before completion is accepted |

**Validation rules**

- Absent is legitimate only for an enable; a rotation starts from a configured generation.
- The active descriptor must equal the journal's target, or — for a rotation — an allowed
  starting generation.
- After any write, the descriptor is re-read from `config.json` and compared to the verified
  target before completion is accepted (FR-004).

**State transitions**: `absent → new` (enable); `old → new` (rotation); any other pairing is
unresolved evidence and is refused with both records retained.

## Unit

One helper process and everything it owns.

| Field | Type | Notes |
|---|---|---|
| pid | integer | the leader |
| token | integer | identity, so a recycled pid is not mistaken for the original |
| group | job name (Windows) or process group id (POSIX) | what can still reach the tree after the leader is gone |
| kind | `"job"` or `"pgid"` | which primitive owns the tree |

**Validation rules**

- A unit that cannot be given its ownership primitive is terminated, not started (Principle I).
- Settlement covers termination, output collection, reader cancellation, report removal and
  registry release under one owner and one budget (FR-006).
- A failed ownership report ends the whole unit; a failure to end it is reported with the
  group identity (FR-007).

**Liveness, and why it needs a bound**: two questions exist and they are not the same. *Is the
tree empty?* is answered by the ownership primitive and is already polled to a deadline. *Is
this pid still answering?* is answered per process, and on Windows keeps answering for a short
interval after the tree proves empty — measured at about 10 ms. Any assertion about the second
question polls to a deadline (FR-008).
