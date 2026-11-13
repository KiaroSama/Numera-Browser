use super::*;
source_test!(
    c000_two_field_edits_both_survive,
    "tests/test_request_atomicity.py::test_two_field_edits_both_survive",
    {
        patch_fields();
    }
);
source_test!(
    c000_a_stale_full_profile_is_refused,
    "tests/test_request_atomicity.py::test_a_stale_full_profile_is_refused",
    {
        revision(true);
    }
);
source_test!(
    c000_a_fresh_full_profile_still_saves,
    "tests/test_request_atomicity.py::test_a_fresh_full_profile_still_saves",
    {
        revision(false);
    }
);
source_test!(
    c000_a_client_without_a_revision_is_not_refused,
    "tests/test_request_atomicity.py::test_a_client_without_a_revision_is_not_refused",
    {
        let tmp = common::TempDirectory::new("contract-no-revision").expect("root");
        let p = made(tmp.path(), json!({"name":"p","notes":"keep"}));
        let (s, v) = put(tmp.path(), &p["id"], json!({"name":"renamed"}));
        assert_eq!(s, 200, "{v}");
        assert_eq!(v["notes"], "keep");
    }
);
source_test!(
    c000_a_failed_profile_write_leaves_the_default_engine_alone,
    "tests/test_request_atomicity.py::test_a_failed_profile_write_leaves_the_default_engine_alone",
    {
        write_fault("create", 1);
    }
);
source_test!(
    c000_a_failed_config_write_rolls_the_profile_back,
    "tests/test_request_atomicity.py::test_a_failed_config_write_rolls_the_profile_back",
    {
        write_fault("create", 3);
    }
);
source_test!(
    c000_a_successful_request_commits_both,
    "tests/test_request_atomicity.py::test_a_successful_request_commits_both",
    {
        let tmp = common::TempDirectory::new("contract-request-commit").expect("root");
        let p = made(tmp.path(), json!({"name":"p","engine":"camoufox"}));
        assert_eq!(p["engine"], "camoufox");
        assert_eq!(
            config(tmp.path()).read().expect("config")["default_engine"],
            "camoufox"
        );
    }
);
source_test!(
    c000_a_failed_bulk_leaves_neither_profiles_nor_config_changed,
    "tests/test_request_atomicity.py::test_a_failed_bulk_leaves_neither_profiles_nor_config_changed",
    {
        write_fault("bulk", 4);
    }
);
