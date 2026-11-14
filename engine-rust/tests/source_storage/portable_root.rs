use super::*;
source_test!(
    an_explicit_dir_beats_everything,
    "tests/test_portable_root.py::test_an_explicit_dir_beats_everything",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            numera_core::storage::data_root::resolve(
                Some(&root.join("explicit")),
                Some(&root.join("from-env")),
                &root.join("app"),
                &root.join("home")
            )
            .expect("root"),
            root.join("explicit")
        );
    }
);
source_test!(
    the_env_var_is_used_when_there_is_no_argument,
    "tests/test_portable_root.py::test_the_env_var_is_used_when_there_is_no_argument",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            numera_core::storage::data_root::resolve(
                None,
                Some(&root.join("from-env")),
                &root.join("app"),
                &root.join("home")
            )
            .expect("root"),
            root.join("from-env")
        );
    }
);
source_test!(
    a_writable_app_dir_is_preferred_so_the_app_stays_portable,
    "tests/test_portable_root.py::test_a_writable_app_dir_is_preferred_so_the_app_stays_portable",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            numera_core::storage::data_root::resolve(
                None,
                None,
                &root.join("app"),
                &root.join("home")
            )
            .expect("root"),
            root.join("app/data")
        );
        assert!(
            fs::read_dir(root.join("app/data"))
                .expect("portable")
                .next()
                .is_none(),
            "writability probe reclaimed"
        );
    }
);
source_test!(
    an_unwritable_app_dir_falls_back_to_home,
    "tests/test_portable_root.py::test_an_unwritable_app_dir_falls_back_to_home",
    {
        let temp = temp();
        let root = temp.path();
        fs::create_dir_all(root.join("app").parent().expect("parent")).expect("parent");
        fs::write(root.join("app"), b"not a writable directory").expect("unwritable app");
        assert_eq!(
            numera_core::storage::data_root::resolve(
                None,
                None,
                &root.join("app"),
                &root.join("home")
            )
            .expect("fallback"),
            root.join("home/.numera")
        );
    }
);
source_test!(
    the_pre_rename_directory_is_used_when_it_is_the_only_one,
    "tests/test_portable_root.py::test_the_pre_rename_directory_is_used_when_it_is_the_only_one",
    {
        let temp = temp();
        let root = temp.path();
        fs::create_dir_all(root.join("home/.persona")).expect("legacy home");
        fs::create_dir_all(root.join("app").parent().expect("parent")).expect("parent");
        fs::write(root.join("app"), b"not a writable directory").expect("unwritable app");
        assert_eq!(
            numera_core::storage::data_root::resolve(
                None,
                None,
                &root.join("app"),
                &root.join("home")
            )
            .expect("fallback"),
            root.join("home/.persona")
        );
    }
);
source_test!(
    import_carries_profiles_pool_config_and_sessions,
    "tests/test_portable_root.py::test_import_carries_profiles_pool_config_and_sessions",
    {
        let temp = temp();
        let root = temp.path();
        let src = legacy(root);
        let dst = root.join("new");
        let result = numera_core::storage::first_run::import_from(&src, &dst).expect("import");
        assert_eq!(result.profiles, 2);
        for name in ["profiles/a.json", "proxies.json", "config.json"] {
            assert!(dst.join(name).is_file());
        }
        assert_eq!(
            fs::read(dst.join("user-data/abc123/Cookies")).expect("session"),
            b"session"
        );
    }
);
source_test!(
    import_never_overwrites_the_destination,
    "tests/test_portable_root.py::test_import_never_overwrites_the_destination",
    {
        let temp = temp();
        let root = temp.path();
        let src = legacy(root);
        let dst = root.join("new");
        fs::create_dir_all(dst.join("profiles")).expect("destination");
        fs::write(dst.join("profiles/a.json"), br#"{"id":"MINE"}"#).expect("mine");
        let result = numera_core::storage::first_run::import_from(&src, &dst).expect("import");
        assert!(result.skipped.iter().any(|s| s.starts_with("profiles")));
        assert_eq!(raw(&dst, "profiles/a.json")["id"], "MINE");
    }
);
source_test!(
    the_source_is_left_untouched,
    "tests/test_portable_root.py::test_the_source_is_left_untouched",
    {
        let temp = temp();
        let root = temp.path();
        let src = legacy(root);
        let dst = root.join("new");
        numera_core::storage::first_run::import_from(&src, &dst).expect("import");
        assert!(src.join("profiles/a.json").is_file());
        assert!(src.join("proxies.json").is_file());
    }
);
source_test!(
    it_runs_once_and_then_stops,
    "tests/test_portable_root.py::test_it_runs_once_and_then_stops",
    {
        let temp = temp();
        let root = temp.path();
        let src = legacy(root);
        let dst = root.join("new");
        assert!(
            numera_core::storage::first_run::run_if_needed(&dst, std::slice::from_ref(&src))
                .expect("first")
                .is_some()
        );
        assert!(
            numera_core::storage::first_run::run_if_needed(&dst, &[src])
                .expect("second")
                .is_none()
        );
        assert!(dst.join(".imported").is_file());
    }
);
source_test!(
    a_marker_is_written_even_with_nothing_to_import,
    "tests/test_portable_root.py::test_a_marker_is_written_even_with_nothing_to_import",
    {
        let temp = temp();
        let root = temp.path();
        let dst = root.join("new");
        assert!(
            numera_core::storage::first_run::run_if_needed(&dst, &[root.join("nope")])
                .expect("nothing")
                .is_none()
        );
        assert!(dst.join(".imported").exists());
    }
);
source_test!(
    importing_from_the_destination_itself_is_a_no_op,
    "tests/test_portable_root.py::test_importing_from_the_destination_itself_is_a_no_op",
    {
        let temp = temp();
        let root = temp.path();
        let src = legacy(root);
        assert!(
            numera_core::storage::first_run::run_if_needed(&src, std::slice::from_ref(&src))
                .expect("same destination")
                .is_none()
        );
        assert!(src.join("profiles/a.json").is_file());
    }
);
source_test!(
    credentials_never_reach_the_log_case_011,
    "tests/test_portable_root.py::test_credentials_never_reach_the_log[connecting to http://alice:hunter2@gw.example:3128-hunter2]",
    {
        let out =
            numera_core::runtime_log::redact("connecting to http://alice:hunter2@gw.example:3128");
        assert!(!out.contains("hunter2"));
        assert!(out.contains("[REDACTED]"));
    }
);
source_test!(
    credentials_never_reach_the_log_case_012,
    "tests/test_portable_root.py::test_credentials_never_reach_the_log[proxy pass=s3cr3t-value applied-s3cr3t-value]",
    {
        let out = numera_core::runtime_log::redact("proxy pass=s3cr3t-value applied");
        assert!(!out.contains("s3cr3t-value"));
        assert!(out.contains("[REDACTED]"));
    }
);
source_test!(
    credentials_never_reach_the_log_case_013,
    "tests/test_portable_root.py::test_credentials_never_reach_the_log[{\"password\": \"topsecret\"}-topsecret]",
    {
        let out = numera_core::runtime_log::redact("{\"password\": \"topsecret\"}");
        assert!(!out.contains("topsecret"));
        assert!(out.contains("[REDACTED]"));
    }
);
source_test!(
    credentials_never_reach_the_log_case_014,
    "tests/test_portable_root.py::test_credentials_never_reach_the_log[Authorization: Bearer abcdef0123456789-abcdef0123456789]",
    {
        let out = numera_core::runtime_log::redact("Authorization: Bearer abcdef0123456789");
        assert!(!out.contains("abcdef0123456789"));
        assert!(out.contains("[REDACTED]"));
    }
);
source_test!(
    credentials_never_reach_the_log_case_015,
    "tests/test_portable_root.py::test_credentials_never_reach_the_log[api_key: sk-live-9f8e7d6c5b4a-sk-live-9f8e7d6c5b4a]",
    {
        let out = numera_core::runtime_log::redact("api_key: sk-live-9f8e7d6c5b4a");
        assert!(!out.contains("sk-live-9f8e7d6c5b4a"));
        assert!(out.contains("[REDACTED]"));
    }
);
source_test!(
    redaction_leaves_ordinary_text_alone,
    "tests/test_portable_root.py::test_redaction_leaves_ordinary_text_alone",
    {
        let line = "launched profile a3bcf1a93b22 with engine camoufox on 127.0.0.1:8787";
        assert_eq!(numera_core::runtime_log::redact(line), line);
    }
);
source_test!(
    redaction_is_applied_by_the_handler_not_the_call_site,
    "tests/test_portable_root.py::test_redaction_is_applied_by_the_handler_not_the_call_site",
    {
        let temp = temp();
        let root = temp.path();
        let log = numera_core::runtime_log::RuntimeLog::new(root, "source-storage");
        let path = log.path().expect("log created").to_owned();
        log.message(
            "WARNING",
            "plugin",
            "upstream http://bob:leakme@proxy.example:8080 refused",
        );
        drop(log);
        let text = fs::read_to_string(path).expect("log contents");
        assert!(!text.contains("leakme"));
        assert!(text.contains("[REDACTED]"));
    }
);
source_test!(
    redaction_does_not_break_numeric_format_args,
    "tests/test_portable_root.py::test_redaction_does_not_break_numeric_format_args",
    {
        let temp = temp();
        let root = temp.path();
        let log = numera_core::runtime_log::RuntimeLog::new(root, "source-storage");
        let path = log.path().expect("log created").to_owned();
        log.message(
            "WARNING",
            "plugin",
            &format!(
                "imported from {} - {} profile(s), {:.1} MB",
                "/old/place", 3, 1.5
            ),
        );
        drop(log);
        let text = fs::read_to_string(path).expect("log contents");
        assert!(text.contains("3 profile(s)") && text.contains("1.5 MB"));
    }
);
source_test!(
    redaction_still_scrubs_string_args,
    "tests/test_portable_root.py::test_redaction_still_scrubs_string_args",
    {
        let temp = temp();
        let root = temp.path();
        let log = numera_core::runtime_log::RuntimeLog::new(root, "source-storage");
        let path = log.path().expect("log created").to_owned();
        log.message(
            "WARNING",
            "plugin",
            &format!("upstream {} failed", "http://bob:hunter2@gw:3128"),
        );
        drop(log);
        let text = fs::read_to_string(path).expect("log contents");
        assert!(!text.contains("hunter2"));
        assert!(text.contains("[REDACTED]"));
    }
);
source_test!(
    setup_reports_failure_instead_of_claiming_a_file,
    "tests/test_portable_root.py::test_setup_reports_failure_instead_of_claiming_a_file",
    {
        let temp = temp();
        let root = temp.path();
        let path = root.join("not-a-directory");
        fs::write(&path, b"blocked").expect("owned blocker");
        let output = process::output(process::start(
            "portable_root::setup_reports_failure_instead_of_claiming_a_file",
            root,
            "logger",
            "log-failure",
            path.to_str().expect("blocker path"),
        ));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Runtime file logging unavailable")
        );
    }
);
source_test!(
    a_partly_failed_import_is_retried_on_the_next_run,
    "tests/test_portable_root.py::test_a_partly_failed_import_is_retried_on_the_next_run",
    {
        let temp = temp();
        let root = temp.path();
        let src = legacy(root);
        let dst = root.join("new");
        let first = testing::with_observer(error_at("before-copy-file", "a.json", 13), || {
            numera_core::storage::first_run::run_if_needed(&dst, std::slice::from_ref(&src))
        })
        .expect("first run")
        .expect("summary");
        assert!(!first.failed.is_empty());
        assert!(!dst.join(".imported").exists());
        let second = numera_core::storage::first_run::run_if_needed(&dst, &[src])
            .expect("retry")
            .expect("summary");
        assert!(second.failed.is_empty());
        assert!(dst.join("profiles/a.json").is_file());
        assert!(dst.join(".imported").exists());
    }
);
source_test!(
    an_existing_directory_is_merged_not_skipped,
    "tests/test_portable_root.py::test_an_existing_directory_is_merged_not_skipped",
    {
        let temp = temp();
        let root = temp.path();
        let src = root.join("old");
        fs::create_dir_all(src.join("profiles")).expect("source");
        for i in 0..3 {
            fs::write(src.join(format!("profiles/old{i}.json")), b"{}").expect("legacy profile");
        }
        let dst = root.join("new");
        fs::create_dir_all(dst.join("profiles")).expect("destination");
        fs::write(dst.join("profiles/mine.json"), br#"{"keep": true}"#).expect("mine");
        numera_core::storage::first_run::run_if_needed(&dst, &[src]).expect("merge");
        assert_eq!(
            json_files(&dst, "profiles"),
            ["mine.json", "old0.json", "old1.json", "old2.json"]
        );
        assert_eq!(
            fs::read(dst.join("profiles/mine.json")).expect("mine bytes"),
            br#"{"keep": true}"#
        );
    }
);
source_test!(
    the_import_covers_everything_the_product_stores,
    "tests/test_portable_root.py::test_the_import_covers_everything_the_product_stores",
    {
        let temp = temp();
        let root = temp.path();
        let src = legacy(root);
        let dst = root.join("new");
        for name in ["dashboard-config.json", "tokens.json", "schedules.json"] {
            fs::write(src.join(name), b"{}").expect("source record");
        }
        fs::create_dir_all(src.join("extensions/abc")).expect("extension");
        numera_core::storage::first_run::run_if_needed(&dst, &[src]).expect("import all");
        for name in ["dashboard-config.json", "tokens.json", "schedules.json"] {
            assert!(dst.join(name).is_file(), "{name}");
        }
        assert!(dst.join("extensions/abc").is_dir());
    }
);
source_test!(
    an_existing_but_unwritable_portable_dir_does_not_win,
    "tests/test_portable_root.py::test_an_existing_but_unwritable_portable_dir_does_not_win",
    {
        let temp = temp();
        let root = temp.path();
        fs::create_dir_all(root.join("app").parent().expect("parent")).expect("parent");
        fs::create_dir_all(root.join("app")).expect("app");
        fs::write(root.join("app/data"), b"not a writable directory").expect("unwritable app");
        assert_eq!(
            numera_core::storage::data_root::resolve(
                None,
                None,
                &root.join("app"),
                &root.join("home")
            )
            .expect("fallback"),
            root.join("home/.numera")
        );
    }
);
