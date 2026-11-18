use super::*;
source_test!(
    an_edit_that_succeeded_survives_a_failing_restore,
    "tests/test_restore_race.py::test_an_edit_that_succeeded_survives_a_failing_restore",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, zip) = restored_fixture(root);
        let test = "restore_race::an_edit_that_succeeded_survives_a_failing_restore";
        let editor = process::start(test, root, "editor", "edit-note", &p.id);
        process::reached(root, "editor", "ready");
        let restorer = process::start(
            test,
            root,
            "restorer",
            "restore-before-fail",
            zip.to_str().expect("archive path"),
        );
        process::reached(root, "restorer", "staged");
        process::release(root, "editor");
        process::reached(root, "editor", "edited");
        process::release(root, "restorer");
        process::finish(editor);
        process::finish(restorer);
        assert_eq!(
            store(root).get(&p.id).expect("get").expect("profile").notes,
            "kept"
        );
        assert_eq!(
            raw(root, &format!("dashboard/{}.json", p.id))["notes"],
            "kept"
        );
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("live session"),
            b"the live session"
        );
        assert!(directories(&root.join("user-data"), ".replaced-").is_empty());
    }
);
source_test!(
    a_write_cannot_land_inside_the_restore_transaction,
    "tests/test_restore_race.py::test_a_write_cannot_land_inside_the_restore_transaction",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, zip) = restored_fixture(root);
        let test = "restore_race::a_write_cannot_land_inside_the_restore_transaction";
        let editor = process::start(test, root, "editor", "edit-note", &p.id);
        process::reached(root, "editor", "ready");
        let restorer = process::start(
            test,
            root,
            "restorer",
            "restore-transfer",
            zip.to_str().expect("archive path"),
        );
        process::reached(root, "restorer", "swapping");
        process::release(root, "editor");
        process::reached(root, "editor", "trying");
        process::release(root, "restorer");
        process::finish(editor);
        process::finish(restorer);
        assert_eq!(
            store(root).get(&p.id).expect("get").expect("profile").notes,
            "kept"
        );
        assert_eq!(
            raw(root, &format!("dashboard/{}.json", p.id))["notes"],
            "kept"
        );
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("live session"),
            b"the live session"
        );
        assert!(directories(&root.join("user-data"), ".replaced-").is_empty());
    }
);
source_test!(
    a_session_that_cannot_be_put_back_is_named_in_the_error,
    "tests/test_restore_race.py::test_a_session_that_cannot_be_put_back_is_named_in_the_error",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, zip) = restored_fixture(root);
        let error = testing::with_observer(
            |event| {
                if ["before-restore-transfer", "before-restore-session-rollback"]
                    .contains(&event.operation)
                {
                    return Err(io::Error::other(
                        if event.operation == "before-restore-transfer" {
                            "in use"
                        } else {
                            "session held open"
                        },
                    ));
                }
                Ok(Action::Continue)
            },
            || store(root).restore(&zip, false, "source-storage-host", 1001.0),
        )
        .err()
        .expect("stranded session");
        assert!(
            error.to_string().contains(".replaced-")
                && error
                    .to_string()
                    .contains(ud.to_str().expect("session path"))
                && error.to_string().contains("in use")
        );
        let numera_core::storage::repo::RepoError::Io(error) = error else {
            panic!("typed session error")
        };
        let stranded = error
            .get_ref()
            .and_then(|e| e.downcast_ref::<paths::SessionStranded>())
            .expect("typed stranded session");
        assert_eq!(stranded.destination, ud);
        assert!(stranded.retained.join("Cookies").is_file());
        assert_eq!(
            store(root).get(&p.id).expect("get").expect("profile").name,
            "alpha"
        );
    }
);
source_test!(
    a_restore_that_fails_before_the_lease_touches_nothing,
    "tests/test_restore_race.py::test_a_restore_that_fails_before_the_lease_touches_nothing",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, _zip) = restored_fixture(root);
        let bad = root.join("evil.zip");
        archive(
            &bad,
            Some(serde_json::to_value(&p).expect("profile")),
            &[("user-data/../../../pwned.txt", b"x")],
        );
        let result = testing::with_observer(
            |event| {
                assert_ne!(
                    event.operation, "before-restore-lease",
                    "unsafe archive reached lease"
                );
                Ok(Action::Continue)
            },
            || store(root).restore(&bad, false, "source-storage-host", 1001.0),
        );
        assert!(result.is_err());
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("live session"),
            b"the live session"
        );
        assert!(directories(root, "numera-restore-").is_empty());
    }
);
source_test!(
    a_replaced_session_that_cannot_be_reclaimed_is_named,
    "tests/test_restore_race.py::test_a_replaced_session_that_cannot_be_reclaimed_is_named",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, zip) = restored_fixture(root);
        edit_note(root, &p.id, "changed");
        let warning = process::output(process::start(
            "restore_race::a_replaced_session_that_cannot_be_reclaimed_is_named",
            root,
            "cleanup",
            "restore-skip",
            zip.to_str().expect("zip path"),
        ));
        let warning = String::from_utf8_lossy(&warning.stderr);
        assert!(warning.contains(".replaced-") && warning.contains(&p.id));
        assert_eq!(
            store(root).get(&p.id).expect("get").expect("profile").notes,
            ""
        );
        let leftovers = directories(&root.join("user-data"), ".replaced-");
        assert_eq!(leftovers.len(), 1);
        assert!(leftovers[0].join("Cookies").is_file());
        assert!(ud.join("Cookies").is_file());
    }
);
source_test!(
    reclaiming_a_replaced_session_does_not_block_the_data_root,
    "tests/test_restore_race.py::test_reclaiming_a_replaced_session_does_not_block_the_data_root",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, zip) = restored_fixture(root);
        let other = saved(root, "other", 2);
        let (sender, receiver) = mpsc::channel();
        let (go, release) = mpsc::channel();
        let directory = root.to_owned();
        let worker = thread::spawn(move || {
            testing::with_observer(
                move |event| {
                    if event.operation == "before-restore-reclaim" {
                        sender.send(()).expect("cleanup reached");
                        release
                            .recv_timeout(Duration::from_secs(10))
                            .expect("release cleanup");
                    }
                    Ok(Action::Continue)
                },
                || store(&directory).restore(&zip, false, "source-storage-host", 1001.0),
            )
        });
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("cleanup reached");
        edit_note(root, &other.id, "went through");
        go.send(()).expect("release cleanup");
        worker.join().expect("restore worker").expect("restore");
        assert_eq!(
            store(root)
                .get(&other.id)
                .expect("get other")
                .expect("profile")
                .notes,
            "went through"
        );
        assert!(ud.join("Cookies").is_file());
        assert!(directories(&root.join("user-data"), ".replaced-").is_empty());
        assert_eq!(
            store(root)
                .get(&p.id)
                .expect("get restored")
                .expect("profile")
                .name,
            "alpha"
        );
    }
);
