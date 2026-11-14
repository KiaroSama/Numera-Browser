use super::*;

fn fd_capture_control(timeout: bool) -> TestResult {
    let mut child = command("fd-capture")?;
    if timeout {
        child.arg("timeout");
    }
    let output = capture::run(&mut child, limit(Duration::from_secs(5)))?;
    assert_eq!(
        output.exit,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observation: Value = serde_json::from_slice(&output.stdout)?;
    capture_initialization_evidence(&observation);
    assert_eq!(
        observation["before"], observation["after"],
        "captured pipe handles leaked"
    );
    assert_eq!(observation["timed_out"], timeout);
    #[cfg(target_os = "linux")]
    assert_eq!(
        observation["threads_unchanged"], true,
        "capture created a surviving reader thread"
    );
    Ok(())
}
fn live_writer_control() -> TestResult {
    let t = temp("live-writer-close")?;
    let marker = t.path().join("writer.json");
    let mut child = command("fd-failure")?;
    child.arg(&marker);
    let output = capture::run(&mut child, limit(Duration::from_secs(5)));
    // Rescue only after observing the result, so fixture cleanup cannot make
    // a live writer or an unbounded local close pass the assertions.
    if output.is_err() && marker.is_file() {
        let record: Value = serde_json::from_slice(&fs::read(&marker)?)?;
        let _ = recorded_tree::settle(&record, Duration::from_secs(1));
    }
    let output = output?;
    if output.exit != 0 && marker.is_file() {
        let record: Value = serde_json::from_slice(&fs::read(&marker)?)?;
        let _ = recorded_tree::settle(&record, Duration::from_secs(1));
    }
    assert_eq!(
        output.exit,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observation: Value = serde_json::from_slice(&output.stdout)?;
    capture_initialization_evidence(&observation);
    assert_eq!(
        observation["writer_live"], true,
        "fixture killed its writer before accepting cancellation"
    );
    assert_eq!(
        observation["retained"], true,
        "failed cleanup lost the actual owner"
    );
    assert!(
        observation["elapsed_ms"].as_u64().expect("elapsed") < 650,
        "capture cleanup exceeded its budget"
    );
    assert_eq!(
        observation["before_cleanup"].as_u64(),
        Some(
            observation["before"].as_u64().expect("before")
                + observation["owner_extra"].as_u64().expect("owner handles")
        ),
        "pipe readers survived local cancellation"
    );
    assert_eq!(
        observation["before"], observation["after_cleanup"],
        "retry retained process/pipe handles"
    );
    Ok(())
}
fn held_writers(separate: bool, timed_out: bool) -> TestResult {
    use numera_core::ownership::capture_units::Units;
    let t = temp("held-inherited-writers")?;
    let mut child = command("reported-tree")?;
    child
        .arg(t.path())
        .arg(if separate { "separate" } else { "same" })
        .arg(if timed_out { "hold" } else { "exit" });
    let mut units = Units::new(t.path(), &mut child)?;
    let wall = if timed_out {
        Duration::from_millis(500)
    } else {
        Duration::from_secs(4)
    };
    let begin = Instant::now();
    let answer = units.run(&mut child, limit(wall), || false);
    let elapsed = begin.elapsed();
    // A separately owned descendant must have been reported by a real trusted
    // spawn. Its identified process is observed only after capture returned.
    let artifact = t.path().join("descendant.json");
    assert!(
        artifact.is_file(),
        "fixture never spawned the relevant descendant"
    );
    let descendant: Value = serde_json::from_slice(&fs::read(&artifact)?)?;
    let pid = u32::try_from(descendant["pid"].as_u64().expect("descendant pid"))?;
    let end = Instant::now() + Duration::from_secs(1);
    while process_running(pid, descendant["pidToken"].as_u64())? {
        assert!(
            Instant::now() < end,
            "identified inherited writer survived capture"
        );
        thread::sleep(Duration::from_millis(5));
    }
    units.settle(Duration::from_secs(1))?;
    if timed_out {
        assert_eq!(
            answer.err().ok_or("wedged leader returned success")?.kind(),
            io::ErrorKind::TimedOut
        );
    } else {
        let output = answer?;
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, b"TAIL");
    }
    assert!(
        elapsed < wall + Duration::from_secs(2),
        "capture exceeded original wall plus cleanup grace: {elapsed:?}"
    );
    assert!(
        !units.pending(),
        "settled capture receipt scope was not released"
    );
    Ok(())
}

source_test!(
    helper_lifecycle_a_timed_out_helper_closes_its_pipes,
    "tests/test_helper_lifecycle.py::test_a_timed_out_helper_closes_its_pipes",
    { fd_capture_control(true) }
);
source_test!(
    helper_lifecycle_a_helper_that_finishes_closes_its_pipes,
    "tests/test_helper_lifecycle.py::test_a_helper_that_finishes_closes_its_pipes",
    { fd_capture_control(false) }
);
source_test!(
    helper_bounded_lifetime_a_reader_that_ended_is_still_closed,
    "tests/test_helper_bounded_lifetime.py::test_a_reader_that_ended_is_still_closed",
    { fd_capture_control(false) }
);
source_test!(
    helper_ownership_no_reader_thread_outlives_the_call,
    "tests/test_helper_ownership.py::test_no_reader_thread_outlives_the_call",
    { fd_capture_control(false) }
);
source_test!(
    helper_ownership_a_closed_writer_lets_the_reader_finish,
    "tests/test_helper_ownership.py::test_a_closed_writer_lets_the_reader_finish",
    { fd_capture_control(false) }
);
source_test!(
    helper_bounded_lifetime_closing_does_not_block_on_a_reader_holding_a_live_writer,
    "tests/test_helper_bounded_lifetime.py::test_closing_does_not_block_on_a_reader_holding_a_live_writer",
    { live_writer_control() }
);
source_test!(
    helper_completion_regression_stream_close_cancels_readers_without_waiting_for_live_writers,
    "tests/test_helper_completion_regression.py::test_stream_close_cancels_readers_without_waiting_for_live_writers",
    { live_writer_control() }
);
source_test!(
    helper_bounded_lifetime_an_unsettled_helper_records_the_readers_it_retained,
    "tests/test_helper_bounded_lifetime.py::test_an_unsettled_helper_records_the_readers_it_retained",
    { live_writer_control() }
);
source_test!(
    helper_terminal_state_a_helper_that_exits_leaving_a_child_still_has_its_tree_ended,
    "tests/test_helper_terminal_state.py::test_a_helper_that_exits_leaving_a_child_still_has_its_tree_ended",
    { held_writers(false, false) }
);
source_test!(
    helper_terminal_state_a_grandchild_holding_the_pipes_does_not_hold_the_result,
    "tests/test_helper_terminal_state.py::test_a_grandchild_holding_the_pipes_does_not_hold_the_result",
    { held_writers(false, false) }
);
source_test!(
    helper_ownership_a_grandchild_in_the_same_group_is_the_control,
    "tests/test_helper_ownership.py::test_a_grandchild_in_the_same_group_is_the_control",
    { held_writers(false, false) }
);
source_test!(
    helper_ownership_a_driver_holding_both_writers_does_not_truncate_the_answer,
    "tests/test_helper_ownership.py::test_a_driver_holding_both_writers_does_not_truncate_the_answer",
    { held_writers(true, false) }
);
source_test!(
    helper_ownership_the_driver_is_gone_according_to_the_operating_system,
    "tests/test_helper_ownership.py::test_the_driver_is_gone_according_to_the_operating_system",
    { held_writers(true, false) }
);
source_test!(
    helper_terminal_state_a_driver_in_its_own_session_is_ended_too,
    "tests/test_helper_terminal_state.py::test_a_driver_in_its_own_session_is_ended_too",
    { held_writers(true, false) }
);
source_test!(
    helper_lifecycle_matrix_same_or_separate_tree_holding_writers_is_ended_bfb2d63a,
    "tests/test_helper_lifecycle_matrix.py::test_same_or_separate_tree_holding_writers_is_ended[False-False]",
    { held_writers(false, false) }
);
source_test!(
    helper_lifecycle_matrix_same_or_separate_tree_holding_writers_is_ended_62db4bf4,
    "tests/test_helper_lifecycle_matrix.py::test_same_or_separate_tree_holding_writers_is_ended[False-True]",
    { held_writers(true, false) }
);
source_test!(
    helper_lifecycle_matrix_same_or_separate_tree_holding_writers_is_ended_5ae3e90d,
    "tests/test_helper_lifecycle_matrix.py::test_same_or_separate_tree_holding_writers_is_ended[True-False]",
    { held_writers(false, true) }
);
source_test!(
    helper_lifecycle_matrix_same_or_separate_tree_holding_writers_is_ended_a58f1d99,
    "tests/test_helper_lifecycle_matrix.py::test_same_or_separate_tree_holding_writers_is_ended[True-True]",
    { held_writers(true, true) }
);
