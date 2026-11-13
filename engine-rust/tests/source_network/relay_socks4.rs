use super::*;
source_test!(
    c000_a_socks4_proxy_carries_a_real_request,
    "tests/test_relay_socks4.py::test_a_socks4_proxy_carries_a_real_request",
    {
        let (site, socks, mut relay) = socks4_relay(None, true);
        let (status, body) = exchange(&relay.url(), "GET", &site.url("/"), &[], &[]);
        assert_eq!(status, 200, "{body}");
        assert_eq!(body, "NUMERA_TEST_ORIGIN / body=0");
        let seen = socks.seen.lock().expect("handshake");
        assert!(seen.iter().any(|v| v["version"] == 4 && v["command"] == 1));
        drop(seen);
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_the_destination_is_resolved_by_the_proxy_not_by_us,
    "tests/test_relay_socks4.py::test_the_destination_is_resolved_by_the_proxy_not_by_us",
    {
        let (site, socks, mut relay) = socks4_relay(None, true);
        let target = format!("http://localhost:{}/", site.server.address.port());
        assert_eq!(exchange(&relay.url(), "GET", &target, &[], &[]).0, 200);
        let seen = socks.seen.lock().expect("handshake");
        let connect = seen.iter().find(|v| v["connect"] == true).expect("CONNECT");
        assert_eq!(connect["address"], json!([0, 0, 0, 1]));
        assert_eq!(connect["host"], "localhost");
        assert_eq!(connect["port"], site.server.address.port());
        drop(seen);
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_the_user_id_goes_in_the_ident_field,
    "tests/test_relay_socks4.py::test_the_user_id_goes_in_the_ident_field",
    {
        let (site, socks, mut relay) = socks4_relay(Some("tenant-7"), true);
        assert_eq!(
            exchange(&relay.url(), "GET", &site.url("/"), &[], &[]).0,
            200
        );
        assert!(
            socks
                .seen
                .lock()
                .expect("handshake")
                .iter()
                .any(|v| v["ident"] == "tenant-7")
        );
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_a_refusal_is_reported_as_a_refusal,
    "tests/test_relay_socks4.py::test_a_refusal_is_reported_as_a_refusal",
    {
        let (site, _socks, mut relay) = socks4_relay(None, false);
        assert_eq!(
            exchange(&relay.url(), "GET", &site.url("/"), &[], &[]).0,
            502
        );
        assert!(
            relay
                .last_error()
                .expect("refusal reason")
                .contains("SOCKS4")
        );
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_a_socks4_proxy_with_a_password_is_refused_with_the_real_reason,
    "tests/test_relay_socks4.py::test_a_socks4_proxy_with_a_password_is_refused_with_the_real_reason",
    {
        let p = Proxy::from_record(
            &json!({"server":"socks4://1.2.3.4:1080","username":"u","password":"p"}),
            false,
        )
        .expect("proxy");
        assert!(
            Relay::start(&p)
                .err()
                .expect("password refused before dialing")
                .to_string()
                .contains("no password in the protocol")
        );
    }
);
source_test!(
    c000_socks4_is_no_longer_reported_as_untestable,
    "tests/test_relay_socks4.py::test_socks4_is_no_longer_reported_as_untestable",
    {
        let origin = Site::body(Some(br#"{"ip":"203.0.113.7"}"#.to_vec()));
        let socks = Socks::new(SocksKind::Four, origin.server.address, false, true, true);
        let entry = json!({"type":"SOCKS4","host":"127.0.0.1","port":socks.server.address.port()});
        let endpoint = origin.url("/ip");
        let result = runtime(proxy_check::testing::check_at(
            &entry,
            Duration::from_secs(3),
            &[&endpoint],
        ));
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["ip"], "203.0.113.7");
        assert!(
            socks
                .seen
                .lock()
                .expect("handshake")
                .iter()
                .any(|e| e["connect"] == true)
        );
    }
);
