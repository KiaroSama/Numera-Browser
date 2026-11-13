use super::*;
source_test!(
    c000_batch_create_persists_the_revision_it_returns,
    "tests/test_revision_and_patch.py::test_batch_create_persists_the_revision_it_returns",
    {
        revision_disk("batch");
    }
);
source_test!(
    c000_import_persists_the_revision_it_returns,
    "tests/test_revision_and_patch.py::test_import_persists_the_revision_it_returns",
    {
        revision_disk("import");
    }
);
source_test!(
    c000_two_consecutive_bulk_updates_persist_every_revision,
    "tests/test_revision_and_patch.py::test_two_consecutive_bulk_updates_persist_every_revision",
    {
        revision_disk("bulk");
    }
);
source_test!(
    c000_a_revision_held_from_before_a_bulk_edit_is_refused,
    "tests/test_revision_and_patch.py::test_a_revision_held_from_before_a_bulk_edit_is_refused",
    {
        let tmp = common::TempDirectory::new("contract-stale-bulk").expect("root");
        let p = made(tmp.path(), json!({"name":"p"}));
        assert_eq!(
            http(
                tmp.path(),
                "POST",
                "/api/profiles/bulk",
                json!({"ids":[p["id"]],"patch":{"notes":"moved on"}})
            )
            .0,
            200
        );
        let mut stale = p.clone();
        stale["notes"] = "mine".into();
        let (s, v) = put(tmp.path(), &p["id"], stale);
        assert_eq!(s, 409);
        assert_eq!(v["kind"], "StaleProfile");
        assert_eq!(
            read(tmp.path(), "dashboard", p["id"].as_str().expect("id"))["notes"],
            "moved on"
        );
    }
);
source_test!(
    c000_a_notes_patch_and_a_tags_patch_both_survive,
    "tests/test_revision_and_patch.py::test_a_notes_patch_and_a_tags_patch_both_survive",
    {
        processes(
            "a_notes_patch_and_a_tags_patch_both_survive",
            seed,
            |root, role, wire| {
                let p = seeded(root);
                let id = p["id"].as_str().expect("id");
                if role == "holder" {
                    dash(root)
                        .patch(
                            id,
                            |mut current| {
                                current["notes"] = "from the notes patch".into();
                                wire.say("inside");
                                wire.wait();
                                Ok((current, serde_json::Map::new()))
                            },
                            None,
                            None,
                            1002.0,
                        )
                        .expect("notes patch");
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    dash(root)
                        .patch(
                            id,
                            |mut current| {
                                current["tags"] = json!(["late"]);
                                Ok((current, serde_json::Map::new()))
                            },
                            None,
                            None,
                            1003.0,
                        )
                        .expect("tags patch");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                let p = seeded(root);
                let final_p = read(root, "dashboard", p["id"].as_str().expect("id"));
                assert_eq!(final_p["notes"], "from the notes patch");
                assert_eq!(final_p["tags"], json!(["late"]));
                assert_eq!(final_p["_rev"], 3);
            },
        );
    }
);
source_test!(
    c000_a_bulk_edit_and_a_single_patch_both_survive,
    "tests/test_revision_and_patch.py::test_a_bulk_edit_and_a_single_patch_both_survive",
    {
        processes(
            "a_bulk_edit_and_a_single_patch_both_survive",
            seed,
            |root, role, wire| {
                let p = seeded(root);
                let id = p["id"].as_str().expect("id");
                if role == "holder" {
                    dash(root)
                        .patch(
                            id,
                            |mut current| {
                                current["notes"] = "from the notes patch".into();
                                wire.say("inside");
                                wire.wait();
                                Ok((current, serde_json::Map::new()))
                            },
                            None,
                            None,
                            1002.0,
                        )
                        .expect("notes patch");
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    dash(root)
                        .patch_many(
                            &[id.to_owned()],
                            |_, mut current| {
                                current["tags"] = json!(["late"]);
                                Ok(Some(current))
                            },
                            serde_json::Map::new(),
                            None,
                            1003.0,
                        )
                        .expect("bulk tags");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                let p = seeded(root);
                let final_p = read(root, "dashboard", p["id"].as_str().expect("id"));
                assert_eq!(final_p["notes"], "from the notes patch");
                assert_eq!(final_p["tags"], json!(["late"]));
                assert_eq!(final_p["_rev"], 3);
            },
        );
    }
);
source_test!(
    c000_a_delete_is_not_undone_by_a_concurrent_listing,
    "tests/test_revision_and_patch.py::test_a_delete_is_not_undone_by_a_concurrent_listing",
    {
        processes(
            "delete-listing-race",
            seed,
            |root, role, wire| {
                if role == "holder" {
                    use numera_core::vault::testing::{Action, with_observer};
                    let p = seeded(root);
                    let mut gate = wire.cloned();
                    with_observer(
                        move |e| {
                            if e.operation == "before-delete-metadata"
                                && e.path.parent().is_some_and(|p| p.ends_with("profiles"))
                            {
                                gate.say("inside");
                                gate.wait();
                            }
                            Ok(Action::Continue)
                        },
                        || {
                            dash(root).delete(
                                p["id"].as_str().expect("id"),
                                "source-contract-host",
                                1002.0,
                            )
                        },
                    )
                    .expect("delete");
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    let (s, v) = http(root, "GET", "/api/profiles", Value::Null);
                    assert_eq!(s, 200);
                    assert_eq!(v, json!([]));
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                assert!(ids(root, "dashboard").is_empty());
                assert!(ids(root, "profiles").is_empty());
            },
        );
    }
);
