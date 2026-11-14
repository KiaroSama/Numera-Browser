use super::*;
source_test!(
    c000_a_profile_name_that_is_not_text_is_refused,
    "tests/test_cli_import_write_contract.py::test_a_profile_name_that_is_not_text_is_refused[123]",
    {
        let tmp = common::TempDirectory::new("contract-name").expect("root");
        let mut p = profile("p");
        p.name = json!(123);
        let before = snapshot(tmp.path());
        assert!(
            repo(tmp.path())
                .save(p, 1001.0)
                .err()
                .expect("bad name")
                .to_string()
                .contains("name")
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c001_a_profile_name_that_is_not_text_is_refused,
    "tests/test_cli_import_write_contract.py::test_a_profile_name_that_is_not_text_is_refused[12.5]",
    {
        let tmp = common::TempDirectory::new("contract-name").expect("root");
        let mut p = profile("p");
        p.name = json!(12.5);
        let before = snapshot(tmp.path());
        assert!(
            repo(tmp.path())
                .save(p, 1001.0)
                .err()
                .expect("bad name")
                .to_string()
                .contains("name")
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c002_a_profile_name_that_is_not_text_is_refused,
    "tests/test_cli_import_write_contract.py::test_a_profile_name_that_is_not_text_is_refused[bad2]",
    {
        let tmp = common::TempDirectory::new("contract-name").expect("root");
        let mut p = profile("p");
        p.name = Value::Array(vec![json!("a")]);
        let before = snapshot(tmp.path());
        assert!(
            repo(tmp.path())
                .save(p, 1001.0)
                .err()
                .expect("bad name")
                .to_string()
                .contains("name")
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c003_a_profile_name_that_is_not_text_is_refused,
    "tests/test_cli_import_write_contract.py::test_a_profile_name_that_is_not_text_is_refused[bad3]",
    {
        let tmp = common::TempDirectory::new("contract-name").expect("root");
        let mut p = profile("p");
        p.name = Value::Object(serde_json::Map::from_iter([("x".into(), json!(1))]));
        let before = snapshot(tmp.path());
        assert!(
            repo(tmp.path())
                .save(p, 1001.0)
                .err()
                .expect("bad name")
                .to_string()
                .contains("name")
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c004_a_profile_name_that_is_not_text_is_refused,
    "tests/test_cli_import_write_contract.py::test_a_profile_name_that_is_not_text_is_refused[None]",
    {
        let tmp = common::TempDirectory::new("contract-name").expect("root");
        let mut p = profile("p");
        p.name = Value::Null;
        let before = snapshot(tmp.path());
        assert!(
            repo(tmp.path())
                .save(p, 1001.0)
                .err()
                .expect("bad name")
                .to_string()
                .contains("name")
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c005_a_profile_name_that_is_not_text_is_refused,
    "tests/test_cli_import_write_contract.py::test_a_profile_name_that_is_not_text_is_refused[True]",
    {
        let tmp = common::TempDirectory::new("contract-name").expect("root");
        let mut p = profile("p");
        p.name = json!(true);
        let before = snapshot(tmp.path());
        assert!(
            repo(tmp.path())
                .save(p, 1001.0)
                .err()
                .expect("bad name")
                .to_string()
                .contains("name")
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c000_an_ordinary_name_is_untouched,
    "tests/test_cli_import_write_contract.py::test_an_ordinary_name_is_untouched",
    {
        let tmp = common::TempDirectory::new("contract-names").expect("root");
        for (i, name) in ["alpha", "", "  spaces  ", "Ünïcodé ✓", "123"]
            .iter()
            .enumerate()
        {
            let mut p = profile(name);
            p.id = format!("name-{i}");
            assert_eq!(
                repo(tmp.path())
                    .save(p, 1001.0)
                    .expect("ordinary name")
                    .name,
                *name
            );
        }
    }
);
source_test!(
    c000_the_cli_import_door_is_closed,
    "tests/test_cli_import_write_contract.py::test_the_cli_import_door_is_closed",
    {
        cli_refused(
            json!([{"name":123,"os":"win","userAgent":"Mozilla/5.0","resolution":"1920x1080"}]),
            "name",
        );
    }
);
source_test!(
    c000_a_good_vendor_export_still_imports_from_the_cli,
    "tests/test_cli_import_write_contract.py::test_a_good_vendor_export_still_imports_from_the_cli",
    {
        good_vendor();
    }
);
source_test!(
    c000_the_dashboard_never_adopts_a_name_it_cannot_render,
    "tests/test_cli_import_write_contract.py::test_the_dashboard_never_adopts_a_name_it_cannot_render",
    {
        let tmp = common::TempDirectory::new("contract-adopt-name").expect("root");
        repo(tmp.path())
            .save(profile("fine"), 1001.0)
            .expect("save");
        for p in repo(tmp.path()).list(1002.0).expect("list") {
            assert!(p.name.is_string());
        }
        for id in ids(tmp.path(), "profiles") {
            assert!(read(tmp.path(), "profiles", &id)["name"].is_string());
        }
    }
);
