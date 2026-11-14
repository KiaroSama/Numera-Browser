use super::*;
source_test!(
    a_successful_delete_leaves_no_tombstone,
    "tests/test_repo_authority.py::test_a_successful_delete_leaves_no_tombstone",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "kept", 1);
        let id = &p.id;
        let ud = sessions(root, id);
        assert!(
            store(root)
                .delete(id, "source-storage-host", 1001.0)
                .expect("delete")
        );
        assert!(!ud.exists());
        assert!(directories(&root.join("user-data"), ".deleting-").is_empty());
    }
);
source_test!(
    a_tombstone_that_will_not_go_is_reported_not_swallowed,
    "tests/test_repo_authority.py::test_a_tombstone_that_will_not_go_is_reported_not_swallowed",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "kept", 1);
        let id = &p.id;
        let ud = sessions(root, id);
        let warning = process::output(process::start(
            "repo_authority::a_tombstone_that_will_not_go_is_reported_not_swallowed",
            root,
            "cleanup",
            "delete-skip",
            id,
        ));
        let warning = String::from_utf8_lossy(&warning.stderr);
        assert!(warning.contains(".deleting-") && warning.contains(id));
        assert!(store(root).get(id).expect("get").is_none());
        assert!(!root.join(format!("dashboard/{id}.json")).exists());
        let retained = directories(&root.join("user-data"), ".deleting-");
        assert_eq!(retained.len(), 1);
        assert!(retained[0].join("Cookies").is_file());
        assert!(!ud.exists());
    }
);
source_test!(
    a_failed_metadata_delete_puts_everything_back,
    "tests/test_repo_authority.py::test_a_failed_metadata_delete_puts_everything_back",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "kept", 1);
        let id = &p.id;
        let ud = sessions(root, id);
        let before = pair(root, id);
        let result = testing::with_observer(
            |event| {
                if event.operation == "before-delete-metadata"
                    && event.path.parent().is_some_and(|p| p.ends_with("profiles"))
                {
                    return Err(io::Error::other("no"));
                }
                Ok(Action::Continue)
            },
            || store(root).delete(id, "source-storage-host", 1001.0),
        );
        assert!(result.is_err());
        assert_eq!(pair(root, id), before);
        assert!(ud.join("Cookies").is_file());
        assert!(directories(&root.join("user-data"), ".deleting-").is_empty());
    }
);
source_test!(
    a_rollback_that_also_fails_names_every_casualty,
    "tests/test_repo_authority.py::test_a_rollback_that_also_fails_names_every_casualty",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "kept", 1);
        let id = &p.id;
        let ud = sessions(root, id);
        let error = testing::with_observer(
            |event| {
                if event.operation == "before-delete-metadata"
                    && event.path.parent().is_some_and(|p| p.ends_with("profiles"))
                {
                    return Err(io::Error::other("the metadata delete failed"));
                }
                if event.operation == "before-restore" {
                    return Err(io::Error::other("and so did putting it back"));
                }
                Ok(Action::Continue)
            },
            || store(root).delete(id, "source-storage-host", 1001.0),
        )
        .expect_err("rollback failed");
        let numera_core::storage::repo::RepoError::Io(error) = error else {
            panic!("typed I/O carrier missing")
        };
        let failure = error
            .get_ref()
            .and_then(|e| e.downcast_ref::<numera_core::storage::repo::DeleteRollback>())
            .expect("typed delete rollback");
        assert!(failure.cause.to_string().contains("metadata delete failed"));
        assert!(!failure.failures.is_empty());
        assert!(
            failure
                .failures
                .iter()
                .any(|(path, _)| path.parent().is_some_and(|p| p.ends_with("dashboard")))
        );
        assert!(ud.join("Cookies").is_file());
    }
);
source_test!(
    a_cli_edit_can_clear_notes,
    "tests/test_repo_authority.py::test_a_cli_edit_can_clear_notes",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"noted","notes":"remember me"}));
        let id = p["id"].as_str().expect("id");
        edit_note(root, id, "");
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            ""
        );
        assert_eq!(raw(root, &format!("dashboard/{id}.json"))["notes"], "");
    }
);
source_test!(
    removing_the_last_tag_reaches_both_copies,
    "tests/test_repo_authority.py::test_removing_the_last_tag_reaches_both_copies",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"tagged","tags":["keep"]}));
        let id = p["id"].as_str().expect("id");
        store(root)
            .edit(
                id,
                |p| {
                    p.tags.clear();
                    Ok(None)
                },
                1001.0,
            )
            .expect("clear tags");
        assert!(
            store(root)
                .get(id)
                .expect("get")
                .expect("profile")
                .tags
                .is_empty()
        );
        assert_eq!(
            raw(root, &format!("dashboard/{id}.json"))["tags"],
            json!([])
        );
    }
);
source_test!(
    clearing_urls_extensions_and_proxy_reaches_both_copies,
    "tests/test_repo_authority.py::test_clearing_urls_extensions_and_proxy_reaches_both_copies",
    {
        let temp = temp();
        let root = temp.path();
        let mut p = profile("full", 1);
        p.startup_urls = vec!["https://a.test".into()];
        p.proxy = Some(
            Proxy::from_record(&json!({"server":"http://1.2.3.4:8080"}), false).expect("proxy"),
        );
        let p = store(root).save(p, 1000.0).expect("save");
        store(root)
            .edit(
                &p.id,
                |p| {
                    p.startup_urls.clear();
                    p.proxy = None;
                    Ok(None)
                },
                1001.0,
            )
            .expect("clear");
        let saved = store(root).get(&p.id).expect("get").expect("profile");
        assert!(saved.startup_urls.is_empty() && saved.proxy.is_none());
        let d = raw(root, &format!("dashboard/{}.json", p.id));
        assert!([json!(""), json!([]), Value::Null].contains(&d["startupUrls"]));
        assert!(d["proxy"].is_null() || d["proxy"] == json!({}));
    }
);
source_test!(
    apply_does_not_overwrite_an_edit_it_never_saw,
    "tests/test_repo_authority.py::test_apply_does_not_overwrite_an_edit_it_never_saw",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id").to_owned();
        let edit_root = root.to_owned();
        let edit_id = id.clone();
        let fired = Arc::new(Mutex::new(false));
        let seen = fired.clone();
        let outcome = testing::with_observer(
            move |event| {
                if event.operation == "after-profile-selection" {
                    assert!(!*seen.lock().expect("fired"));
                    *seen.lock().expect("fired") = true;
                    edit_note(&edit_root, &edit_id, "written by the API");
                }
                Ok(Action::Continue)
            },
            || {
                cli(root, |commands| {
                    commands.apply(
                        json!({"all":true,"add_tag":["cli"],"yes":true})
                            .as_object()
                            .expect("args"),
                        root,
                        1001.0,
                        |_| Ok(true),
                    )
                })
            },
        )
        .expect("CLI mutation");
        assert_eq!(outcome.exit, 0);
        assert!(*fired.lock().expect("fired"));
        let saved = store(root).get(&id).expect("get").expect("profile");
        assert_eq!(saved.notes, "written by the API");
        assert_eq!(
            raw(root, &format!("dashboard/{id}.json"))["notes"],
            "written by the API"
        );
        assert!(saved.tags.contains(&"cli".into()));
    }
);
source_test!(
    pool_assign_does_not_overwrite_an_edit_it_never_saw,
    "tests/test_repo_authority.py::test_pool_assign_does_not_overwrite_an_edit_it_never_saw",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "shared");
        let id = p["id"].as_str().expect("id").to_owned();
        pool(root)
            .add_many(numera_core::pool::parse_many("http://u:p@5.6.7.8:3128:US"))
            .expect("pool");
        let edit_root = root.to_owned();
        let edit_id = id.clone();
        let fired = Arc::new(Mutex::new(false));
        let seen = fired.clone();
        let outcome = testing::with_observer(
            move |event| {
                if event.operation == "after-profile-selection" {
                    assert!(!*seen.lock().expect("fired"));
                    *seen.lock().expect("fired") = true;
                    edit_note(&edit_root, &edit_id, "written by the API");
                }
                Ok(Action::Continue)
            },
            || {
                cli(root, |profiles| {
                    let store = pool(root);
                    numera_core::cli::pool_commands::Pool {
                        store: &store,
                        profiles,
                    }
                    .assign(json!({"all":true}).as_object().expect("args"), 1001.0)
                })
            },
        )
        .expect("CLI mutation");
        assert_eq!(outcome.exit, 0);
        assert!(*fired.lock().expect("fired"));
        let saved = store(root).get(&id).expect("get").expect("profile");
        assert_eq!(saved.notes, "written by the API");
        assert_eq!(
            raw(root, &format!("dashboard/{id}.json"))["notes"],
            "written by the API"
        );
        assert!(
            saved
                .proxy
                .expect("assigned proxy")
                .server
                .contains("5.6.7.8")
        );
    }
);
source_test!(
    a_failing_write_rolls_the_whole_patch_back,
    "tests/test_repo_authority.py::test_a_failing_write_rolls_the_whole_patch_back",
    {
        let temp = temp();
        let root = temp.path();
        let rows = [
            saved(root, "one", 1),
            saved(root, "two", 2),
            saved(root, "three", 3),
        ];
        let before = snapshot(root);
        let result = testing::with_observer(nth_failure(4), || {
            store(root).patch_many(
                &rows.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
                |p| {
                    p.notes = "bulk".into();
                    Ok(true)
                },
                Default::default(),
                "reject",
                1001.0,
            )
        });
        assert!(result.is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    patch_many_skips_a_profile_that_is_already_gone,
    "tests/test_repo_authority.py::test_patch_many_skips_a_profile_that_is_already_gone",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "kept", 1);
        let written = store(root)
            .patch_many(
                &[p.id.clone(), "nosuchprofile00".into()],
                |p| {
                    p.notes = "here".into();
                    Ok(true)
                },
                Default::default(),
                "reject",
                1001.0,
            )
            .expect("patch");
        assert_eq!(
            written.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            [p.id.as_str()]
        );
    }
);
source_test!(
    a_delete_committing_before_the_lock_is_a_clean_miss,
    "tests/test_repo_authority.py::test_a_delete_committing_before_the_lock_is_a_clean_miss",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "doomed", 1);
        let path = root.join(format!("profiles/{}.json", p.id));
        let fired = Arc::new(Mutex::new(false));
        let seen = fired.clone();
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-profile-get-lock" {
                    fs::remove_file(&path)?;
                    *seen.lock().expect("fired") = true;
                }
                Ok(Action::Continue)
            },
            || store(root).get(&p.id),
        )
        .expect("racing get");
        assert!(result.is_none());
        assert!(*fired.lock().expect("fired"));
    }
);
source_test!(
    a_rename_before_the_lease_refuses_the_delete,
    "tests/test_repo_authority.py::test_a_rename_before_the_lease_refuses_the_delete",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let directory = root.to_owned();
        let id = p.id.clone();
        let fired = Arc::new(Mutex::new(false));
        let seen = fired.clone();
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-profile-delete-lease" {
                    *seen.lock().expect("fired") = true;
                    store(&directory)
                        .edit(
                            &id,
                            |p| {
                                p.name = "beta".into();
                                Ok(None)
                            },
                            1001.0,
                        )
                        .expect("racing rename");
                    saved(&directory, "alpha", 2);
                }
                Ok(Action::Continue)
            },
            || store(root).delete("alpha", "source-storage-host", 1002.0),
        );
        assert!(*fired.lock().expect("fired"));
        let error = result.expect_err("target moved");
        assert!(error.to_string().contains("means"), "{error}");
        assert!(store(root).get(&p.id).expect("get renamed").is_some());
    }
);
source_test!(
    a_delete_by_id_after_a_rename_still_works,
    "tests/test_repo_authority.py::test_a_delete_by_id_after_a_rename_still_works",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let directory = root.to_owned();
        let id = p.id.clone();
        let fired = Arc::new(Mutex::new(false));
        let seen = fired.clone();
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-profile-delete-lease" {
                    *seen.lock().expect("fired") = true;
                    store(&directory)
                        .edit(
                            &id,
                            |p| {
                                p.name = "beta".into();
                                Ok(None)
                            },
                            1001.0,
                        )
                        .expect("racing rename");
                }
                Ok(Action::Continue)
            },
            || store(root).delete(&p.id, "source-storage-host", 1002.0),
        );
        assert!(*fired.lock().expect("fired"));
        assert!(result.expect("ID delete"));
        assert!(store(root).get(&p.id).expect("get deleted").is_none());
    }
);
source_test!(
    reclaiming_a_deleted_session_does_not_block_the_data_root,
    "tests/test_repo_authority.py::test_reclaiming_a_deleted_session_does_not_block_the_data_root",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "kept", 1);
        let id = &p.id;
        let ud = sessions(root, id);
        let other = saved(root, "other", 2);
        let (sender, receiver) = mpsc::channel();
        let (go, release) = mpsc::channel();
        let directory = root.to_owned();
        let target = id.clone();
        let worker = thread::spawn(move || {
            testing::with_observer(
                move |event| {
                    if event.operation == "before-session-reclaim" {
                        sender.send(()).expect("cleanup reached");
                        release
                            .recv_timeout(Duration::from_secs(10))
                            .expect("release cleanup");
                    }
                    Ok(Action::Continue)
                },
                || store(&directory).delete(&target, "source-storage-host", 1001.0),
            )
        });
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("cleanup reached");
        edit_note(root, &other.id, "went through");
        go.send(()).expect("release");
        assert!(
            worker
                .join()
                .expect("delete worker")
                .expect("committed delete")
        );
        assert_eq!(
            store(root)
                .get(&other.id)
                .expect("get other")
                .expect("profile")
                .notes,
            "went through"
        );
        assert!(store(root).get(id).expect("get deleted").is_none());
        assert!(!ud.exists());
        assert!(directories(&root.join("user-data"), ".deleting-").is_empty());
    }
);
