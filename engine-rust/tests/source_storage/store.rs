use super::*;
source_test!(save_and_get, "tests/test_store.py::test_save_and_get", {
    let temp = temp();
    let root = temp.path();
    let p = saved(root, "acct-01", 1);
    let loaded = store(root).get(&p.id).expect("get").expect("profile");
    assert_eq!(loaded.name, "acct-01");
    assert_eq!(
        serde_json::to_value(&loaded.fingerprint).expect("fingerprint"),
        serde_json::to_value(&p.fingerprint).expect("fingerprint")
    );
});
source_test!(
    resolve_by_name_and_id,
    "tests/test_store.py::test_resolve_by_name_and_id",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "named", 2);
        assert_eq!(
            store(root)
                .resolve("named", 1001.0)
                .expect("resolve")
                .expect("profile")
                .id,
            p.id
        );
        assert_eq!(
            store(root)
                .resolve(&p.id, 1001.0)
                .expect("resolve")
                .expect("profile")
                .name,
            "named"
        );
        assert!(
            store(root)
                .resolve("nope", 1001.0)
                .expect("missing resolve")
                .is_none()
        );
    }
);
source_test!(
    list_is_sorted_by_creation,
    "tests/test_store.py::test_list_is_sorted_by_creation",
    {
        let temp = temp();
        let root = temp.path();
        let mut a = profile("a", 1);
        a.created_at = 100.0;
        let mut b = profile("b", 2);
        b.created_at = 200.0;
        store(root).save(b, 1000.0).expect("save b");
        store(root).save(a, 1000.0).expect("save a");
        assert_eq!(
            store(root)
                .list(1001.0)
                .expect("creation-order list")
                .iter()
                .map(|p| p.name.as_str().expect("name"))
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
);
source_test!(delete, "tests/test_store.py::test_delete", {
    let temp = temp();
    let root = temp.path();
    let p = saved(root, "temp", 3);
    assert!(
        store(root)
            .delete(&p.id, "source-storage-host", 1001.0)
            .expect("delete")
    );
    assert!(store(root).get(&p.id).expect("get deleted").is_none());
    assert!(
        !store(root)
            .delete(&p.id, "source-storage-host", 1001.0)
            .expect("missing delete")
    );
});
source_test!(
    export_import_roundtrip,
    "tests/test_store.py::test_export_import_roundtrip",
    {
        let temp = temp();
        let root = temp.path();
        let mut p = profile("portable", 4);
        p.proxy = Some(
            Proxy::from_record(&json!({"server":"socks5://h:1080","country":"US"}), false)
                .expect("proxy"),
        );
        p.tags = vec!["client-x".into()];
        p.notes = "hello".into();
        let p = store(root).save(p, 1000.0).expect("save");
        let out = root.join("exp.json");
        store(root).export(&p.id, &out).expect("export");
        let imported = store(&root.join("other"))
            .import_file(&out, 1001.0)
            .expect("import");
        assert_eq!(imported.name, "portable");
        assert_eq!(imported.proxy.expect("proxy").server, "socks5://h:1080");
        assert_eq!(imported.tags, ["client-x"]);
    }
);
source_test!(
    user_data_dir_created,
    "tests/test_store.py::test_user_data_dir_created",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "ud", 5);
        let path = store(root).user_data_path(&p.id).expect("user data");
        assert!(path.exists() && path.is_dir());
        temp.cleanup().expect("owned root cleanup");
        assert!(!path.exists());
    }
);
