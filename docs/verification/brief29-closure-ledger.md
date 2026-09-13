# Closure ledger — N14, N02, N04 (0.34.0)

**Baseline reviewed:** `d40415b648f854178b88eb8aa88551e9e6ea5979` (0.33.0).
**Candidate:** the single commit on `fix/brief29-remediation`; `git log -1` on
that branch is the fix commit for every row below.
**Platform actually executed:** Windows 11 26200, CPython 3.13.15, pytest 9.1.1,
Node 24.18.0, Rust 1.98.0. Linux legs are CI-only and are recorded as **not
executed here** rather than as passes.

States: `OPEN → REPRODUCED → FIXED_LOCAL → VERIFIED → CLOSED`, `BLOCKED` with a
cause. `VERIFIED` here means a regression that fails on the baseline behaviour
and passes now, observed on this host.

## The 19 prior diagnostics

The review's 19 failures were component/adapter diagnostics, not 19 independent
defects. Their individual identifiers live in a supplied evidence bundle
(`Numera-Browser_d40415b_Evidence.zip`) that is **not present in this repository
or in the attachment**, so they are mapped by acceptance group and count rather
than by per-diagnostic id. Nothing is omitted: every group the brief names has
rows below, and each row states what was reproduced.

| Group | Brief's subcases | Rows below |
|---|---|---|
| N14 | A, B, C | 9 |
| N02 | A, B, C | 8 |
| N04 | A, B | 6 |

## N14 — bounded lifecycle, complete ownership, truthful completion

| ID | Entry point | Baseline failure evidence | Regression node | OS/backend | Result |
|---|---|---|---|---|---|
| N14.A-1 | `helpers.run_helper` | Helper wrote `TRAILING-NO-NEWLINE`, exited 0; caller got `returncode=0`, `stdout=''`. 6.89s against a 3s budget. | `test_helper_ownership.py::test_a_driver_holding_both_writers_does_not_truncate_the_answer` | Win / real children | VERIFIED |
| N14.A-2 | `proctree` + OS | Driver in its own job survived or was unverifiable from `settle()`'s return alone | `::test_the_driver_is_gone_according_to_the_operating_system` | Win / real children | VERIFIED |
| N14.A-3 | `helpers.run_helper` | Same-group pipe holder — the control that separates "held pipe" from "left the group" | `::test_a_grandchild_in_the_same_group_is_the_control` | Win / real children | VERIFIED |
| N14.A-4 | `helpers.run_helper` | Timeout path with a separate session, against a declared bound | `::test_a_wedged_helper_with_a_driver_times_out_within_the_declared_bound` | Win / real children | VERIFIED |
| N14.A-5 | `helpers.run_helper` | Normal and non-zero exits, trailing body without a newline | `::test_a_plain_helper_returns_its_whole_output` (2 cases) | Win / real children | VERIFIED |
| N14.A-6 | `helper_streams.Streams` | A reader still waiting on an open writer reported completion | `::test_a_reader_that_cannot_reach_the_end_says_so` + `::test_a_closed_writer_lets_the_reader_finish` | Win+POSIX / real OS pipe | VERIFIED |
| N14.A-7 | `helpers.run_helper` | Daemon readers outlived the call holding output | `::test_no_reader_thread_outlives_the_call` | Win / real children | VERIFIED |
| N14.B-1 | `HelperManager` | `admit` → `end_all` released a `starting` record: `live=1 → 0`, `state=settled`, caller then spawned | `test_helper_admission.py::test_a_shutdown_does_not_release_a_reservation_that_has_not_spawned` | Win | VERIFIED |
| N14.B-2 | `helpers.run_helper` | Shutdown raced against three pause points, 20 repeats each | `::test_a_shutdown_racing_a_start_never_loses_the_outcome` (3 × 20) | Win | VERIFIED |
| N14.B-3 | `helpers.run_helper` | Temp-file / spawn / attach failure; attach failure leaked two `BufferedReader`s (ResourceWarning) | `::test_a_report_file_that_cannot_be_created_starts_nothing`, `::test_a_spawn_failure_leaves_no_reservation`, `::test_an_attach_failure_does_not_lose_the_process_it_created` | Win | VERIFIED |
| N14.B-4 | `HelperManager.end_all` | Repeated, concurrent, and two-app shutdown; other app's sentinel must survive | `::test_shutting_down_twice_is_not_a_different_answer`, `::test_concurrent_shutdowns_do_not_race_each_other`, `::test_one_app_s_shutdown_leaves_another_app_s_helper_alone` | Win | VERIFIED |
| N14.C-1 | `unit_report.read` | Missing / unreadable / truncated / malformed / no-path all returned `[]`, same as a helper that owned nothing | `test_helper_ownership.py::test_a_damaged_report_is_not_an_empty_one` (3), `::test_a_missing_report_is_not_an_empty_one`, `::test_no_report_path_at_all_is_not_an_empty_one` | Win+POSIX | VERIFIED |
| N14.C-2 | `unit_report.read` | Valid zero units, and several units | `::test_an_empty_report_is_a_real_answer`, `::test_several_units_all_come_back` | Win+POSIX | VERIFIED |
| N14.C-3 | `HelperManager.settle` | Lost evidence cleared the registry | `::test_a_damaged_report_strands_the_helper_rather_than_clearing_it` | Win | VERIFIED |
| N14.C-4 | `proctree._spawn_leader` | `_report_unit` swallowed `OSError`; a driver whose ownership could not be recorded started anyway | `::test_a_report_that_cannot_be_written_ends_the_launch` | Win / real child | VERIFIED |

**Not executed here:** Linux legs of every N14 row, and the four-backend live
lifecycle matrix beyond the bundled Chromium available on this host. Recorded as
CI work, not as passes.

## N02 — one completion invariant for every vault path

Fixtures are built through real writers with known credential values in all
three stores; no `enc:v2:abc`, no `verifier="v"`. The oracle decrypts every
stored credential with the descriptor the config names and compares it to the
planted value (`assert_fully_recovered`), never the production scanner.

| ID | Entry point | Baseline failure evidence | Regression node | Result |
|---|---|---|---|---|
| N02.A-1 | `vault.recover` | 3 credentials re-encrypted under a foreign descriptor: `recover()` returned **True**, committed the wrong descriptor, **deleted the journal**; afterwards nothing opened under any password | `test_vault_completion_invariant.py::test_ciphertext_from_a_foreign_descriptor_is_refused` | VERIFIED |
| N02.A-2 | `vault.recover` | Refusal must not be destructive | `::test_the_foreign_data_is_still_recoverable_afterwards` | VERIFIED |
| N02.A-3 | `vault.recover` | Healthy enable must still complete | `::test_an_enable_whose_data_really_is_ours_completes` | VERIFIED |
| N02.A-4 | `vault.recover` | No key ⇒ actionable refusal, evidence kept | `::test_without_a_password_an_interrupted_enable_refuses` | VERIFIED |
| N02.B-1 | `vault.recover` | Nonempty damaged verifier committed unused; `check_password` then rejected the committed value | `::test_a_damaged_verifier_is_never_committed` | VERIFIED |
| N02.B-2 | `_descriptor_ok` / `crypto.check_password` | Non-base64 salt raised `binascii.Error` out of a yes/no question | `::test_an_unusable_descriptor_is_refused` (4 cases) | VERIFIED |
| N02.C-1 | `vault.recover` | Plaintext pool password survived a resumed rotation, both with and without config already naming the new key; `recover()` returned True | `::test_a_resume_does_not_finish_over_a_readable_credential` (2 cases) | VERIFIED |
| N02.C-2 | `vault.recover` | New config over old data; malformed holder containers; wrong/missing passwords; empty control; reopen twice | `::test_a_new_config_over_old_data_is_not_a_finished_rotation`, `::test_a_malformed_holder_container_blocks_completion` (3), `::test_a_wrong_password_refuses_and_changes_nothing` (2), `::test_a_resume_with_no_passwords_refuses_and_keeps_the_salt`, `::test_a_root_with_no_secrets_at_all_completes`, `::test_the_store_opens_twice_after_the_protocol_settles` | VERIFIED |

Kill-boundary checkpoints from 0.32.0/0.33.0 are retained unchanged
(`test_vault_recovery_completeness.py`, 10 scenarios with real `os._exit`
children) and the config-sync-error → rollback → retry control is retained
(`test_vault_commit_protocol.py`).

**Contract changed, with replacement coverage:**
`test_vault_recovery_integrity.py::test_a_readable_root_still_completes` became
`..._whose_descriptor_already_committed_still_completes` and gained a sibling
`test_an_uncommitted_descriptor_is_not_adopted_without_a_key`. Rationale: the
old assertion committed a descriptor with no key on the strength of "everything
starts with `enc:`", which is the N02.A defect itself.

**Not executed here:** POSIX durability/locking legs; physical power-loss
behaviour is not claimed — process-kill tests are not proof of it, and the brief
says so.

## N04 — complete model validation before any write

| ID | Entry point | Baseline failure evidence | Regression node | Result |
|---|---|---|---|---|
| N04.A-1 | `ProfileStore.save` | `fingerprint=None` / list / int / str turned into "all optional fields absent" by generic attribute lookup | `test_write_contract_complete_record.py` (container cases) | VERIFIED |
| N04.B-1 | `ProfileStore.save` | `screen_width=inf` written to disk as the non-JSON token `Infinity` | `test_write_contract_complete_record.py` | VERIFIED |
| N04.B-2 | `ProfileStore.save` | `user_agent=123`, `timezone=[]`, `viewport_height=-1`, `seed`/`languages=None` unlisted by any rule | `test_write_contract_complete_record.py` | VERIFIED |
| N04.B-3 | models ↔ table | A model field with no policy was undetectable | `test_model_policy_coverage.py` — diffs `dataclasses.fields()` against `field_policy`; a new field breaks the build | VERIFIED |
| N04.B-4 | API / CLI / batch | Bad **later** batch entry; zero partial writes; revision conflicts; secret intent | `test_write_contract_boundaries.py` | VERIFIED |
| N04.B-5 | API listing + dashboard | Legacy missing/numeric/null names through list/health/search and a rendered dashboard | `test_write_contract_boundaries.py`, `dashboard/test/legacy-name-render.test.jsx` | VERIFIED |

**Found while closing N04, fixed here:** `server/stores.py:_claim_names` passed
`stored_names()` straight into `name_clash`, which puts every stored name in a
`set` — so one legacy record holding `name: ["a"]` made **every** API edit a 500,
for every profile. Both write paths now go through `write_contract.readable_names`.
The tripwire `xfail` that recorded it has been removed because the defect is
fixed, not deferred.

## Carried forward without rework

N01, N03, N05–N13, N15–N17 keep their existing regressions and are not reopened.
Impact assessment for this change set: N03's policy reads are untouched;
N15/N16's native and packaging controls are unaffected by these modules;
N01's controls sit on the launch path, which `helpers` feeds — its suites are in
the integrated run and pass.
