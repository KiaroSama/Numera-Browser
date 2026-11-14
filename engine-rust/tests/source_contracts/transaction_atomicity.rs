use super::*;
source_test!(
    c000_a_failing_config_write_leaves_no_profiles_from_a_batch_create,
    "tests/test_transaction_atomicity.py::test_a_failing_config_write_leaves_no_profiles_from_a_batch_create",
    {
        write_fault("batch", 5);
    }
);
source_test!(
    c000_a_profile_transaction_and_a_settings_change_both_survive,
    "tests/test_transaction_atomicity.py::test_a_profile_transaction_and_a_settings_change_both_survive",
    {
        processes(
            "a_profile_transaction_and_a_settings_change_both_survive",
            seed,
            |root, role, wire| {
                let p = seeded(root);
                if role == "holder" {
                    dash(root)
                        .patch(
                            p["id"].as_str().expect("id"),
                            |mut current| {
                                wire.say("inside");
                                wire.wait();
                                current["engine"] = "camoufox".into();
                                Ok((
                                    current,
                                    serde_json::Map::from_iter([(
                                        "default_engine".into(),
                                        json!("camoufox"),
                                    )]),
                                ))
                            },
                            None,
                            None,
                            1002.0,
                        )
                        .expect("profile transaction");
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    config(root)
                        .set("headless_launch", json!(true))
                        .expect("settings write");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                let cfg = config(root).read().expect("config");
                assert_eq!(cfg["headless_launch"], true);
                assert_eq!(cfg["default_engine"], "camoufox");
            },
        );
    }
);
source_test!(
    c000_a_rolled_back_transaction_cannot_undo_a_committed_settings_change,
    "tests/test_transaction_atomicity.py::test_a_rolled_back_transaction_cannot_undo_a_committed_settings_change",
    {
        processes(
            "a_rolled_back_transaction_cannot_undo_a_committed_settings_change",
            seed,
            |root, role, wire| {
                let p = seeded(root);
                if role == "holder" {
                    use numera_core::vault::testing::{Action, with_observer};
                    let mut gate = wire.cloned();
                    let mut failed = false;
                    let result = with_observer(
                        move |e| {
                            if !failed
                                && e.operation == "after-atomic-write"
                                && e.path.file_name().is_some_and(|p| p == "config.json")
                            {
                                failed = true;
                                gate.say("inside");
                                gate.wait();
                                return Err(std::io::Error::other("no space left on device"));
                            }
                            Ok(Action::Continue)
                        },
                        || {
                            dash(root).patch(
                                p["id"].as_str().expect("id"),
                                |mut current| {
                                    current["engine"] = "camoufox".into();
                                    Ok((
                                        current,
                                        serde_json::Map::from_iter([(
                                            "default_engine".into(),
                                            json!("camoufox"),
                                        )]),
                                    ))
                                },
                                None,
                                None,
                                1002.0,
                            )
                        },
                    );
                    assert!(result.is_err());
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    config(root)
                        .set("headless_launch", json!(true))
                        .expect("settings write");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                let cfg = config(root).read().expect("config");
                assert_eq!(cfg["headless_launch"], true);
                assert_ne!(cfg.get("default_engine"), Some(&json!("camoufox")));
            },
        );
    }
);
source_test!(
    c000_a_failed_rollback_is_raised_with_the_files_it_could_not_restore,
    "tests/test_transaction_atomicity.py::test_a_failed_rollback_is_raised_with_the_files_it_could_not_restore",
    {
        rollback_failures();
    }
);
source_test!(
    c000_a_failed_folder_rollback_is_raised_too,
    "tests/test_transaction_atomicity.py::test_a_failed_folder_rollback_is_raised_too",
    {
        folder_rollback_failure();
    }
);
source_test!(
    c000_two_processes_cannot_create_the_same_token_name,
    "tests/test_transaction_atomicity.py::test_two_processes_cannot_create_the_same_token_name",
    {
        processes(
            "token-name-race",
            |_| {},
            |root, _, wire| {
                wire.say("ready");
                wire.wait();
                let tokens = numera_core::http::tokens::TokenStore::new(root.to_owned());
                match tokens.create("CI runner", 1000.0) {
                    Ok(_) => wire.say("created"),
                    Err(e) => {
                        assert!(e.to_string().contains("already exists"));
                        wire.say("refused");
                    }
                }
            },
            |root, peers| {
                peers.expect("holder", "ready");
                peers.expect("later", "ready");
                peers.release("holder");
                peers.release("later");
                let outcomes = [peers.next("holder"), peers.next("later")];
                assert_eq!(
                    outcomes.iter().filter(|s| s.as_str() == "created").count(),
                    1
                );
                let rows = numera_core::http::tokens::TokenStore::new(root.to_owned()).list();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0]["name"], "CI runner");
            },
        );
    }
);
source_test!(
    c000_the_api_and_the_cli_report_the_same_conflict,
    "tests/test_transaction_atomicity.py::test_the_api_and_the_cli_report_the_same_conflict",
    {
        token_conflict();
    }
);
