use super::*;
source_test!(
    c000_deleting_a_session_waits_for_the_browser_that_has_it,
    "tests/test_public_boundaries.py::test_deleting_a_session_waits_for_the_browser_that_has_it",
    {
        processes(
            "session-lease",
            |root| {
                let p = repo(root).save(profile("p"), 1000.0).expect("profile");
                let live = repo(root).user_data_path(&p.id).expect("session");
                fs::write(live.join("Cookies"), "live").expect("bytes");
                fs::write(
                    root.join("seed.json"),
                    encode(&serde_json::to_value(p).expect("record")),
                )
                .expect("seed");
            },
            |root, role, wire| {
                if role == "holder" {
                    let p = seeded(root);
                    let lease = SessionLease::acquire(
                        &SessionLease::path_for(root, p["id"].as_str().expect("id"))
                            .expect("lease path"),
                    )
                    .expect("session owner");
                    wire.say("holding");
                    wire.wait();
                    drop(lease);
                    wire.say("released");
                } else {
                    wire.wait();
                    wire.say("done");
                }
            },
            |root, peers| {
                peers.expect("holder", "holding");
                let p = seeded(root);
                let id = p["id"].as_str().expect("id");
                let live = repo(root).user_data_path(id).expect("live");
                assert!(matches!(
                    repo(root).delete_session(id, "source-contract-host"),
                    Err(numera_core::storage::repo::RepoError::SessionBusy(_))
                ));
                assert_eq!(
                    fs::read_to_string(live.join("Cookies")).expect("live bytes"),
                    "live"
                );
                assert!(
                    !fs::read_dir(live.parent().expect("session parent"))
                        .expect("sessions")
                        .any(|e| e
                            .expect("entry")
                            .file_name()
                            .to_string_lossy()
                            .contains(".deleting-"))
                );
                peers.release("holder");
                peers.expect("holder", "released");
                repo(root)
                    .delete_session(id, "source-contract-host")
                    .expect("unheld delete");
                assert!(!live.exists());
                peers.release("later");
                peers.expect("later", "done");
            },
        );
    }
);
source_test!(
    c000_deleting_a_session_lets_go_before_it_reclaims,
    "tests/test_public_boundaries.py::test_deleting_a_session_lets_go_before_it_reclaims",
    {
        cleanup_outside_lease();
    }
);
source_test!(
    c000_two_acknowledged_config_changes_both_survive,
    "tests/test_public_boundaries.py::test_two_acknowledged_config_changes_both_survive",
    {
        processes(
            "two_acknowledged_config_changes_both_survive",
            |root| {
                config(root)
                    .set("default_engine", json!("playwright"))
                    .expect("seed");
            },
            |root, role, wire| {
                if role == "holder" {
                    let lock = DataLock::transaction(root).expect("transaction");
                    let guard = lock.acquire_configured().expect("root ownership");
                    let mut values = config(root).read().expect("read snapshot");
                    wire.say("inside");
                    wire.wait();
                    values.insert("headless_launch".into(), json!(true));
                    config(root)
                        .replace_locked(values)
                        .expect("locked snapshot write");
                    drop(guard);
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    config(root)
                        .set("default_engine", json!("patchright"))
                        .expect("acknowledged setting");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                let cfg = config(root).read().expect("committed config");
                assert_eq!(cfg["default_engine"], "patchright");
                assert_eq!(cfg["headless_launch"], true);
            },
        );
    }
);
source_test!(
    c000_replacing_the_whole_config_needs_the_transaction_that_read_it,
    "tests/test_public_boundaries.py::test_replacing_the_whole_config_needs_the_transaction_that_read_it",
    {
        let tmp = common::TempDirectory::new("contract-whole-config").expect("root");
        config(tmp.path())
            .set("default_engine", json!("playwright"))
            .expect("seed");
        assert!(
            config(tmp.path())
                .replace_locked(serde_json::Map::from_iter([(
                    "default_engine".into(),
                    json!("camoufox")
                )]))
                .expect_err("root transaction required")
                .to_string()
                .contains("root transaction")
        );
        assert_eq!(
            config(tmp.path())
                .default_engine(&BTreeMap::new())
                .expect("engine"),
            "playwright"
        );
        let lock = DataLock::transaction(tmp.path()).expect("transaction");
        {
            let _guard = lock.acquire_configured().expect("owner");
            let mut values = config(tmp.path()).read().expect("snapshot");
            values.insert("default_engine".into(), json!("camoufox"));
            config(tmp.path())
                .replace_locked(values)
                .expect("owned replacement");
        }
        assert_eq!(
            config(tmp.path())
                .default_engine(&BTreeMap::new())
                .expect("engine"),
            "camoufox"
        );
    }
);
source_test!(
    c000_a_settings_change_waits_for_the_transaction,
    "tests/test_public_boundaries.py::test_a_settings_change_waits_for_the_transaction",
    {
        processes(
            "a_settings_change_waits_for_the_transaction",
            |root| {
                config(root)
                    .set("default_engine", json!("playwright"))
                    .expect("seed");
            },
            |root, role, wire| {
                if role == "holder" {
                    let lock = DataLock::transaction(root).expect("transaction");
                    let guard = lock.acquire_configured().expect("root ownership");
                    let mut values = config(root).read().expect("read snapshot");
                    wire.say("inside");
                    wire.wait();
                    assert_eq!(values.remove("default_engine"), Some(json!("playwright")));
                    drop(guard);
                    wire.say("done");
                } else {
                    wire.wait();
                    wire.say("trying");
                    config(root)
                        .set("default_engine", json!("camoufox"))
                        .expect("acknowledged setting");
                    wire.say("done");
                }
            },
            |root, peers| {
                blocking_interleave(peers);
                let cfg = config(root).read().expect("committed config");
                assert_eq!(cfg["default_engine"], "camoufox");
            },
        );
    }
);
