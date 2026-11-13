use super::*;
source_test!(
    c000_a_socks_proxy_is_no_longer_reported_as_untestable,
    "tests/test_proxy_check_socks.py::test_a_socks_proxy_is_no_longer_reported_as_untestable",
    {
        socks_health("verdict", false);
    }
);
source_test!(
    c000_a_wrong_socks_password_fails_and_says_so,
    "tests/test_proxy_check_socks.py::test_a_wrong_socks_password_fails_and_says_so",
    {
        let result = failed_socks_check(true, false, true);
        assert_eq!(result["ok"], false);
        let error = result["error"].as_str().expect("error").to_lowercase();
        assert!(error.contains("credential"), "{error}");
        assert!(!error.contains("502"));
    }
);
source_test!(
    c000_a_socks_upstream_that_refuses_the_target_fails,
    "tests/test_proxy_check_socks.py::test_a_socks_upstream_that_refuses_the_target_fails",
    {
        let result = failed_socks_check(false, true, false);
        assert_eq!(result["ok"], false);
        assert!(result["error"].as_str().is_some_and(|e| !e.is_empty()));
    }
);
source_test!(
    c000_a_dead_socks_port_fails_rather_than_hanging,
    "tests/test_proxy_check_socks.py::test_a_dead_socks_port_fails_rather_than_hanging",
    {
        dead_socks_check("SOCKS5");
    }
);
source_test!(
    c000_a_dead_socks4_proxy_fails_rather_than_shrugging,
    "tests/test_proxy_check_socks.py::test_a_dead_socks4_proxy_fails_rather_than_shrugging",
    {
        dead_socks_check("SOCKS4");
    }
);
source_test!(
    c000_the_check_routes_through_the_socks_server,
    "tests/test_proxy_check_socks.py::test_the_check_routes_through_the_socks_server[False]",
    {
        socks_health("route", false);
    }
);
source_test!(
    c001_the_check_routes_through_the_socks_server,
    "tests/test_proxy_check_socks.py::test_the_check_routes_through_the_socks_server[True]",
    {
        socks_health("route", true);
    }
);
