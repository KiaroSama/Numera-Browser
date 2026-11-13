use super::*;
source_test!(
    c000_the_destination_check_itself_fails_closed,
    "tests/test_rotate_legacy_endpoint.py::test_the_destination_check_itself_fails_closed[http://example.test:99999/rotate]",
    {
        assert!(
            rotation::testing::public_destination(
                "http://example.test:99999/rotate",
                &["8.8.8.8".parse().expect("public address")]
            )
            .is_err()
        );
    }
);
source_test!(
    c001_the_destination_check_itself_fails_closed,
    "tests/test_rotate_legacy_endpoint.py::test_the_destination_check_itself_fails_closed[http://[oops/rotate]",
    {
        assert!(
            rotation::testing::public_destination(
                "http://[oops/rotate",
                &["8.8.8.8".parse().expect("public address")]
            )
            .is_err()
        );
    }
);
source_test!(
    c000_the_api_answers_with_the_problem_and_no_traceback,
    "tests/test_rotate_legacy_endpoint.py::test_the_api_answers_with_the_problem_and_no_traceback[http://example.test:99999/rotate]",
    {
        legacy_rotate("http://example.test:99999/rotate", "http");
    }
);
source_test!(
    c001_the_api_answers_with_the_problem_and_no_traceback,
    "tests/test_rotate_legacy_endpoint.py::test_the_api_answers_with_the_problem_and_no_traceback[http://[oops/rotate]",
    {
        legacy_rotate("http://[oops/rotate", "http");
    }
);
source_test!(
    c000_the_cli_reports_it_and_exits_nonzero,
    "tests/test_rotate_legacy_endpoint.py::test_the_cli_reports_it_and_exits_nonzero[http://example.test:99999/rotate]",
    {
        legacy_rotate("http://example.test:99999/rotate", "cli");
    }
);
source_test!(
    c001_the_cli_reports_it_and_exits_nonzero,
    "tests/test_rotate_legacy_endpoint.py::test_the_cli_reports_it_and_exits_nonzero[http://[oops/rotate]",
    {
        legacy_rotate("http://[oops/rotate", "cli");
    }
);
