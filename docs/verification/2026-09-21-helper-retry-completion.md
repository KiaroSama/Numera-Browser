**FIRST: follow the maintainer's repository/workspace rules and all applicable hooks. Never disable a check or claim a hook ran when it was unavailable.**

# Retry completion from current output evidence (N14)

Base: `bca9f07f4cb3e872f817e1e8b4547fa5feed3298`. This is a narrow follow-up to the merged helper lifecycle work; it does not rewrite that implementation.

## Root cause and implemented correction

A reader can reach genuine EOF before its thread finishes exiting. A short settlement then correctly retains the helper, complete output, report, and an output_problem. On retry, the actual reader has finished and its pipe closes, but the old output_problem remains truthy, so the helper never reaches SETTLED.

The existing incomplete-output branch remains unchanged. Its new else branch clears output_problem only when the latest collect reports complete output and close reports no retained resources. Every other release condition, the settlement lock, process checks, ownership receipt, and report-removal order remain intact. Cancellation and a failed reader are not treated as EOF.

## Executed regression

`engine/tests/test_helper_retry_completion.py` uses an actual subprocess, actual stdout/stderr and real pumps. An event delays only thread exit after genuine EOF; observations precede fixture teardown. It verifies retained failure first, exact OUT/ERR on retry, cleared transient diagnostic, report removal and registry release, and idempotent third settlement. The genuine-read-error control remains unsuccessful over repeated attempts.

Fresh isolated comparison on Linux/Python 3.13.5: baseline **2 failed, 1 passed**; a snapshot with only this production file changed **3 passed**, zero errors/skips, pytest exit 0. ResourceWarning and pytest unraisable/thread warning wrappers were errors. The uploaded production blob is `125e6c539ef864a791277ba0b950bc302aa1e929`, identical to the tested bytes.

The local snapshot contains selected real modules without package initializers; this is not a full application, API/CLI, native Windows or packaged-build certification. The committed regression imports the ordinary project modules and is collected by existing engine CI. No dependency or duplicate workflow is needed.

## Owner-agent acceptance and completion

Run this file plus existing helper admission, ownership, deadline, stream, route/shutdown and platform tests through the approved full-checkout runner. Inspect the exact candidate SHA's required CI, not historical main results. Preserve the genuine-read-error and before-teardown assertions. Fix any integration problem on this PR's branch without weakening checks; do not create another branch unless required.

Once all mandatory gates pass, the owner-authorized agent may merge. If direct merging is unsuitable, integrate the corrected equivalent, commit/push, verify the target, and only then close the superseded PR with that evidence. An unavailable required runner is BLOCKED, not DONE. The submitting assistant must not merge or close anything.

## References

- https://docs.python.org/3/library/threading.html#threading.Thread.join
- https://docs.pytest.org/en/stable/how-to/capture-warnings.html
- https://github.com/KiaroSama/Numera-Browser/blob/bca9f07f4cb3e872f817e1e8b4547fa5feed3298/engine/numera/server/helpers.py
