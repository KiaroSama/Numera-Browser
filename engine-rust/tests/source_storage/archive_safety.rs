use super::*;
source_test!(
    a_normal_backup_still_round_trips,
    "tests/test_archive_safety.py::test_a_normal_backup_still_round_trips",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let ud = store(root).user_data_path(&p.id).expect("session");
        fs::create_dir_all(ud.join("Default")).expect("Default");
        fs::write(ud.join("Default/Cookies"), b"jar").expect("cookies");
        let zip = root.join("backup.zip");
        store(root)
            .backup(&p.id, &zip, "source-storage-host")
            .expect("backup");
        let other = root.join("restored");
        let back = store(&other)
            .restore(&zip, false, "source-storage-host", 1001.0)
            .expect("restore");
        assert_eq!(back.name, "alpha");
        assert_eq!(
            fs::read(
                store(&other)
                    .user_data_path(&back.id)
                    .expect("restored session")
                    .join("Default/Cookies")
            )
            .expect("restored cookies"),
            b"jar"
        );
    }
);
source_test!(
    a_parent_traversal_member_is_refused,
    "tests/test_archive_safety.py::test_a_parent_traversal_member_is_refused",
    {
        let temp = temp();
        let root = temp.path();
        let zip = root.join("evil.zip");
        let before = store(root)
            .list(1000.0)
            .expect("before")
            .into_iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>();
        archived_profile(&zip, &[("user-data/../../../pwned.txt", b"x")]);
        let error = store(root)
            .restore(&zip, false, "source-storage-host", 1001.0)
            .err()
            .expect("unsafe member refusal");
        assert!(
            error.to_string().to_lowercase().contains("escape")
                || error.to_string().to_lowercase().contains("drive"),
            "{error}"
        );
        assert_eq!(
            store(root)
                .list(1002.0)
                .expect("after")
                .into_iter()
                .map(|p| p.id)
                .collect::<BTreeSet<_>>(),
            before
        );
        assert!(!root.join("pwned.txt").exists());
        assert!(!root.parent().expect("parent").join("pwned.txt").exists());
    }
);
source_test!(
    a_leading_slash_is_neutralised_rather_than_escaping,
    "tests/test_archive_safety.py::test_a_leading_slash_is_neutralised_rather_than_escaping",
    {
        let temp = temp();
        let root = temp.path();
        let zip = root.join("odd.zip");
        archived_profile(&zip, &[("user-data//etc/pwned", b"x")]);
        let data = root.join("data");
        let p = store(&data)
            .restore(&zip, false, "source-storage-host", 1000.0)
            .expect("restore");
        assert!(
            store(&data)
                .user_data_path(&p.id)
                .expect("session")
                .join("etc/pwned")
                .is_file()
        );
        assert!(!root.join("etc").exists());
    }
);
source_test!(
    a_windows_drive_member_is_refused,
    "tests/test_archive_safety.py::test_a_windows_drive_member_is_refused",
    {
        let temp = temp();
        let root = temp.path();
        let zip = root.join("evil.zip");
        let before = store(root)
            .list(1000.0)
            .expect("before")
            .into_iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>();
        archived_profile(&zip, &[("user-data/C:/pwned.txt", b"x")]);
        let error = store(root)
            .restore(&zip, false, "source-storage-host", 1001.0)
            .err()
            .expect("unsafe member refusal");
        assert!(
            error.to_string().to_lowercase().contains("escape")
                || error.to_string().to_lowercase().contains("drive"),
            "{error}"
        );
        assert_eq!(
            store(root)
                .list(1002.0)
                .expect("after")
                .into_iter()
                .map(|p| p.id)
                .collect::<BTreeSet<_>>(),
            before
        );
        assert!(!root.join("pwned.txt").exists());
        assert!(!root.parent().expect("parent").join("pwned.txt").exists());
    }
);
source_test!(
    a_hostile_archive_leaves_no_half_written_profile,
    "tests/test_archive_safety.py::test_a_hostile_archive_leaves_no_half_written_profile",
    {
        let temp = temp();
        let root = temp.path();
        let zip = root.join("evil.zip");
        let before = store(root)
            .list(1000.0)
            .expect("before")
            .into_iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>();
        archived_profile(
            &zip,
            &[
                ("user-data/Default/ok.txt", b"fine"),
                ("user-data/../../../pwned.txt", b"x"),
            ],
        );
        let error = store(root)
            .restore(&zip, false, "source-storage-host", 1001.0)
            .err()
            .expect("unsafe member refusal");
        assert!(
            error.to_string().to_lowercase().contains("escape")
                || error.to_string().to_lowercase().contains("drive"),
            "{error}"
        );
        assert_eq!(
            store(root)
                .list(1002.0)
                .expect("after")
                .into_iter()
                .map(|p| p.id)
                .collect::<BTreeSet<_>>(),
            before
        );
        assert!(!root.join("pwned.txt").exists());
        assert!(!root.parent().expect("parent").join("pwned.txt").exists());
    }
);
source_test!(
    a_malformed_profile_json_is_a_clear_error,
    "tests/test_archive_safety.py::test_a_malformed_profile_json_is_a_clear_error",
    {
        let temp = temp();
        let root = temp.path();
        let zip = root.join("bad.zip");
        archive(&zip, None, &[("profile.json", b"{ not json")]);
        assert!(
            store(root)
                .restore(&zip, false, "source-storage-host", 1000.0)
                .is_err()
        );
    }
);
source_test!(
    an_archive_with_no_profile_json_is_a_clear_error,
    "tests/test_archive_safety.py::test_an_archive_with_no_profile_json_is_a_clear_error",
    {
        let temp = temp();
        let root = temp.path();
        let zip = root.join("bad.zip");
        archive(&zip, None, &[("user-data/Default/x", b"x")]);
        assert!(
            store(root)
                .restore(&zip, false, "source-storage-host", 1000.0)
                .is_err()
        );
    }
);
source_test!(
    restore_replaces_the_session_rather_than_merging_into_it,
    "tests/test_archive_safety.py::test_restore_replaces_the_session_rather_than_merging_into_it",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let ud = store(root).user_data_path(&p.id).expect("session");
        fs::write(ud.join("stale.txt"), b"from old session").expect("stale");
        let zip = root.join("b.zip");
        archive(
            &zip,
            Some(serde_json::to_value(&p).expect("profile")),
            &[("user-data/fresh.txt", b"new")],
        );
        store(root)
            .restore(&zip, false, "source-storage-host", 1001.0)
            .expect("restore");
        assert!(ud.join("fresh.txt").exists());
        assert!(!ud.join("stale.txt").exists());
    }
);
source_test!(
    a_sibling_prefix_member_cannot_escape_the_extension_folder,
    "tests/test_archive_safety.py::test_a_sibling_prefix_member_cannot_escape_the_extension_folder",
    {
        let temp = temp();
        let root = temp.path();
        let dest = root.join("ext");
        fs::create_dir(&dest).expect("extension folder");
        let zip = root.join("e.zip");
        archive(
            &zip,
            None,
            &[
                ("manifest.json", br#"{"name":"x","version":"1"}"#),
                ("../ext-evil/pwned.txt", b"console.log(1)"),
            ],
        );
        let result = numera_core::storage::extension_package::unpack(
            &fs::read(&zip).expect("zip bytes"),
            &dest,
            false,
        );
        let error = result.expect_err("escape refusal");
        assert!(error.to_string().contains("outside the folder"), "{error}");
        assert!(!root.join("ext-evil").exists());
        assert!(!root.join("pwned.txt").exists());
    }
);
source_test!(
    a_parent_traversal_member_cannot_escape_the_extension_folder,
    "tests/test_archive_safety.py::test_a_parent_traversal_member_cannot_escape_the_extension_folder",
    {
        let temp = temp();
        let root = temp.path();
        let dest = root.join("ext");
        fs::create_dir(&dest).expect("extension folder");
        let zip = root.join("e.zip");
        archive(
            &zip,
            None,
            &[
                ("manifest.json", br#"{"name":"x","version":"1"}"#),
                ("../../pwned.txt", b"console.log(1)"),
            ],
        );
        let result = numera_core::storage::extension_package::unpack(
            &fs::read(&zip).expect("zip bytes"),
            &dest,
            false,
        );
        let error = result.expect_err("escape refusal");
        assert!(error.to_string().contains("outside the folder"), "{error}");
        assert!(!root.join("ext-evil").exists());
        assert!(!root.join("pwned.txt").exists());
    }
);
source_test!(
    an_ordinary_extension_still_unpacks,
    "tests/test_archive_safety.py::test_an_ordinary_extension_still_unpacks",
    {
        let temp = temp();
        let root = temp.path();
        let dest = root.join("ext");
        fs::create_dir(&dest).expect("extension folder");
        let zip = root.join("e.zip");
        archive(
            &zip,
            None,
            &[
                ("manifest.json", br#"{"name":"x","version":"1"}"#),
                ("js/content.js", b"console.log(1)"),
            ],
        );
        let result = numera_core::storage::extension_package::unpack(
            &fs::read(&zip).expect("zip bytes"),
            &dest,
            false,
        );
        result.expect("ordinary unpack");
        assert!(dest.join("manifest.json").is_file());
        assert!(dest.join("js/content.js").is_file());
    }
);
source_test!(
    an_icon_path_cannot_point_at_a_sibling_directory,
    "tests/test_archive_safety.py::test_an_icon_path_cannot_point_at_a_sibling_directory",
    {
        let temp = temp();
        let root = temp.path();
        let folder = root.join("ext");
        fs::create_dir_all(folder.join("js")).expect("extension");
        let evil = root.join("ext-evil");
        fs::create_dir(&evil).expect("sibling");
        fs::write(evil.join("secret.png"), b"x").expect("secret");
        fs::write(
            folder.join("manifest.json"),
            br#"{"name":"x","version":"1","icons":{"128":"../ext-evil/secret.png"}}"#,
        )
        .expect("manifest");
        assert!(
            numera_core::storage::extensions::describe(&folder)
                .expect("describe")
                .icon
                .is_none()
        );
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_012,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[CON]",
    {
        let error = paths::safe_id("CON").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_013,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[PRN]",
    {
        let error = paths::safe_id("PRN").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_014,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[AUX]",
    {
        let error = paths::safe_id("AUX").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_015,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[NUL]",
    {
        let error = paths::safe_id("NUL").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_016,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[COM1]",
    {
        let error = paths::safe_id("COM1").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_017,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[COM9]",
    {
        let error = paths::safe_id("COM9").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_018,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[LPT1]",
    {
        let error = paths::safe_id("LPT1").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_019,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[LPT9]",
    {
        let error = paths::safe_id("LPT9").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_020,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[con]",
    {
        let error = paths::safe_id("con").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_windows_device_name_is_not_a_usable_profile_id_case_021,
    "tests/test_archive_safety.py::test_a_windows_device_name_is_not_a_usable_profile_id[NuL]",
    {
        let error = paths::safe_id("NuL").expect_err("reserved portable device ID");
        assert!(error.to_string().contains("reserved device name"));
    }
);
source_test!(
    a_device_name_with_an_extension_is_refused_too_case_022,
    "tests/test_archive_safety.py::test_a_device_name_with_an_extension_is_refused_too[nul.json]",
    {
        assert!(paths::safe_id("nul.json").is_err());
    }
);
source_test!(
    a_device_name_with_an_extension_is_refused_too_case_023,
    "tests/test_archive_safety.py::test_a_device_name_with_an_extension_is_refused_too[COM1.backup]",
    {
        assert!(paths::safe_id("COM1.backup").is_err());
    }
);
source_test!(
    a_device_name_with_an_extension_is_refused_too_case_024,
    "tests/test_archive_safety.py::test_a_device_name_with_an_extension_is_refused_too[aux.tar.gz]",
    {
        assert!(paths::safe_id("aux.tar.gz").is_err());
    }
);
source_test!(
    a_device_name_with_an_extension_is_refused_too_case_025,
    "tests/test_archive_safety.py::test_a_device_name_with_an_extension_is_refused_too[CON.txt]",
    {
        assert!(paths::safe_id("CON.txt").is_err());
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_026,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[CONS]",
    {
        assert_eq!(paths::safe_id("CONS").expect("legal ID"), "CONS");
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_027,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[console]",
    {
        assert_eq!(paths::safe_id("console").expect("legal ID"), "console");
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_028,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[COM]",
    {
        assert_eq!(paths::safe_id("COM").expect("legal ID"), "COM");
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_029,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[COM10]",
    {
        assert_eq!(paths::safe_id("COM10").expect("legal ID"), "COM10");
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_030,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[LPT10]",
    {
        assert_eq!(paths::safe_id("LPT10").expect("legal ID"), "LPT10");
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_031,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[NULL]",
    {
        assert_eq!(paths::safe_id("NULL").expect("legal ID"), "NULL");
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_032,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[auxiliary]",
    {
        assert_eq!(paths::safe_id("auxiliary").expect("legal ID"), "auxiliary");
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_033,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[ab12cd34ef56]",
    {
        assert_eq!(
            paths::safe_id("ab12cd34ef56").expect("legal ID"),
            "ab12cd34ef56"
        );
    }
);
source_test!(
    names_that_merely_look_like_devices_are_fine_case_034,
    "tests/test_archive_safety.py::test_names_that_merely_look_like_devices_are_fine[a-b_c]",
    {
        assert_eq!(paths::safe_id("a-b_c").expect("legal ID"), "a-b_c");
    }
);
source_test!(
    profile_id_requires_a_complete_portable_filename_case_035,
    "tests/test_archive_safety.py::test_profile_id_requires_a_complete_portable_filename[valid\\n]",
    {
        assert!(paths::safe_id("valid\n").is_err());
    }
);
source_test!(
    profile_id_requires_a_complete_portable_filename_case_036,
    "tests/test_archive_safety.py::test_profile_id_requires_a_complete_portable_filename[valid\\r\\n]",
    {
        assert!(paths::safe_id("valid\r\n").is_err());
    }
);
source_test!(
    profile_id_requires_a_complete_portable_filename_case_037,
    "tests/test_archive_safety.py::test_profile_id_requires_a_complete_portable_filename[../escape]",
    {
        assert!(paths::safe_id("../escape").is_err());
    }
);
source_test!(
    profile_id_requires_a_complete_portable_filename_case_038,
    "tests/test_archive_safety.py::test_profile_id_requires_a_complete_portable_filename[a/b]",
    {
        assert!(paths::safe_id("a/b").is_err());
    }
);
source_test!(
    profile_id_requires_a_complete_portable_filename_case_039,
    "tests/test_archive_safety.py::test_profile_id_requires_a_complete_portable_filename[a\\\\b]",
    {
        assert!(paths::safe_id("a\\b").is_err());
    }
);
source_test!(
    json_replacement_never_overwrites_an_unowned_temporary_file,
    "tests/test_archive_safety.py::test_json_replacement_never_overwrites_an_unowned_temporary_file",
    {
        let temp = temp();
        let root = temp.path();
        let target = root.join("profile.json");
        let sentinel = root.join("unrelated.json");
        fs::write(&sentinel, b"preserve").expect("sentinel");
        let collision = Arc::new(Mutex::new(None));
        let observed = collision.clone();
        let error = testing::with_observer(
            move |event| {
                if event.operation == "before-temporary-create" {
                    fs::write(&event.path, b"preexisting owner")?;
                    *observed.lock().expect("observed") = Some(event.path.clone());
                }
                Ok(Action::Continue)
            },
            || write_json(&target, &json!({"name":"fixture"})),
        )
        .expect_err("create_new collision");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(sentinel).expect("sentinel"), b"preserve");
        assert!(!target.exists());
        assert_eq!(
            fs::read(
                collision
                    .lock()
                    .expect("collision")
                    .as_ref()
                    .expect("temporary")
            )
            .expect("existing temporary"),
            b"preexisting owner"
        );
    }
);
source_test!(
    profile_path_never_resolves_one_id_into_another,
    "tests/test_archive_safety.py::test_profile_path_never_resolves_one_id_into_another",
    {
        let temp = temp();
        let root = temp.path();
        let target = root.join("first.json");
        let other = root.join("second.json");
        fs::write(&other, b"preserve").expect("other owner");
        let error = testing::with_path_resolver(
            move |path| Ok((path == target).then(|| other.clone())),
            || paths::id_path(root, "first", ".json"),
        )
        .expect_err("linked ID");
        assert!(error.to_string().contains("linked"));
        assert_eq!(
            fs::read(root.join("second.json")).expect("other owner"),
            b"preserve"
        );
        // An actual same-file spelling alias is valid on a case-insensitive
        // host. A hard link supplies that same identity control on Linux.
        let stored = root.join("fixture.json");
        let alias = root.join("Fixture.json");
        fs::write(&stored, b"same owned file").expect("stored spelling");
        if !alias.exists() {
            fs::hard_link(&stored, &alias).expect("same-file alias");
        }
        let expected = alias.clone();
        testing::with_path_resolver(
            move |path| Ok((path == alias).then(|| stored.clone())),
            || {
                assert_eq!(
                    paths::id_path(root, "Fixture", ".json").expect("same file"),
                    expected
                )
            },
        );
    }
);
source_test!(
    a_linked_session_claim_stays_owned_instead_of_appearing_absent,
    "tests/test_archive_safety.py::test_a_linked_session_claim_stays_owned_instead_of_appearing_absent",
    {
        let temp = temp();
        let root = temp.path();
        use numera_core::ownership::claims::{self, Claim, State};
        let directory = root.join("sessions");
        fs::create_dir(&directory).expect("sessions");
        let claim = directory.join("fixture.json");
        fs::write(&claim, b"preserve uncertain owner").expect("claim");
        let other = directory.join("other.json");
        testing::with_path_resolver(
            move |path| Ok((path == claim).then(|| other.clone())),
            || {
                let claim = claims::read(root, "fixture");
                assert!(matches!(claim, Claim::Unreadable));
                assert_eq!(claims::state(&claim, "host"), State::Unknown);
                assert_eq!(
                    claims::refuse_if_busy(root, "fixture", "host")
                        .expect_err("busy")
                        .kind(),
                    io::ErrorKind::WouldBlock
                );
                assert!(matches!(claims::read(root, "../escape"), Claim::Absent));
            },
        );
        assert_eq!(
            fs::read(directory.join("fixture.json")).expect("uncertain claim"),
            b"preserve uncertain owner"
        );
    }
);
source_test!(
    archive_path_is_refused_before_probing_an_outside_target_case_043,
    "tests/test_archive_safety.py::test_archive_path_is_refused_before_probing_an_outside_target[../outside]",
    {
        let temp = temp();
        let root = temp.path();
        let records = root.join("records");
        fs::create_dir(&records).expect("records");
        let error = testing::with_path_resolver(
            |path| {
                panic!(
                    "escaping member reached physical resolution: {}",
                    path.display()
                )
            },
            || archive_paths::member_path(&records, "../outside"),
        )
        .expect_err("lexical escape");
        assert!(error.to_string().contains("escapes"));
    }
);
source_test!(
    archive_path_is_refused_before_probing_an_outside_target_case_044,
    "tests/test_archive_safety.py::test_archive_path_is_refused_before_probing_an_outside_target[../records-evil/file]",
    {
        let temp = temp();
        let root = temp.path();
        let records = root.join("records");
        fs::create_dir(&records).expect("records");
        let error = testing::with_path_resolver(
            |path| {
                panic!(
                    "escaping member reached physical resolution: {}",
                    path.display()
                )
            },
            || archive_paths::member_path(&records, "../records-evil/file"),
        )
        .expect_err("lexical escape");
        assert!(error.to_string().contains("escapes"));
    }
);
source_test!(
    archive_path_is_refused_before_probing_an_outside_target_case_045,
    "tests/test_archive_safety.py::test_archive_path_is_refused_before_probing_an_outside_target[child/../../outside]",
    {
        let temp = temp();
        let root = temp.path();
        let records = root.join("records");
        fs::create_dir(&records).expect("records");
        let error = testing::with_path_resolver(
            |path| {
                panic!(
                    "escaping member reached physical resolution: {}",
                    path.display()
                )
            },
            || archive_paths::member_path(&records, "child/../../outside"),
        )
        .expect_err("lexical escape");
        assert!(error.to_string().contains("escapes"));
    }
);
source_test!(
    physical_check_keeps_links_before_parent_components_case_046,
    "tests/test_archive_safety.py::test_physical_check_keeps_links_before_parent_components[archive]",
    {
        let temp = temp();
        let root = temp.path();
        let records = root.join("records");
        fs::create_dir(&records).expect("records");
        let member = records.join("linked/../target");
        let outside = root.join("outside/target");
        let hits = Arc::new(Mutex::new(0));
        let seen = hits.clone();
        let error = testing::with_path_resolver(
            move |path| {
                if path == member {
                    *seen.lock().expect("hits") += 1;
                    Ok(Some(outside.clone()))
                } else {
                    Ok(None)
                }
            },
            || archive_paths::member_path(&records, "linked/../target").map_err(io::Error::other),
        )
        .expect_err("physical escape");
        assert!(error.to_string().contains("escapes"));
        assert_eq!(
            *hits.lock().expect("hits"),
            1,
            "physical resolution retained link before parent"
        );
    }
);
source_test!(
    physical_check_keeps_links_before_parent_components_case_047,
    "tests/test_archive_safety.py::test_physical_check_keeps_links_before_parent_components[profile]",
    {
        let temp = temp();
        let root = temp.path();
        let records = root.join("records");
        fs::create_dir(&records).expect("records");
        let member = records.join("linked/../target");
        let outside = root.join("outside/target");
        let hits = Arc::new(Mutex::new(0));
        let seen = hits.clone();
        let error = testing::with_path_resolver(
            move |path| {
                if path == member {
                    *seen.lock().expect("hits") += 1;
                    Ok(Some(outside.clone()))
                } else {
                    Ok(None)
                }
            },
            || paths::id_path(&records, "linked", "/../target").map_err(io::Error::other),
        )
        .expect_err("physical escape");
        assert!(error.to_string().contains("escapes"));
        assert_eq!(
            *hits.lock().expect("hits"),
            1,
            "physical resolution retained link before parent"
        );
    }
);
source_test!(
    physical_sibling_is_refused_before_accessing_the_resolved_target,
    "tests/test_archive_safety.py::test_physical_sibling_is_refused_before_accessing_the_resolved_target",
    {
        let temp = temp();
        let root = temp.path();
        let records = root.join("records");
        fs::create_dir(&records).expect("records");
        let member = records.join("linked");
        let outside = root.join("records-evil/target");
        let forbidden = outside.clone();
        let error = testing::with_path_resolver(
            move |path| Ok((path == member).then(|| outside.clone())),
            || {
                testing::with_observer(
                    move |event| {
                        if event.operation == "before-path-final-probe" {
                            assert_ne!(
                                event.path, forbidden,
                                "outside target reached final filesystem resolution"
                            );
                        }
                        Ok(Action::Continue)
                    },
                    || archive_paths::member_path(&records, "linked"),
                )
            },
        )
        .expect_err("physical sibling escape");
        assert!(error.to_string().contains("escapes"));
    }
);
source_test!(
    profile_path_rejects_an_escaping_suffix_before_filesystem_access,
    "tests/test_archive_safety.py::test_profile_path_rejects_an_escaping_suffix_before_filesystem_access",
    {
        let temp = temp();
        let root = temp.path();
        let error = testing::with_observer(
            |event| {
                assert_ne!(
                    event.operation, "before-path-probe",
                    "escaping suffix reached filesystem probe"
                );
                Ok(Action::Continue)
            },
            || paths::id_path(root, "fixture", "/../../outside"),
        )
        .expect_err("suffix escape");
        assert!(error.to_string().contains("escapes"));
    }
);
source_test!(
    json_replacement_uses_private_file_permissions_on_posix,
    "tests/test_archive_safety.py::test_json_replacement_uses_private_file_permissions_on_posix",
    {
        let temp = temp();
        let root = temp.path();
        let target = root.join("credentials.json");
        write_json(&target, &json!({"fixture":"synthetic"})).expect("private write");
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&target).expect("bytes")).expect("JSON"),
            json!({"fixture":"synthetic"})
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&target)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
);
