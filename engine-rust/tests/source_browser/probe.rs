use super::*;
fn leaks(ips: &[&str], exit: Option<&str>) -> Value {
    browser::network_grades::leaks(&json!({"supported":true,"ips":ips}), exit)
        .expect("browser contract")
}
fn chrome() -> Value {
    json!({"ja3":"771,4865-4866-4867,0-23-65281,29-23,0","ja3Hash":"abc123","ja4":"t13d1516h2_8daaf6152771_d8a2da3f94cd","http2":"hash","grease":true,"greaseKnown":true,"cipherCount":15})
}
fn tls(v: &Value) -> Value {
    browser::probe_grades::tls(v).expect("browser contract")
}
fn bad(v: &Value, name: &str) {
    assert!(
        v["checks"]
            .as_array()
            .expect("browser contract")
            .iter()
            .any(
                |c| c["name"].as_str().expect("browser contract").contains(name)
                    && c["ok"] == false
            )
    );
}
source_test!(
    test_private_ipv4_ranges,
    "tests/test_probe.py::test_private_ipv4_ranges",
    {
        for ip in [
            "10.0.0.1",
            "192.168.1.5",
            "172.16.4.4",
            "172.31.255.1",
            "127.0.0.1",
            "169.254.10.1",
        ] {
            assert_eq!(leaks(&[ip], None)["exposed"], json!([]), "{ip}");
        }
    }
);
source_test!(
    test_public_ipv4_is_not_private,
    "tests/test_probe.py::test_public_ipv4_is_not_private",
    {
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",
            "192.169.0.1",
            "203.0.113.9",
        ] {
            assert_eq!(leaks(&[ip], None)["exposed"], json!([ip]), "{ip}");
        }
    }
);
source_test!(
    test_private_ipv6,
    "tests/test_probe.py::test_private_ipv6",
    {
        for ip in ["fd12::1", "fe80::1", "::1"] {
            assert_eq!(leaks(&[ip], None)["exposed"], json!([]));
        }
    }
);
source_test!(
    test_public_ipv6_is_not_private,
    "tests/test_probe.py::test_public_ipv6_is_not_private",
    {
        assert_eq!(
            leaks(&["2001:4860:4860::8888"], None)["exposed"],
            json!(["2001:4860:4860::8888"])
        );
    }
);
source_test!(
    test_garbage_is_treated_as_private,
    "tests/test_probe.py::test_garbage_is_treated_as_private",
    {
        for ip in ["not-an-ip", "999.1.1"] {
            assert_eq!(leaks(&[ip], None)["exposed"], json!([]));
        }
    }
);
source_test!(
    test_tls_real_chromium_passes_every_check,
    "tests/test_probe.py::test_tls_real_chromium_passes_every_check",
    {
        let r = tls(&chrome());
        assert_eq!(r["ok"], true);
        assert!(
            r["ja4"]
                .as_str()
                .expect("browser contract")
                .starts_with("t13")
        );
        assert!(
            r["checks"]
                .as_array()
                .expect("browser contract")
                .iter()
                .all(|c| c["ok"] == true)
        );
    }
);
source_test!(
    test_tls_flags_a_non_tls13_handshake,
    "tests/test_probe.py::test_tls_flags_a_non_tls13_handshake",
    {
        let mut v = chrome();
        v["ja4"] = json!("t12d0810h1_aaaa_bbbb");
        let r = tls(&v);
        assert_eq!(r["ok"], false);
        bad(&r, "TLS 1.3");
    }
);
source_test!(
    test_tls_flags_missing_http2,
    "tests/test_probe.py::test_tls_flags_missing_http2",
    {
        let mut v = chrome();
        v["ja4"] = json!("t13d1516_aaaa_bbbb");
        bad(&tls(&v), "HTTP/2");
    }
);
source_test!(
    test_tls_flags_missing_grease_when_ciphers_were_exposed,
    "tests/test_probe.py::test_tls_flags_missing_grease_when_ciphers_were_exposed",
    {
        let mut v = chrome();
        v["grease"] = json!(false);
        bad(&tls(&v), "GREASE");
    }
);
source_test!(
    test_tls_does_not_penalise_grease_when_service_hides_ciphers,
    "tests/test_probe.py::test_tls_does_not_penalise_grease_when_service_hides_ciphers",
    {
        let mut v = chrome();
        v["grease"] = json!(false);
        v["greaseKnown"] = json!(false);
        let r = tls(&v);
        assert!(
            !r["checks"]
                .as_array()
                .expect("browser contract")
                .iter()
                .any(|c| c["name"]
                    .as_str()
                    .expect("browser contract")
                    .contains("GREASE"))
        );
        assert!(r["grease"].is_null());
    }
);
source_test!(
    test_tls_reports_error_when_no_service_answers,
    "tests/test_probe.py::test_tls_reports_error_when_no_service_answers",
    {
        let r = tls(&Value::Null);
        assert_eq!(r["ok"], false);
        assert!(r.get("error").is_some());
    }
);
source_test!(
    test_private_addresses_are_not_a_leak,
    "tests/test_probe.py::test_private_addresses_are_not_a_leak",
    {
        let r = leaks(&["192.168.1.7", "10.0.0.2"], None);
        assert_eq!(r["exposed"], json!([]));
        assert_eq!(r["checks"][0]["ok"], true);
    }
);
source_test!(
    test_public_address_that_is_not_the_exit_ip_is_a_leak,
    "tests/test_probe.py::test_public_address_that_is_not_the_exit_ip_is_a_leak",
    {
        let r = leaks(&["203.0.113.9"], Some("8.8.8.8"));
        assert_eq!(r["exposed"], json!(["203.0.113.9"]));
        assert_eq!(r["checks"][0]["ok"], false);
    }
);
source_test!(
    test_the_exit_ip_itself_is_not_a_leak,
    "tests/test_probe.py::test_the_exit_ip_itself_is_not_a_leak",
    {
        let r = leaks(&["8.8.8.8"], Some("8.8.8.8"));
        assert_eq!(r["exposed"], json!([]));
        assert_eq!(r["checks"][0]["ok"], true);
    }
);
