**FIRST: obey the maintainer's repository/workspace rules and all applicable hooks. Do not disable a check or misrepresent an unavailable verification as passed.**

# Preserve uncertainty in procfs and ps observations (N14)

Base: `bca9f07f4cb3e872f817e1e8b4547fa5feed3298`. This PR changes only `posix_session.py` in production; it is independent of the helper-retry PR.

## Defect and implemented correction

The process-group scanner skipped every failed stat read, not just entries that disappeared. Permission/I/O errors and malformed procfs records could therefore produce an empty-group answer while an actual child remained alive. The fallback also ignored the exit status of ps and accepted incomplete rows as evidence of absence.

Treat only ENOENT/ESRCH as normal entry disappearance. Keep an uncertain flag for other observation/parse errors, return an unknown/present result on failed enumeration, and exclude an explicitly ignored PID before reading its stat. Require a successful ps exit and a usable process table; reject malformed rows conservatively. Preserve healthy live/reaped-child, zombie-only, unrelated-group and disappearance controls. No process signalling or identity policy is expanded.

This conservative choice can retain a group when an unreadable entry prevents proving it empty. It must not be changed into guessing identities or signalling unrelated processes.

## Fresh executed tests

`engine/tests/test_posix_observation_errors.py` contains 16 cases. Unchanged baseline: **8 failed, 8 passed**. An isolated snapshot with only this production correction: **16 passed**, zero errors/skips, pytest exit 0. Actual Linux child processes were used for the stat and direct stop-result cases; assertions occurred before teardown. The failing baseline directly returned successful end_tree while Popen.poll still showed a live child.

Source blob `0af475fc0e0ac1b35f3653df30ca1ab2ddf522fe` matches the tested local file. ResourceWarning and pytest's thread/unraisable wrappers were treated as errors. Local execution used selected actual modules without package initializers, not the complete application or a native macOS runner. POSIX-only and procfs-only tests carry explicit platform guards; native Windows code is unchanged.

## Deliberately separate unpublished item

The earlier review also identified the generic process-group probe-error branch in `proctree.alive_tree`. Its file upload was stopped by the tool and it is **not changed or claimed fixed by this PR**. Its separate regression is not counted among these 16 scanner tests. The numeric-validation correction is also not part of this PR. This scoped publication must not be described as closing all prior review findings.

## Reviewer acceptance

Use the approved full checkout/hooks to run the new file, existing process/session/helper tests and applicable native Linux/macOS checks. Keep the current module split and all existing tests. Validate the candidate's own CI and combine with the helper-retry PR before asserting integration success. Existing non-live engine CI discovers this file; no dependency or duplicate workflow is required.

If correct and required gates pass, the owner-authorized agent may merge. Otherwise fix this same branch, commit/push and rerun. If applying an equivalent correction manually, verify the target implementation before closing the superseded PR. An unavailable required gate is BLOCKED, not DONE. The submitting assistant must never merge or close anything.

## References

- https://docs.kernel.org/filesystems/proc.html
- https://docs.python.org/3/library/subprocess.html#subprocess.CompletedProcess.returncode
- https://docs.pytest.org/en/stable/how-to/capture-warnings.html
- https://github.com/KiaroSama/Numera-Browser/blob/bca9f07f4cb3e872f817e1e8b4547fa5feed3298/engine/numera/posix_session.py
