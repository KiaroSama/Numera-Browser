use super::*;
source_test!(
    c000_a_failing_bulk_leaves_neither_profiles_nor_the_default_engine_changed,
    "tests/test_profile_transactions.py::test_a_failing_bulk_leaves_neither_profiles_nor_the_default_engine_changed",
    {
        write_fault("bulk", 3);
    }
);
source_test!(
    c000_bulk_refuses_an_unknown_engine_and_changes_nothing,
    "tests/test_profile_transactions.py::test_bulk_refuses_an_unknown_engine_and_changes_nothing",
    {
        refused_field("bulk", "engine", json!("bogus"));
    }
);
source_test!(
    c000_bulk_still_applies_a_known_engine,
    "tests/test_profile_transactions.py::test_bulk_still_applies_a_known_engine",
    {
        let tmp = common::TempDirectory::new("contract-bulk-engine").expect("root");
        let p = made(tmp.path(), json!({"name":"a"}));
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[p["id"]],"patch":{"engine":"camoufox"}}),
        );
        assert_eq!(s, 200, "{v}");
        assert_eq!(
            read(tmp.path(), "dashboard", p["id"].as_str().expect("id"))["engine"],
            "camoufox"
        );
        assert_eq!(
            config(tmp.path()).read().expect("config")["default_engine"],
            "camoufox"
        );
    }
);
source_test!(
    c000_an_export_naming_an_engine_produces_a_profile_using_it,
    "tests/test_profile_transactions.py::test_an_export_naming_an_engine_produces_a_profile_using_it",
    {
        vendor_alias(
            json!({"name":"imported","engine":"camoufox","os":"win"}),
            "engine",
            json!("camoufox"),
        );
    }
);
source_test!(
    c000_an_export_naming_something_unsupported_is_not_an_explicit_choice,
    "tests/test_profile_transactions.py::test_an_export_naming_something_unsupported_is_not_an_explicit_choice",
    {
        let tmp = common::TempDirectory::new("contract-import-engine").expect("root");
        let before = config(tmp.path())
            .default_engine(&BTreeMap::new())
            .expect("default");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(json!({"name":"vendor","browserEngine":"Chrome"})),
        );
        assert_eq!(s, 200, "{v}");
        assert_eq!(
            config(tmp.path())
                .default_engine(&BTreeMap::new())
                .expect("default"),
            before
        );
    }
);
source_test!(
    c000_a_failing_batch_rollback_does_not_erase_a_concurrent_save,
    "tests/test_profile_transactions.py::test_a_failing_batch_rollback_does_not_erase_a_concurrent_save",
    {
        processes(
            "batch-rollback-race",
            |root| {
                let a = made(root, json!({"name":"victim","notes":"original"}));
                let b = made(root, json!({"name":"other","notes":"original"}));
                fs::write(
                    root.join("seed.json"),
                    encode(&json!({"victim":a,"other":b})),
                )
                .expect("seed");
            },
            |root, role, wire| {
                let p = seeded(root);
                if role == "holder" {
                    use numera_core::vault::testing::{Action, with_observer};
                    let other = p["other"]["id"].as_str().expect("id").to_owned();
                    let mut gate = wire.cloned();
                    let mut failed = false;
                    let mut rows = vec![p["victim"].clone(), p["other"].clone()];
                    for row in &mut rows {
                        row["notes"] = "batch".into();
                    }
                    let result = with_observer(
                        move |e| {
                            if !failed
                                && e.operation == "before-batch-write"
                                && e.path.file_stem().is_some_and(|s| s == other.as_str())
                            {
                                failed = true;
                                gate.say("inside");
                                gate.wait();
                                return Err(std::io::Error::other("no space left on device"));
                            }
                            Ok(Action::Continue)
                        },
                        || dash(root).save_many(rows, serde_json::Map::new(), None, 1002.0),
                    );
                    assert!(result.is_err());
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    dash(root)
                        .patch(
                            p["victim"]["id"].as_str().expect("id"),
                            |mut current| {
                                current["notes"] = "EDITED-BY-THE-OTHER-REQUEST".into();
                                Ok((current, serde_json::Map::new()))
                            },
                            None,
                            None,
                            1003.0,
                        )
                        .expect("concurrent save");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                let p = seeded(root);
                assert_eq!(
                    read(root, "dashboard", p["victim"]["id"].as_str().expect("id"))["notes"],
                    "EDITED-BY-THE-OTHER-REQUEST"
                );
            },
        );
    }
);
source_test!(
    c000_a_folder_rename_and_a_new_profile_in_it_cannot_split,
    "tests/test_profile_transactions.py::test_a_folder_rename_and_a_new_profile_in_it_cannot_split",
    {
        processes(
            "folder-create-race",
            |root| {
                numera_core::storage::folders::Folders::new(root.to_owned())
                    .create("source")
                    .expect("folder");
                made(root, json!({"name":"existing","folder":"source"}));
            },
            |root, role, wire| {
                if role == "holder" {
                    use numera_core::vault::testing::{Action, with_observer};
                    let mut gate = wire.cloned();
                    let mut first = true;
                    with_observer(
                        move |e| {
                            if first && e.operation == "before-batch-write" {
                                first = false;
                                gate.say("inside");
                                gate.wait();
                            }
                            Ok(Action::Continue)
                        },
                        || {
                            numera_core::storage::folders::Folders::new(root.to_owned())
                                .rename("source", "dest")
                        },
                    )
                    .expect("rename");
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    dash(root)
                        .save(
                            json!({"name":"late","folder":"source"}),
                            serde_json::Map::new(),
                            None,
                            None,
                            1002.0,
                        )
                        .expect("late profile");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                assert!(
                    !numera_core::storage::folders::Folders::new(root.to_owned())
                        .listing()
                        .expect("folders")
                        .contains(&"source".into())
                );
                for p in dash(root).list().expect("profiles") {
                    assert_eq!(p["folder"], "dest");
                }
            },
        );
    }
);
source_test!(
    c000_delete_racing_a_save_leaves_one_coherent_state,
    "tests/test_profile_transactions.py::test_delete_racing_a_save_leaves_one_coherent_state",
    {
        let tmp = common::TempDirectory::new("contract-delete-save-race").expect("root");
        let p = made(tmp.path(), json!({"name":"contested","notes":"v1"}));
        let id = p["id"].as_str().expect("id");
        let barrier = Barrier::new(2);
        thread::scope(|scope| {
            let saving = scope.spawn(|| {
                barrier.wait();
                let mut changed = p.clone();
                changed["notes"] = "v2".into();
                let _ = dash(tmp.path()).save(changed, serde_json::Map::new(), None, None, 1002.0);
            });
            barrier.wait();
            let _ = dash(tmp.path()).delete(id, "source-contract-host", 1002.0);
            saving.join().expect("saver finished");
        });
        assert_eq!(
            dash(tmp.path()).get(id).expect("dashboard").is_some(),
            tmp.path()
                .join("profiles")
                .join(format!("{id}.json"))
                .exists()
        );
    }
);
