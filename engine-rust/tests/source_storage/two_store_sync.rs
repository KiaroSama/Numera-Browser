use super::*;
source_test!(
    a_cli_edit_reaches_the_dashboard_copy,
    "tests/test_two_store_sync.py::test_a_cli_edit_reaches_the_dashboard_copy",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        let outcome = cli(root, |commands| {
            commands.edit(
                id,
                json!({"notes":"from the cli"}).as_object().expect("args"),
                1001.0,
            )
        })
        .expect("CLI edit");
        assert_eq!(outcome.exit, 0);
        assert_eq!(
            raw(root, &format!("dashboard/{id}.json"))["notes"],
            "from the cli"
        );
        assert_eq!(
            http(root, "GET", "/api/profiles", Value::Null).1[0]["notes"],
            "from the cli"
        );
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            "from the cli"
        );
    }
);
source_test!(
    a_cli_regen_reaches_the_dashboard_copy,
    "tests/test_two_store_sync.py::test_a_cli_regen_reaches_the_dashboard_copy",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        let before = raw(root, &format!("dashboard/{id}.json"))["userAgent"].clone();
        let outcome = cli(root, |commands| {
            commands.regen(id, Some("linux"), None, 1001.0)
        })
        .expect("regen");
        assert_eq!(outcome.exit, 0);
        let after = raw(root, &format!("dashboard/{id}.json"));
        assert_ne!(after["userAgent"], before);
        assert!(after["userAgent"].as_str().expect("UA").contains("Linux"));
        assert_eq!(after["os"], "Linux");
    }
);
source_test!(
    a_cli_delete_removes_both_copies,
    "tests/test_two_store_sync.py::test_a_cli_delete_removes_both_copies",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        assert!(root.join(format!("dashboard/{id}.json")).is_file());
        let outcome = cli(root, |commands| {
            commands.delete(&[id.into()], true, 1001.0, |_| Ok(true))
        })
        .expect("CLI delete");
        assert_eq!(outcome.exit, 0);
        assert!(!root.join(format!("dashboard/{id}.json")).exists());
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
    a_cli_edit_keeps_dashboard_only_fields,
    "tests/test_two_store_sync.py::test_a_cli_edit_keeps_dashboard_only_fields",
    {
        let temp = temp();
        let root = temp.path();
        let (status, p) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"kept","folder":"Ads","tags":["x"]}),
        );
        assert_eq!(status, 200, "{p}");
        let id = p["id"].as_str().expect("id");
        let out = cli(root, |commands| {
            commands.edit(id, json!({"notes":"n"}).as_object().expect("args"), 1001.0)
        })
        .expect("CLI edit");
        assert_eq!(out.exit, 0);
        let after = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(after["folder"], "Ads");
        assert_eq!(after["notes"], "n");
    }
);
source_test!(
    an_api_edit_is_visible_to_the_cli,
    "tests/test_two_store_sync.py::test_an_api_edit_is_visible_to_the_cli",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        assert_eq!(
            http(
                root,
                "PUT",
                &format!("/api/profiles/{id}"),
                json!({"notes":"typed in the ui"})
            )
            .0,
            200
        );
        let shown = cli(root, |commands| commands.show(id, 1001.0)).expect("CLI show");
        assert_eq!(shown.exit, 0);
        assert!(shown.payload.to_string().contains("typed in the ui"));
    }
);
source_test!(
    two_simultaneous_same_name_creates_have_one_winner,
    "tests/test_two_store_sync.py::test_two_simultaneous_same_name_creates_have_one_winner",
    {
        let temp = temp();
        let root = temp.path();
        let test = "two_store_sync::two_simultaneous_same_name_creates_have_one_winner";
        let first = process::start(test, root, "first", "name-create", "");
        let second = process::start(test, root, "second", "name-create", "");
        for who in ["first", "second"] {
            process::reached(root, who, "ready");
        }
        for who in ["first", "second"] {
            process::release(root, who);
        }
        process::finish(first);
        process::finish(second);
        assert_eq!(
            ["first", "second"]
                .iter()
                .filter(|who| raw(root, &format!("worker-{who}-result.json"))["won"] == true)
                .count(),
            1
        );
        assert_eq!(store(root).list(1001.0).expect("profiles").len(), 1);
    }
);
source_test!(
    the_repo_refuses_a_duplicate_name,
    "tests/test_two_store_sync.py::test_the_repo_refuses_a_duplicate_name",
    {
        let temp = temp();
        let root = temp.path();
        saved(root, "one", 1);
        assert!(matches!(
            store(root).save(profile("one", 2), 1000.0),
            Err(numera_core::storage::repo::RepoError::NameTaken(_))
        ));
    }
);
source_test!(
    a_restore_over_an_existing_id_replaces_both_copies,
    "tests/test_two_store_sync.py::test_a_restore_over_an_existing_id_replaces_both_copies",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        assert_eq!(
            http(
                root,
                "PUT",
                &format!("/api/profiles/{id}"),
                json!({"folder":"clients","notes":"keep me"})
            )
            .0,
            200
        );
        let zip = root.join("backup.zip");
        store(root)
            .backup(id, &zip, "source-storage-host")
            .expect("backup");
        store(root)
            .edit(
                id,
                |p| {
                    p.name = "changed".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("rename");
        store(root)
            .restore(&zip, false, "source-storage-host", 1002.0)
            .expect("restore");
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").name,
            "shared"
        );
        let twin = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(twin["name"], "shared");
        assert_eq!(twin["folder"], "clients");
        assert_eq!(twin["notes"], "keep me");
    }
);
source_test!(
    a_failed_restore_commit_leaves_both_copies_alone,
    "tests/test_two_store_sync.py::test_a_failed_restore_commit_leaves_both_copies_alone",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        let zip = root.join("backup.zip");
        store(root)
            .backup(id, &zip, "source-storage-host")
            .expect("backup");
        store(root)
            .edit(
                id,
                |p| {
                    p.name = "current".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("rename");
        let before = pair(root, id);
        let result = testing::with_observer(error_at("before-atomic-write", "", 28), || {
            store(root).restore(&zip, false, "source-storage-host", 1002.0)
        });
        assert!(result.is_err());
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").name,
            "current"
        );
        assert_eq!(pair(root, id), before);
    }
);
source_test!(
    an_import_of_an_existing_id_updates_both_copies,
    "tests/test_two_store_sync.py::test_an_import_of_an_existing_id_updates_both_copies",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        let path = root.join("p.json");
        store(root).export(id, &path).expect("export");
        store(root)
            .edit(
                id,
                |p| {
                    p.name = "after".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("rename");
        store(root).import_file(&path, 1002.0).expect("import");
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").name,
            "shared"
        );
        assert_eq!(raw(root, &format!("dashboard/{id}.json"))["name"], "shared");
    }
);
source_test!(
    a_cli_note_and_an_api_tag_edit_both_survive,
    "tests/test_two_store_sync.py::test_a_cli_note_and_an_api_tag_edit_both_survive",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id");
        let test = "two_store_sync::a_cli_note_and_an_api_tag_edit_both_survive";
        let holder = process::start(test, root, "holder", "edit-inside", id);
        process::reached(root, "holder", "inside");
        let writer = process::start(test, root, "writer", "patch-tags", id);
        process::reached(root, "writer", "ready");
        process::release(root, "writer");
        process::reached(root, "writer", "trying");
        process::release(root, "holder");
        process::finish(holder);
        process::finish(writer);
        let final_row = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(final_row["notes"], "from the cli");
        assert_eq!(final_row["tags"], json!(["from-the-api"]));
    }
);
