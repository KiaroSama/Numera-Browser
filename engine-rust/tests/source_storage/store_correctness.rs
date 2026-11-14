use super::*;
source_test!(
    two_seedless_profiles_get_different_seeds,
    "tests/test_store_correctness.py::test_two_seedless_profiles_get_different_seeds",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let b = create(root, "b");
        assert!(a["seed"].as_i64().is_some_and(|v| v != 0));
        assert!(b["seed"].as_i64().is_some_and(|v| v != 0));
        assert_ne!(a["seed"], b["seed"]);
    }
);
source_test!(
    a_seed_is_stable_across_a_round_trip,
    "tests/test_store_correctness.py::test_a_seed_is_stable_across_a_round_trip",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let rows = http(root, "GET", "/api/profiles", Value::Null).1;
        assert_eq!(
            rows.as_array()
                .expect("profiles")
                .iter()
                .find(|p| p["id"] == id)
                .expect("mine")["seed"],
            a["seed"]
        );
    }
);
source_test!(
    an_explicit_seed_is_never_overwritten,
    "tests/test_store_correctness.py::test_an_explicit_seed_is_never_overwritten",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"a","seed":4242}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(made["seed"], 4242);
    }
);
source_test!(
    new_is_never_written_to_disk,
    "tests/test_store_correctness.py::test_new_is_never_written_to_disk",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"a","_new":true}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert!(made.get("_new").is_none());
        assert!(
            raw(
                root,
                &format!("dashboard/{}.json", made["id"].as_str().expect("id"))
            )
            .get("_new")
            .is_none()
        );
    }
);
source_test!(
    new_is_stripped_on_update_too,
    "tests/test_store_correctness.py::test_new_is_stripped_on_update_too",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let uri = format!("/api/profiles/{id}");
        let mut patch = a.clone();
        patch["_new"] = true.into();
        patch["notes"] = "x".into();
        let (status, _body) = http(root, "PUT", &uri, patch);
        assert_eq!(status, 200, "HTTP response status mismatch");
        let disk = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(disk["notes"], "x");
        assert!(disk.get("_new").is_none());
    }
);
source_test!(
    an_edit_changes_the_same_profile,
    "tests/test_store_correctness.py::test_an_edit_changes_the_same_profile",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let uri = format!("/api/profiles/{id}");
        let (status, changed) = http(root, "PUT", &uri, json!({"name":"renamed"}));
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(changed["id"], id);
        let listed = http(root, "GET", "/api/profiles", Value::Null).1;
        assert_eq!(listed.as_array().expect("rows").len(), 1);
        assert_eq!(listed[0]["id"], id);
        assert_eq!(listed[0]["name"], "renamed");
        assert_eq!(json_files(root, "dashboard"), [format!("{id}.json")]);
    }
);
source_test!(
    repeated_edits_do_not_accumulate_profiles,
    "tests/test_store_correctness.py::test_repeated_edits_do_not_accumulate_profiles",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let uri = format!("/api/profiles/{id}");
        for i in 0..5 {
            let (status, _body) = http(root, "PUT", &uri, json!({"name":format!("n{i}")}));
            assert_eq!(status, 200, "HTTP response status mismatch");
        }
        assert_eq!(
            http(root, "GET", "/api/profiles", Value::Null)
                .1
                .as_array()
                .expect("rows")
                .len(),
            1
        );
        assert_eq!(json_files(root, "dashboard"), [format!("{id}.json")]);
        assert_eq!(raw(root, &format!("dashboard/{id}.json"))["name"], "n4");
    }
);
source_test!(
    the_body_the_api_returns_is_accepted_back,
    "tests/test_store_correctness.py::test_the_body_the_api_returns_is_accepted_back",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let uri = format!("/api/profiles/{id}");
        let mut patch = a.clone();
        assert!(a.get("status").is_some() && a.get("lastActive").is_some());
        patch["name"] = "inline".into();
        let (status, _body) = http(root, "PUT", &uri, patch);
        assert_eq!(status, 200, "HTTP response status mismatch");
        let disk = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(disk["name"], "inline");
    }
);
source_test!(
    an_edit_cannot_move_a_profile_to_another_id,
    "tests/test_store_correctness.py::test_an_edit_cannot_move_a_profile_to_another_id",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let uri = format!("/api/profiles/{id}");
        let b = create(root, "b");
        let (status, body) = http(root, "PUT", &uri, json!({"name":"moved","id":b["id"]}));
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(body["id"], id);
        assert_eq!(raw(root, &format!("dashboard/{id}.json"))["name"], "moved");
        assert_eq!(
            raw(
                root,
                &format!("dashboard/{}.json", b["id"].as_str().expect("bid"))
            )["name"],
            "b"
        );
        assert_eq!(json_files(root, "dashboard").len(), 2);
    }
);
source_test!(
    editing_a_profile_that_does_not_exist_is_a_404,
    "tests/test_store_correctness.py::test_editing_a_profile_that_does_not_exist_is_a_404",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            http(
                root,
                "PUT",
                "/api/profiles/deadbeef1234",
                json!({"name":"x"})
            )
            .0,
            404
        );
        assert!(json_files(root, "dashboard").is_empty());
    }
);
source_test!(
    a_typo_is_refused_rather_than_persisted,
    "tests/test_store_correctness.py::test_a_typo_is_refused_rather_than_persisted",
    {
        let temp = temp();
        let root = temp.path();
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"a","nmae":"typo"}),
        );
        assert_eq!(status, 422, "HTTP response status mismatch");
        assert!(body["detail"].as_str().expect("detail").contains("nmae"));
        assert!(
            http(root, "GET", "/api/profiles", Value::Null)
                .1
                .as_array()
                .expect("rows")
                .is_empty()
        );
    }
);
source_test!(
    a_typo_on_update_is_refused_and_changes_nothing,
    "tests/test_store_correctness.py::test_a_typo_on_update_is_refused_and_changes_nothing",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let uri = format!("/api/profiles/{id}");
        let (status, _body) = http(root, "PUT", &uri, json!({"name":"b","notez":"typo"}));
        assert_eq!(status, 422, "HTTP response status mismatch");
        assert_eq!(raw(root, &format!("dashboard/{id}.json"))["name"], "a");
    }
);
source_test!(
    a_typo_in_a_bulk_patch_is_refused,
    "tests/test_store_correctness.py::test_a_typo_in_a_bulk_patch_is_refused",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[id],"patch":{"folderr":"x"}}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
        assert!(body["detail"].as_str().expect("detail").contains("folderr"));
    }
);
source_test!(
    a_typo_in_a_batch_entry_is_refused,
    "tests/test_store_correctness.py::test_a_typo_in_a_batch_entry_is_refused",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"ok"},{"name":"bad","tagz":[]}]}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
        assert!(
            http(root, "GET", "/api/profiles", Value::Null)
                .1
                .as_array()
                .expect("rows")
                .is_empty()
        );
    }
);
source_test!(
    every_real_dashboard_field_is_still_accepted,
    "tests/test_store_correctness.py::test_every_real_dashboard_field_is_still_accepted",
    {
        let temp = temp();
        let root = temp.path();
        let mut shape = conversion::to_dashboard(
            &conversion::to_engine(&json!({}), 1000.0).expect("default profile"),
        );
        numera_core::http::profile_fields::strip_server_owned(
            shape.as_object_mut().expect("shape"),
        )
        .expect("strip server fields");
        shape["name"] = "full".into();
        assert!(!shape.as_object().expect("shape").is_empty());
        let (status, _body) = http(root, "POST", "/api/profiles", shape);
        assert_eq!(status, 200, "HTTP response status mismatch");
    }
);
source_test!(
    bulk_tag_helpers_are_not_unknown_fields,
    "tests/test_store_correctness.py::test_bulk_tag_helpers_are_not_unknown_fields",
    {
        let temp = temp();
        let root = temp.path();
        let (status, a) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"a","tags":["keep"]}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let id = a["id"].as_str().expect("id");
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[id],"patch":{"addTags":["new"]}}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(
            raw(root, &format!("dashboard/{id}.json"))["tags"]
                .as_array()
                .expect("tags")
                .iter()
                .map(|v| v.as_str().expect("tag"))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["keep", "new"])
        );
    }
);
source_test!(
    an_imported_profile_reloads_as_an_existing_one,
    "tests/test_store_correctness.py::test_an_imported_profile_reloads_as_an_existing_one",
    {
        let temp = temp();
        let root = temp.path();
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles/import",
            json!({"text":serde_json::to_string(&json!([vendor_export()])).expect("export")}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(body["imported"], 1);
        let rows = http(root, "GET", "/api/profiles", Value::Null).1;
        let rows = rows.as_array().expect("profiles");
        let mine = rows
            .iter()
            .filter(|p| p["name"] == "imported")
            .collect::<Vec<_>>();
        assert_eq!(mine.len(), 1);
        assert!(mine[0].get("_new").is_none());
        assert!(
            raw(
                root,
                &format!("dashboard/{}.json", mine[0]["id"].as_str().expect("id"))
            )
            .get("_new")
            .is_none()
        );
        assert_eq!(
            rows.iter()
                .map(|p| p["id"].as_str().expect("id"))
                .collect::<BTreeSet<_>>()
                .len(),
            rows.len()
        );
    }
);
source_test!(
    an_import_does_not_change_the_default_engine,
    "tests/test_store_correctness.py::test_an_import_does_not_change_the_default_engine",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            http(
                root,
                "PUT",
                "/api/config",
                json!({"default_engine":"camoufox"})
            )
            .0,
            200
        );
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/import",
            json!({"text":serde_json::to_string(&json!([vendor_export()])).expect("export")}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(
            http(root, "GET", "/api/config", Value::Null).1["default_engine"],
            "camoufox"
        );
    }
);
source_test!(
    a_non_string_import_body_is_a_4xx_not_a_500,
    "tests/test_store_correctness.py::test_a_non_string_import_body_is_a_4xx_not_a_500",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            http(
                root,
                "POST",
                "/api/profiles/import",
                json!({"text":{"not":"a string"}})
            )
            .0,
            400
        );
    }
);
source_test!(
    a_client_supplied_id_cannot_escape_the_data_directory_case_019,
    "tests/test_store_correctness.py::test_a_client_supplied_id_cannot_escape_the_data_directory[../../escaped]",
    {
        let temp = temp();
        let root = temp.path();
        let before = json_files(root.parent().expect("parent"), "");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"evil","id":"../../escaped"}),
        );
        if status == 200 {
            assert_ne!(body["id"], "../../escaped");
        } else {
            assert!(
                [400, 422].contains(&status),
                "HTTP response status mismatch"
            );
        }
        assert_eq!(json_files(root.parent().expect("parent"), ""), before);
    }
);
source_test!(
    a_client_supplied_id_cannot_escape_the_data_directory_case_020,
    "tests/test_store_correctness.py::test_a_client_supplied_id_cannot_escape_the_data_directory[..]",
    {
        let temp = temp();
        let root = temp.path();
        let before = json_files(root.parent().expect("parent"), "");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"evil","id":".."}),
        );
        if status == 200 {
            assert_ne!(body["id"], "..");
        } else {
            assert!(
                [400, 422].contains(&status),
                "HTTP response status mismatch"
            );
        }
        assert_eq!(json_files(root.parent().expect("parent"), ""), before);
    }
);
source_test!(
    a_client_supplied_id_cannot_escape_the_data_directory_case_021,
    "tests/test_store_correctness.py::test_a_client_supplied_id_cannot_escape_the_data_directory[a/b]",
    {
        let temp = temp();
        let root = temp.path();
        let before = json_files(root.parent().expect("parent"), "");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"evil","id":"a/b"}),
        );
        if status == 200 {
            assert_ne!(body["id"], "a/b");
        } else {
            assert!(
                [400, 422].contains(&status),
                "HTTP response status mismatch"
            );
        }
        assert_eq!(json_files(root.parent().expect("parent"), ""), before);
    }
);
source_test!(
    a_client_supplied_id_cannot_escape_the_data_directory_case_022,
    "tests/test_store_correctness.py::test_a_client_supplied_id_cannot_escape_the_data_directory[C:/x]",
    {
        let temp = temp();
        let root = temp.path();
        let before = json_files(root.parent().expect("parent"), "");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"evil","id":"C:/x"}),
        );
        if status == 200 {
            assert_ne!(body["id"], "C:/x");
        } else {
            assert!(
                [400, 422].contains(&status),
                "HTTP response status mismatch"
            );
        }
        assert_eq!(json_files(root.parent().expect("parent"), ""), before);
    }
);
source_test!(
    a_client_supplied_id_cannot_escape_the_data_directory_case_023,
    "tests/test_store_correctness.py::test_a_client_supplied_id_cannot_escape_the_data_directory[]",
    {
        let temp = temp();
        let root = temp.path();
        let before = json_files(root.parent().expect("parent"), "");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"evil","id":""}),
        );
        if status == 200 {
            assert_ne!(body["id"], "");
        } else {
            assert!(
                [400, 422].contains(&status),
                "HTTP response status mismatch"
            );
        }
        assert_eq!(json_files(root.parent().expect("parent"), ""), before);
    }
);
source_test!(
    a_client_supplied_id_cannot_escape_the_data_directory_case_024,
    "tests/test_store_correctness.py::test_a_client_supplied_id_cannot_escape_the_data_directory[xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx]",
    {
        let temp = temp();
        let root = temp.path();
        let before = json_files(root.parent().expect("parent"), "");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"evil","id":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}),
        );
        if status == 200 {
            assert_ne!(
                body["id"],
                "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
            );
        } else {
            assert!(
                [400, 422].contains(&status),
                "HTTP response status mismatch"
            );
        }
        assert_eq!(json_files(root.parent().expect("parent"), ""), before);
    }
);
source_test!(
    the_store_itself_refuses_a_dangerous_id,
    "tests/test_store_correctness.py::test_the_store_itself_refuses_a_dangerous_id",
    {
        let temp = temp();
        let root = temp.path();
        for bad in ["../../escaped", "a/b", ".."] {
            assert!(paths::id_path(&root.join("profiles"), bad, ".json").is_err());
            assert!(store(root).user_data_path(bad).is_err());
            assert!(paths::id_path(&root.join("dashboard"), bad, ".json").is_err());
        }
    }
);
source_test!(
    a_bulk_patch_cannot_write_private_metadata,
    "tests/test_store_correctness.py::test_a_bulk_patch_cannot_write_private_metadata",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[id],"patch":{"_trust":{"grade":"A"},"notes":"kept"}}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let disk = raw(root, &format!("dashboard/{id}.json"));
        assert_ne!(disk["_trust"], json!({"grade":"A"}));
        assert_eq!(disk["notes"], "kept");
    }
);
source_test!(
    a_malformed_bulk_body_is_a_4xx_not_a_500_case_027,
    "tests/test_store_correctness.py::test_a_malformed_bulk_body_is_a_4xx_not_a_500[body0]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids": "not-a-list", "patch": {}}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
    }
);
source_test!(
    a_malformed_bulk_body_is_a_4xx_not_a_500_case_028,
    "tests/test_store_correctness.py::test_a_malformed_bulk_body_is_a_4xx_not_a_500[body1]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids": [1, 2], "patch": {}}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
    }
);
source_test!(
    a_malformed_bulk_body_is_a_4xx_not_a_500_case_029,
    "tests/test_store_correctness.py::test_a_malformed_bulk_body_is_a_4xx_not_a_500[body2]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids": [], "patch": "not-an-object"}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
    }
);
source_test!(
    a_malformed_bulk_body_is_a_4xx_not_a_500_case_030,
    "tests/test_store_correctness.py::test_a_malformed_bulk_body_is_a_4xx_not_a_500[body3]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids": [], "patch": 5}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
    }
);
source_test!(
    a_malformed_batch_body_is_a_4xx_not_a_500_case_031,
    "tests/test_store_correctness.py::test_a_malformed_batch_body_is_a_4xx_not_a_500[body0]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles": "not-a-list"}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
    }
);
source_test!(
    a_malformed_batch_body_is_a_4xx_not_a_500_case_032,
    "tests/test_store_correctness.py::test_a_malformed_batch_body_is_a_4xx_not_a_500[body1]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles": [1, 2]}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
    }
);
source_test!(
    a_malformed_batch_body_is_a_4xx_not_a_500_case_033,
    "tests/test_store_correctness.py::test_a_malformed_batch_body_is_a_4xx_not_a_500[body2]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles": [{"name": "ok"}, "not-an-object"]}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
    }
);
source_test!(
    a_minimal_create_uses_the_default_without_rewriting_it,
    "tests/test_store_correctness.py::test_a_minimal_create_uses_the_default_without_rewriting_it",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            http(
                root,
                "PUT",
                "/api/config",
                json!({"default_engine":"camoufox"})
            )
            .0,
            200
        );
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"no engine named"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(
            http(root, "GET", "/api/config", Value::Null).1["default_engine"],
            "camoufox"
        );
    }
);
source_test!(
    a_real_choice_does_move_the_default,
    "tests/test_store_correctness.py::test_a_real_choice_does_move_the_default",
    {
        let temp = temp();
        let root = temp.path();
        assert_eq!(
            http(
                root,
                "PUT",
                "/api/config",
                json!({"default_engine":"camoufox"})
            )
            .0,
            200
        );
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"a","engine":"playwright"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(
            http(root, "GET", "/api/config", Value::Null).1["default_engine"],
            "playwright"
        );
    }
);
source_test!(
    a_bulk_patch_cannot_rewrite_an_id,
    "tests/test_store_correctness.py::test_a_bulk_patch_cannot_rewrite_an_id",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[id],"patch":{"id":"hijacked"}}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert!(root.join(format!("dashboard/{id}.json")).is_file());
        assert!(!root.join("dashboard/hijacked.json").exists());
        assert_eq!(raw(root, &format!("dashboard/{id}.json"))["id"], id);
    }
);
source_test!(
    a_bulk_patch_cannot_break_a_list_field,
    "tests/test_store_correctness.py::test_a_bulk_patch_cannot_break_a_list_field",
    {
        let temp = temp();
        let root = temp.path();
        let a = create(root, "a");
        let id = a["id"].as_str().expect("id");
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[id],"patch":{"tags":"not-a-list"}}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
        assert!(raw(root, &format!("dashboard/{id}.json"))["tags"].is_array());
    }
);
source_test!(
    a_malformed_batch_writes_nothing_at_all,
    "tests/test_store_correctness.py::test_a_malformed_batch_writes_nothing_at_all",
    {
        let temp = temp();
        let root = temp.path();
        let before = http(root, "GET", "/api/profiles", Value::Null).1;
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"fine"},{"name":12345}]}),
        );
        assert!(
            [400, 422].contains(&status),
            "HTTP response status mismatch"
        );
        assert_eq!(http(root, "GET", "/api/profiles", Value::Null).1, before);
    }
);
source_test!(
    a_batch_cannot_carry_its_own_ids,
    "tests/test_store_correctness.py::test_a_batch_cannot_carry_its_own_ids",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"a","id":"chosen-by-client"}]}),
        );
        if status < 400 {
            assert_eq!(status, 200, "HTTP response status mismatch");
            assert!(
                http(root, "GET", "/api/profiles", Value::Null)
                    .1
                    .as_array()
                    .expect("rows")
                    .iter()
                    .all(|p| p["id"] != "chosen-by-client")
            );
        } else {
            assert!(
                [400, 422].contains(&status),
                "HTTP response status mismatch"
            );
        }
    }
);
source_test!(
    a_valid_batch_still_works,
    "tests/test_store_correctness.py::test_a_valid_batch_still_works",
    {
        let temp = temp();
        let root = temp.path();
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"one"},{"name":"two"}]}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let rows = http(root, "GET", "/api/profiles", Value::Null).1;
        let names = rows
            .as_array()
            .expect("rows")
            .iter()
            .map(|p| p["name"].as_str().expect("name"))
            .collect::<BTreeSet<_>>();
        assert!(names.contains("one") && names.contains("two"));
    }
);
source_test!(
    two_concurrent_pool_updates_both_survive,
    "tests/test_store_correctness.py::test_two_concurrent_pool_updates_both_survive",
    {
        let temp = temp();
        let root = temp.path();
        let entries = pool(root)
            .add_many(
                (0..2)
                    .map(|i| json!({"type":"HTTP","host":format!("10.1.0.{i}"),"port":"8080"}))
                    .collect(),
            )
            .expect("pool entries");
        let ready = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();
        for (i, entry) in entries.iter().cloned().enumerate() {
            let root = root.to_owned();
            let ready = ready.clone();
            workers.push(thread::spawn(move || {
                pool(&root).list().expect("stale read");
                ready.wait();
                pool(&root)
                    .update(
                        entry["id"].as_str().expect("id"),
                        json!({"status":if 2==2&&i==1{"failed"}else{"ok"},"note":entry["host"]})
                            .as_object()
                            .expect("patch")
                            .clone(),
                    )
                    .expect("update");
            }));
        }
        for worker in workers {
            worker.join().expect("writer");
        }
        let final_rows = pool(root).list().expect("final pool");
        assert_eq!(final_rows.len(), 2);
        for (i, entry) in entries.iter().enumerate() {
            let row = final_rows
                .iter()
                .find(|r| r["id"] == entry["id"])
                .expect("survived");
            assert_eq!(
                row["status"],
                if 2 == 2 && i == 1 { "failed" } else { "ok" }
            );
        }
    }
);
source_test!(
    eight_concurrent_updates_all_persist,
    "tests/test_store_correctness.py::test_eight_concurrent_updates_all_persist",
    {
        let temp = temp();
        let root = temp.path();
        let entries = pool(root)
            .add_many(
                (0..8)
                    .map(|i| json!({"type":"HTTP","host":format!("10.1.0.{i}"),"port":"8080"}))
                    .collect(),
            )
            .expect("pool entries");
        let ready = Arc::new(Barrier::new(8));
        let mut workers = Vec::new();
        for (i, entry) in entries.iter().cloned().enumerate() {
            let root = root.to_owned();
            let ready = ready.clone();
            workers.push(thread::spawn(move || {
                pool(&root).list().expect("stale read");
                ready.wait();
                pool(&root)
                    .update(
                        entry["id"].as_str().expect("id"),
                        json!({"status":if 8==2&&i==1{"failed"}else{"ok"},"note":entry["host"]})
                            .as_object()
                            .expect("patch")
                            .clone(),
                    )
                    .expect("update");
            }));
        }
        for worker in workers {
            worker.join().expect("writer");
        }
        let final_rows = pool(root).list().expect("final pool");
        assert_eq!(final_rows.len(), 8);
        for (i, entry) in entries.iter().enumerate() {
            let row = final_rows
                .iter()
                .find(|r| r["id"] == entry["id"])
                .expect("survived");
            assert_eq!(
                row["status"],
                if 8 == 2 && i == 1 { "failed" } else { "ok" }
            );
        }
    }
);
source_test!(
    a_failed_write_leaves_the_previous_file_intact,
    "tests/test_store_correctness.py::test_a_failed_write_leaves_the_previous_file_intact",
    {
        let temp = temp();
        let root = temp.path();
        pool(root)
            .add_many(vec![json!({"type":"HTTP","host":"10.2.0.1","port":"8080"})])
            .expect("first");
        let before = fs::read(root.join("proxies.json")).expect("before");
        let result =
            testing::with_observer(error_at("before-atomic-write", "proxies.json", 28), || {
                pool(root).add_many(vec![json!({"type":"HTTP","host":"10.2.0.2","port":"8080"})])
            });
        assert!(result.is_err());
        assert_eq!(fs::read(root.join("proxies.json")).expect("after"), before);
        assert_eq!(pool(root).list().expect("pool").len(), 1);
    }
);
