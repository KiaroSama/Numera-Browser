use super::*;

fn receipt_failure(damage: &str) -> TestResult {
    let t = temp("receipt-failure-worker")?;
    let mut child = command("receipt-run")?;
    child
        .arg(t.path())
        .arg(damage)
        .env_remove(unit_report::ENV)
        .env_remove(unit_report::SCOPE_ENV);
    let begin = Instant::now();
    let answer = capture::run(&mut child, limit(Duration::from_secs(5)));
    let elapsed = begin.elapsed();
    // Independent artifact permits fixture rescue if the worker regresses.
    // Acceptance records were written before any such teardown can run.
    let record = t.path().join("escaped.json");
    if record.is_file() {
        let value: Value = serde_json::from_slice(&fs::read(&record)?)?;
        let _ = recorded_tree::settle(&value, Duration::from_secs(1));
    }
    let output = answer?;
    assert_eq!(
        output.exit,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Value = serde_json::from_slice(&output.stdout)?;
    handle_attribution_evidence(&observed);
    assert!(
        elapsed < Duration::from_secs(5),
        "worker required supervisor rescue"
    );
    if damage == "create" {
        assert_eq!(observed["refused"], true);
        assert_eq!(observed["spawned"], false);
        return Ok(());
    }
    assert_eq!(
        observed["failed"], true,
        "unverified ownership returned an ordinary result"
    );
    assert_eq!(
        observed["after_retry_pending"], false,
        "explicit fixture retry did not release the receipt"
    );
    assert!(
        observed["elapsed_ms"].as_u64().expect("elapsed") < 1400,
        "failed settlement waited on unprovable EOF before local cancellation"
    );
    if damage == "spawn" {
        assert_eq!(observed["pending"], false);
        return Ok(());
    }
    assert_eq!(
        observed["before_retry_pending"], true,
        "failed receipt evidence was silently released"
    );
    if damage != "leader" {
        assert_eq!(
            observed["before"], observed["after"],
            "failed settlement retained local pipe handles"
        );
    }
    if damage == "leader" {
        assert_eq!(
            observed["producer_live"], true,
            "fixture ended the producer before acceptance"
        );
        assert_eq!(
            observed["retained_tree"], true,
            "uncertain live producer lost its original owner"
        );
        assert_eq!(
            observed["receipt_exists"], true,
            "zero units were accepted while the producer could still create work"
        );
        let diagnostic = observed["diagnostic"].as_str().ok_or("diagnostic absent")?;
        assert!(
            diagnostic.contains("stdout_eof=true") && diagnostic.contains("stderr_eof=true"),
            "real EOF control did not reach both pipe readers: {diagnostic}"
        );
    }
    if damage == "remove" {
        assert_eq!(
            observed["receipt_exists"], true,
            "failed unlink discarded the owned receipt"
        );
    }
    Ok(())
}

source_test!(
    helper_admission_a_report_file_that_cannot_be_created_starts_nothing,
    "tests/test_helper_admission.py::test_a_report_file_that_cannot_be_created_starts_nothing",
    { receipt_failure("create") }
);
source_test!(
    helper_lifecycle_matrix_report_failure_precedes_any_driver_spawn,
    "tests/test_helper_lifecycle_matrix.py::test_report_failure_precedes_any_driver_spawn",
    { receipt_failure("create") }
);
source_test!(
    helper_ownership_a_report_that_cannot_be_written_ends_the_launch,
    "tests/test_helper_ownership.py::test_a_report_that_cannot_be_written_ends_the_launch",
    { receipt_failure("create") }
);
source_test!(
    helper_admission_a_spawn_failure_leaves_no_reservation,
    "tests/test_helper_admission.py::test_a_spawn_failure_leaves_no_reservation",
    { receipt_failure("spawn") }
);
source_test!(
    helper_lifecycle_matrix_failed_report_cleanup_is_not_released,
    "tests/test_helper_lifecycle_matrix.py::test_failed_report_cleanup_is_not_released",
    { receipt_failure("remove") }
);
source_test!(
    helper_ownership_a_damaged_report_strands_the_helper_rather_than_clearing_it,
    "tests/test_helper_ownership.py::test_a_damaged_report_strands_the_helper_rather_than_clearing_it",
    { receipt_failure("malformed") }
);
source_test!(
    helper_terminal_state_a_missing_report_is_not_a_helper_that_owned_nothing,
    "tests/test_helper_terminal_state.py::test_a_missing_report_is_not_a_helper_that_owned_nothing",
    { receipt_failure("missing") }
);
source_test!(helper_completion_regression_worker_exits_after_unresolved_report_without_supervisor_rescue_ffa63583,"tests/test_helper_completion_regression.py::test_worker_exits_after_unresolved_report_without_supervisor_rescue[missing]",{
receipt_failure("missing")
});
source_test!(helper_completion_regression_worker_exits_after_unresolved_report_without_supervisor_rescue_60ec9bb7,"tests/test_helper_completion_regression.py::test_worker_exits_after_unresolved_report_without_supervisor_rescue[malformed]",{
receipt_failure("malformed")
});
source_test!(helper_completion_regression_worker_exits_after_unresolved_report_without_supervisor_rescue_941b7ecd,"tests/test_helper_completion_regression.py::test_worker_exits_after_unresolved_report_without_supervisor_rescue[utf8]",{
receipt_failure("utf8")
});
source_test!(
    helper_completion_regression_failed_settlement_preserves_cancellation_budget_ffa63583,
    "tests/test_helper_completion_regression.py::test_failed_settlement_preserves_cancellation_budget[missing]",
    { receipt_failure("missing") }
);
source_test!(
    helper_completion_regression_failed_settlement_preserves_cancellation_budget_60ec9bb7,
    "tests/test_helper_completion_regression.py::test_failed_settlement_preserves_cancellation_budget[malformed]",
    { receipt_failure("malformed") }
);
source_test!(
    helper_completion_regression_failed_settlement_preserves_cancellation_budget_941b7ecd,
    "tests/test_helper_completion_regression.py::test_failed_settlement_preserves_cancellation_budget[utf8]",
    { receipt_failure("utf8") }
);
source_test!(
    helper_completion_regression_failed_settlement_preserves_cancellation_budget_dc9c5edb,
    "tests/test_helper_completion_regression.py::test_failed_settlement_preserves_cancellation_budget[tree]",
    { receipt_failure("tree") }
);
source_test!(
    helper_completion_regression_failed_settlement_preserves_cancellation_budget_4d5b6609,
    "tests/test_helper_completion_regression.py::test_failed_settlement_preserves_cancellation_budget[leader]",
    { receipt_failure("leader") }
);
