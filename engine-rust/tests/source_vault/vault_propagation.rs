use super::*;
source_test!(
    serve_gets_an_unlocked_store_when_the_cli_was_unlocked,
    "tests/test_vault_propagation.py::test_serve_gets_an_unlocked_store_when_the_cli_was_unlocked",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let (status, rows) = http(root, Some(MASTER), "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200, "{rows}");
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .get(&prof.id)
                    .expect("server store get")
                    .expect("profile")
            ),
            "PROXY-PASSWORD"
        );
        assert_eq!(rows[0]["proxy"]["pass"], "PROXY-PASSWORD");
    }
);
source_test!(
    without_a_password_the_server_store_stays_locked,
    "tests/test_vault_propagation.py::test_without_a_password_the_server_store_stays_locked",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let (status, rows) = http(root, None, "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200);
        assert_eq!(rows[0]["proxy"]["pass"], Value::Null);
        assert!(!repo(root, None).unlocked().expect("locked server store"));
    }
);
source_test!(
    a_child_inherits_the_unlock_by_environment,
    "tests/test_vault_propagation.py::test_a_child_inherits_the_unlock_by_environment",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let command = numera_core::mcp_command::child_command(
            Path::new("native-numera"),
            Some(root),
            Some(MASTER),
            vec!["--json".into(), "list".into()],
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "NUMERA_PASSWORD" && value.is_some_and(|v| v == MASTER))
        );
        assert!(std::env::vars_os().count() > 1);
    }
);
source_test!(
    the_password_never_appears_in_a_child_argv,
    "tests/test_vault_propagation.py::test_the_password_never_appears_in_a_child_argv",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let command = numera_core::mcp_command::child_command(
            Path::new("native-numera"),
            Some(root),
            Some(MASTER),
            vec!["--json".into(), "launch".into(), prof.id.clone()],
        );
        assert!(
            !command
                .get_args()
                .any(|arg| arg.to_string_lossy().contains(MASTER))
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "NUMERA_PASSWORD" && value.is_some_and(|v| v == MASTER))
        );
        let unlocked = repo(root, Some(MASTER))
            .get(&prof.id)
            .expect("resolved browser profile")
            .expect("profile");
        assert_eq!(secret(&unlocked), "PROXY-PASSWORD");
        let relay = numera_core::network::relay::Relay::start(
            unlocked.proxy.as_ref().expect("resolved proxy"),
        )
        .expect("native credential relay");
        let relay_url = relay.url();
        let options = numera_core::browser::options::build(
            &unlocked,
            "playwright",
            "owned-user-data",
            true,
            &[],
            &[],
            Some(&relay_url),
        )
        .expect("native browser launch options");
        assert_eq!(options["proxy"]["server"], relay_url);
        assert!(!options.to_string().contains("PROXY-PASSWORD"));
        assert!(!options.to_string().contains(MASTER));
        let mut adapter = std::process::Command::new("native-browser-runtime");
        adapter.args(["--sdk-adapter", "--backend", "playwright"]);
        let private_keys = [
            "NUMERA_PASSWORD",
            "NUMERA_API_TOKEN",
            "NUMERA_UPDATE_TOKEN",
            "NUMERA_UPDATE_TICKET",
            "NUMERA_TREE",
            "NUMERA_READY_FILE",
        ];
        for key in private_keys {
            adapter.env(key, "private-credential");
        }
        adapter.env("SOURCE_PUBLIC_SETTING", "kept");
        numera_core::browser::adapter::restrict_environment(&mut adapter);
        for key in private_keys {
            assert!(
                adapter
                    .get_envs()
                    .any(|(name, value)| name == key && value.is_none()),
                "{key}"
            );
        }
        assert!(
            adapter
                .get_envs()
                .any(|(name, value)| name == "SOURCE_PUBLIC_SETTING"
                    && value.is_some_and(|value| value == "kept"))
        );
        assert!(
            !adapter
                .get_args()
                .any(|arg| arg.to_string_lossy().contains("private-credential"))
        );
    }
);
source_test!(
    a_backup_refuses_rather_than_dropping_the_password,
    "tests/test_vault_propagation.py::test_a_backup_refuses_rather_than_dropping_the_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let locked = repo(root, None);
        assert!(locked.secret_withheld(&prof.id).expect("withheld"));
        let path = root.join("out.zip");
        assert!(locked.backup(&prof.id, &path, "source-vault-host").is_err());
        assert!(!path.exists());
    }
);
source_test!(
    an_export_refuses_rather_than_dropping_the_password,
    "tests/test_vault_propagation.py::test_an_export_refuses_rather_than_dropping_the_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let locked = repo(root, None);
        assert!(locked.secret_withheld(&prof.id).expect("withheld"));
        let path = root.join("out.json");
        assert!(locked.export(&prof.id, &path).is_err());
        assert!(!path.exists());
    }
);
source_test!(
    unlocked_export_and_backup_still_carry_the_password,
    "tests/test_vault_propagation.py::test_unlocked_export_and_backup_still_carry_the_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let unlocked = repo(root, Some(MASTER));
        let out = root.join("ok.json");
        unlocked.export(&prof.id, &out).expect("export");
        assert!(
            fs::read_to_string(out)
                .expect("export text")
                .contains("PROXY-PASSWORD")
        );
        assert!(
            unlocked
                .backup(&prof.id, &root.join("ok.zip"), "source-vault-host")
                .expect("backup")
                .expect("archive")
                .exists()
        );
    }
);
source_test!(
    a_profile_without_a_secret_exports_while_locked,
    "tests/test_vault_propagation.py::test_a_profile_without_a_secret_exports_while_locked",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let plain = repo(root, Some(MASTER))
            .save(model("no-secret", 9, None), 1001.0)
            .expect("passwordless profile");
        let locked = repo(root, None);
        assert!(
            !locked
                .secret_withheld(&plain.id)
                .expect("no withheld credential")
        );
        assert!(
            locked
                .export(&plain.id, &root.join("plain.json"))
                .expect("locked passwordless export")
                .is_some()
        );
    }
);
source_test!(
    a_locked_proxy_test_is_refused_not_recorded_as_failed,
    "tests/test_vault_propagation.py::test_a_locked_proxy_test_is_refused_not_recorded_as_failed",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let id = pool(root, None).list().expect("pool")[0]["id"]
            .as_str()
            .expect("ID")
            .to_owned();
        let (status, response) = http(
            root,
            None,
            "POST",
            &format!("/api/proxies/{id}/test"),
            json!({}),
        );
        assert_eq!(status, 409, "{response}");
        assert_ne!(
            pool(root, None).get(&id).expect("get").expect("proxy")["status"],
            "failed"
        );
    }
);
source_test!(
    a_locked_assign_is_refused,
    "tests/test_vault_propagation.py::test_a_locked_assign_is_refused",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        saved(root, "target", 2, None);
        let (status, response) = http(root, None, "POST", "/api/proxies/assign", json!({}));
        assert_eq!(status, 409, "{response}");
    }
);
source_test!(
    unlocked_assign_still_works,
    "tests/test_vault_propagation.py::test_unlocked_assign_still_works",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        saved(root, "target", 2, None);
        let (status, response) = http(root, Some(MASTER), "POST", "/api/proxies/assign", json!({}));
        assert_eq!(status, 200, "{response}");
    }
);
source_test!(
    is_withheld_only_fires_on_a_real_locked_secret,
    "tests/test_vault_propagation.py::test_is_withheld_only_fires_on_a_real_locked_secret",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let cipher = raw_pair(root, &prof.id).0["proxy"]["password"]
            .as_str()
            .expect("cipher")
            .to_owned();
        assert!(vault::is_envelope(&cipher));
        assert!(
            repo(root, None)
                .secret_withheld(&prof.id)
                .expect("locked withheld")
        );
        assert!(
            !repo(root, Some(MASTER))
                .secret_withheld(&prof.id)
                .expect("unlocked readable")
        );
        for (i, value) in [Value::Null, json!(""), json!("plaintext")]
            .into_iter()
            .enumerate()
        {
            let mut record = raw_pair(root, &prof.id).0;
            record["proxy"]["password"] = value;
            let id = format!("plain-{i}");
            record["id"] = id.clone().into();
            write(root, &format!("profiles/{id}.json"), record);
            assert!(
                !repo(root, None)
                    .secret_withheld(&id)
                    .expect("non-envelope not withheld")
            );
        }
    }
);
