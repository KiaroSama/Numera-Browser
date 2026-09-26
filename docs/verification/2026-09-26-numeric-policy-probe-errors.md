# Flat numeric policy and process-group probe errors (N04, N14)

Base: `bca9f07f4cb3e872f817e1e8b4547fa5feed3298`. The two corrections the 2026-09-21 review
left unpublished. Their patch package was never delivered, so they are implemented here from
the review's written specification, not from its patch bytes.

## N04 - flat numbers obey the model's rule

`_dashboard_problems` checked the flat dashboard numbers (`cores`, `memory`, `mediaVideo`,
`mediaAudio`, `mediaSpeakers`, `maxTouchPoints`, `devicePixelRatio`, `seed`) for a range only,
and the API's `routes_profiles._value_problems` repeated the same ranges-only check. So
`cores = 1.5` was accepted at both doors and `convert.to_engine_profile` then cast it with
`int()` and stored `1`; `memory = 3.5` passed although the complete model refuses it.

Both doors now call `write_contract.flat_number_problem`, which reads the rule from
`field_policy.FINGERPRINT` through each entry's `dash` name: the same bounds, whole-number
requirement and permitted fractions (`memory` 0.25 / 0.5) the complete model applies, on the
caller's original value. A flat number without exactly one model rule stops the import.

A third door had the same gap: `DashboardStore.save` converts straight away, without the API's
field check, and the core cast stored `1` in the engine copy while the dashboard copy kept
`1.5`. `server/convert.to_engine_profile` now calls `write_contract.require_flat_numbers`
first, as PR #22 proposed. The core converter stays tolerant so older stored records still load,
and the read-only overviews already catch a refusal.

Refusal messages quoted the caller's value with `str()`/`repr()`, which raise `ValueError`
past CPython's 4300-digit integer limit, so `cores = 10**5000` escaped as a conversion error.
`_display()` describes such a value by type and bit length. The process-wide limit is untouched
and secrets stay unquoted.

## N14 - only ESRCH proves a group gone

`proctree.alive_tree` answered "gone" for any `OSError` from `killpg(pgid, 0)` other than
`PermissionError`. Only `ProcessLookupError` (ESRCH) proves absence (`man 2 kill`; psutil's
`pid_exists` follows the same rule). Any other probe error now keeps the tree owned.

## Tests

- `engine/tests/test_flat_numeric_policy.py`: contract cases, the server-conversion seam for
  every flat field, and the legacy-reader control.
- `engine/tests/test_dashboard_numeric_boundaries.py` (taken from PR #22): every HTTP write
  route (create, update, batch, bulk, native import) and the mirrored store, with both copies
  byte-compared before and after a refusal.
- Together 137 cases; with the conversion guard reverted, 61 fail.
- `engine/tests/test_process_observation_errors.py`: 5 cases. On base: EINVAL and EIO failed.
- Local light run, Windows, Python 3.13, the project's warning gates: all pass. The heavy pass
  is the CI run on the pushed commit.
