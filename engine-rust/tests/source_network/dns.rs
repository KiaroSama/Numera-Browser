use super::*;
source_test!(
    c000_no_proxy_country_is_baseline_only,
    "tests/test_dns.py::test_no_proxy_country_is_baseline_only",
    {
        let result = dns_grade(vec![dns_row("US"), dns_row("US")], None);
        assert_eq!(result["ok"], true);
        assert_eq!(result["countries"], json!(["US"]));
    }
);
source_test!(
    c000_all_resolvers_in_proxy_country_passes,
    "tests/test_dns.py::test_all_resolvers_in_proxy_country_passes",
    {
        let result = dns_grade(vec![dns_row("DE"), dns_row("DE")], Some("DE"));
        assert_eq!(result["ok"], true);
        assert!(
            result["checks"]
                .as_array()
                .expect("checks")
                .iter()
                .any(|c| c["name"]
                    .as_str()
                    .expect("check name")
                    .contains("no ISP leak"))
        );
    }
);
source_test!(
    c000_resolver_outside_proxy_country_is_a_leak,
    "tests/test_dns.py::test_resolver_outside_proxy_country_is_a_leak",
    {
        let result = dns_grade(vec![dns_row("DE"), dns_row("US")], Some("DE"));
        assert_eq!(result["ok"], false);
        let leak = result["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .find(|c| {
                c["name"]
                    .as_str()
                    .expect("check name")
                    .contains("proxy country")
            })
            .expect("country check");
        assert_eq!(leak["ok"], false);
        assert!(leak["detail"].as_str().expect("detail").contains("US"));
    }
);
source_test!(
    c000_ignores_non_resolver_rows,
    "tests/test_dns.py::test_ignores_non_resolver_rows",
    {
        let result = dns_grade(
            vec![
                dns_row("DE"),
                json!({"type":"ip","country_code":"DE"}),
                json!({"type":"conclusion","ip":"|"}),
            ],
            Some("DE"),
        );
        assert_eq!(result["resolvers"].as_array().expect("resolvers").len(), 1);
        assert_eq!(result["ok"], true);
    }
);
source_test!(
    c000_no_resolvers_observed_fails_the_first_check,
    "tests/test_dns.py::test_no_resolvers_observed_fails_the_first_check",
    {
        let result = dns_grade(vec![json!({"type":"ip","country_code":"US"})], Some("US"));
        assert_eq!(result["ok"], false);
        assert_eq!(result["checks"][0]["ok"], false);
    }
);
