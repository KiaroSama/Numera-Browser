use super::*;
source_test!(
    parse_host_port,
    "tests/test_proxypool.py::test_parse_host_port",
    {
        assert_eq!(
            numera_core::pool::parse_line("1.2.3.4:8080"),
            Some(
                json!({"type":"HTTP","host":"1.2.3.4","port":"8080","user":null,"pass":null,"country":null,"label":null})
            )
        );
    }
);
source_test!(
    parse_keeps_the_label_beside_the_country_code,
    "tests/test_proxypool.py::test_parse_keeps_the_label_beside_the_country_code",
    {
        let one = numera_core::pool::parse_line("gw.example:3128:user:pass:IT 1").expect("one");
        let two = numera_core::pool::parse_line("gw.example:3128:user:pass:IT 2").expect("two");
        assert_eq!(
            (one["country"].clone(), one["label"].clone()),
            (json!("IT"), json!("IT 1"))
        );
        assert_eq!(
            (two["country"].clone(), two["label"].clone()),
            (json!("IT"), json!("IT 2"))
        );
        assert_ne!(one["label"], two["label"]);
        assert!(
            numera_core::pool::parse_line("gw.example:3128:IT").expect("country")["label"]
                .is_null()
        );
    }
);
source_test!(
    parse_host_port_user_pass,
    "tests/test_proxypool.py::test_parse_host_port_user_pass",
    {
        let p =
            numera_core::pool::parse_line("1.2.3.4:8080:alice:secret").expect("parse credentials");
        assert_eq!(p["host"], "1.2.3.4");
        assert_eq!(p["port"], "8080");
        assert_eq!(p["user"], "alice");
        assert_eq!(p["pass"], "secret");
    }
);
source_test!(
    parse_user_pass_at_host,
    "tests/test_proxypool.py::test_parse_user_pass_at_host",
    {
        let p =
            numera_core::pool::parse_line("alice:secret@1.2.3.4:8080").expect("parse credentials");
        assert_eq!(p["host"], "1.2.3.4");
        assert_eq!(p["port"], "8080");
        assert_eq!(p["user"], "alice");
        assert_eq!(p["pass"], "secret");
    }
);
source_test!(
    parse_scheme_and_socks,
    "tests/test_proxypool.py::test_parse_scheme_and_socks",
    {
        assert_eq!(
            numera_core::pool::parse_line("socks5://host.io:1080").expect("SOCKS")["type"],
            "SOCKS5"
        );
        assert_eq!(
            numera_core::pool::parse_line("https://u:p@host.io:3128").expect("HTTPS")["type"],
            "HTTPS"
        );
    }
);
source_test!(
    parse_trailing_country_code,
    "tests/test_proxypool.py::test_parse_trailing_country_code",
    {
        assert_eq!(
            numera_core::pool::parse_line("1.2.3.4:8080:u:p:US").expect("US")["country"],
            "US"
        );
        assert_eq!(
            numera_core::pool::parse_line("1.2.3.4:8080:DE").expect("DE")["country"],
            "DE"
        );
    }
);
source_test!(
    parse_rejects_junk,
    "tests/test_proxypool.py::test_parse_rejects_junk",
    {
        for bad in ["not a proxy", "# comment", "", "host-without-port"] {
            assert!(numera_core::pool::parse_line(bad).is_none(), "{bad}");
        }
    }
);
source_test!(
    parse_many_skips_bad_lines,
    "tests/test_proxypool.py::test_parse_many_skips_bad_lines",
    {
        let out = numera_core::pool::parse_many(
            "1.2.3.4:8080
# a comment

garbage
user:pass@5.6.7.8:3128
",
        );
        assert_eq!(out.len(), 2);
        assert_eq!(
            out.iter()
                .map(|p| p["host"].as_str().expect("host"))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["1.2.3.4", "5.6.7.8"])
        );
    }
);
source_test!(
    dashboard_engine_round_trip_preserves_identity,
    "tests/test_proxypool.py::test_dashboard_engine_round_trip_preserves_identity",
    {
        let mut p = profile("rt", 5);
        p.engine = "cloak".into();
        p.fingerprint = fingerprint::generate(GenerateOptions {
            seed: 5,
            os: Some("macos"),
            locale: Some("de-DE"),
            ..Default::default()
        })
        .expect("mac");
        p.proxy=Some(Proxy::from_record(&json!({"server":"socks5://9.9.9.9:1080","username":"x","password":"y","country":"DE"}),false).expect("proxy"));
        p.tags = vec!["t".into()];
        let d = conversion::to_dashboard(&p);
        assert_eq!(d["os"], "macOS");
        assert_eq!(d["engine"], "cloak");
        assert_eq!(d["proxy"]["type"], "SOCKS5");
        assert_eq!(d["proxy"]["host"], "9.9.9.9");
        assert_eq!(d["proxy"]["country"], "DE");
        let back = conversion::to_engine(&d, 1000.0).expect("roundtrip");
        assert_eq!(back.fingerprint.os, "macos");
        assert_eq!(
            back.fingerprint.webgl_renderer,
            p.fingerprint.webgl_renderer
        );
        assert!(
            back.proxy
                .expect("proxy")
                .server
                .starts_with("socks5://9.9.9.9")
        );
    }
);
source_test!(
    import_shaped_profile_is_coherent,
    "tests/test_proxypool.py::test_import_shaped_profile_is_coherent",
    {
        let (p,_)=numera_core::importers::model(&json!({"name":"gl","os":"mac","navigator":{"userAgent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128.0.0.0 Safari/537.36"}}),1,1000.0).expect("import");
        let d = conversion::to_dashboard(&p);
        let back = conversion::to_engine(&d, 1000.0).expect("conversion");
        assert!(
            identity::validate(&back.fingerprint)
                .expect("coherence")
                .is_empty()
        );
    }
);
source_test!(
    sticky_session_proxies_are_not_collapsed,
    "tests/test_proxypool.py::test_sticky_session_proxies_are_not_collapsed",
    {
        let temp = temp();
        let root = temp.path();
        let lines = ["IT", "BE", "UK", "TR"]
            .iter()
            .flat_map(|cc| {
                (1..5)
                    .map(move |n| format!("socks5://gate.example.com:32325:acct-1:token-{cc}-{n}"))
            })
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            pool(root)
                .add_many(numera_core::pool::parse_many(&lines))
                .expect("add")
                .len(),
            16
        );
        assert_eq!(pool(root).list().expect("list").len(), 16);
        assert!(
            pool(root)
                .add_many(numera_core::pool::parse_many(&lines))
                .expect("duplicate")
                .is_empty()
        );
        assert_eq!(pool(root).list().expect("unchanged").len(), 16);
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_011,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[1.2.3.4:8080]",
    {
        let first = numera_core::pool::parse_line("1.2.3.4:8080").expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_012,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[http://1.2.3.4:8080]",
    {
        let first = numera_core::pool::parse_line("http://1.2.3.4:8080").expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_013,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[socks5://1.2.3.4:1080]",
    {
        let first = numera_core::pool::parse_line("socks5://1.2.3.4:1080").expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_014,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[user:pass@1.2.3.4:8080]",
    {
        let first =
            numera_core::pool::parse_line("user:pass@1.2.3.4:8080").expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_015,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[socks5://user:pass@1.2.3.4:1080]",
    {
        let first = numera_core::pool::parse_line("socks5://user:pass@1.2.3.4:1080")
            .expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_016,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[1.2.3.4:8080:user:pass]",
    {
        let first =
            numera_core::pool::parse_line("1.2.3.4:8080:user:pass").expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_017,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[1.2.3.4:8080:US]",
    {
        let first = numera_core::pool::parse_line("1.2.3.4:8080:US").expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_018,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[socks5://user:pass@1.2.3.4:1080:IT 1]",
    {
        let first = numera_core::pool::parse_line("socks5://user:pass@1.2.3.4:1080:IT 1")
            .expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_re_imports_to_the_same_proxy_case_019,
    "tests/test_proxypool.py::test_export_re_imports_to_the_same_proxy[1.2.3.4:8080:user:pass:DE]",
    {
        let first =
            numera_core::pool::parse_line("1.2.3.4:8080:user:pass:DE").expect("fixture parses");
        let line = numera_core::pool::to_line(&first).expect("export");
        let again = numera_core::pool::parse_line(&line).expect("export parses");
        for key in ["type", "host", "port", "user", "pass", "country"] {
            assert_eq!(again[key], first[key], "{key} via {line}");
        }
    }
);
source_test!(
    export_keeps_a_password_containing_a_colon,
    "tests/test_proxypool.py::test_export_keeps_a_password_containing_a_colon",
    {
        let p = json!({"type":"SOCKS5","host":"1.2.3.4","port":"1080","user":"u","pass":"a:b:c","country":null,"label":null});
        let line = numera_core::pool::to_line(&p).expect("export");
        assert_eq!(
            numera_core::pool::parse_line(&line).expect("parse")["pass"],
            "a:b:c"
        );
    }
);
source_test!(
    export_does_not_repeat_the_country_when_a_label_carries_it,
    "tests/test_proxypool.py::test_export_does_not_repeat_the_country_when_a_label_carries_it",
    {
        let p = numera_core::pool::parse_line("socks5://1.2.3.4:1080:IT 1").expect("parse");
        let line = numera_core::pool::to_line(&p).expect("export");
        assert_eq!(line.matches("IT").count(), 1);
        assert_eq!(
            numera_core::pool::parse_line(&line).expect("parse again")["label"],
            "IT 1"
        );
    }
);
