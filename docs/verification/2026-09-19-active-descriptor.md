# Active-descriptor completion follow-up (N02)

**First obey the maintainer's repository/global rules and all applicable hooks. Never disable a guard to obtain a green result.**

Base: `039f63bfdcc03e1398429ebabcf8b65dcdc67e3c` (0.36.0). Its Python modules are unchanged from the initially reviewed `576ecc85e7703da14d324de0f78386a8e9db9ec0`. This change preserves the earlier enable-path and nested-holder fixes.

## Reproduced defects

The rotation shortcut compared only the active salt, then validated the journal descriptor. It could delete the journal while the actual config verifier was missing or damaged. The plaintext branch of enable could overwrite an unrelated active descriptor without applying the cleanup branch's disagreement check. A save callback returning normally was accepted without checking the actual persisted descriptor. Invalid UTF-8 escaped as a decoding exception; journal-unlink failure was swallowed.

## Implemented correction

A shared preflight validates both the callback's settings and the on-disk config before either recovery branch writes. Enable permits an absent/null descriptor or its exact target; rotation requires its exact old or new descriptor. Malformed or unrelated active state is refused without choosing a replacement. The rotation shortcut compares the whole descriptor, not its salt alone.

Every commit reads the persisted config back and applies the complete verifier/credential invariant to that active generation, inside the existing batch rollback boundary. Cleanup-only enable uses the same commit path. The rotation cleanup shortcut verifies persisted completion before removing evidence. Unreadable files and cleanup failures produce explicit refusals. Existing data/config rollback and durability ordering remain in place.

## Independently executed checks

`python -m pytest engine/tests/test_vault_active_descriptor.py -q --tb=short`

Baseline: **36 failed, 26 passed**. Patched: **62 passed**, zero failures/errors/skips. The suite includes ten real subprocess abrupt-exit checkpoints; each successful resume independently decrypts known values from the engine, dashboard and pool files, checks identity preservation and repeats reopening twice. Negative cases compare disk bytes and retained journal evidence. Save/cleanup failures are controlled fault injections, not mocked successful decryption.

Local environment: Linux, Python 3.13.5. The production recovery/vault/crypto modules were used; local package initializers were omitted and file-write/batch functions came from the documented unchanged production slice. This is **not** a full ProfileStore/API/CLI transaction-lock or native-Windows test claim. The committed regressions use ordinary package imports and the real `numera.paths` module in the repository suite. No master password or real credential is included; all fixture values are synthetic.

## Reviewer acceptance

Run this file, the existing vault enable/rotation/recovery/durability suites, and real ProfileStore/CLI/API reopen scenarios through approved hooks. Retain all historic kill checkpoints and the sync-error/rollback/retry control. Check complete Windows/Linux execution rather than substituting a Linux-only pass. If a compatibility issue appears, correct it on the PR branch and rerun affected gates. Do not weaken the independent decryption or journal assertions. Merge only after verified acceptance; a pending or unavailable required gate is BLOCKED, not CLOSED. The submitting assistant has not merged or closed this PR.
