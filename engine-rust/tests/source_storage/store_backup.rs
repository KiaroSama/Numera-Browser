use super::*;
source_test!(
    backup_then_restore_brings_back_profile_and_session,
    "tests/test_store_backup.py::test_backup_then_restore_brings_back_profile_and_session",
    {
        let temp = temp();
        let root = temp.path();
        let a = root.join("a");
        let p = saved(&a, "Acct", 7);
        let ud = store(&a).user_data_path(&p.id).expect("user data");
        fs::create_dir_all(ud.join("Default")).expect("session dir");
        fs::write(ud.join("Default/Cookies"), b"session-bytes").expect("session bytes");
        let archive = root.join("acct.zip");
        assert_eq!(
            store(&a)
                .backup(&p.id, &archive, "source-storage-host")
                .expect("backup"),
            Some(archive.clone())
        );
        assert!(archive.exists());
        let b = root.join("b");
        let restored = store(&b)
            .restore(&archive, false, "source-storage-host", 1001.0)
            .expect("restore");
        assert_eq!(restored.id, p.id);
        assert_eq!(restored.name, "Acct");
        let got = store(&b)
            .get(&p.id)
            .expect("get")
            .expect("restored profile");
        assert_eq!(
            got.fingerprint.seed,
            json!(7).as_number().expect("seed").clone()
        );
        assert_eq!(
            fs::read(
                store(&b)
                    .user_data_path(&p.id)
                    .expect("user data")
                    .join("Default/Cookies")
            )
            .expect("restored session"),
            b"session-bytes"
        );
    }
);
source_test!(
    backup_missing_profile_returns_none,
    "tests/test_store_backup.py::test_backup_missing_profile_returns_none",
    {
        let temp = temp();
        let root = temp.path();
        assert!(
            store(root)
                .backup("nope", &root.join("x.zip"), "source-storage-host")
                .expect("missing backup")
                .is_none()
        );
    }
);
source_test!(
    restore_with_new_id_makes_a_copy,
    "tests/test_store_backup.py::test_restore_with_new_id_makes_a_copy",
    {
        let temp = temp();
        let root = temp.path();
        let a = root.join("a");
        let p = saved(&a, "Acct", 7);
        let ud = store(&a).user_data_path(&p.id).expect("user data");
        fs::create_dir_all(ud.join("Default")).expect("session dir");
        fs::write(ud.join("Default/Cookies"), b"session-bytes").expect("session bytes");
        let archive = root.join("acct.zip");
        assert_eq!(
            store(&a)
                .backup(&p.id, &archive, "source-storage-host")
                .expect("backup"),
            Some(archive.clone())
        );
        assert!(archive.exists());
        let copy = store(&a)
            .restore(&archive, true, "source-storage-host", 1001.0)
            .expect("clone restore");
        assert_ne!(copy.id, p.id);
        let ids = store(&a)
            .list(1002.0)
            .expect("list")
            .into_iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>();
        assert!(ids.contains(&p.id) && ids.contains(&copy.id));
        assert_eq!(
            fs::read(
                store(&a)
                    .user_data_path(&copy.id)
                    .expect("clone data")
                    .join("Default/Cookies")
            )
            .expect("cloned session"),
            b"session-bytes"
        );
    }
);
