use super::*;
source_test!(
    no_secret_is_readable_anywhere_after_enabling,
    "tests/test_vault_covers_the_root.py::test_no_secret_is_readable_anywhere_after_enabling",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        enable(root, MASTER);
        for secret in ["ENGINE-SECRET", "DASHBOARD-SECRET", "POOL-SECRET"] {
            assert!(leaked(root, secret).is_empty(), "{secret}");
        }
    }
);
source_test!(
    the_root_reports_no_plaintext_secrets_left,
    "tests/test_vault_covers_the_root.py::test_the_root_reports_no_plaintext_secrets_left",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        assert!(
            completion::problems(root, &descriptors()[0], Some(NEW))
                .expect("before scan")
                .iter()
                .any(|p| p.code == "plaintext_left")
        );
        enable(root, MASTER);
        assert_no_plain(root, MASTER);
    }
);
source_test!(
    every_secret_bearing_file_is_in_the_site_table,
    "tests/test_vault_covers_the_root.py::test_every_secret_bearing_file_is_in_the_site_table",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        let sites = completion::sites(root).expect("site table");
        assert!(sites.iter().any(|s| s.path == root.join("proxies.json")));
        assert!(
            sites
                .iter()
                .any(|s| s.path.parent().is_some_and(|p| p.ends_with("profiles")))
        );
        assert!(
            sites
                .iter()
                .any(|s| s.path.parent().is_some_and(|p| p.ends_with("dashboard")))
        );
    }
);
source_test!(
    a_failure_part_way_leaves_no_vault_and_nothing_converted,
    "tests/test_vault_covers_the_root.py::test_a_failure_part_way_leaves_no_vault_and_nothing_converted",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        let before = snapshot(root);
        let mut calls = 0;
        assert!(
            testing::with_observer(
                move |e: &Event| {
                    if e.operation == "encrypt" {
                        calls += 1;
                        if calls == 3 {
                            return Err(io::Error::from_raw_os_error(28));
                        }
                    }
                    Ok(Action::Continue)
                },
                || transaction::enable(root, MASTER)
            )
            .is_err()
        );
        assert!(!repo(root, None).vault_enabled().expect("failed enable"));
        for (path, bytes) in before {
            assert_eq!(fs::read(root.join(path)).expect("restored bytes"), bytes);
        }
    }
);
source_test!(
    a_config_write_failure_also_rolls_the_secrets_back,
    "tests/test_vault_covers_the_root.py::test_a_config_write_failure_also_rolls_the_secrets_back",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        let before = snapshot(root);
        assert!(
            testing::with_observer(error_at("before-config", "config.json", 13), || {
                transaction::enable(root, MASTER)
            })
            .is_err()
        );
        assert!(!repo(root, None).vault_enabled().expect("failed enable"));
        for (path, bytes) in before {
            assert_eq!(fs::read(root.join(path)).expect("restored bytes"), bytes);
        }
    }
);
source_test!(
    unlocked_reads_give_back_the_real_secrets,
    "tests/test_vault_covers_the_root.py::test_unlocked_reads_give_back_the_real_secrets",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let engine = populated(root);
        enable(root, MASTER);
        assert!(repo(root, Some(MASTER)).unlocked().expect("unlock"));
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .get(&engine.id)
                    .expect("get")
                    .expect("profile")
            ),
            "ENGINE-SECRET"
        );
        let rows = dash(root, Some(MASTER)).list().expect("dashboard");
        assert_eq!(
            rows.iter()
                .find(|r| r["name"] == "dash-one")
                .expect("dashboard profile")["proxy"]["pass"],
            "DASHBOARD-SECRET"
        );
        assert_eq!(
            pool(root, Some(MASTER)).list().expect("pool")[0]["pass"],
            "POOL-SECRET"
        );
    }
);
source_test!(
    a_locked_read_yields_nothing_rather_than_the_ciphertext,
    "tests/test_vault_covers_the_root.py::test_a_locked_read_yields_nothing_rather_than_the_ciphertext",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        enable(root, MASTER);
        assert!(!repo(root, None).unlocked().expect("locked"));
        let rows = dash(root, None).list().expect("dashboard");
        assert_eq!(
            rows.iter()
                .find(|r| r["name"] == "dash-one")
                .expect("profile")["proxy"]["pass"],
            Value::Null
        );
        assert_eq!(
            pool(root, None).list().expect("pool")[0]["pass"],
            Value::Null
        );
    }
);
source_test!(
    saving_again_does_not_encrypt_twice,
    "tests/test_vault_covers_the_root.py::test_saving_again_does_not_encrypt_twice",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        enable(root, MASTER);
        let mut d = dash(root, Some(MASTER))
            .list()
            .expect("dashboard")
            .into_iter()
            .find(|r| r["name"] == "dash-one")
            .expect("profile");
        d["notes"] = "edited".into();
        dash(root, Some(MASTER))
            .save(d, Default::default(), None, None, 1001.0)
            .expect("dashboard edit");
        assert_eq!(
            dash(root, Some(MASTER))
                .list()
                .expect("dashboard")
                .iter()
                .find(|r| r["name"] == "dash-one")
                .expect("profile")["proxy"]["pass"],
            "DASHBOARD-SECRET"
        );
        let row = pool(root, Some(MASTER)).list().expect("pool")[0].clone();
        pool(root, Some(MASTER))
            .update(
                row["id"].as_str().expect("ID"),
                json!({"country":"DE"}).as_object().expect("patch").clone(),
            )
            .expect("pool edit");
        assert_eq!(
            pool(root, Some(MASTER)).list().expect("pool")[0]["pass"],
            "POOL-SECRET"
        );
        assert!(leaked(root, "POOL-SECRET").is_empty());
    }
);
source_test!(
    a_locked_edit_keeps_the_stored_secret,
    "tests/test_vault_covers_the_root.py::test_a_locked_edit_keeps_the_stored_secret",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        enable(root, MASTER);
        let entry = pool(root, None).list().expect("pool")[0].clone();
        assert_eq!(entry["pass"], Value::Null);
        pool(root, None)
            .update(
                entry["id"].as_str().expect("ID"),
                json!({"country":"FR"}).as_object().expect("patch").clone(),
            )
            .expect("edit locked pool");
        let entry = pool(root, Some(MASTER)).list().expect("pool")[0].clone();
        assert_eq!(entry["pass"], "POOL-SECRET");
        assert_eq!(entry["country"], "FR");
    }
);
source_test!(
    pool_identity_survives_encryption,
    "tests/test_vault_covers_the_root.py::test_pool_identity_survives_encryption",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        enable(root, MASTER);
        assert!(pool(root,Some(MASTER)).add_many(vec![json!({"type":"SOCKS5","host":"9.9.9.9","port":"1080","user":"u","pass":"POOL-SECRET"})]).expect("deduplicate").is_empty());
        assert_eq!(pool(root, Some(MASTER)).list().expect("pool").len(), 1);
    }
);
source_test!(
    a_new_secret_while_locked_is_refused_not_written,
    "tests/test_vault_covers_the_root.py::test_a_new_secret_while_locked_is_refused_not_written",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _engine = populated(root);
        enable(root, MASTER);
        let before = snapshot(root);
        assert!(
            pool(root, None)
                .add_many(vec![pool_entry("1.1.1.1", json!("NEW-PLAINTEXT"))])
                .is_err()
        );
        assert!(leaked(root, "NEW-PLAINTEXT").is_empty());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_pool_without_an_engine_store_still_works,
    "tests/test_vault_covers_the_root.py::test_a_pool_without_an_engine_store_still_works",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        pool(root, None)
            .add_many(vec![pool_entry("2.2.2.2", json!("plain"))])
            .expect("no vault pool");
        assert_eq!(pool(root, None).list().expect("pool")[0]["pass"], "plain");
    }
);
source_test!(
    both_real_call_sites_give_the_pool_a_vault,
    "tests/test_vault_covers_the_root.py::test_both_real_call_sites_give_the_pool_a_vault",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = pool_vault(root);
        cli(root, None, |_, commands| {
            assert_eq!(commands.export(&[], None).expect("CLI locked pool").exit, 1)
        });
        let (status, rows) = http(root, None, "GET", "/api/proxies", Value::Null);
        assert_eq!(status, 200);
        assert_eq!(rows[0]["pass"], Value::Null);
        let (status, rows) = http(root, Some(MASTER), "GET", "/api/proxies", Value::Null);
        assert_eq!(status, 200);
        assert_eq!(rows[0]["pass"], "PROXY-PASSWORD");
        assert_eq!(rows[0]["id"], id);
    }
);
