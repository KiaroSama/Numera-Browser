# Data-lock ownership and acquisition budget

**First follow all maintainer repository/workspace/global rules and applicable hooks. Never disable a hook, weaken an assertion, or bypass a required check.**

Baseline: `bca9f07f4cb3e872f817e1e8b4547fa5feed3298` (0.36.0). PRs #18, #19 and #20 are already merged; this change does not reopen or rewrite their implemented fixes.

## Reproduced problems

`holding_transaction(root)` checked the lock's process-wide recursion depth, not whether the calling thread owned it. While thread A held the root transaction, thread B passed `ConfigStore._save_config_locked`'s precondition and replaced the real config file without joining A's transaction. A valid RLock elsewhere in the process is not permission for this caller to replace a snapshot.

`_DataLock.__enter__` acquired its RLock without a timeout. `NUMERA_LOCK_TIMEOUT` started only after that acquisition, in the OS-lock loop. A caller could therefore wait indefinitely on another thread, or consume two budgets across the thread and process stages. NaN/infinite/negative/unsupported timeout values were accepted. A wrong-thread `__exit__` changed depth and dropped the OS lock before RLock.release finally refused the caller. The timeout message also suggested deleting a potentially active lock file, which can split the lock domain on POSIX.

## Implemented correction

- Record the actual owning Thread object and process id after the first successful acquisition; preserve reentry depth only for that owner.
- Make `holding_transaction` check this calling-thread ownership without waiting. Keep the full-config replacement guard strict rather than acquiring a new transaction around an already stale snapshot.
- Compute one monotonic acquisition deadline before RLock.acquire and pass its remainder to the OS-lock loop. Zero retains immediate-attempt semantics. Invalid or unsupported settings use the existing 30-second fallback.
- Reject wrong-thread release before any depth/fd mutation. Release the in-process lock in a finally block on cleanup failure.
- Keep the existing OS primitives, lockfile names, root-before-file hierarchy, SessionLease behavior and public configuration API. Do not remove lock files as recovery.

The bound covers lock contention and polling, not an uninterruptible kernel/filesystem call or unlimited scheduling suspension. The change does not promise otherwise.

## Executed local evidence

`python -m pytest engine/tests/test_data_lock_ownership.py -q`

Baseline: **15 failed, 9 passed**. Patched: **24 passed**, zero errors/skips. Cases cover a real foreign-thread config replacement, ownership queries, root/file thread contention, invalid/valid timeout values, deterministic shared-budget accounting, wrong-thread exit, reentry, acquisition fault cleanup, 32 concurrent configuration updates, and a real second Python process that times out then acquires the unchanged lock file.

The current merged code's previous 170 regression cases also passed independently before this patch. The local snapshot uses complete hash-matched `paths.py` and `config_store.py`, not the historical file-writing slice. It still omits top-level package initialization and does not claim a local full-app/native-Windows/browser run. The checked-in tests use ordinary project imports and belong in the existing full Python/Windows CI suites. ResourceWarning and pytest's thread/unraisable wrappers were errors.

## Reviewer closure

Run the new file together with existing cross-process locking, config transaction, rollback, vault, session-lease and API tests through the approved hooks, then inspect the exact PR SHA's complete CI. Preserve all scenarios and both OS paths. An infrastructure failure is not a passed product gate. Fix any integration issue on this PR before approval; do not merge only its documentation or close it without equivalent tested behavior on the destination branch. The submitting assistant must neither merge nor close the PR.

## References

- Python 3.10 RLock ownership and timed acquisition: https://docs.python.org/3.10/library/threading.html#rlock-objects
- Python timeout platform maximum: https://docs.python.org/3.10/library/threading.html#threading.TIMEOUT_MAX
- Python file locking: https://docs.python.org/3/library/fcntl.html#fcntl.flock
- Python Windows byte-range locks: https://docs.python.org/3/library/msvcrt.html#msvcrt.locking
