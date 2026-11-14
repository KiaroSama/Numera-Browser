use super::*;
#[test]
/// tests/test_launch_extras.py::test_a_known_resolver_ip_becomes_its_doh_endpoint
fn test_a_known_resolver_ip_becomes_its_doh_endpoint() {
    let mut p = profile();
    p.dns = "1.1.1.1".into();
    let options = launch_options(&p).expect("known DNS resolver");
    let args = options["args"].as_array().expect("browser contract");
    assert!(args.contains(&json!(
        "--dns-over-https-templates=https://cloudflare-dns.com/dns-query"
    )));
    assert!(args.contains(&json!("--dns-over-https-mode=secure")));
}
#[test]
/// tests/test_launch_extras.py::test_a_doh_url_is_passed_through
fn test_a_doh_url_is_passed_through() {
    let mut p = profile();
    p.dns = "https://doh.example/dns-query".into();
    let options = launch_options(&p).expect("custom DNS resolver");
    assert!(
        options["args"]
            .as_array()
            .expect("browser contract")
            .contains(&json!(
                "--dns-over-https-templates=https://doh.example/dns-query"
            ))
    );
}
#[test]
/// tests/test_launch_extras.py::test_no_dns_configured_adds_no_flags
fn test_no_dns_configured_adds_no_flags() {
    let options = launch_options(&profile()).expect("browser contract");
    assert!(
        options["args"]
            .as_array()
            .expect("browser contract")
            .iter()
            .all(|v| !v
                .as_str()
                .expect("browser contract")
                .contains("dns-over-https"))
    );
}
#[test]
/// tests/test_launch_extras.py::test_a_nameserver_chromium_cannot_use_is_refused_not_ignored
fn test_a_nameserver_chromium_cannot_use_is_refused_not_ignored() {
    let mut p = profile();
    p.dns = "192.168.1.1".into();
    assert!(
        launch_options(&p)
            .expect_err("expected browser rejection")
            .to_string()
            .contains("DNS-over-HTTPS")
    );
}

struct Resources {
    _dir: common::TempDirectory,
    session: numera_core::ownership::session::Session,
    p: Profile,
    root: std::path::PathBuf,
}
impl Resources {
    fn new() -> Self {
        let dir = fresh();
        let p = profile();
        let session =
            numera_core::ownership::session::Session::admit(dir.path(), &p.id, "browser-source")
                .expect("browser contract");
        let root = dir.path().join("user-data").join(&p.id);
        Self {
            _dir: dir,
            session,
            p,
            root,
        }
    }
    fn run(&self) {
        browser::resources::prepare(&self.session, &self.p, 1_700_000_000.0)
            .expect("browser contract")
    }
    fn seed(&self, paths: &[&str]) {
        for rel in paths {
            let path = self.root.join(rel);
            std::fs::create_dir_all(path.parent().expect("browser contract"))
                .expect("browser contract");
            std::fs::write(path, b"x").expect("browser contract");
        }
    }
    fn doc(&self) -> Value {
        serde_json::from_slice(
            &std::fs::read(self.root.join("Default/Bookmarks")).expect("browser contract"),
        )
        .expect("browser contract")
    }
    fn bar(&self) -> Vec<Value> {
        self.doc()["roots"]["bookmark_bar"]["children"]
            .as_array()
            .expect("browser contract")
            .clone()
    }
}
fn proxied(marked: &[&str]) -> Profile {
    let mut p = profile();
    p.proxy = Some(
        numera_core::proxy::Proxy::from_record(
            &json!({"server":"http://x:1","country":"US"}),
            true,
        )
        .expect("browser contract"),
    );
    p.ip_based = marked.iter().map(|s| (*s).into()).collect();
    p.fingerprint.timezone = "America/New_York".into();
    p.fingerprint.locale = "en-US".into();
    p.fingerprint.languages = vec!["en-US".into(), "en".into()];
    p
}
fn editor_keys(field: &str) -> std::collections::BTreeSet<String> {
    let a = numera_dashboard::model::App::default();
    let mut ed = numera_dashboard::model::Editor::new(conversion::to_dashboard(&profile()));
    ed.tab = "advanced".into();
    let html = numera_dashboard::render::editor_view(&a, &ed);
    let needle = format!("data-id=\"{field}|");
    html.split(&needle)
        .skip(1)
        .map(|part| part.split('"').next().expect("browser contract").into())
        .collect()
}
source_test!(
    test_bookmarks_are_written_where_chromium_reads_them,
    "tests/test_launch_extras.py::test_bookmarks_are_written_where_chromium_reads_them",
    {
        let mut r = Resources::new();
        r.p.bookmarks = vec![json!({"name":"GitHub","url":"https://github.com"})];
        r.run();
        let bar = r.bar();
        assert_eq!(bar.len(), 1);
        assert_eq!(bar[0]["name"], "GitHub");
        assert_eq!(bar[0]["url"], "https://github.com");
        assert_eq!(bar[0]["type"], "url");
        assert!(
            bar[0]["date_added"]
                .as_str()
                .expect("browser contract")
                .parse::<u64>()
                .expect("browser contract")
                > 13_000_000_000_000_000
        );
    }
);
source_test!(
    test_relaunching_does_not_stack_duplicates,
    "tests/test_launch_extras.py::test_relaunching_does_not_stack_duplicates",
    {
        let mut r = Resources::new();
        r.p.bookmarks = vec![json!({"name":"GitHub","url":"https://github.com"})];
        r.run();
        r.run();
        assert_eq!(r.bar().len(), 1);
    }
);
source_test!(
    test_a_bookmark_the_user_made_in_the_browser_survives,
    "tests/test_launch_extras.py::test_a_bookmark_the_user_made_in_the_browser_survives",
    {
        let mut r = Resources::new();
        r.p.bookmarks = vec![json!({"name":"A","url":"https://a.test"})];
        r.run();
        let mut d = r.doc();
        d["roots"]["bookmark_bar"]["children"].as_array_mut().expect("browser contract").push(json!({"date_added":"0","id":"99","type":"url","name":"Mine","url":"https://mine.test"}));
        std::fs::write(r.root.join("Default/Bookmarks"), d.to_string()).expect("browser contract");
        r.p.bookmarks
            .push(json!({"name":"B","url":"https://b.test"}));
        r.run();
        let bar = r.bar();
        assert_eq!(
            bar.iter().map(|b| b["url"].clone()).collect::<Vec<_>>(),
            vec![
                json!("https://a.test"),
                json!("https://mine.test"),
                json!("https://b.test")
            ]
        );
        assert_eq!(
            bar.iter()
                .map(|b| b["id"].to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            bar.len()
        );
    }
);
source_test!(
    test_no_bookmarks_leaves_the_file_alone,
    "tests/test_launch_extras.py::test_no_bookmarks_leaves_the_file_alone",
    {
        let r = Resources::new();
        r.run();
        assert!(!r.root.join("Default").exists());
    }
);
source_test!(
    test_a_corrupt_bookmarks_file_is_replaced_rather_than_crashing_the_launch,
    "tests/test_launch_extras.py::test_a_corrupt_bookmarks_file_is_replaced_rather_than_crashing_the_launch",
    {
        let mut r = Resources::new();
        r.seed(&["Default/Bookmarks"]);
        std::fs::write(r.root.join("Default/Bookmarks"), b"{ not json").expect("browser contract");
        r.p.bookmarks = vec![json!({"name":"A","url":"https://a.test"})];
        r.run();
        assert_eq!(r.bar()[0]["url"], "https://a.test");
    }
);
source_test!(
    test_no_checksum_is_written,
    "tests/test_launch_extras.py::test_no_checksum_is_written",
    {
        let mut r = Resources::new();
        r.p.bookmarks = vec![json!({"name":"A","url":"https://a.test"})];
        r.run();
        assert!(r.doc().get("checksum").is_none());
    }
);
source_test!(
    test_timezone_and_languages_follow_the_real_exit_country,
    "tests/test_launch_extras.py::test_timezone_and_languages_follow_the_real_exit_country",
    {
        let p = proxied(&["timezone", "languages"]);
        let out = browser::ip_identity::resolve(&p, Some("DE")).expect("browser contract");
        assert_eq!(out.fingerprint.timezone, "Europe/Berlin");
        assert_eq!(out.fingerprint.locale, "de-DE");
        assert_eq!(out.fingerprint.languages[0], "de-DE");
    }
);
source_test!(
    test_only_the_marked_signals_move,
    "tests/test_launch_extras.py::test_only_the_marked_signals_move",
    {
        let p = proxied(&["timezone"]);
        let out = browser::ip_identity::resolve(&p, Some("DE")).expect("browser contract");
        assert_eq!(out.fingerprint.timezone, "Europe/Berlin");
        assert_eq!(out.fingerprint.locale, "en-US");
    }
);
source_test!(
    test_geolocation_follows_the_country_the_proxy_actually_exits_from,
    "tests/test_launch_extras.py::test_geolocation_follows_the_country_the_proxy_actually_exits_from",
    {
        let p = proxied(&["geolocation"]);
        let out = browser::ip_identity::resolve(&p, Some("TR")).expect("browser contract");
        assert_eq!(
            out.proxy
                .as_ref()
                .expect("browser contract")
                .country
                .as_deref(),
            Some("TR")
        );
        let o = launch_options(&out).expect("browser contract");
        let d: Value = serde_json::from_str(include_str!("../../src/device_catalog.json"))
            .expect("browser contract");
        assert_eq!(
            o["geolocation"]["latitude"],
            d["catalog"]["COUNTRY_GEO"]["TR"][0]
        );
        assert_eq!(
            o["geolocation"]["longitude"],
            d["catalog"]["COUNTRY_GEO"]["TR"][1]
        );
    }
);
source_test!(
    test_nothing_marked_means_no_lookup,
    "tests/test_launch_extras.py::test_nothing_marked_means_no_lookup",
    {
        let p = proxied(&[]);
        assert!(!browser::ip_identity::needs_lookup(&p));
        assert_eq!(
            serde_json::to_value(
                browser::ip_identity::resolve(&p, Some("DE")).expect("browser contract")
            )
            .expect("browser contract"),
            serde_json::to_value(p).expect("browser contract")
        );
    }
);
source_test!(
    test_an_unreachable_proxy_leaves_the_stored_identity_alone,
    "tests/test_launch_extras.py::test_an_unreachable_proxy_leaves_the_stored_identity_alone",
    {
        let p = proxied(&["timezone"]);
        assert!(browser::ip_identity::needs_lookup(&p));
        assert_eq!(
            browser::ip_identity::resolve(&p, None)
                .expect("browser contract")
                .fingerprint
                .timezone,
            p.fingerprint.timezone
        );
    }
);
source_test!(
    test_the_profiles_own_flags_are_on_the_command_line,
    "tests/test_launch_extras.py::test_the_profiles_own_flags_are_on_the_command_line",
    {
        let mut p = profile();
        p.launch_args = vec!["--mute-audio".into()];
        p.dns = "9.9.9.9".into();
        let o = browser::options::build(
            &p,
            "playwright",
            "/owned/profile",
            true,
            &["--internal".into()],
            &[],
            None,
        )
        .expect("browser contract");
        for arg in [
            "--mute-audio",
            "--internal",
            "--dns-over-https-templates=https://dns.quad9.net/dns-query",
        ] {
            assert!(
                o["args"]
                    .as_array()
                    .expect("browser contract")
                    .contains(&json!(arg))
            );
        }
    }
);
source_test!(
    test_the_new_fields_survive_export_and_import,
    "tests/test_launch_extras.py::test_the_new_fields_survive_export_and_import",
    {
        let mut p = profile();
        p.launch_args = vec!["--mute-audio".into()];
        p.bookmarks = vec![json!({"name":"A","url":"https://a.test"})];
        p.dns = "1.1.1.1".into();
        p.ip_based = vec!["timezone".into()];
        let back = Profile::from_record(&serde_json::to_value(&p).expect("browser contract"))
            .expect("browser contract");
        assert_eq!(back.launch_args, p.launch_args);
        assert_eq!(back.bookmarks, p.bookmarks);
        assert_eq!(back.dns, p.dns);
        assert_eq!(back.ip_based, p.ip_based);
    }
);
source_test!(
    test_a_profile_saved_before_these_fields_existed_still_loads,
    "tests/test_launch_extras.py::test_a_profile_saved_before_these_fields_existed_still_loads",
    {
        let mut d = serde_json::to_value(profile()).expect("browser contract");
        for k in ["launch_args", "bookmarks", "dns", "ip_based"] {
            d.as_object_mut().expect("browser contract").remove(k);
        }
        let back = Profile::from_record(&d).expect("browser contract");
        assert!(
            back.launch_args.is_empty()
                && back.bookmarks.is_empty()
                && back.dns.is_empty()
                && back.ip_based.is_empty()
        );
    }
);
source_test!(
    test_an_unticked_store_is_gone_before_the_browser_opens,
    "tests/test_launch_extras.py::test_an_unticked_store_is_gone_before_the_browser_opens",
    {
        let mut r = Resources::new();
        r.seed(&[
            "Default/History",
            "Default/History-journal",
            "Default/Login Data",
            "Default/Network/Cookies",
        ]);
        r.p.clear_on_start = vec!["history".into()];
        r.run();
        assert!(!r.root.join("Default/History").exists());
        assert!(!r.root.join("Default/History-journal").exists());
        assert!(r.root.join("Default/Login Data").is_file());
        assert!(r.root.join("Default/Network/Cookies").is_file());
    }
);
source_test!(
    test_a_cleared_directory_goes_with_its_contents,
    "tests/test_launch_extras.py::test_a_cleared_directory_goes_with_its_contents",
    {
        let mut r = Resources::new();
        r.seed(&["Default/Local Storage/leveldb/000003.log"]);
        r.p.clear_on_start = vec!["localStorage".into()];
        r.run();
        assert!(!r.root.join("Default/Local Storage").exists());
    }
);
source_test!(
    test_the_default_profile_carries_everything,
    "tests/test_launch_extras.py::test_the_default_profile_carries_everything",
    {
        let r = Resources::new();
        let paths = [
            "Default/History",
            "Default/Login Data",
            "Default/Network/Cookies",
        ];
        r.seed(&paths);
        r.run();
        assert!(paths.iter().all(|p| r.root.join(p).exists()));
    }
);
source_test!(
    test_an_unknown_key_cannot_delete_anything,
    "tests/test_launch_extras.py::test_an_unknown_key_cannot_delete_anything",
    {
        let mut r = Resources::new();
        r.seed(&["Default/History"]);
        r.p.clear_on_start = vec!["../../etc".into(), "everything".into()];
        r.run();
        assert!(r.root.join("Default/History").exists());
    }
);
source_test!(
    test_clearing_bookmarks_still_reseeds_the_configured_ones,
    "tests/test_launch_extras.py::test_clearing_bookmarks_still_reseeds_the_configured_ones",
    {
        let mut r = Resources::new();
        r.p.bookmarks = vec![json!({"name":"A","url":"https://a.test"})];
        r.run();
        r.seed(&["Default/Bookmarks.bak"]);
        r.p.clear_on_start = vec!["bookmarks".into()];
        r.run();
        assert_eq!(
            r.bar().iter().map(|v| v["url"].clone()).collect::<Vec<_>>(),
            vec![json!("https://a.test")]
        );
        assert!(!r.root.join("Default/Bookmarks.bak").exists());
    }
);
source_test!(
    test_the_dashboard_and_the_engine_agree_on_the_key_names,
    "tests/test_launch_extras.py::test_the_dashboard_and_the_engine_agree_on_the_key_names",
    {
        let keys = editor_keys("clearOnStart");
        assert_eq!(
            keys,
            [
                "cookies",
                "localStorage",
                "indexedDb",
                "sessions",
                "history",
                "passwords",
                "extensionData",
                "bookmarks"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        let p = conversion::to_engine(&json!({"id":"p","name":"p","clearOnStart":keys}), 1000.0)
            .expect("browser contract");
        assert_eq!(
            p.clear_on_start
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            keys
        );
    }
);
source_test!(
    test_switching_off_google_services_adds_the_flags_that_do_it,
    "tests/test_launch_extras.py::test_switching_off_google_services_adds_the_flags_that_do_it",
    {
        let mut p = profile();
        p.switched_off = vec!["googleServices".into()];
        let o = launch_options(&p).expect("browser contract");
        assert!(
            o["args"]
                .as_array()
                .expect("browser contract")
                .contains(&json!("--disable-sync"))
        );
        assert!(
            o["args"]
                .as_array()
                .expect("browser contract")
                .contains(&json!("--disable-background-networking"))
        );
    }
);
source_test!(
    test_the_default_leaves_chromium_alone,
    "tests/test_launch_extras.py::test_the_default_leaves_chromium_alone",
    {
        let o = launch_options(&profile()).expect("browser contract");
        assert!(
            !o["args"]
                .as_array()
                .expect("browser contract")
                .contains(&json!("--disable-sync"))
        );
        assert!(
            !o["args"]
                .as_array()
                .expect("browser contract")
                .contains(&json!("--disable-component-update"))
        );
    }
);
source_test!(
    test_an_unknown_key_adds_no_flags,
    "tests/test_launch_extras.py::test_an_unknown_key_adds_no_flags",
    {
        let mut p = profile();
        let before = launch_options(&p).expect("browser contract");
        p.switched_off = vec!["whatever".into()];
        assert_eq!(launch_options(&p).expect("browser contract"), before);
    }
);
source_test!(
    test_nothing_here_uses_disable_features,
    "tests/test_launch_extras.py::test_nothing_here_uses_disable_features",
    {
        let mut p = profile();
        p.switched_off = vec!["googleServices".into(), "systemExtensions".into()];
        assert!(
            launch_options(&p).expect("browser contract")["args"]
                .as_array()
                .expect("browser contract")
                .iter()
                .all(|v| !v
                    .as_str()
                    .expect("browser contract")
                    .starts_with("--disable-features")
                    && !v
                        .as_str()
                        .expect("browser contract")
                        .starts_with("--enable-features"))
        );
    }
);
source_test!(
    test_the_dashboard_and_the_engine_agree_on_the_switch_names,
    "tests/test_launch_extras.py::test_the_dashboard_and_the_engine_agree_on_the_switch_names",
    {
        let keys = editor_keys("switchedOff");
        assert_eq!(
            keys,
            ["googleServices", "systemExtensions"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        let p = conversion::to_engine(&json!({"id":"p","name":"p","switchedOff":keys}), 1000.0)
            .expect("browser contract");
        assert_eq!(
            p.switched_off
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            keys
        );
    }
);
source_test!(
    test_the_flags_and_the_dns_flag_survive_together,
    "tests/test_launch_extras.py::test_the_flags_and_the_dns_flag_survive_together",
    {
        let mut p = profile();
        p.dns = "1.1.1.1".into();
        p.switched_off = vec!["googleServices".into(), "systemExtensions".into()];
        let o = launch_options(&p).expect("browser contract");
        let args = o["args"].as_array().expect("browser contract");
        assert!(args.contains(&json!(
            "--dns-over-https-templates=https://cloudflare-dns.com/dns-query"
        )));
        assert!(args.contains(&json!(
            "--disable-component-extensions-with-background-pages"
        )));
        assert!(!args.iter().any(|v| {
            v.as_str()
                .expect("browser contract")
                .starts_with("--disable-features")
        }));
    }
);
source_test!(
    test_switched_off_survives_export_and_import,
    "tests/test_launch_extras.py::test_switched_off_survives_export_and_import",
    {
        let mut p = profile();
        p.switched_off = vec!["googleServices".into()];
        let mut d = serde_json::to_value(&p).expect("browser contract");
        assert_eq!(
            Profile::from_record(&d)
                .expect("browser contract")
                .switched_off,
            p.switched_off
        );
        d.as_object_mut()
            .expect("browser contract")
            .remove("switched_off");
        assert!(
            Profile::from_record(&d)
                .expect("browser contract")
                .switched_off
                .is_empty()
        );
    }
);
