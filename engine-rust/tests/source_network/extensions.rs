use super::*;
source_test!(
    c000_no_extensions_means_no_flags,
    "tests/test_extensions.py::test_no_extensions_means_no_flags",
    {
        let tmp = common::TempDirectory::new("source-net-no-flags").expect("root");
        assert!(ext_flags(vec![], tmp.path()).is_empty());
    }
);
source_test!(
    c000_both_flags_are_emitted,
    "tests/test_extensions.py::test_both_flags_are_emitted",
    {
        let tmp = common::TempDirectory::new("source-net-two-flags").expect("root");
        let ext = tmp.path().join("ublock");
        fs::create_dir(&ext).expect("folder");
        let flags = ext_flags(vec![ext.to_string_lossy().into_owned()], tmp.path());
        assert_eq!(flags.len(), 2);
        assert!(flags[0].starts_with("--disable-extensions-except="));
        assert!(flags[1].starts_with("--load-extension="));
        let selected = flags[1]
            .strip_prefix("--load-extension=")
            .expect("extension flag");
        assert_eq!(
            Path::new(selected)
                .canonicalize()
                .expect("selected directory"),
            ext.canonicalize().expect("extension directory")
        );
    }
);
source_test!(
    c000_multiple_paths_are_comma_joined,
    "tests/test_extensions.py::test_multiple_paths_are_comma_joined",
    {
        let tmp = common::TempDirectory::new("source-net-joined-flags").expect("root");
        let paths = ["a", "b"].map(|n| {
            let p = tmp.path().join(n);
            fs::create_dir(&p).expect("folder");
            p.to_string_lossy().into_owned()
        });
        let flags = ext_flags(paths.to_vec(), tmp.path());
        assert_eq!(flags[1].matches(',').count(), 1);
    }
);
source_test!(
    c000_missing_folders_are_dropped,
    "tests/test_extensions.py::test_missing_folders_are_dropped",
    {
        let tmp = common::TempDirectory::new("source-net-missing-flags").expect("root");
        let real = tmp.path().join("real");
        fs::create_dir(&real).expect("folder");
        let flags = ext_flags(
            vec![
                real.to_string_lossy().into_owned(),
                tmp.path().join("gone").to_string_lossy().into_owned(),
            ],
            tmp.path(),
        );
        assert!(flags[1].contains("real"));
        assert!(!flags[1].contains("gone"));
    }
);
source_test!(
    c000_only_missing_folders_means_no_flags,
    "tests/test_extensions.py::test_only_missing_folders_means_no_flags",
    {
        let tmp = common::TempDirectory::new("source-net-missing-only").expect("root");
        assert!(
            ext_flags(
                vec![tmp.path().join("nope").to_string_lossy().into_owned()],
                tmp.path()
            )
            .is_empty()
        );
    }
);
source_test!(
    c000_extensions_survive_a_save_round_trip,
    "tests/test_extensions.py::test_extensions_survive_a_save_round_trip",
    {
        let mut p = profile("x");
        p.extensions = vec!["/some/ext".into()];
        let record = serde_json::to_value(&p).expect("profile record");
        assert_eq!(
            Profile::from_record(&record)
                .expect("model round trip")
                .extensions,
            ["/some/ext"]
        );
    }
);
source_test!(
    c000_profiles_saved_before_extensions_existed_still_load,
    "tests/test_extensions.py::test_profiles_saved_before_extensions_existed_still_load",
    {
        let mut record = serde_json::to_value(profile("x")).expect("profile record");
        record
            .as_object_mut()
            .expect("profile")
            .remove("extensions");
        assert!(
            Profile::from_record(&record)
                .expect("old profile")
                .extensions
                .is_empty()
        );
    }
);
source_test!(
    c000_default_extensions_round_trip,
    "tests/test_extensions.py::test_default_extensions_round_trip",
    {
        let tmp = common::TempDirectory::new("source-net-defaults").expect("root");
        assert!(stored_defaults(tmp.path()).is_empty());
        let ext = unpacked(tmp.path(), "ublock");
        config(tmp.path())
            .set_default_extensions(vec![ext.to_string_lossy().into_owned()])
            .expect("defaults save");
        assert_eq!(
            stored_defaults(tmp.path()),
            json!([ext]).as_array().expect("expected").clone()
        );
    }
);
source_test!(
    c000_new_profiles_inherit_the_default_set,
    "tests/test_extensions.py::test_new_profiles_inherit_the_default_set",
    {
        let tmp = common::TempDirectory::new("source-net-inherit").expect("root");
        let ext = unpacked(tmp.path(), "shared-ext");
        config(tmp.path())
            .set_default_extensions(vec![ext.to_string_lossy().into_owned()])
            .expect("default extensions");
        let outcome = commands(tmp.path(), |c| {
            c.create(
                json!({"name":"inherits"}).as_object().expect("args"),
                1000.0,
            )
        })
        .expect("native create");
        assert_eq!(outcome.exit, 0);
        assert_eq!(
            repository(tmp.path())
                .resolve("inherits", 1001.0)
                .expect("lookup")
                .expect("profile")
                .extensions,
            [ext.to_string_lossy().into_owned()]
        );
    }
);
source_test!(
    c000_an_empty_folder_is_not_an_extension,
    "tests/test_extensions.py::test_an_empty_folder_is_not_an_extension",
    {
        let tmp = common::TempDirectory::new("source-net-empty-extension").expect("root");
        let empty = tmp.path().join("not-an-extension");
        fs::create_dir(&empty).expect("empty");
        assert_ne!(
            add_extension(tmp.path(), empty.to_str().expect("path")).exit,
            0
        );
        assert!(
            repository(tmp.path())
                .resolve("target", 1001.0)
                .expect("profile")
                .expect("target")
                .extensions
                .is_empty()
        );
    }
);
source_test!(
    c000_a_web_store_url_is_not_an_extension_path,
    "tests/test_extensions.py::test_a_web_store_url_is_not_an_extension_path",
    {
        let tmp = common::TempDirectory::new("source-net-url-extension").expect("root");
        assert_ne!(
            add_extension(tmp.path(), "https://chrome.google.com/webstore/detail/abc").exit,
            0
        );
        assert!(
            repository(tmp.path())
                .resolve("target", 1001.0)
                .expect("profile")
                .expect("target")
                .extensions
                .is_empty()
        );
    }
);
source_test!(
    c000_a_real_unpacked_extension_is_still_added,
    "tests/test_extensions.py::test_a_real_unpacked_extension_is_still_added",
    {
        let tmp = common::TempDirectory::new("source-net-add-extension").expect("root");
        let ext = unpacked(tmp.path(), "ublock");
        assert_eq!(
            add_extension(tmp.path(), ext.to_str().expect("path")).exit,
            0
        );
        assert_eq!(
            repository(tmp.path())
                .resolve("target", 1001.0)
                .expect("profile")
                .expect("target")
                .extensions,
            [ext.to_string_lossy().into_owned()]
        );
    }
);
source_test!(
    c000_bulk_apply_pushes_extensions_onto_existing_profiles,
    "tests/test_extensions.py::test_bulk_apply_pushes_extensions_onto_existing_profiles",
    {
        let mut p = profile("old");
        assert!(p.extensions.is_empty());
        let changed =
            numera_core::bulk::apply(&mut p, &json!({"extensions":["/opt/ext/ublock"]}), None)
                .expect("bulk extension patch");
        assert!(changed.iter().any(|f| f == "extensions"));
        assert_eq!(p.extensions, ["/opt/ext/ublock"]);
    }
);
