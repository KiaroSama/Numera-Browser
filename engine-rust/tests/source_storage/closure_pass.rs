use super::*;
source_test!(
    a_detached_child_argv_carries_no_password_case_000,
    "tests/test_closure_pass.py::test_a_detached_child_argv_carries_no_password[argv0]",
    {
        let temp = temp();
        let root = temp.path();
        let out = numera_core::jobs::child_argv(
            &["--password", "SECRET", "launch", "p", "--detach"].map(str::to_owned),
            root,
        );
        assert!(!out.iter().any(|a| a.contains("SECRET")));
        assert!(!out.iter().any(|a| a == "--password" || a == "--detach"));
        assert!(out.iter().any(|a| a == "launch") && out.iter().any(|a| a == "p"));
    }
);
source_test!(
    a_detached_child_argv_carries_no_password_case_001,
    "tests/test_closure_pass.py::test_a_detached_child_argv_carries_no_password[argv1]",
    {
        let temp = temp();
        let root = temp.path();
        let out = numera_core::jobs::child_argv(
            &["--password=SECRET", "launch", "p", "--detach"].map(str::to_owned),
            root,
        );
        assert!(!out.iter().any(|a| a.contains("SECRET")));
        assert!(!out.iter().any(|a| a == "--password" || a == "--detach"));
        assert!(out.iter().any(|a| a == "launch") && out.iter().any(|a| a == "p"));
    }
);
source_test!(
    a_detached_child_argv_carries_no_password_case_002,
    "tests/test_closure_pass.py::test_a_detached_child_argv_carries_no_password[argv2]",
    {
        let temp = temp();
        let root = temp.path();
        let out = numera_core::jobs::child_argv(
            &["launch", "p", "--password", "SECRET", "--detach"].map(str::to_owned),
            root,
        );
        assert!(!out.iter().any(|a| a.contains("SECRET")));
        assert!(!out.iter().any(|a| a == "--password" || a == "--detach"));
        assert!(out.iter().any(|a| a == "launch") && out.iter().any(|a| a == "p"));
    }
);
source_test!(
    a_name_only_put_changes_only_the_name,
    "tests/test_closure_pass.py::test_a_name_only_put_changes_only_the_name",
    {
        let temp = temp();
        let root = temp.path();
        let (status, p) = http(
            root,
            "POST",
            "/api/profiles",
            json!({"name":"orig","engine":"camoufox","tags":["keep"],"notes":"important","folder":"Work","startupUrls":"https://example.com","proxy":{"type":"SOCKS5","host":"9.9.9.9","port":"1080","user":"u"}}),
        );
        assert_eq!(status, 200, "{p}");
        let id = p["id"].as_str().expect("id");
        let before = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(
            http(
                root,
                "PUT",
                &format!("/api/profiles/{id}"),
                json!({"name":"renamed"})
            )
            .0,
            200
        );
        let after = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(after["name"], "renamed");
        for (key, value) in before.as_object().expect("before") {
            if !["name", "_rev"].contains(&key.as_str()) {
                assert_eq!(&after[key], value, "unmentioned field {key}");
            }
        }
    }
);
source_test!(
    a_partial_put_preserves_server_owned_bookkeeping,
    "tests/test_closure_pass.py::test_a_partial_put_preserves_server_owned_bookkeeping",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "orig");
        let id = p["id"].as_str().expect("id");
        let mut data = raw(root, &format!("dashboard/{id}.json"));
        data["_trust"] = json!({"grade":"A"});
        data["_last_active"] = 1234.0.into();
        write(root, &format!("dashboard/{id}.json"), data.clone());
        assert_eq!(
            http(
                root,
                "PUT",
                &format!("/api/profiles/{id}"),
                json!({"name":"renamed"})
            )
            .0,
            200
        );
        let after = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(after["_trust"], json!({"grade":"A"}));
        assert_eq!(after["_last_active"], 1234.0);
        assert_eq!(after["_created"], data["_created"]);
    }
);
source_test!(
    a_put_can_still_change_what_it_names,
    "tests/test_closure_pass.py::test_a_put_can_still_change_what_it_names",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "a");
        assert_eq!(
            http(
                root,
                "PUT",
                &format!("/api/profiles/{}", p["id"].as_str().expect("id")),
                json!({"tags":["y","z"]})
            )
            .0,
            200
        );
        let rows = http(root, "GET", "/api/profiles", Value::Null).1;
        assert_eq!(rows[0]["tags"], json!(["y", "z"]));
    }
);
source_test!(
    a_held_session_leaves_the_profile_entirely_intact,
    "tests/test_closure_pass.py::test_a_held_session_leaves_the_profile_entirely_intact",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"held","tags":[]}));
        let id = p["id"].as_str().expect("id");
        let ud = sessions(root, id);
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-session-detach" {
                    return Err(io::Error::other("in use"));
                }
                Ok(Action::Continue)
            },
            || dash(root).delete(id, "source-storage-host", 1001.0),
        );
        assert!(result.is_err());
        assert!(root.join(format!("dashboard/{id}.json")).is_file());
        assert!(root.join(format!("profiles/{id}.json")).is_file());
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("live session"),
            b"the live session"
        );
    }
);
source_test!(
    a_metadata_failure_puts_the_session_back,
    "tests/test_closure_pass.py::test_a_metadata_failure_puts_the_session_back",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"held","tags":[]}));
        let id = p["id"].as_str().expect("id");
        let ud = sessions(root, id);
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-delete-metadata"
                    && event.path.parent().is_some_and(|p| p.ends_with("profiles"))
                {
                    return Err(io::Error::other("in use"));
                }
                Ok(Action::Continue)
            },
            || dash(root).delete(id, "source-storage-host", 1001.0),
        );
        assert!(result.is_err());
        assert!(root.join(format!("dashboard/{id}.json")).is_file());
        assert!(root.join(format!("profiles/{id}.json")).is_file());
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("live session"),
            b"the live session"
        );
    }
);
source_test!(
    a_tombstone_that_cannot_be_reclaimed_still_commits,
    "tests/test_closure_pass.py::test_a_tombstone_that_cannot_be_reclaimed_still_commits",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"held","tags":[]}));
        let id = p["id"].as_str().expect("id");
        let ud = sessions(root, id);
        let warning = process::output(process::start(
            "closure_pass::a_tombstone_that_cannot_be_reclaimed_still_commits",
            root,
            "cleanup",
            "delete-skip",
            id,
        ));
        let warning = String::from_utf8_lossy(&warning.stderr);
        assert!(
            warning.contains("still on disk")
                && warning.contains(".deleting-")
                && warning.contains(id)
        );
        assert!(!root.join(format!("dashboard/{id}.json")).exists());
        assert!(!directories(&root.join("user-data"), ".deleting-").is_empty());
        assert!(!ud.exists());
    }
);
source_test!(
    a_locked_pool_export_is_refused,
    "tests/test_closure_pass.py::test_a_locked_pool_export_is_refused",
    {
        let temp = temp();
        let root = temp.path();
        vaulted(root);
        let (status, body) = http(root, "POST", "/api/proxies/export", json!({}));
        assert_eq!(status, 409, "{body}");
        assert!(!body.to_string().contains("PROXY-PASSWORD"));
    }
);
source_test!(
    a_locked_bulk_rotate_is_refused,
    "tests/test_closure_pass.py::test_a_locked_bulk_rotate_is_refused",
    {
        let temp = temp();
        let root = temp.path();
        vaulted(root);
        let rows = http(root, "GET", "/api/proxies", Value::Null).1;
        assert_eq!(
            http(
                root,
                "POST",
                "/api/proxies/rotate",
                json!({"ids":[rows[0]["id"]]})
            )
            .0,
            409
        );
    }
);
source_test!(
    an_unlocked_export_still_carries_the_credential,
    "tests/test_closure_pass.py::test_an_unlocked_export_still_carries_the_credential",
    {
        let temp = temp();
        let root = temp.path();
        vaulted(root);
        let (status, body) = http_password(
            root,
            "POST",
            "/api/proxies/export",
            json!({}),
            Some("master".into()),
        );
        assert_eq!(status, 200, "{body}");
        assert!(
            body["text"]
                .as_str()
                .expect("text")
                .contains("PROXY-PASSWORD")
        );
    }
);
source_test!(
    a_withheld_secret_does_not_dedupe_as_passwordless,
    "tests/test_closure_pass.py::test_a_withheld_secret_does_not_dedupe_as_passwordless",
    {
        let temp = temp();
        let root = temp.path();
        vaulted(root);
        assert!(
            !pool(root)
                .add_many(vec![
                    json!({"type":"HTTP","host":"5.6.7.8","port":"8080","user":"u"})
                ])
                .expect("passwordless distinct")
                .is_empty()
        );
    }
);
source_test!(
    the_importer_keeps_the_protocol_the_export_declared_case_014,
    "tests/test_closure_pass.py::test_the_importer_keeps_the_protocol_the_export_declared[socks4-SOCKS4]",
    {
        let(p,_)=numera_core::importers::model(&json!({"name":"x","proxy":{"host":"1.2.3.4","port":1080,"type":"socks4","mode":"socks4"}}),1,1000.0).expect("import");
        assert!(p.proxy.expect("proxy").server.starts_with("socks4://"));
    }
);
source_test!(
    the_importer_keeps_the_protocol_the_export_declared_case_015,
    "tests/test_closure_pass.py::test_the_importer_keeps_the_protocol_the_export_declared[socks5-SOCKS5]",
    {
        let(p,_)=numera_core::importers::model(&json!({"name":"x","proxy":{"host":"1.2.3.4","port":1080,"type":"socks5","mode":"socks5"}}),1,1000.0).expect("import");
        assert!(p.proxy.expect("proxy").server.starts_with("socks5://"));
    }
);
source_test!(
    the_importer_keeps_the_protocol_the_export_declared_case_016,
    "tests/test_closure_pass.py::test_the_importer_keeps_the_protocol_the_export_declared[https-HTTPS]",
    {
        let(p,_)=numera_core::importers::model(&json!({"name":"x","proxy":{"host":"1.2.3.4","port":1080,"type":"https","mode":"https"}}),1,1000.0).expect("import");
        assert!(p.proxy.expect("proxy").server.starts_with("https://"));
    }
);
source_test!(
    the_importer_keeps_the_protocol_the_export_declared_case_017,
    "tests/test_closure_pass.py::test_the_importer_keeps_the_protocol_the_export_declared[http-HTTP]",
    {
        let (p, _) = numera_core::importers::model(
            &json!({"name":"x","proxy":{"host":"1.2.3.4","port":1080,"type":"http","mode":"http"}}),
            1,
            1000.0,
        )
        .expect("import");
        assert!(p.proxy.expect("proxy").server.starts_with("http://"));
    }
);
source_test!(
    a_bulk_update_that_fails_part_way_changes_nothing,
    "tests/test_closure_pass.py::test_a_bulk_update_that_fails_part_way_changes_nothing",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"p0"},{"name":"p1"},{"name":"p2"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let before = snapshot(root);
        let (status, body) = testing::with_observer(nth_failure(3), || {
            http(
                root,
                "POST",
                "/api/profiles/bulk",
                json!({"ids":made["ids"],"patch":{"notes":"edited"}}),
            )
        });
        assert!(status >= 500, "{body}");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    an_import_that_fails_part_way_writes_nothing,
    "tests/test_closure_pass.py::test_an_import_that_fails_part_way_writes_nothing",
    {
        let temp = temp();
        let root = temp.path();
        let text = serde_json::to_string(
            &json!([{"name":"imported-0"},{"name":"imported-1"},{"name":"imported-2"}]),
        )
        .expect("exports");
        let (status, body) = testing::with_observer(nth_failure(3), || {
            http(root, "POST", "/api/profiles/import", json!({"text":text}))
        });
        assert!(status >= 500, "{body}");
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
    a_successful_bulk_update_still_applies,
    "tests/test_closure_pass.py::test_a_successful_bulk_update_still_applies",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"a"},{"name":"b"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":made["ids"],"patch":{"notes":"edited"}}),
        );
        assert_eq!(status, 200, "{body}");
        assert!(
            http(root, "GET", "/api/profiles", Value::Null)
                .1
                .as_array()
                .expect("profiles")
                .iter()
                .all(|p| p["notes"] == "edited")
        );
    }
);
source_test!(
    a_folder_rename_and_a_profile_save_both_survive,
    "tests/test_closure_pass.py::test_a_folder_rename_and_a_profile_save_both_survive",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"p","tags":[],"folder":"Work"}));
        let id = p["id"].as_str().expect("id").to_owned();
        numera_core::storage::folders::Folders::new(root.to_owned())
            .create("Work")
            .expect("Work");
        let a = root.to_owned();
        let rename = thread::spawn(move || {
            let folders = numera_core::storage::folders::Folders::new(a);
            for _ in 0..10 {
                folders.rename("Work", "Client").expect("rename");
                folders.rename("Client", "Work").expect("rename back");
            }
        });
        let b = root.to_owned();
        let target = id.clone();
        let edit = thread::spawn(move || {
            for i in 0..10 {
                let mut current = dash(&b).get(&target).expect("get").expect("profile");
                current["notes"] = format!("edit-{i}").into();
                dash(&b)
                    .save(current, Default::default(), None, None, 1001.0)
                    .expect("save");
            }
        });
        rename.join().expect("rename worker");
        edit.join().expect("edit worker");
        let final_row = dash(root).get(&id).expect("get").expect("profile");
        assert!(
            final_row["notes"]
                .as_str()
                .expect("notes")
                .starts_with("edit-")
        );
        assert!(["Work", "Client"].contains(&final_row["folder"].as_str().expect("folder")));
    }
);
source_test!(
    only_globally_routable_destinations_are_allowed_case_022,
    "tests/test_closure_pass.py::test_only_globally_routable_destinations_are_allowed[http://100.64.0.1/rotate]",
    {
        assert!(public_destination("http://100.64.0.1/rotate").is_err());
    }
);
source_test!(
    only_globally_routable_destinations_are_allowed_case_023,
    "tests/test_closure_pass.py::test_only_globally_routable_destinations_are_allowed[http://100.127.255.254/rotate]",
    {
        assert!(public_destination("http://100.127.255.254/rotate").is_err());
    }
);
source_test!(
    only_globally_routable_destinations_are_allowed_case_024,
    "tests/test_closure_pass.py::test_only_globally_routable_destinations_are_allowed[http://127.0.0.1/rotate]",
    {
        assert!(public_destination("http://127.0.0.1/rotate").is_err());
    }
);
source_test!(
    only_globally_routable_destinations_are_allowed_case_025,
    "tests/test_closure_pass.py::test_only_globally_routable_destinations_are_allowed[http://169.254.169.254/latest/]",
    {
        assert!(public_destination("http://169.254.169.254/latest/").is_err());
    }
);
source_test!(
    only_globally_routable_destinations_are_allowed_case_026,
    "tests/test_closure_pass.py::test_only_globally_routable_destinations_are_allowed[http://10.0.0.5/rotate]",
    {
        assert!(public_destination("http://10.0.0.5/rotate").is_err());
    }
);
source_test!(
    only_globally_routable_destinations_are_allowed_case_027,
    "tests/test_closure_pass.py::test_only_globally_routable_destinations_are_allowed[http://192.0.2.1/rotate]",
    {
        assert!(public_destination("http://192.0.2.1/rotate").is_err());
    }
);
source_test!(
    only_globally_routable_destinations_are_allowed_case_028,
    "tests/test_closure_pass.py::test_only_globally_routable_destinations_are_allowed[http://[fc00::1]/rotate]",
    {
        assert!(public_destination("http://[fc00::1]/rotate").is_err());
    }
);
source_test!(
    a_public_destination_is_allowed_and_pinned,
    "tests/test_closure_pass.py::test_a_public_destination_is_allowed_and_pinned",
    {
        let (_, pinned) = public_destination("https://example.com/rotate").expect("public");
        assert_eq!(
            pinned,
            "93.184.216.34"
                .parse::<std::net::IpAddr>()
                .expect("fixture address")
        );
        assert!(numera_core::network::public_ip::is_global(pinned));
    }
);
source_test!(
    the_connection_is_pinned_to_the_address_that_was_validated,
    "tests/test_closure_pass.py::test_the_connection_is_pinned_to_the_address_that_was_validated",
    {
        let address = "93.184.216.34".parse().expect("first DNS result");
        let (url, pinned) = numera_core::network::proxy_rotate::testing::public_destination(
            "http://rebind.test/rotate",
            &[address],
        )
        .expect("pin");
        assert_eq!(url.host_str(), Some("rebind.test"));
        assert_eq!(pinned, address);
        assert!(!pinned.is_loopback());
        let later = "127.0.0.1".parse().expect("second DNS result");
        assert!(
            numera_core::network::proxy_rotate::testing::public_destination(url.as_str(), &[later])
                .is_err()
        );
        assert_eq!(
            pinned, address,
            "connection pin remains first validated address"
        );
    }
);
source_test!(
    a_non_http_scheme_is_still_refused,
    "tests/test_closure_pass.py::test_a_non_http_scheme_is_still_refused",
    {
        for url in ["ftp://example.com/x", "file:///etc/passwd"] {
            assert!(public_destination(url).is_err());
        }
    }
);
source_test!(
    crypto_still_round_trips_after_all_of_this,
    "tests/test_closure_pass.py::test_crypto_still_round_trips_after_all_of_this",
    {
        let salt = new_salt();
        assert_eq!(
            numera_core::vault::decrypt(
                "pw",
                &salt,
                &numera_core::vault::encrypt("pw", &salt, "x").expect("encrypt")
            )
            .expect("decrypt"),
            "x"
        );
    }
);
source_test!(
    the_job_record_never_holds_the_password,
    "tests/test_closure_pass.py::test_the_job_record_never_holds_the_password",
    {
        let temp = temp();
        let root = temp.path();
        let mut p = profile("secret", 1);
        p.proxy=Some(Proxy::from_record(&json!({"server":"http://1.2.3.4:8080","username":"u","password":"PROXY-CREDENTIAL"}),false).expect("proxy"));
        let p = store(root).save(p, 1000.0).expect("save");
        store(root).enable_vault("MASTER-SECRET").expect("vault");
        let out = root.join("exported.json");
        let argv = vec![
            "export".into(),
            p.id.clone(),
            out.to_string_lossy().into_owned(),
            "--json".into(),
        ];
        let jobs = numera_core::jobs::JobStore::new(root.to_owned());
        let record = jobs
            .spawn_detached(numera_core::jobs::SpawnOptions {
                executable: Path::new(env!("CARGO_BIN_EXE_numera-engine")),
                argv: &argv,
                command: "export",
                profile: Some(&p.id),
                password: Some("MASTER-SECRET"),
            })
            .expect("owned detached unlocked child");
        let id = record["id"].as_str().expect("id");
        wait_job(&jobs, id);
        let on_disk = fs::read_to_string(root.join(format!("jobs/{id}.json"))).expect("job record");
        assert!(!on_disk.contains("MASTER-SECRET"));
        assert!(!record.to_string().contains("MASTER-SECRET"));
        assert!(!argv.iter().any(|arg| arg.contains("MASTER-SECRET")));
        assert_eq!(
            raw(root, "exported.json")["proxy"]["password"],
            "PROXY-CREDENTIAL",
            "actual child received master password via environment"
        );
    }
);
