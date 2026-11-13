use super::*;
source_test!(
    a_failed_delete_puts_the_session_back,
    "tests/test_delete_rollback.py::test_a_failed_delete_puts_the_session_back",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"victim"}));
        let id = p["id"].as_str().expect("id");
        let ud = sessions(root, id);
        let before = pair(root, id);
        let result = testing::with_observer(error_at("before-delete-metadata", "", 13), || {
            dash(root).delete(id, "source-storage-host", 1001.0)
        });
        assert!(result.is_err());
        assert!(dash(root).get(id).expect("get").is_some());
        assert_eq!(pair(root, id), before);
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("restored session"),
            b"the live session"
        );
        assert!(directories(&root.join("user-data"), ".deleting-").is_empty());
    }
);
source_test!(
    a_rollback_that_cannot_finish_names_the_tombstone,
    "tests/test_delete_rollback.py::test_a_rollback_that_cannot_finish_names_the_tombstone",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"victim"}));
        let id = p["id"].as_str().expect("id");
        let ud = sessions(root, id);
        let error = testing::with_observer(
            |event| {
                if ["before-delete-metadata", "before-session-restore"].contains(&event.operation) {
                    return Err(io::Error::other(
                        if event.operation == "before-delete-metadata" {
                            "metadata is read-only"
                        } else {
                            "session held open"
                        },
                    ));
                }
                Ok(Action::Continue)
            },
            || dash(root).delete(id, "source-storage-host", 1001.0),
        )
        .expect_err("rollback incomplete");
        assert!(error.to_string().contains(".deleting-") && error.to_string().contains(id));
        let numera_core::storage::repo::RepoError::Io(error) = error else {
            panic!("typed I/O rollback carrier")
        };
        let rollback = error
            .get_ref()
            .and_then(|e| e.downcast_ref::<numera_core::storage::repo::DeleteRollback>())
            .expect("typed rollback");
        assert!(rollback.cause.to_string().contains("metadata is read-only"));
        assert!(std::error::Error::source(rollback).is_some());
        assert!(rollback.failures.iter().any(|(_, e)| {
            e.get_ref()
                .is_some_and(|cause| cause.is::<paths::SessionStranded>())
        }));
        assert_eq!(rollback.session.as_ref().expect("stranded session").1, ud);
        assert!(
            rollback
                .session
                .as_ref()
                .expect("tombstone")
                .0
                .join("Cookies")
                .is_file()
        );
    }
);
source_test!(
    the_metadata_comes_back_through_the_atomic_writer,
    "tests/test_delete_rollback.py::test_the_metadata_comes_back_through_the_atomic_writer",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"victim"}));
        let id = p["id"].as_str().expect("id");
        let ud = sessions(root, id);
        let path = root.join(format!("dashboard/{id}.json"));
        let before = fs::read(&path).expect("before");
        let reached = Arc::new(Mutex::new(false));
        let seen = reached.clone();
        let restores = Arc::new(Mutex::new(Vec::new()));
        let writes = restores.clone();
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-delete-metadata"
                    && event.path.parent().is_some_and(|p| p.ends_with("profiles"))
                {
                    *seen.lock().expect("reached") = true;
                    return Err(io::Error::other("engine read-only"));
                }
                if event.operation == "before-atomic-replace" {
                    writes.lock().expect("writes").push(event.path.clone());
                }
                Ok(Action::Continue)
            },
            || dash(root).delete(id, "source-storage-host", 1001.0),
        );
        assert!(result.is_err());
        assert!(*reached.lock().expect("reached"));
        assert_eq!(fs::read(&path).expect("restored"), before);
        assert!(serde_json::from_slice::<Value>(&before).is_ok());
        assert!(restores.lock().expect("writes").contains(&path));
        assert!(ud.join("Cookies").is_file());
    }
);
source_test!(
    a_successful_delete_still_removes_everything,
    "tests/test_delete_rollback.py::test_a_successful_delete_still_removes_everything",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"victim"}));
        let id = p["id"].as_str().expect("id");
        let ud = sessions(root, id);
        assert!(
            dash(root)
                .delete(id, "source-storage-host", 1001.0)
                .expect("delete")
        );
        assert!(dash(root).get(id).expect("get").is_none());
        assert!(!root.join(format!("dashboard/{id}.json")).exists());
        assert!(!ud.exists());
    }
);
