use super::*;
source_test!(
    c000_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[screen-0x0]",
    {
        refused_field("create", "screen", json!("0x0"));
    }
);
source_test!(
    c001_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[screen--5x-5]",
    {
        refused_field("create", "screen", json!("-5x-5"));
    }
);
source_test!(
    c002_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[screen-potato]",
    {
        refused_field("create", "screen", json!("potato"));
    }
);
source_test!(
    c003_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[screen-infx1080]",
    {
        refused_field("create", "screen", json!("infx1080"));
    }
);
source_test!(
    c004_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[screen-1920]",
    {
        refused_field("create", "screen", json!("1920"));
    }
);
source_test!(
    c005_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[screen-99999x10]",
    {
        refused_field("create", "screen", json!("99999x10"));
    }
);
source_test!(
    c006_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[cores--4]",
    {
        refused_field("create", "cores", json!(-4));
    }
);
source_test!(
    c007_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[cores-10000000000000]",
    {
        refused_field("create","cores",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c008_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[memory--1]",
    {
        refused_field("create", "memory", json!(-1));
    }
);
source_test!(
    c009_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[maxTouchPoints-99]",
    {
        refused_field("create", "maxTouchPoints", json!(99));
    }
);
source_test!(
    c010_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[devicePixelRat-0]",
    {
        refused_field("create", "devicePixelRatio", json!(0));
    }
);
source_test!(
    c011_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[userAgent-123]",
    {
        refused_field("create", "userAgent", json!(123));
    }
);
source_test!(
    c012_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[timezone-[]]",
    {
        refused_field("create", "timezone", Value::Array(vec![]));
    }
);
source_test!(
    c013_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[tags-[{}]]",
    {
        refused_field(
            "create",
            "tags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
        );
    }
);
source_test!(
    c014_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[name-123]",
    {
        refused_field("create", "name", json!(123));
    }
);
source_test!(
    c015_the_create_route_refuses_by_field_without_writing,
    "tests/test_write_contract_boundaries.py::test_the_create_route_refuses_by_field_without_writing[dnt-yes]",
    {
        refused_field("create", "dnt", json!("yes"));
    }
);
source_test!(
    c000_the_update_route_refuses_the_same_way,
    "tests/test_write_contract_boundaries.py::test_the_update_route_refuses_the_same_way[screen]",
    {
        refused_field("update", "screen", json!("0x0"));
    }
);
source_test!(
    c001_the_update_route_refuses_the_same_way,
    "tests/test_write_contract_boundaries.py::test_the_update_route_refuses_the_same_way[cores]",
    {
        refused_field("update", "cores", json!(-4));
    }
);
source_test!(
    c002_the_update_route_refuses_the_same_way,
    "tests/test_write_contract_boundaries.py::test_the_update_route_refuses_the_same_way[userAgent]",
    {
        refused_field("update", "userAgent", json!(123));
    }
);
source_test!(
    c000_a_refused_request_never_answers_with_a_traceback,
    "tests/test_write_contract_boundaries.py::test_a_refused_request_never_answers_with_a_traceback",
    {
        let tmp = common::TempDirectory::new("contract-no-traceback").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles",
            json!({"name":"x","cores":-4}),
        );
        assert_ne!(s, 500);
        assert!(!v.to_string().contains("Traceback"));
    }
);
source_test!(
    c000_an_ordinary_create_and_edit_still_work,
    "tests/test_write_contract_boundaries.py::test_an_ordinary_create_and_edit_still_work",
    {
        let tmp = common::TempDirectory::new("contract-edit-revision").expect("root");
        let mut p = made(tmp.path(), json!({"name":"p","notes":"first"}));
        let rev = p["_rev"].as_u64().expect("revision");
        p["notes"] = "second".into();
        let (s, v) = put(tmp.path(), &p["id"].clone(), p);
        assert_eq!(s, 200);
        assert_eq!(v["notes"], "second");
        assert_eq!(v["_rev"], rev + 1);
    }
);
source_test!(
    c000_a_stale_revision_still_conflicts,
    "tests/test_write_contract_boundaries.py::test_a_stale_revision_still_conflicts",
    {
        revision(true);
    }
);
source_test!(
    c000_a_duplicate_name_still_conflicts,
    "tests/test_write_contract_boundaries.py::test_a_duplicate_name_still_conflicts",
    {
        let tmp = common::TempDirectory::new("contract-name-conflict").expect("root");
        made(tmp.path(), json!({"name":"one-of-a-kind"}));
        assert_eq!(
            http(
                tmp.path(),
                "POST",
                "/api/profiles",
                json!({"name":"one-of-a-kind"})
            )
            .0,
            409
        );
    }
);
source_test!(
    c000_the_folder_survives_a_validated_write,
    "tests/test_write_contract_boundaries.py::test_the_folder_survives_a_validated_write",
    {
        let tmp = common::TempDirectory::new("contract-folder").expect("root");
        let p = made(tmp.path(), json!({"name":"p","folder":"Research"}));
        let (s, v) = put(tmp.path(), &p["id"], json!({"notes":"moved"}));
        assert_eq!(s, 200);
        assert_eq!(v["folder"], "Research");
    }
);
source_test!(
    c000_an_untouched_proxy_password_is_preserved_across_an_edit,
    "tests/test_write_contract_boundaries.py::test_an_untouched_proxy_password_is_preserved_across_an_edit",
    {
        password_edit("preserve");
    }
);
source_test!(
    c000_a_deliberate_password_clear_still_clears,
    "tests/test_write_contract_boundaries.py::test_a_deliberate_password_clear_still_clears",
    {
        password_edit("clear");
    }
);
source_test!(
    c000_every_engine_twin_the_api_writes_satisfies_the_contract,
    "tests/test_write_contract_boundaries.py::test_every_engine_twin_the_api_writes_satisfies_the_contract",
    {
        let tmp = common::TempDirectory::new("contract-twin-contract").expect("root");
        made(tmp.path(), json!({"name":"one"}));
        made(tmp.path(), json!({"name":"two","engine":"camoufox"}));
        valid_twin(tmp.path());
    }
);
source_test!(
    c000_the_cli_import_exits_non_zero_and_writes_nothing,
    "tests/test_write_contract_boundaries.py::test_the_cli_import_exits_non_zero_and_writes_nothing[numeric-name]",
    {
        cli_refused(
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                ("os".into(), json!("win")),
                ("userAgent".into(), json!("Mozilla/5.0")),
                ("resolution".into(), json!("1920x1080")),
                ("name".into(), json!(123)),
            ]))]),
            "name",
        );
    }
);
source_test!(
    c001_the_cli_import_exits_non_zero_and_writes_nothing,
    "tests/test_write_contract_boundaries.py::test_the_cli_import_exits_non_zero_and_writes_nothing[zero-resolution]",
    {
        cli_refused(
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                ("os".into(), json!("win")),
                ("userAgent".into(), json!("Mozilla/5.0")),
                ("resolution".into(), json!("0x0")),
                ("name".into(), json!("ok")),
            ]))]),
            "screen",
        );
    }
);
source_test!(
    c002_the_cli_import_exits_non_zero_and_writes_nothing,
    "tests/test_write_contract_boundaries.py::test_the_cli_import_exits_non_zero_and_writes_nothing[object-tag]",
    {
        cli_refused(
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                ("os".into(), json!("win")),
                ("userAgent".into(), json!("Mozilla/5.0")),
                ("resolution".into(), json!("1920x1080")),
                ("name".into(), json!("ok")),
                (
                    "tags".into(),
                    Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
                ),
            ]))]),
            "tags",
        );
    }
);
source_test!(
    c000_a_bad_later_entry_refuses_the_whole_import,
    "tests/test_write_contract_boundaries.py::test_a_bad_later_entry_refuses_the_whole_import",
    {
        cli_refused(
            json!([{"name":"first","os":"win","resolution":"1920x1080"},{"name":"second","resolution":"0x0"},{"name":"third"}]),
            "screen",
        );
    }
);
source_test!(
    c000_a_good_vendor_export_still_imports,
    "tests/test_write_contract_boundaries.py::test_a_good_vendor_export_still_imports",
    {
        good_vendor();
    }
);
source_test!(
    c000_an_unsupported_format_is_refused_rather_than_reinterpreted,
    "tests/test_write_contract_boundaries.py::test_an_unsupported_format_is_refused_rather_than_reinterpreted",
    {
        cli_refused(
            native(json!([{"id":"native","name":"native"}])),
            "Numera profile export",
        );
    }
);
source_test!(
    c000_an_export_from_a_newer_build_is_refused_by_version,
    "tests/test_write_contract_boundaries.py::test_an_export_from_a_newer_build_is_refused_by_version",
    {
        cli_refused(json!({"numeraProfiles":2,"profiles":[]}), "newer Numera");
    }
);
source_test!(
    c000_a_damaged_record_does_not_take_a_read_path_down,
    "tests/test_write_contract_boundaries.py::test_a_damaged_record_does_not_take_a_read_path_down[/api/profiles]",
    {
        damaged_view("/api/profiles");
    }
);
source_test!(
    c001_a_damaged_record_does_not_take_a_read_path_down,
    "tests/test_write_contract_boundaries.py::test_a_damaged_record_does_not_take_a_read_path_down[/api/profiles/health]",
    {
        damaged_view("/api/profiles/health");
    }
);
source_test!(
    c002_a_damaged_record_does_not_take_a_read_path_down,
    "tests/test_write_contract_boundaries.py::test_a_damaged_record_does_not_take_a_read_path_down[/api/profiles/collisions]",
    {
        damaged_view("/api/profiles/collisions");
    }
);
source_test!(
    c000_every_listed_profile_has_the_display_fields_a_reader_dereferences,
    "tests/test_write_contract_boundaries.py::test_every_listed_profile_has_the_display_fields_a_reader_dereferences",
    {
        damaged_view("/api/profiles");
    }
);
source_test!(
    c000_the_healthy_record_is_still_listed_alongside_the_damaged_ones,
    "tests/test_write_contract_boundaries.py::test_the_healthy_record_is_still_listed_alongside_the_damaged_ones",
    {
        damaged_view("/api/profiles");
    }
);
source_test!(
    c000_the_healthy_record_is_still_editable_alongside_the_damaged_ones,
    "tests/test_write_contract_boundaries.py::test_the_healthy_record_is_still_editable_alongside_the_damaged_ones",
    {
        let tmp = common::TempDirectory::new("contract-legacy-edit").expect("root");
        damaged(tmp.path());
        let p = made(tmp.path(), json!({"name":"healthy"}));
        let (s, v) = put(tmp.path(), &p["id"], json!({"notes":"still editable"}));
        assert_eq!(s, 200, "{v}");
    }
);
source_test!(
    c000_a_damaged_stored_name_does_not_make_the_store_unwritable,
    "tests/test_write_contract_boundaries.py::test_a_damaged_stored_name_does_not_make_the_store_unwritable",
    {
        let tmp = common::TempDirectory::new("contract-damaged-name").expect("root");
        legacy(tmp.path());
        let p = repo(tmp.path())
            .save(profile("new-arrival"), 1001.0)
            .expect("save alongside damage");
        assert_eq!(p.name, "new-arrival");
    }
);
source_test!(
    c000_a_numeric_stored_name_still_counts_as_taken,
    "tests/test_write_contract_boundaries.py::test_a_numeric_stored_name_still_counts_as_taken",
    {
        let tmp = common::TempDirectory::new("contract-numeric-clash").expect("root");
        legacy(tmp.path());
        let e = repo(tmp.path())
            .save(profile("123"), 1001.0)
            .err()
            .expect("name taken");
        assert!(matches!(
            e,
            numera_core::storage::repo::RepoError::NameTaken(_)
        ));
    }
);
source_test!(
    c000_the_damaged_bytes_on_disk_are_left_exactly_as_they_were,
    "tests/test_write_contract_boundaries.py::test_the_damaged_bytes_on_disk_are_left_exactly_as_they_were",
    {
        let tmp = common::TempDirectory::new("contract-damaged-bytes").expect("root");
        damaged(tmp.path());
        let before = snapshot(tmp.path());
        http(tmp.path(), "GET", "/api/profiles", Value::Null);
        http(tmp.path(), "GET", "/api/profiles/health", Value::Null);
        assert_listing_preserves_original_files(tmp.path(), &before, true);
    }
);
source_test!(
    c000_a_damaged_name_is_still_searchable_by_the_engine,
    "tests/test_write_contract_boundaries.py::test_a_damaged_name_is_still_searchable_by_the_engine",
    {
        let tmp = common::TempDirectory::new("contract-name-search").expect("root");
        let p = repo(tmp.path())
            .save(profile("findable"), 1001.0)
            .expect("save");
        assert_eq!(
            repo(tmp.path())
                .resolve("findable", 1002.0)
                .expect("resolve")
                .expect("found")
                .id,
            p.id
        );
        assert!(
            repo(tmp.path())
                .resolve("nothing-here", 1002.0)
                .expect("resolve")
                .is_none()
        );
    }
);
source_test!(
    c000_the_repair_does_not_let_the_damage_be_written_back,
    "tests/test_write_contract_boundaries.py::test_the_repair_does_not_let_the_damage_be_written_back",
    {
        let tmp = common::TempDirectory::new("contract-damage-write").expect("root");
        damaged(tmp.path());
        let (s, v) = put(tmp.path(), &json!("aaaaaaaaaaaa"), json!({"name":123}));
        assert_refused(s, &v, "name");
    }
);
