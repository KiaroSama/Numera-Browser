use super::*;
fn p(country: Option<&str>, locale: &str) -> Profile {
    let mut p = profile();
    p.fingerprint = numera_core::fingerprint::generate(numera_core::fingerprint::GenerateOptions {
        seed: 1,
        locale: Some(locale),
        ..Default::default()
    })
    .expect("browser contract");
    p.proxy = country.map(|country| {
        numera_core::proxy::Proxy::from_record(
            &json!({"server":"http://h:1","country":country}),
            true,
        )
        .expect("browser contract")
    });
    p
}
fn options(p: &Profile, backend: &str, headless: bool) -> Value {
    browser::options::build(p, backend, "/owned/profile", headless, &[], &[], None)
        .expect("browser contract")
}
source_test!(
    test_builtin_engines_registered,
    "tests/test_drivers.py::test_builtin_engines_registered",
    {
        let m = browser::sdk::metadata();
        for n in ["playwright", "patchright", "camoufox"] {
            assert!(m["backends"].get(n).is_some());
        }
    }
);
source_test!(
    test_availability_map_covers_all,
    "tests/test_drivers.py::test_availability_map_covers_all",
    {
        let m = browser::sdk::metadata();
        assert_eq!(
            m["backends"]
                .as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            ["playwright", "patchright", "camoufox", "cloak"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        assert!(
            m["backends"]
                .as_object()
                .expect("browser contract")
                .values()
                .all(|v| v["installed"].is_boolean())
        );
    }
);
source_test!(
    test_unknown_engine_raises,
    "tests/test_drivers.py::test_unknown_engine_raises",
    {
        let e = browser::options::build(
            &profile(),
            "does-not-exist",
            "/owned/profile",
            false,
            &[],
            &[],
            None,
        )
        .expect_err("expected browser rejection");
        assert!(e.to_string().contains("Unknown engine"));
    }
);
source_test!(
    test_uninstalled_engine_raises_with_hint,
    "tests/test_drivers.py::test_uninstalled_engine_raises_with_hint",
    {
        let d = fresh();
        let mut c = support::worker(d.path());
        c.env("NUMERA_CAMOUFOX_DRIVER", d.path().join("missing-camoufox"));
        let mut adapter = browser::adapter::Adapter::spawn(
            d.path(),
            "uninstalled",
            "source-browser-host",
            &mut c,
            std::time::Duration::from_secs(5),
        )
        .expect("inert SDK worker");
        assert_eq!(
            adapter.metadata()["backends"]["camoufox"]["installed"],
            false
        );
        let error = adapter
            .open(
                "camoufox",
                serde_json::Map::new(),
                vec![],
                std::time::Duration::from_secs(5),
            )
            .expect_err("uninstalled SDK refused");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains("camoufox"));
        drop(adapter);
        assert!(!d.path().join("missing-camoufox/driver-started").exists());
    }
);
source_test!(
    test_profile_defaults_to_playwright_engine,
    "tests/test_drivers.py::test_profile_defaults_to_playwright_engine",
    {
        let p = profile();
        assert_eq!(p.engine, "playwright");
        assert_eq!(
            Profile::from_record(&serde_json::to_value(p).expect("browser contract"))
                .expect("browser contract")
                .engine,
            "playwright"
        );
    }
);
source_test!(
    test_geolocation_follows_proxy_country,
    "tests/test_drivers.py::test_geolocation_follows_proxy_country",
    {
        let o = options(&p(Some("DE"), "en-US"), "playwright", false);
        assert_eq!(
            o["geolocation"]["latitude"]
                .as_f64()
                .expect("browser contract")
                .round(),
            53.0
        );
        assert_eq!(
            o["geolocation"]["longitude"]
                .as_f64()
                .expect("browser contract")
                .round(),
            13.0
        );
    }
);
source_test!(
    test_geolocation_falls_back_to_locale_without_proxy,
    "tests/test_drivers.py::test_geolocation_falls_back_to_locale_without_proxy",
    {
        let o = options(&p(None, "fr-FR"), "playwright", false);
        assert_eq!(
            o["geolocation"]["latitude"]
                .as_f64()
                .expect("browser contract")
                .round(),
            49.0
        );
        assert_eq!(
            o["geolocation"]["longitude"]
                .as_f64()
                .expect("browser contract")
                .round(),
            2.0
        );
    }
);
source_test!(
    test_geolocation_none_for_unknown_country,
    "tests/test_drivers.py::test_geolocation_none_for_unknown_country",
    {
        assert!(
            options(&p(Some("ZZ"), "en-US"), "playwright", false)
                .get("geolocation")
                .is_none()
        );
    }
);
source_test!(
    test_webrtc_forces_proxy_when_one_is_set,
    "tests/test_drivers.py::test_webrtc_forces_proxy_when_one_is_set",
    {
        assert!(
            options(&p(Some("US"), "en-US"), "playwright", false)["args"]
                .as_array()
                .expect("browser contract")
                .iter()
                .any(|v| v
                    .as_str()
                    .expect("browser contract")
                    .contains("disable_non_proxied_udp"))
        );
    }
);
source_test!(
    test_webrtc_hides_local_ip_without_proxy,
    "tests/test_drivers.py::test_webrtc_hides_local_ip_without_proxy",
    {
        assert!(
            options(&p(None, "en-US"), "playwright", false)["args"]
                .as_array()
                .expect("browser contract")
                .iter()
                .any(|v| v
                    .as_str()
                    .expect("browser contract")
                    .contains("default_public_interface_only"))
        );
    }
);
source_test!(
    test_the_launcher_hands_chromium_the_ua_metadata,
    "tests/test_drivers.py::test_the_launcher_hands_chromium_the_ua_metadata",
    {
        let d = fresh();
        let mut p = profile();
        p.fingerprint = fp("windows", 3);
        let adapter =
            support::native_open(d.path(), "playwright", "normal", &p).expect("SDK fixture launch");
        let requests = support::requests(&d.path().join("driver"));
        let launch = requests
            .iter()
            .find(|r| r["method"] == "launchPersistentContext")
            .expect("actual persistent launch request");
        assert_eq!(launch["guid"], "chromium");
        assert_eq!(launch["params"]["headless"], true);
        assert_eq!(
            launch["params"]["userDataDir"],
            json!(d.path().join("user-data").join(&p.id))
        );
        let override_ = requests
            .iter()
            .find(|r| {
                r["method"] == "send" && r["params"]["method"] == "Emulation.setUserAgentOverride"
            })
            .expect("actual CDP override");
        assert_eq!(
            override_["params"]["params"]["userAgentMetadata"],
            numera_core::identity::metadata(&p.fingerprint).expect("browser contract")
        );
        assert_eq!(
            override_["params"]["params"]["userAgent"],
            p.fingerprint.user_agent
        );
        drop(adapter);
        support::wait(&d.path().join("driver/driver-stopped"));
    }
);
source_test!(
    test_the_high_entropy_hints_are_not_pinned_as_headers,
    "tests/test_drivers.py::test_the_high_entropy_hints_are_not_pinned_as_headers",
    {
        let o = options(&profile(), "playwright", true);
        let h = o["extra_http_headers"]
            .as_object()
            .expect("browser contract");
        assert!(h.contains_key("sec-ch-ua"));
        for k in [
            "sec-ch-ua-platform-version",
            "sec-ch-ua-arch",
            "sec-ch-ua-model",
            "sec-ch-ua-bitness",
            "sec-ch-ua-full-version-list",
            "sec-ch-ua-wow64",
        ] {
            assert!(!h.contains_key(k));
        }
    }
);
source_test!(
    test_headless_uses_the_full_chromium_channel,
    "tests/test_drivers.py::test_headless_uses_the_full_chromium_channel",
    {
        assert_eq!(
            options(&profile(), "playwright", true)["channel"],
            "chromium"
        );
    }
);
source_test!(
    test_headed_leaves_the_channel_alone,
    "tests/test_drivers.py::test_headed_leaves_the_channel_alone",
    {
        assert!(options(&profile(), "playwright", false)["channel"].is_null());
    }
);
source_test!(
    test_camoufox_launch_and_session_build_the_same_options,
    "tests/test_drivers.py::test_camoufox_launch_and_session_build_the_same_options",
    {
        let mut p = profile();
        p.fingerprint = fp("macos", 1);
        p.fingerprint.locale = "de-DE".into();
        p.proxy = Some(
            numera_core::proxy::Proxy::from_record(&json!({"server":"http://1.2.3.4:8080"}), true)
                .expect("browser contract"),
        );
        let o = options(&p, "camoufox", true);
        assert_eq!(o["os"], "macos");
        assert_eq!(o["locale"], "de-DE");
        assert_eq!(o["persistent_context"], true);
        assert!(o.get("proxy").is_some());
    }
);
source_test!(
    test_camoufox_geoip_follows_the_profiles_own_choice,
    "tests/test_drivers.py::test_camoufox_geoip_follows_the_profiles_own_choice",
    {
        let mut p = p(Some("US"), "en-US");
        assert!(options(&p, "camoufox", true).get("geoip").is_none());
        p.ip_based = vec!["timezone".into()];
        assert_eq!(options(&p, "camoufox", true)["geoip"], true);
    }
);
source_test!(
    test_the_all_or_nothing_limitation_is_stated,
    "tests/test_drivers.py::test_the_all_or_nothing_limitation_is_stated",
    {
        let mut p = p(Some("US"), "en-US");
        let mut seen = Vec::new();
        for signal in ["timezone", "languages", "geolocation"] {
            p.ip_based = vec![signal.into()];
            let o = options(&p, "camoufox", true);
            assert_eq!(o["geoip"], true);
            seen.push(o);
        }
        assert_eq!(seen[0], seen[1]);
        assert_eq!(seen[1], seen[2]);
    }
);
source_test!(
    test_a_mobile_profile_does_not_become_a_silently_wrong_desktop,
    "tests/test_drivers.py::test_a_mobile_profile_does_not_become_a_silently_wrong_desktop",
    {
        let mut p = profile();
        p.fingerprint = fp("android", 1);
        assert_eq!(options(&p, "camoufox", true)["os"], "windows");
    }
);
