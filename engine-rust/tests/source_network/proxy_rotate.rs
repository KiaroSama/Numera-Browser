use super::*;
source_test!(
    c000_no_url_is_a_clear_refusal_not_a_silent_no_op,
    "tests/test_proxy_rotate.py::test_no_url_is_a_clear_refusal_not_a_silent_no_op",
    {
        let res = runtime(rotation::rotate(
            &rotation_entry(None, None),
            Duration::from_secs(2),
        ));
        assert_eq!(res["rotated"], false);
        assert!(
            res["error"]
                .as_str()
                .expect("refusal")
                .contains("change-IP URL")
        );
    }
);
source_test!(
    c000_a_non_http_url_is_refused_before_anything_is_fetched,
    "tests/test_proxy_rotate.py::test_a_non_http_url_is_refused_before_anything_is_fetched",
    {
        let res = runtime(rotation::rotate(
            &rotation_entry(Some("ftp://example.test/rotate"), None),
            Duration::from_secs(2),
        ));
        assert_eq!(res["rotated"], false);
        assert!(res["error"].as_str().expect("refusal").contains("http(s)"));
    }
);
source_test!(
    c000_the_cooldown_refuses_instead_of_returning_the_same_ip,
    "tests/test_proxy_rotate.py::test_the_cooldown_refuses_instead_of_returning_the_same_ip",
    {
        let endpoint = Site::body(Some(b"OK".to_vec()));
        let mut p = rotation_entry(Some(&endpoint.url("/rotate")), None);
        p["rotatedAt"] = json!(998.0);
        let res = runtime(rotation::testing::rotate_at(
            &p,
            1000.0,
            json!({"ok":true,"ip":"9.9.9.9"}),
            Duration::from_secs(2),
        ));
        assert_eq!(res["rotated"], false);
        assert!(
            res["error"]
                .as_str()
                .expect("cooldown")
                .contains("rate-limit")
        );
        assert!(endpoint.seen.lock().expect("endpoint hits").is_empty());
    }
);
source_test!(
    c000_a_dead_endpoint_is_reported_and_the_proxy_is_not_marked_rotated,
    "tests/test_proxy_rotate.py::test_a_dead_endpoint_is_reported_and_the_proxy_is_not_marked_rotated",
    {
        let closed = TcpListener::bind("127.0.0.1:0").expect("owned dead endpoint");
        let address = closed.local_addr().expect("address");
        drop(closed);
        let p = rotation_entry(Some(&format!("http://{address}/rotate")), None);
        let res = runtime(rotation::testing::rotate_at(
            &p,
            1000.0,
            json!({"ok":true,"ip":"9.9.9.9"}),
            Duration::from_secs(2),
        ));
        assert_eq!(res["rotated"], false);
        assert_eq!(res["ok"], false);
        assert!(res["error"].as_str().is_some_and(|s| !s.is_empty()));
    }
);
source_test!(
    c000_an_endpoint_that_answers_but_does_not_move_the_ip_is_not_a_success,
    "tests/test_proxy_rotate.py::test_an_endpoint_that_answers_but_does_not_move_the_ip_is_not_a_success",
    {
        let endpoint = Site::body(Some(b"rotation queued".to_vec()));
        let p = rotation_entry(Some(&endpoint.url("/rotate")), Some("5.5.5.5"));
        let res = runtime(rotation::testing::rotate_at(
            &p,
            1000.0,
            json!({"ok":true,"ip":"5.5.5.5"}),
            Duration::from_secs(2),
        ));
        assert_eq!(endpoint.seen.lock().expect("endpoint hits").len(), 1);
        assert_eq!(res["rotated"], false);
        let error = res["error"].as_str().expect("unchanged IP reason");
        assert!(error.contains("still 5.5.5.5"));
        assert!(error.contains("rotation queued"));
    }
);
source_test!(
    c000_a_real_move_is_reported_with_both_addresses,
    "tests/test_proxy_rotate.py::test_a_real_move_is_reported_with_both_addresses",
    {
        let endpoint = Site::body(Some(b"OK".to_vec()));
        let p = rotation_entry(Some(&endpoint.url("/rotate")), Some("5.5.5.5"));
        let res = runtime(rotation::testing::rotate_at(
            &p,
            1000.0,
            json!({"ok":true,"ip":"9.9.9.9"}),
            Duration::from_secs(2),
        ));
        assert_eq!(res["rotated"], true);
        assert_eq!(res["previousIp"], "5.5.5.5");
        assert_eq!(res["ip"], "9.9.9.9");
        assert!(res.get("error").is_none_or(|v| v.is_null() || v == ""));
    }
);
source_test!(
    c000_a_first_rotate_with_no_previous_ip_counts_as_moved,
    "tests/test_proxy_rotate.py::test_a_first_rotate_with_no_previous_ip_counts_as_moved",
    {
        let endpoint = Site::body(Some(b"OK".to_vec()));
        let p = rotation_entry(Some(&endpoint.url("/rotate")), None);
        let res = runtime(rotation::testing::rotate_at(
            &p,
            1000.0,
            json!({"ok":true,"ip":"9.9.9.9"}),
            Duration::from_secs(2),
        ));
        assert_eq!(res["rotated"], true);
    }
);
source_test!(
    c000_the_endpoint_is_fetched_directly_not_through_the_proxy,
    "tests/test_proxy_rotate.py::test_the_endpoint_is_fetched_directly_not_through_the_proxy",
    {
        let endpoint = Site::body(Some(b"OK".to_vec()));
        let p = rotation_entry(Some(&endpoint.url("/rotate")), None);
        assert_eq!(p["host"], "127.0.0.1");
        assert_eq!(p["port"], 1);
        let res = runtime(rotation::testing::rotate_at(
            &p,
            1000.0,
            json!({"ok":true,"ip":"9.9.9.9"}),
            Duration::from_secs(2),
        ));
        assert_eq!(endpoint.seen.lock().expect("direct provider hits").len(), 1);
        assert_eq!(res["rotated"], true);
    }
);
source_test!(
    c000_the_protocol_survives_a_round_trip,
    "tests/test_proxy_rotate.py::test_the_protocol_survives_a_round_trip[HTTP]",
    {
        let p=conversion::to_engine(&json!({"name":"x","os":"Windows","proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080"}}),1000.0).expect("proxy profile");
        assert!(
            p.proxy
                .as_ref()
                .expect("proxy")
                .server
                .starts_with(&format!("{}://", "HTTP".to_lowercase()))
        );
        assert_eq!(conversion::to_dashboard(&p)["proxy"]["type"], "HTTP");
    }
);
source_test!(
    c001_the_protocol_survives_a_round_trip,
    "tests/test_proxy_rotate.py::test_the_protocol_survives_a_round_trip[HTTPS]",
    {
        let p=conversion::to_engine(&json!({"name":"x","os":"Windows","proxy":{"type":"HTTPS","host":"1.2.3.4","port":"8080"}}),1000.0).expect("proxy profile");
        assert!(
            p.proxy
                .as_ref()
                .expect("proxy")
                .server
                .starts_with(&format!("{}://", "HTTPS".to_lowercase()))
        );
        assert_eq!(conversion::to_dashboard(&p)["proxy"]["type"], "HTTPS");
    }
);
source_test!(
    c002_the_protocol_survives_a_round_trip,
    "tests/test_proxy_rotate.py::test_the_protocol_survives_a_round_trip[SOCKS5]",
    {
        let p=conversion::to_engine(&json!({"name":"x","os":"Windows","proxy":{"type":"SOCKS5","host":"1.2.3.4","port":"8080"}}),1000.0).expect("proxy profile");
        assert!(
            p.proxy
                .as_ref()
                .expect("proxy")
                .server
                .starts_with(&format!("{}://", "SOCKS5".to_lowercase()))
        );
        assert_eq!(conversion::to_dashboard(&p)["proxy"]["type"], "SOCKS5");
    }
);
source_test!(
    c003_the_protocol_survives_a_round_trip,
    "tests/test_proxy_rotate.py::test_the_protocol_survives_a_round_trip[SOCKS4]",
    {
        let p=conversion::to_engine(&json!({"name":"x","os":"Windows","proxy":{"type":"SOCKS4","host":"1.2.3.4","port":"8080"}}),1000.0).expect("proxy profile");
        assert!(
            p.proxy
                .as_ref()
                .expect("proxy")
                .server
                .starts_with(&format!("{}://", "SOCKS4".to_lowercase()))
        );
        assert_eq!(conversion::to_dashboard(&p)["proxy"]["type"], "SOCKS4");
    }
);
source_test!(
    c000_an_unsupported_protocol_is_refused_not_downgraded,
    "tests/test_proxy_rotate.py::test_an_unsupported_protocol_is_refused_not_downgraded",
    {
        assert!(pool::server_url("QUIC", "1.2.3.4", "8080").is_err());
    }
);
source_test!(
    c000_ipv6_survives_both_directions,
    "tests/test_proxy_rotate.py::test_ipv6_survives_both_directions",
    {
        let p=conversion::to_engine(&json!({"name":"x","os":"Windows","proxy":{"type":"SOCKS5","host":"2001:db8::1","port":"1080"}}),1000.0).expect("IPv6 proxy");
        assert_eq!(
            p.proxy.as_ref().expect("proxy").server,
            "socks5://[2001:db8::1]:1080"
        );
        let back = conversion::to_dashboard(&p);
        assert_eq!(back["proxy"]["host"], "2001:db8::1");
        assert_eq!(back["proxy"]["port"], "1080");
    }
);
source_test!(
    c000_credential_variants_are_different_proxies,
    "tests/test_proxy_rotate.py::test_credential_variants_are_different_proxies",
    {
        let mut identities = BTreeSet::new();
        for (user, password) in [("session-a", "x"), ("session-b", "x"), ("session-a", "y")] {
            identities.insert(serde_json::to_string(&pool::identity(&json!({"type":"HTTP","host":"gw.vendor.test","port":"8000","user":user,"pass":password}))).expect("proxy identity"));
        }
        assert_eq!(identities.len(), 3);
    }
);
source_test!(
    c000_split_server_understands_ipv6,
    "tests/test_proxy_rotate.py::test_split_server_understands_ipv6[http://1.2.3.4:8080-want0]",
    {
        split_server("http://1.2.3.4:8080", ("http", "1.2.3.4", "8080"));
    }
);
source_test!(
    c001_split_server_understands_ipv6,
    "tests/test_proxy_rotate.py::test_split_server_understands_ipv6[socks5://[2001:db8::1]:1080-want1]",
    {
        split_server(
            "socks5://[2001:db8::1]:1080",
            ("socks5", "2001:db8::1", "1080"),
        );
    }
);
source_test!(
    c002_split_server_understands_ipv6,
    "tests/test_proxy_rotate.py::test_split_server_understands_ipv6[1.2.3.4:3128-want2]",
    {
        split_server("1.2.3.4:3128", ("http", "1.2.3.4", "3128"));
    }
);
source_test!(
    c003_split_server_understands_ipv6,
    "tests/test_proxy_rotate.py::test_split_server_understands_ipv6[[fe80::1]:8080-want3]",
    {
        split_server("[fe80::1]:8080", ("http", "fe80::1", "8080"));
    }
);
