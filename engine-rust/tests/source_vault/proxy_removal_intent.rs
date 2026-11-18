use super::*;
source_test!(
    the_four_things_a_write_can_say,
    "tests/test_proxy_removal_intent.py::test_the_four_things_a_write_can_say",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let _root = temp.path();
        for (value, expected) in [
            (json!({"name":"x"}), "preserve"),
            (json!({"proxy":null}), "remove"),
            (json!({"proxy":{}}), "preserve"),
            (json!({"proxy":{"pass":null}}), "preserve"),
            (json!({"proxy":{"pass":""}}), "clear"),
            (json!({"proxy":{"pass":"x"}}), "replace"),
            (Value::Null, "preserve"),
            (json!({"proxy":"nonsense"}), "preserve"),
        ] {
            assert_eq!(conversion::secret_intent(&value), expected);
        }
    }
);
source_test!(
    removing_a_proxy_removes_it_from_both_copies,
    "tests/test_proxy_removal_intent.py::test_removing_a_proxy_removes_it_from_both_copies",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let uri = format!("/api/profiles/{}", prof.id);
        let _before = raw_pair(root, &prof.id);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"name":"alpha","proxy":null}),
        );
        assert_eq!(status, 200, "{response}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(engine["proxy"].is_null());
        assert!(dashboard["proxy"].is_null());
        assert!(
            repo(root, None)
                .get(&prof.id)
                .expect("get")
                .expect("profile")
                .proxy
                .is_none()
        );
    }
);
source_test!(
    removing_a_divergent_proxy_is_allowed_and_settles_it,
    "tests/test_proxy_removal_intent.py::test_removing_a_divergent_proxy_is_allowed_and_settles_it",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let uri = format!("/api/profiles/{}", prof.id);
        diverge(root, &prof.id);
        let _before = raw_pair(root, &prof.id);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"name":"alpha","proxy":null}),
        );
        assert_eq!(status, 200, "{response}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(engine["proxy"].is_null());
        assert!(dashboard["proxy"].is_null());
        assert!(
            repo(root, None)
                .get(&prof.id)
                .expect("get")
                .expect("profile")
                .proxy
                .is_none()
        );
    }
);
source_test!(
    an_unrelated_edit_over_a_divergence_is_still_refused,
    "tests/test_proxy_removal_intent.py::test_an_unrelated_edit_over_a_divergence_is_still_refused",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let uri = format!("/api/profiles/{}", prof.id);
        diverge(root, &prof.id);
        let before = raw_pair(root, &prof.id);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"name":"alpha","notes":"hello"}),
        );
        assert_eq!(status, 409, "{response}");
        assert_eq!(raw_pair(root, &prof.id), before);
    }
);
source_test!(
    a_locked_vault_refuses_a_removal_it_cannot_read,
    "tests/test_proxy_removal_intent.py::test_a_locked_vault_refuses_a_removal_it_cannot_read",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let uri = format!("/api/profiles/{}", prof.id);
        enable(root, MASTER);
        let before = raw_pair(root, &prof.id);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"name":"alpha","proxy":null}),
        );
        assert_eq!(status, 423, "{response}");
        assert_eq!(raw_pair(root, &prof.id), before);
    }
);
source_test!(
    an_unlocked_vault_removes_the_sealed_credential,
    "tests/test_proxy_removal_intent.py::test_an_unlocked_vault_removes_the_sealed_credential",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let uri = format!("/api/profiles/{}", prof.id);
        enable(root, MASTER);
        let _before = raw_pair(root, &prof.id);
        let (status, response) = http(
            root,
            Some(MASTER),
            "PUT",
            &uri,
            json!({"id":prof.id,"name":"alpha","proxy":null}),
        );
        assert_eq!(status, 200, "{response}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(engine["proxy"].is_null());
        assert!(dashboard["proxy"].is_null());
        assert!(
            repo(root, None)
                .get(&prof.id)
                .expect("get")
                .expect("profile")
                .proxy
                .is_none()
        );
    }
);
source_test!(
    a_profile_that_never_had_a_proxy_can_still_be_saved_locked,
    "tests/test_proxy_removal_intent.py::test_a_profile_that_never_had_a_proxy_can_still_be_saved_locked",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, None);
        enable(root, MASTER);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/profiles/{}", prof.id),
            json!({"id":prof.id,"name":"alpha","proxy":null}),
        );
        assert_eq!(status, 200, "{response}");
    }
);
source_test!(
    the_python_path_refuses_the_same_removal,
    "tests/test_proxy_removal_intent.py::test_the_python_path_refuses_the_same_removal",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let _uri = format!("/api/profiles/{}", prof.id);
        enable(root, MASTER);
        let before = raw_pair(root, &prof.id);
        let result = repo(root, None).edit(
            &prof.id,
            |p| {
                p.proxy = None;
                Ok(None)
            },
            1001.0,
        );
        assert!(result.is_err());
        assert_eq!(raw_pair(root, &prof.id), before);
    }
);
source_test!(
    the_python_path_allows_it_unlocked,
    "tests/test_proxy_removal_intent.py::test_the_python_path_allows_it_unlocked",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let _uri = format!("/api/profiles/{}", prof.id);
        enable(root, MASTER);
        let _before = raw_pair(root, &prof.id);
        let result = repo(root, Some(MASTER)).edit(
            &prof.id,
            |p| {
                p.proxy = None;
                Ok(None)
            },
            1001.0,
        );
        result.expect("allowed removal");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(engine["proxy"].is_null() && dashboard["proxy"].is_null());
    }
);
source_test!(
    the_python_path_removes_a_divergent_proxy_too,
    "tests/test_proxy_removal_intent.py::test_the_python_path_removes_a_divergent_proxy_too",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let _uri = format!("/api/profiles/{}", prof.id);
        diverge(root, &prof.id);
        let _before = raw_pair(root, &prof.id);
        let result = repo(root, None).edit(
            &prof.id,
            |p| {
                p.proxy = None;
                Ok(None)
            },
            1001.0,
        );
        result.expect("allowed removal");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(engine["proxy"].is_null() && dashboard["proxy"].is_null());
    }
);
source_test!(
    a_proxy_the_caller_never_saw_may_not_be_removed_case_010,
    "tests/test_proxy_removal_intent.py::test_a_proxy_the_caller_never_saw_may_not_be_removed[None-None]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":Value::Null,"password":Value::Null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut raw = raw_pair(root, &prof.id).1;
        raw.as_object_mut().expect("dashboard").remove("proxy");
        write(root, &format!("dashboard/{}.json", prof.id), raw);
        let before = snapshot(root);
        let mut loaded = listed(root, None, &prof.id);
        assert!(loaded.get("proxy").is_none_or(Value::is_null));
        assert_eq!(snapshot(root), before);
        loaded["notes"] = "hi".into();
        loaded["proxy"] = Value::Null;
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/profiles/{}", prof.id),
            loaded,
        );
        assert_eq!(status, 409, "{response}");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_proxy_the_caller_never_saw_may_not_be_removed_case_011,
    "tests/test_proxy_removal_intent.py::test_a_proxy_the_caller_never_saw_may_not_be_removed[u-None]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":json!("u"),"password":Value::Null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut raw = raw_pair(root, &prof.id).1;
        raw.as_object_mut().expect("dashboard").remove("proxy");
        write(root, &format!("dashboard/{}.json", prof.id), raw);
        let before = snapshot(root);
        let mut loaded = listed(root, None, &prof.id);
        assert!(loaded.get("proxy").is_none_or(Value::is_null));
        assert_eq!(snapshot(root), before);
        loaded["notes"] = "hi".into();
        loaded["proxy"] = Value::Null;
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/profiles/{}", prof.id),
            loaded,
        );
        assert_eq!(status, 409, "{response}");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_proxy_the_caller_never_saw_may_not_be_removed_case_012,
    "tests/test_proxy_removal_intent.py::test_a_proxy_the_caller_never_saw_may_not_be_removed[u-secret]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":json!("u"),"password":json!("secret")}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut raw = raw_pair(root, &prof.id).1;
        raw.as_object_mut().expect("dashboard").remove("proxy");
        write(root, &format!("dashboard/{}.json", prof.id), raw);
        let before = snapshot(root);
        let mut loaded = listed(root, None, &prof.id);
        assert!(loaded.get("proxy").is_none_or(Value::is_null));
        assert_eq!(snapshot(root), before);
        loaded["notes"] = "hi".into();
        loaded["proxy"] = Value::Null;
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/profiles/{}", prof.id),
            loaded,
        );
        assert_eq!(status, 409, "{response}");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    the_dashboard_toggle_off_removes_the_proxy_case_013,
    "tests/test_proxy_removal_intent.py::test_the_dashboard_toggle_off_removes_the_proxy[None]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":Value::Null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut loaded = listed(root, None, &prof.id);
        assert!(loaded["proxy"].is_object());
        loaded["proxy"] = Value::Null;
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/profiles/{}", prof.id),
            loaded,
        );
        assert_eq!(status, 200, "{response}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(engine["proxy"].is_null() && dashboard["proxy"].is_null());
    }
);
source_test!(
    the_dashboard_toggle_off_removes_the_proxy_case_014,
    "tests/test_proxy_removal_intent.py::test_the_dashboard_toggle_off_removes_the_proxy[secret]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":json!("secret")}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut loaded = listed(root, None, &prof.id);
        assert!(loaded["proxy"].is_object());
        loaded["proxy"] = Value::Null;
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/profiles/{}", prof.id),
            loaded,
        );
        assert_eq!(status, 200, "{response}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(engine["proxy"].is_null() && dashboard["proxy"].is_null());
    }
);
source_test!(
    explicit_proxy_restore_after_stale_revision_commits_both_copies_case_015,
    "tests/test_proxy_removal_intent.py::test_explicit_proxy_restore_after_stale_revision_commits_both_copies[None-host-other]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":Value::Null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let loaded = listed(root, None, &prof.id);
        let mut local = loaded["proxy"].clone();
        local["host"] = json!("other");
        let uri = format!("/api/profiles/{}", prof.id);
        let (status, removed) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":null}),
        );
        assert_eq!(status, 200, "{removed}");
        let before = raw_pair(root, &prof.id);
        let mut redacted = local.clone();
        redacted.as_object_mut().expect("proxy").remove("pass");
        let (status, stale) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":redacted}),
        );
        assert_eq!(status, 409, "{stale}");
        assert_eq!(stale["kind"], "StaleProfile");
        assert_eq!(raw_pair(root, &prof.id), before);
        let (status, result) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":removed["_rev"],"proxy":local}),
        );
        assert_eq!(status, 200, "{result}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert_eq!(dashboard["proxy"]["host"], "other");
        assert_eq!(dashboard["proxy"]["pass"], "");
        assert_eq!(engine["proxy"]["password"], Value::Null);
        assert_eq!(
            secret(
                &repo(root, None)
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            Value::Null
        );
    }
);
source_test!(
    explicit_proxy_restore_after_stale_revision_commits_both_copies_case_016,
    "tests/test_proxy_removal_intent.py::test_explicit_proxy_restore_after_stale_revision_commits_both_copies[None-port-8082]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":Value::Null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let loaded = listed(root, None, &prof.id);
        let mut local = loaded["proxy"].clone();
        local["port"] = json!("8082");
        let uri = format!("/api/profiles/{}", prof.id);
        let (status, removed) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":null}),
        );
        assert_eq!(status, 200, "{removed}");
        let before = raw_pair(root, &prof.id);
        let mut redacted = local.clone();
        redacted.as_object_mut().expect("proxy").remove("pass");
        let (status, stale) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":redacted}),
        );
        assert_eq!(status, 409, "{stale}");
        assert_eq!(stale["kind"], "StaleProfile");
        assert_eq!(raw_pair(root, &prof.id), before);
        let (status, result) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":removed["_rev"],"proxy":local}),
        );
        assert_eq!(status, 200, "{result}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert_eq!(dashboard["proxy"]["port"], "8082");
        assert_eq!(dashboard["proxy"]["pass"], "");
        assert_eq!(engine["proxy"]["password"], Value::Null);
        assert_eq!(
            secret(
                &repo(root, None)
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            Value::Null
        );
    }
);
source_test!(
    explicit_proxy_restore_after_stale_revision_commits_both_copies_case_017,
    "tests/test_proxy_removal_intent.py::test_explicit_proxy_restore_after_stale_revision_commits_both_copies[None-user-other-user]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":Value::Null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let loaded = listed(root, None, &prof.id);
        let mut local = loaded["proxy"].clone();
        local["user"] = json!("other-user");
        let uri = format!("/api/profiles/{}", prof.id);
        let (status, removed) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":null}),
        );
        assert_eq!(status, 200, "{removed}");
        let before = raw_pair(root, &prof.id);
        let mut redacted = local.clone();
        redacted.as_object_mut().expect("proxy").remove("pass");
        let (status, stale) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":redacted}),
        );
        assert_eq!(status, 409, "{stale}");
        assert_eq!(stale["kind"], "StaleProfile");
        assert_eq!(raw_pair(root, &prof.id), before);
        let (status, result) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":removed["_rev"],"proxy":local}),
        );
        assert_eq!(status, 200, "{result}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert_eq!(dashboard["proxy"]["user"], "other-user");
        assert_eq!(dashboard["proxy"]["pass"], "");
        assert_eq!(engine["proxy"]["password"], Value::Null);
        assert_eq!(
            secret(
                &repo(root, None)
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            Value::Null
        );
    }
);
source_test!(
    explicit_proxy_restore_after_stale_revision_commits_both_copies_case_018,
    "tests/test_proxy_removal_intent.py::test_explicit_proxy_restore_after_stale_revision_commits_both_copies[secret-host-other]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":json!("secret")}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let loaded = listed(root, None, &prof.id);
        let mut local = loaded["proxy"].clone();
        local["host"] = json!("other");
        let uri = format!("/api/profiles/{}", prof.id);
        let (status, removed) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":null}),
        );
        assert_eq!(status, 200, "{removed}");
        let before = raw_pair(root, &prof.id);
        let mut redacted = local.clone();
        redacted.as_object_mut().expect("proxy").remove("pass");
        let (status, stale) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":redacted}),
        );
        assert_eq!(status, 409, "{stale}");
        assert_eq!(stale["kind"], "StaleProfile");
        assert_eq!(raw_pair(root, &prof.id), before);
        let (status, result) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":removed["_rev"],"proxy":local}),
        );
        assert_eq!(status, 200, "{result}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert_eq!(dashboard["proxy"]["host"], "other");
        assert_eq!(dashboard["proxy"]["pass"], "secret");
        assert_eq!(engine["proxy"]["password"], json!("secret"));
        assert_eq!(
            secret(
                &repo(root, None)
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            json!("secret")
        );
    }
);
source_test!(
    explicit_proxy_restore_after_stale_revision_commits_both_copies_case_019,
    "tests/test_proxy_removal_intent.py::test_explicit_proxy_restore_after_stale_revision_commits_both_copies[secret-port-8082]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":json!("secret")}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let loaded = listed(root, None, &prof.id);
        let mut local = loaded["proxy"].clone();
        local["port"] = json!("8082");
        let uri = format!("/api/profiles/{}", prof.id);
        let (status, removed) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":null}),
        );
        assert_eq!(status, 200, "{removed}");
        let before = raw_pair(root, &prof.id);
        let mut redacted = local.clone();
        redacted.as_object_mut().expect("proxy").remove("pass");
        let (status, stale) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":redacted}),
        );
        assert_eq!(status, 409, "{stale}");
        assert_eq!(stale["kind"], "StaleProfile");
        assert_eq!(raw_pair(root, &prof.id), before);
        let (status, result) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":removed["_rev"],"proxy":local}),
        );
        assert_eq!(status, 200, "{result}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert_eq!(dashboard["proxy"]["port"], "8082");
        assert_eq!(dashboard["proxy"]["pass"], "secret");
        assert_eq!(engine["proxy"]["password"], json!("secret"));
        assert_eq!(
            secret(
                &repo(root, None)
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            json!("secret")
        );
    }
);
source_test!(
    explicit_proxy_restore_after_stale_revision_commits_both_copies_case_020,
    "tests/test_proxy_removal_intent.py::test_explicit_proxy_restore_after_stale_revision_commits_both_copies[secret-user-other-user]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":json!("secret")}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let loaded = listed(root, None, &prof.id);
        let mut local = loaded["proxy"].clone();
        local["user"] = json!("other-user");
        let uri = format!("/api/profiles/{}", prof.id);
        let (status, removed) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":null}),
        );
        assert_eq!(status, 200, "{removed}");
        let before = raw_pair(root, &prof.id);
        let mut redacted = local.clone();
        redacted.as_object_mut().expect("proxy").remove("pass");
        let (status, stale) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":loaded["_rev"],"proxy":redacted}),
        );
        assert_eq!(status, 409, "{stale}");
        assert_eq!(stale["kind"], "StaleProfile");
        assert_eq!(raw_pair(root, &prof.id), before);
        let (status, result) = http(
            root,
            None,
            "PUT",
            &uri,
            json!({"id":prof.id,"_rev":removed["_rev"],"proxy":local}),
        );
        assert_eq!(status, 200, "{result}");
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert_eq!(dashboard["proxy"]["user"], "other-user");
        assert_eq!(dashboard["proxy"]["pass"], "secret");
        assert_eq!(engine["proxy"]["password"], json!("secret"));
        assert_eq!(
            secret(
                &repo(root, None)
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            json!("secret")
        );
    }
);
source_test!(
    read_distinguishes_known_passwordless_proxy_from_withheld_case_021,
    "tests/test_proxy_removal_intent.py::test_read_distinguishes_known_passwordless_proxy_from_withheld[missing]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut raw = raw_pair(root, &prof.id).1;
        raw["proxy"].as_object_mut().expect("proxy").remove("pass");
        write(root, &format!("dashboard/{}.json", prof.id), raw);
        let before = raw_pair(root, &prof.id);
        let loaded = listed(root, None, &prof.id);
        assert_eq!(loaded["proxy"]["pass"], "");
        assert_eq!(raw_pair(root, &prof.id), before);
    }
);
source_test!(
    read_distinguishes_known_passwordless_proxy_from_withheld_case_022,
    "tests/test_proxy_removal_intent.py::test_read_distinguishes_known_passwordless_proxy_from_withheld[null]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut raw = raw_pair(root, &prof.id).1;
        raw["proxy"].as_object_mut().expect("proxy").remove("pass");
        raw["proxy"]["pass"] = Value::Null;
        write(root, &format!("dashboard/{}.json", prof.id), raw);
        let before = raw_pair(root, &prof.id);
        let loaded = listed(root, None, &prof.id);
        assert_eq!(loaded["proxy"]["pass"], "");
        assert_eq!(raw_pair(root, &prof.id), before);
    }
);
source_test!(
    read_distinguishes_known_passwordless_proxy_from_withheld_case_023,
    "tests/test_proxy_removal_intent.py::test_read_distinguishes_known_passwordless_proxy_from_withheld[empty]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let mut prof = model("alpha", 1, None);
        prof.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":null}),
                false,
            )
            .expect("proxy"),
        );
        let prof = repo(root, None).save(prof, 1000.0).expect("save");
        let mut raw = raw_pair(root, &prof.id).1;
        raw["proxy"].as_object_mut().expect("proxy").remove("pass");
        raw["proxy"]["pass"] = json!("");
        write(root, &format!("dashboard/{}.json", prof.id), raw);
        let before = raw_pair(root, &prof.id);
        let loaded = listed(root, None, &prof.id);
        assert_eq!(loaded["proxy"]["pass"], "");
        assert_eq!(raw_pair(root, &prof.id), before);
    }
);
source_test!(
    a_withheld_password_stays_null_on_read,
    "tests/test_proxy_removal_intent.py::test_a_withheld_password_stays_null_on_read",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret"));
        let _uri = format!("/api/profiles/{}", prof.id);
        enable(root, MASTER);
        let before = raw_pair(root, &prof.id);
        let loaded = listed(root, None, &prof.id);
        assert_eq!(loaded["proxy"]["pass"], Value::Null);
        assert_eq!(raw_pair(root, &prof.id), before);
    }
);
