use super::*;
source_test!(
    absent_keys_are_left_alone,
    "tests/test_bulk.py::test_absent_keys_are_left_alone",
    {
        let mut p = profile("a", 7);
        p.notes = "keep me".into();
        p.tags = vec!["x".into()];
        assert!(
            bulk_core::apply(&mut p, &json!({}), None)
                .expect("empty patch")
                .is_empty()
        );
        assert_eq!(p.notes, "keep me");
        assert_eq!(p.tags, ["x"]);
    }
);
source_test!(
    engine_and_notes,
    "tests/test_bulk.py::test_engine_and_notes",
    {
        let mut p = profile("a", 7);
        let changed =
            bulk_core::apply(&mut p, &json!({"engine":"cloak","notes":"q3"}), None).expect("patch");
        assert_eq!(
            changed.into_iter().collect::<BTreeSet<_>>(),
            BTreeSet::from(["engine".into(), "notes".into()])
        );
        assert_eq!(p.engine, "cloak");
        assert_eq!(p.notes, "q3");
    }
);
source_test!(
    setting_the_same_value_is_not_a_change,
    "tests/test_bulk.py::test_setting_the_same_value_is_not_a_change",
    {
        let mut p = profile("a", 7);
        p.engine = "cloak".into();
        assert!(
            bulk_core::apply(&mut p, &json!({"engine":"cloak"}), None)
                .expect("same patch")
                .is_empty()
        );
    }
);
source_test!(
    locale_moves_timezone_and_languages,
    "tests/test_bulk.py::test_locale_moves_timezone_and_languages",
    {
        let mut p = profile("a", 7);
        bulk_core::apply(&mut p, &json!({"locale":"de-DE"}), None).expect("locale patch");
        assert_eq!(p.fingerprint.locale, "de-DE");
        assert_eq!(p.fingerprint.timezone, "Europe/Berlin");
        assert_eq!(p.fingerprint.languages[0], "de-DE");
    }
);
source_test!(
    unknown_locale_is_ignored,
    "tests/test_bulk.py::test_unknown_locale_is_ignored",
    {
        let mut p = profile("a", 7);
        let before = p.fingerprint.timezone.clone();
        assert!(
            !bulk_core::apply(&mut p, &json!({"locale":"xx-XX"}), None)
                .expect("unknown locale")
                .iter()
                .any(|v| v == "locale")
        );
        assert_eq!(p.fingerprint.timezone, before);
    }
);
source_test!(
    tags_added_and_removed,
    "tests/test_bulk.py::test_tags_added_and_removed",
    {
        let mut p = profile("a", 7);
        p.tags = vec!["old".into(), "keep".into()];
        assert_eq!(
            bulk_core::apply(
                &mut p,
                &json!({"add_tags":["new"],"remove_tags":["old"]}),
                None
            )
            .expect("tag patch"),
            ["tags"]
        );
        assert_eq!(p.tags, ["keep", "new"]);
    }
);
source_test!(
    tag_already_present_is_not_a_change,
    "tests/test_bulk.py::test_tag_already_present_is_not_a_change",
    {
        let mut p = profile("a", 7);
        p.tags = vec!["keep".into()];
        assert!(
            bulk_core::apply(&mut p, &json!({"add_tags":["keep"]}), None)
                .expect("same tag")
                .is_empty()
        );
    }
);
source_test!(
    proxy_can_be_set_and_explicitly_cleared,
    "tests/test_bulk.py::test_proxy_can_be_set_and_explicitly_cleared",
    {
        let mut p = profile("a", 7);
        bulk_core::apply(
            &mut p,
            &json!({"proxy":{"server":"http://h:1","country":"DE"}}),
            None,
        )
        .expect("set proxy");
        assert_eq!(
            p.proxy.as_ref().expect("proxy").country.as_deref(),
            Some("DE")
        );
        assert_eq!(
            bulk_core::apply(&mut p, &json!({"proxy":null}), None).expect("remove proxy"),
            ["proxy"]
        );
        assert!(p.proxy.is_none());
    }
);
source_test!(
    regen_keeps_os_and_locale_but_changes_the_device,
    "tests/test_bulk.py::test_regen_keeps_os_and_locale_but_changes_the_device",
    {
        let mut p = profile("a", 7);
        let before = p.fingerprint.clone();
        bulk_core::apply(&mut p, &json!({"locale":"de-DE","regen":true}), Some(42))
            .expect("regenerate");
        assert_eq!(p.fingerprint.os, before.os);
        assert_eq!(p.fingerprint.locale, "de-DE");
        assert_ne!(p.fingerprint.seed, before.seed);
    }
);
source_test!(
    regenerated_fingerprint_is_still_coherent,
    "tests/test_bulk.py::test_regenerated_fingerprint_is_still_coherent",
    {
        let mut p = profile("a", 7);
        bulk_core::apply(&mut p, &json!({"regen":true}), Some(42)).expect("regenerate");
        assert!(
            identity::validate(&p.fingerprint)
                .expect("coherence")
                .is_empty()
        );
    }
);
source_test!(select_by_tag, "tests/test_bulk.py::test_select_by_tag", {
    let ps = three();
    assert_eq!(
        bulk_core::select(&ps, &[], Some("prod"), false)
            .iter()
            .map(|p| p.name.as_str().expect("name"))
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
});
source_test!(
    select_by_name_or_id,
    "tests/test_bulk.py::test_select_by_name_or_id",
    {
        let ps = three();
        assert_eq!(
            bulk_core::select(&ps, &["c".into(), ps[0].id.clone()], None, false)
                .iter()
                .map(|p| p.name.as_str().expect("name"))
                .collect::<Vec<_>>(),
            ["a", "c"]
        );
    }
);
source_test!(
    select_never_returns_duplicates,
    "tests/test_bulk.py::test_select_never_returns_duplicates",
    {
        assert_eq!(
            bulk_core::select(&three(), &["a".into()], Some("prod"), false)
                .iter()
                .map(|p| p.name.as_str().expect("name"))
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
);
source_test!(select_all, "tests/test_bulk.py::test_select_all", {
    assert_eq!(bulk_core::select(&three(), &[], None, true).len(), 3);
});
source_test!(
    select_nothing_by_default,
    "tests/test_bulk.py::test_select_nothing_by_default",
    {
        assert!(bulk_core::select(&three(), &[], None, false).is_empty());
    }
);
source_test!(
    generate_profiles_count_and_names,
    "tests/test_bulk.py::test_generate_profiles_count_and_names",
    {
        let ps = batch(
            5,
            BatchRequest {
                prefix: "Batch".into(),
                start: 1,
                pad: 3,
                ..Default::default()
            },
        );
        assert_eq!(ps.len(), 5);
        assert_eq!(
            ps.iter()
                .map(|p| p.name.as_str().expect("name"))
                .collect::<Vec<_>>(),
            [
                "Batch-001",
                "Batch-002",
                "Batch-003",
                "Batch-004",
                "Batch-005"
            ]
        );
    }
);
source_test!(
    generate_profiles_are_all_coherent,
    "tests/test_bulk.py::test_generate_profiles_are_all_coherent",
    {
        for p in batch(
            50,
            BatchRequest {
                oses: vec!["windows".into(), "macos".into(), "linux".into()],
                locales: vec!["en-US".into(), "de-DE".into()],
                ..Default::default()
            },
        ) {
            assert!(
                identity::validate(&p.fingerprint)
                    .expect("coherence")
                    .is_empty(),
                "{}",
                p.name
            );
        }
    }
);
source_test!(
    generate_profiles_spread_across_oses,
    "tests/test_bulk.py::test_generate_profiles_spread_across_oses",
    {
        let ps = batch(
            6,
            BatchRequest {
                oses: vec!["windows".into(), "macos".into()],
                ..Default::default()
            },
        );
        assert_eq!(
            ps.iter()
                .map(|p| p.fingerprint.os.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["windows", "macos"])
        );
        assert_eq!(
            ps.iter().filter(|p| p.fingerprint.os == "windows").count(),
            3
        );
    }
);
source_test!(
    generate_profiles_are_distinct_identities,
    "tests/test_bulk.py::test_generate_profiles_are_distinct_identities",
    {
        assert_eq!(
            batch(
                20,
                BatchRequest {
                    oses: vec!["windows".into()],
                    ..Default::default()
                }
            )
            .iter()
            .map(|p| p.fingerprint.seed.to_string())
            .collect::<BTreeSet<_>>()
            .len(),
            20
        );
    }
);
source_test!(
    generate_profiles_restricts_chrome_versions,
    "tests/test_bulk.py::test_generate_profiles_restricts_chrome_versions",
    {
        for p in batch(
            10,
            BatchRequest {
                versions: vec!["128.0.0.0".into()],
                ..Default::default()
            },
        ) {
            assert_eq!(p.fingerprint.chrome_version, "128.0.0.0");
            assert!(p.fingerprint.user_agent.contains("Chrome/128.0.0.0"));
        }
    }
);
source_test!(
    generate_profiles_reproducible_with_seed,
    "tests/test_bulk.py::test_generate_profiles_reproducible_with_seed",
    {
        let make = || {
            batch(
                4,
                BatchRequest {
                    oses: vec!["windows".into(), "macos".into()],
                    seed: 99,
                    ..Default::default()
                },
            )
            .into_iter()
            .map(|p| serde_json::to_value(p.fingerprint).expect("fingerprint"))
            .collect::<Vec<_>>()
        };
        assert_eq!(make(), make());
    }
);
source_test!(
    generate_profiles_tags_and_engine,
    "tests/test_bulk.py::test_generate_profiles_tags_and_engine",
    {
        assert!(
            batch(
                3,
                BatchRequest {
                    engine: "cloak".into(),
                    tags: vec!["bulk".into(), "q3".into()],
                    ..Default::default()
                }
            )
            .iter()
            .all(|p| p.engine == "cloak" && p.tags == ["bulk", "q3"])
        );
    }
);
source_test!(
    a_version_override_keeps_the_android_ua_coherent,
    "tests/test_bulk.py::test_a_version_override_keeps_the_android_ua_coherent",
    {
        for p in batch(
            5,
            BatchRequest {
                oses: vec!["android".into()],
                versions: vec!["131.0.0.0".into()],
                ..Default::default()
            },
        ) {
            let fp = p.fingerprint;
            assert!(!fp.os_version.is_empty() && !fp.device_model.is_empty());
            assert!(
                fp.user_agent
                    .contains(&format!("Android {}; {}", fp.os_version, fp.device_model))
            );
            assert!(fp.user_agent.contains("131.0.0.0"));
        }
    }
);
source_test!(
    a_version_override_leaves_a_desktop_ua_alone,
    "tests/test_bulk.py::test_a_version_override_leaves_a_desktop_ua_alone",
    {
        for p in batch(
            3,
            BatchRequest {
                oses: vec!["windows".into()],
                versions: vec!["131.0.0.0".into()],
                ..Default::default()
            },
        ) {
            assert!(p.fingerprint.user_agent.contains("Windows NT"));
            assert!(p.fingerprint.user_agent.contains("131.0.0.0"));
            assert!(!p.fingerprint.user_agent.contains("Android"));
        }
    }
);
