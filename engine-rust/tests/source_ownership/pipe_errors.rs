use super::*;

fn pipe_failure(stage: &str) -> TestResult {
    let mut child = command("fd-pipe-failure")?;
    child.arg(stage);
    let output = capture::run(&mut child, limit(Duration::from_secs(5)))?;
    assert_eq!(
        output.exit,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Value = serde_json::from_slice(&output.stdout)?;
    capture_initialization_evidence(&observed);
    assert_eq!(
        observed["before"], observed["after"],
        "failure left a pipe or process handle open"
    );
    assert_eq!(
        observed["child_live"], false,
        "failure lost its actual spawned process"
    );
    assert!(observed["elapsed_ms"].as_u64().expect("elapsed") < 2000);
    assert!(
        observed["error"]
            .as_str()
            .is_some_and(|message| message.contains("controlled"))
    );
    if stage == "prepare" {
        assert_eq!(
            observed["prepared"], 2,
            "second-pipe fault did not follow a real first preparation"
        );
        assert_eq!(observed["reads"], 0);
    }
    if stage == "read" {
        assert_eq!(observed["prepared"], 2);
        assert!(observed["reads"].as_u64().expect("read calls") >= 2);
        assert!(
            observed["error"]
                .as_str()
                .is_some_and(|message| message.contains("stderr read failure"))
        );
    }
    if stage == "spawned" {
        assert_eq!(observed["prepared"], 0);
    }
    Ok(())
}

source_test!(
    helper_completion_regression_partial_reader_start_unwinds_every_started_reader,
    "tests/test_helper_completion_regression.py::test_partial_reader_start_unwinds_every_started_reader",
    { pipe_failure("prepare") }
);
source_test!(
    helper_lifecycle_matrix_partial_start_failure_inside_real_helper_is_joined,
    "tests/test_helper_lifecycle_matrix.py::test_partial_start_failure_inside_real_helper_is_joined",
    { pipe_failure("prepare") }
);
source_test!(
    helper_bounded_lifetime_a_failure_before_attach_still_owns_the_spawned_child,
    "tests/test_helper_bounded_lifetime.py::test_a_failure_before_attach_still_owns_the_spawned_child",
    { pipe_failure("prepare") }
);
source_test!(
    helper_admission_an_attach_failure_does_not_lose_the_process_it_created,
    "tests/test_helper_admission.py::test_an_attach_failure_does_not_lose_the_process_it_created",
    { pipe_failure("prepare") }
);
source_test!(
    helper_retry_completion_genuine_read_error_cannot_be_cleared_by_repeated_settlement,
    "tests/test_helper_retry_completion.py::test_genuine_read_error_cannot_be_cleared_by_repeated_settlement",
    { pipe_failure("read") }
);
source_test!(
    helper_completion_regression_output_failure_is_retained_as_a_terminal_diagnostic,
    "tests/test_helper_completion_regression.py::test_output_failure_is_retained_as_a_terminal_diagnostic",
    { pipe_failure("read") }
);
source_test!(
    helper_completion_regression_failure_before_binding_does_not_lose_the_spawned_child_b0ad2485,
    "tests/test_helper_completion_regression.py::test_failure_before_binding_does_not_lose_the_spawned_child[adopt]",
    { pipe_failure("spawned") }
);
source_test!(
    helper_completion_regression_failure_before_binding_does_not_lose_the_spawned_child_a9190076,
    "tests/test_helper_completion_regression.py::test_failure_before_binding_does_not_lose_the_spawned_child[attach]",
    { pipe_failure("prepare") }
);
