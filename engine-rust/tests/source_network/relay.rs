use super::*;
source_test!(
    c000_the_stub_upstream_refuses_an_unauthenticated_request,
    "tests/test_relay.py::test_the_stub_upstream_refuses_an_unauthenticated_request",
    {
        let up = HttpProxy::new();
        let (status, _) = exchange(&up.server.url(""), "GET", "http://numera.test/", &[], &[]);
        assert_eq!(status, 407);
    }
);
source_test!(
    c000_relay_supplies_the_upstream_credentials,
    "tests/test_relay.py::test_relay_supplies_the_upstream_credentials",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        let (status, body) = exchange(
            &relay.url().expect("ready relay URL"),
            "GET",
            "http://numera.test/page?q=1",
            &[],
            &[],
        );
        assert_eq!(status, 200, "{body}");
        assert!(
            body.contains("NUMERA_TEST_ORIGIN") && body.contains("http://numera.test/page?q=1")
        );
        assert_eq!(
            up.seen
                .lock()
                .expect("upstream observations")
                .last()
                .expect("request")
                .1,
            "relay-user:s3cr3t pass"
        );
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_a_wrong_password_still_fails,
    "tests/test_relay.py::test_a_wrong_password_still_fails",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("wrong")).expect("relay");
        assert_eq!(
            exchange(
                &relay.url().expect("ready relay URL"),
                "GET",
                "http://numera.test/",
                &[],
                &[]
            )
            .0,
            407
        );
        assert_eq!(
            up.seen.lock().expect("observed").last().expect("request").1,
            "relay-user:wrong"
        );
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_the_browsers_own_proxy_authorization_is_replaced,
    "tests/test_relay.py::test_the_browsers_own_proxy_authorization_is_replaced",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        assert_eq!(
            exchange(
                &relay.url().expect("ready relay URL"),
                "GET",
                "http://numera.test/",
                &[("Proxy-Authorization", "Basic bm9wZTpub3Bl")],
                &[]
            )
            .0,
            200
        );
        assert_eq!(
            up.seen.lock().expect("observed").last().expect("request").1,
            "relay-user:s3cr3t pass"
        );
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_connect_tunnel_reaches_the_origin,
    "tests/test_relay.py::test_connect_tunnel_reaches_the_origin",
    {
        let origin = Site::new();
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        let (status, body) = tunnel(&relay, &origin, "/tunnelled");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("NUMERA_TEST_ORIGIN") && body.contains("/tunnelled"));
        let seen = up.seen.lock().expect("observed");
        assert!(seen.iter().any(|(line, _)| line.starts_with("CONNECT ")));
        assert_eq!(seen[0].1, "relay-user:s3cr3t pass");
        drop(seen);
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_socks5_upstream_carries_a_connect_tunnel,
    "tests/test_relay.py::test_socks5_upstream_carries_a_connect_tunnel",
    {
        let origin = Site::new();
        let socks = Socks::new(SocksKind::Five, origin.server.address, true, true, true);
        let mut relay =
            Relay::start(&socks.model(SocksKind::Five, Some("u"), Some("p"))).expect("relay");
        let (status, body) = tunnel(&relay, &origin, "/socks-tunnel");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("NUMERA_TEST_ORIGIN") && body.contains("/socks-tunnel"));
        let seen = socks.seen.lock().expect("observations");
        assert!(seen.iter().any(|v| v["auth"] == "u:p"));
        assert!(
            seen.iter()
                .any(|v| v["host"] == "127.0.0.1" && v["port"] == origin.server.address.port())
        );
        drop(seen);
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_socks5_upstream_rewrites_an_absolute_request,
    "tests/test_relay.py::test_socks5_upstream_rewrites_an_absolute_request",
    {
        let origin = Site::new();
        let socks = Socks::new(SocksKind::Five, origin.server.address, true, true, true);
        let mut relay =
            Relay::start(&socks.model(SocksKind::Five, Some("u"), Some("p"))).expect("relay");
        let (status, body) = exchange(
            &relay.url().expect("ready relay URL"),
            "GET",
            &origin.url("/plain?x=2"),
            &[],
            &[],
        );
        assert_eq!(status, 200);
        assert_eq!(body, "NUMERA_TEST_ORIGIN /plain?x=2 body=0");
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_socks5_upstream_forwards_a_request_body,
    "tests/test_relay.py::test_socks5_upstream_forwards_a_request_body",
    {
        let origin = Site::new();
        let socks = Socks::new(SocksKind::Five, origin.server.address, true, true, true);
        let mut relay =
            Relay::start(&socks.model(SocksKind::Five, Some("u"), Some("p"))).expect("relay");
        let payload = vec![b'n'; 5000];
        let (status, body) = exchange(
            &relay.url().expect("ready relay URL"),
            "POST",
            &origin.url("/post"),
            &[],
            &payload,
        );
        assert_eq!(status, 200, "{body}");
        assert!(body.ends_with("body=5000"));
        assert_eq!(
            origin
                .seen
                .lock()
                .expect("origin requests")
                .last()
                .expect("POST")
                .body,
            payload
        );
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_socks5_wrong_password_is_reported_not_swallowed,
    "tests/test_relay.py::test_socks5_wrong_password_is_reported_not_swallowed",
    {
        let origin = Site::new();
        let socks = Socks::new(SocksKind::Five, origin.server.address, true, true, true);
        let mut relay =
            Relay::start(&socks.model(SocksKind::Five, Some("u"), Some("wrong"))).expect("relay");
        let (status, body) = exchange(
            &relay.url().expect("ready relay URL"),
            "GET",
            "http://numera.test/",
            &[],
            &[],
        );
        assert_eq!(status, 502);
        assert!(body.contains("rejected the credentials"), "{body}");
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_socks4_with_credentials_is_refused_up_front,
    "tests/test_relay.py::test_socks4_with_credentials_is_refused_up_front",
    {
        let proxy = Proxy::from_record(
            &json!({"server":"socks4://127.0.0.1:1080","username":"u","password":"p"}),
            false,
        )
        .expect("proxy");
        assert!(
            Relay::start(&proxy)
                .err()
                .expect("unsupported password refused")
                .to_string()
                .contains("SOCKS4")
        );
    }
);
source_test!(
    c000_a_dead_upstream_answers_502_rather_than_hanging,
    "tests/test_relay.py::test_a_dead_upstream_answers_502_rather_than_hanging",
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("owned dead port");
        let address = listener.local_addr().expect("port");
        drop(listener);
        let proxy = Proxy::from_record(
            &json!({"server":format!("http://{address}"),"username":"u","password":"p"}),
            false,
        )
        .expect("proxy");
        let started = Instant::now();
        let mut relay = Relay::start(&proxy).expect("relay");
        let (status, body) = exchange(
            &relay.url().expect("ready relay URL"),
            "GET",
            "http://numera.test/",
            &[],
            &[],
        );
        assert_eq!(status, 502);
        assert!(body.contains("upstream proxy"));
        relay.close(Duration::from_secs(3)).expect("relay joined");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
);
source_test!(
    c000_a_malformed_request_gets_400_not_a_crash,
    "tests/test_relay.py::test_a_malformed_request_gets_400_not_a_crash",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        let url = reqwest::Url::parse(&relay.url().expect("ready relay URL")).expect("URL");
        let mut socket =
            TcpStream::connect(("127.0.0.1", url.port().expect("port"))).expect("relay socket");
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("deadline");
        socket
            .write_all(b"NOT-A-PROXY-REQUEST\r\n\r\n")
            .expect("malformed request");
        let mut bytes = [0; 4096];
        let count = socket.read(&mut bytes).expect("400 response");
        assert!(String::from_utf8_lossy(&bytes[..count]).contains("400"));
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_close_releases_the_port_and_every_thread,
    "tests/test_relay.py::test_close_releases_the_port_and_every_thread",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        let address = reqwest::Url::parse(&relay.url().expect("ready relay URL")).expect("URL");
        assert_eq!(
            exchange(
                &relay.url().expect("ready relay URL"),
                "GET",
                "http://numera.test/",
                &[],
                &[]
            )
            .0,
            200
        );
        relay
            .close(Duration::from_secs(3))
            .expect("worker completion acknowledged and joined");
        assert!(
            TcpStream::connect_timeout(
                &SocketAddr::from(([127, 0, 0, 1], address.port().expect("port"))),
                Duration::from_millis(200)
            )
            .is_err()
        );
    }
);
source_test!(
    c000_the_tunnel_thread_ends_when_its_sockets_are_pulled,
    "tests/test_relay.py::test_the_tunnel_thread_ends_when_its_sockets_are_pulled",
    {
        let origin = Site::new();
        let mut up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        let socket = open_tunnel(&relay, origin.server.address);
        let started = Instant::now();
        socket
            .shutdown(Shutdown::Both)
            .expect("pull browser socket");
        up.server.finish().expect("upstream socket/worker closure");
        relay
            .close(Duration::from_secs(3))
            .expect("tunnel workers ended without escaped exception");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
);
source_test!(
    c000_close_does_not_hang_on_an_open_tunnel,
    "tests/test_relay.py::test_close_does_not_hang_on_an_open_tunnel",
    {
        let origin = Site::new();
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        let socket = open_tunnel(&relay, origin.server.address);
        let started = Instant::now();
        relay
            .close(Duration::from_secs(3))
            .expect("idle tunnel aborted and joined");
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(socket);
    }
);
source_test!(
    c000_start_and_close_are_idempotent,
    "tests/test_relay.py::test_start_and_close_are_idempotent",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("started ready relay");
        let first = relay.url().expect("ready relay URL");
        assert_eq!(relay.url().expect("ready relay URL"), first);
        assert_eq!(
            exchange(&first, "GET", "http://numera.test/", &[], &[]).0,
            200
        );
        relay.close(Duration::from_secs(3)).expect("first close");
        relay
            .close(Duration::from_secs(3))
            .expect("idempotent close");
        let port = relay.port().expect_err("closed relay exposed its port");
        let url = relay.url().expect_err("closed relay exposed its URL");
        assert_eq!(port.kind(), io::ErrorKind::NotConnected);
        assert_eq!(port.to_string(), "the relay is not running");
        assert_eq!(url.kind(), io::ErrorKind::NotConnected);
        assert_eq!(url.to_string(), "the relay is not running");
    }
);
source_test!(
    c000_two_relays_take_different_ephemeral_ports,
    "tests/test_relay.py::test_two_relays_take_different_ephemeral_ports",
    {
        let up = HttpProxy::new();
        let mut first = Relay::start(&up.model("s3cr3t pass")).expect("first relay");
        let mut second = Relay::start(&up.model("s3cr3t pass")).expect("second relay");
        let a = reqwest::Url::parse(&first.url().expect("ready relay URL")).expect("URL");
        let b = reqwest::Url::parse(&second.url().expect("ready relay URL")).expect("URL");
        assert_ne!(a.port(), b.port());
        assert_eq!(
            first.url().expect("ready relay URL"),
            format!("http://127.0.0.1:{}", a.port().expect("port"))
        );
        first.close(Duration::from_secs(3)).expect("first close");
        second.close(Duration::from_secs(3)).expect("second close");
    }
);
source_test!(
    c000_the_relay_is_not_reachable_off_the_machine,
    "tests/test_relay.py::test_the_relay_is_not_reachable_off_the_machine",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass")).expect("relay");
        let url = reqwest::Url::parse(&relay.url().expect("ready relay URL")).expect("URL");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        let route = std::net::UdpSocket::bind("0.0.0.0:0").expect("local routing probe");
        if route.connect("192.0.2.1:9").is_ok() {
            let ip = route.local_addr().expect("local interface").ip();
            if !ip.is_loopback() && !ip.is_unspecified() {
                assert!(
                    TcpStream::connect_timeout(
                        &SocketAddr::new(ip, url.port().expect("port")),
                        Duration::from_millis(200)
                    )
                    .is_err(),
                    "relay accepted traffic on a non-loopback interface"
                );
            }
        }
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_port_is_unavailable_before_start,
    "tests/test_relay.py::test_port_is_unavailable_before_start",
    {
        let up = HttpProxy::new();
        let mut relay = Relay::start(&up.model("s3cr3t pass"))
            .expect("constructor publishes only a ready relay");
        let url = relay.url().expect("ready relay URL");
        assert_eq!(
            exchange(&url, "GET", "http://numera.test/", &[], &[]).0,
            200,
            "published URL preceded listener readiness"
        );
        relay.close(Duration::from_secs(3)).expect("relay joined");
    }
);
source_test!(
    c000_a_proxy_without_a_port_is_refused,
    "tests/test_relay.py::test_a_proxy_without_a_port_is_refused",
    {
        let p = Proxy::from_record(
            &json!({"server":"http://127.0.0.1","username":"u","password":"p"}),
            false,
        )
        .expect("proxy");
        assert!(Relay::start(&p).is_err());
    }
);
source_test!(
    c000_only_a_credentialed_proxy_gets_a_relay,
    "tests/test_relay.py::test_only_a_credentialed_proxy_gets_a_relay",
    {
        let (cfg, relay) = network::relay::proxy_transport(None).expect("no proxy");
        assert!(cfg.is_none() && relay.is_none());
        let plain =
            Proxy::from_record(&json!({"server":"http://127.0.0.1:1"}), false).expect("plain");
        let (cfg, relay) = network::relay::proxy_transport(Some(&plain)).expect("plain transport");
        assert_eq!(cfg, Some(json!({"server":"http://127.0.0.1:1"})));
        assert!(relay.is_none());
        let auth = Proxy::from_record(
            &json!({"server":"http://127.0.0.1:1","username":"u","password":"p"}),
            false,
        )
        .expect("authenticated");
        let (cfg, relay) =
            network::relay::proxy_transport(Some(&auth)).expect("credentialed transport");
        let mut relay = relay.expect("credentialed native bridge");
        assert_eq!(
            cfg,
            Some(json!({"server":relay.url().expect("ready relay URL")}))
        );
        relay
            .close(Duration::from_secs(3))
            .expect("owned bridge joined");
    }
);
