use super::*;
source_test!(
    api_tokens_test_the_value_is_never_written_to_disk,
    "tests/test_api_tokens.py::test_the_value_is_never_written_to_disk",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let made = s.create("CI runner", 1000.0)?;
        let raw = fs::read_to_string(root.join("tokens.json"))?;
        assert!(!raw.contains(&text(&made, "token")));
        let row = s.list().remove(0);
        assert_eq!(
            row.as_object()
                .expect("token row")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            ["id", "name", "hash", "created", "lastUsed"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        Ok(())
    }
);
source_test!(
    api_tokens_test_verify_matches_only_the_right_token,
    "tests/test_api_tokens.py::test_verify_matches_only_the_right_token",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let made = s.create("one", 1000.0)?;
        assert_eq!(
            s.verify(&text(&made, "token"), 1001.0)?
                .expect("correct token")["name"],
            "one"
        );
        assert!(s.verify(&(text(&made, "token") + "x"), 1001.0)?.is_none());
        assert!(s.verify("", 1001.0)?.is_none());
        Ok(())
    }
);
source_test!(
    api_tokens_test_using_a_token_records_when,
    "tests/test_api_tokens.py::test_using_a_token_records_when",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let made = s.create("one", 1000.0)?;
        assert!(s.list()[0]["lastUsed"].is_null());
        s.verify(&text(&made, "token"), 1001.0)?;
        assert!(!s.list()[0]["lastUsed"].is_null());
        Ok(())
    }
);
source_test!(
    api_tokens_test_a_corrupt_token_file_does_not_lock_everything_out,
    "tests/test_api_tokens.py::test_a_corrupt_token_file_does_not_lock_everything_out",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(root.join("tokens.json"), "{ not json")?;
        assert!(s.list().is_empty());
        Ok(())
    }
);
source_test!(
    api_tokens_test_with_no_tokens_and_no_env_var_the_api_is_open,
    "tests/test_api_tokens.py::test_with_no_tokens_and_no_env_var_the_api_is_open",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 200);
        Ok(())
    }
);
source_test!(
    api_tokens_test_minting_a_token_turns_auth_on,
    "tests/test_api_tokens.py::test_minting_a_token_turns_auth_on",
    {
        let t = fresh();
        token(t.path(), "CI runner");
        let h = Http::new(t.path());
        assert_eq!(h.get("/api/health", None).status, 401);
        assert_eq!(h.get("/api/health", Some("nmr_wrong")).status, 401);
        Ok(())
    }
);
source_test!(
    api_tokens_test_a_named_token_opens_the_api,
    "tests/test_api_tokens.py::test_a_named_token_opens_the_api",
    {
        let t = fresh();
        let made = token(t.path(), "CI runner");
        let h = Http::new(t.path());
        assert_eq!(
            h.get("/api/health", Some(&text(&made, "token"))).status,
            200
        );
        Ok(())
    }
);
source_test!(
    api_tokens_test_revoking_one_token_leaves_the_others_working,
    "tests/test_api_tokens.py::test_revoking_one_token_leaves_the_others_working",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let keep = s.create("keep", 1000.0)?;
        let drop = s.create("drop", 1000.0)?;
        s.revoke(&text(&drop, "id"), false)?;
        let h = Http::new(root);
        assert_eq!(
            h.get("/api/health", Some(&text(&keep, "token"))).status,
            200
        );
        assert_eq!(
            h.get("/api/health", Some(&text(&drop, "token"))).status,
            401
        );
        Ok(())
    }
);
source_test!(
    api_tokens_test_the_env_var_keeps_working_after_a_named_token_exists,
    "tests/test_api_tokens.py::test_the_env_var_keeps_working_after_a_named_token_exists",
    {
        let t = fresh();
        let made = token(t.path(), "CI runner");
        let h = Http::configured(t.path(), "legacy-secret", "", None);
        assert_eq!(h.get("/api/health", Some("legacy-secret")).status, 200);
        assert_eq!(
            h.get("/api/health", Some(&text(&made, "token"))).status,
            200
        );
        Ok(())
    }
);
source_test!(
    api_tokens_test_revoking_every_named_token_does_not_reopen_a_locked_server,
    "tests/test_api_tokens.py::test_revoking_every_named_token_does_not_reopen_a_locked_server",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let made = s.create("one", 1000.0)?;
        s.revoke(&text(&made, "id"), false)?;
        let h = Http::configured(root, "legacy-secret", "", None);
        assert_eq!(h.get("/api/health", None).status, 401);
        Ok(())
    }
);
source_test!(
    api_tokens_test_the_listing_never_returns_a_hash_or_a_value,
    "tests/test_api_tokens.py::test_the_listing_never_returns_a_hash_or_a_value",
    {
        let t = fresh();
        let made = token(t.path(), "CI runner");
        let h = Http::new(t.path());
        let r = h.get("/api/tokens", Some(&text(&made, "token")));
        assert_eq!(r.status, 200);
        assert_eq!(r.body["envToken"], false);
        assert_eq!(r.body["tokens"][0]["name"], "CI runner");
        assert!(!r.text().contains("hash"));
        assert!(!r.text().contains(&text(&made, "token")));
        Ok(())
    }
);
source_test!(
    api_tokens_test_the_listing_says_whether_the_env_var_is_also_in_play,
    "tests/test_api_tokens.py::test_the_listing_says_whether_the_env_var_is_also_in_play",
    {
        let t = fresh();
        let h = Http::configured(t.path(), "legacy-secret", "", None);
        assert_eq!(
            h.get("/api/tokens", Some("legacy-secret")).body["envToken"],
            true
        );
        Ok(())
    }
);
source_test!(
    api_tokens_test_a_token_must_be_named,
    "tests/test_api_tokens.py::test_a_token_must_be_named",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(
            h.post("/api/tokens", json!({"name":"  "}), None).status,
            400
        );
        Ok(())
    }
);
source_test!(
    api_tokens_test_a_duplicate_name_is_refused,
    "tests/test_api_tokens.py::test_a_duplicate_name_is_refused",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let made = h.post("/api/tokens", json!({"name":"CI"}), None);
        assert_eq!(made.status, 200);
        assert_eq!(
            h.post(
                "/api/tokens",
                json!({"name":"CI"}),
                Some(&text(&made.body, "token"))
            )
            .status,
            409
        );
        Ok(())
    }
);
source_test!(
    api_tokens_test_the_value_comes_back_exactly_once,
    "tests/test_api_tokens.py::test_the_value_comes_back_exactly_once",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let made = h.post("/api/tokens", json!({"name":"CI"}), None);
        let secret = text(&made.body, "token");
        assert!(secret.starts_with("nmr_"));
        let listed = h.get("/api/tokens", Some(&secret));
        assert!(listed.body["tokens"][0].get("token").is_none());
        Ok(())
    }
);
source_test!(
    api_tokens_test_the_config_reports_how_many_proxies_are_stored,
    "tests/test_api_tokens.py::test_the_config_reports_how_many_proxies_are_stored",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let pool = numera_core::storage::pool::ProxyPool::new(root.to_owned(), None);
        pool.add_many(vec![
            json!({"type":"HTTP","host":"1.1.1.1","port":8080}),
            json!({"type":"HTTP","host":"2.2.2.2","port":8080}),
        ])?;
        assert_eq!(h.get("/api/config", None).body["proxyCount"], 2);
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_a_fresh_install_is_not_protected,
    "tests/test_auth_fail_closed.py::test_a_fresh_install_is_not_protected",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert!(!s.protected());
        assert_eq!(s.state(), TokenState::Empty);
        t.cleanup()?;
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_minting_the_first_token_turns_protection_on,
    "tests/test_auth_fail_closed.py::test_minting_the_first_token_turns_protection_on",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        assert!(s.protected());
        assert_eq!(s.state(), TokenState::Tokens);
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_protection_survives_revoking_the_last_token,
    "tests/test_auth_fail_closed.py::test_protection_survives_revoking_the_last_token",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let made = s.create("only", 1000.0)?;
        s.revoke(&text(&made, "id"), false)?;
        assert!(s.list().is_empty());
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_protection_survives_the_file_being_deleted,
    "tests/test_auth_fail_closed.py::test_protection_survives_the_file_being_deleted",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::remove_file(root.join("tokens.json"))?;
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_unreadable_storage_is_not_an_empty_list_not_json_at_all_b491c9,
    "tests/test_auth_fail_closed.py::test_unreadable_storage_is_not_an_empty_list[{not json at all]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::write(root.join("tokens.json"), "{not json at all")?;
        assert_eq!(s.state(), TokenState::Unreadable);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_unreadable_storage_is_not_an_empty_list_tokens_54c8d6,
    "tests/test_auth_fail_closed.py::test_unreadable_storage_is_not_an_empty_list[{\"tokens\": []}]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::write(root.join("tokens.json"), "{\"tokens\": []}")?;
        assert_eq!(s.state(), TokenState::Unreadable);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_unreadable_storage_is_not_an_empty_list_null_658aa5,
    "tests/test_auth_fail_closed.py::test_unreadable_storage_is_not_an_empty_list[null]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::write(root.join("tokens.json"), "null")?;
        assert_eq!(s.state(), TokenState::Unreadable);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_unreadable_storage_is_not_an_empty_list_a_string_5b27e0,
    "tests/test_auth_fail_closed.py::test_unreadable_storage_is_not_an_empty_list[\"a string\"]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::write(root.join("tokens.json"), "\"a string\"")?;
        assert_eq!(s.state(), TokenState::Unreadable);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_unreadable_storage_is_not_an_empty_list_42_d87839,
    "tests/test_auth_fail_closed.py::test_unreadable_storage_is_not_an_empty_list[42]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::write(root.join("tokens.json"), "42")?;
        assert_eq!(s.state(), TokenState::Unreadable);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_a_valid_empty_list_is_still_empty,
    "tests/test_auth_fail_closed.py::test_a_valid_empty_list_is_still_empty",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::write(root.join("tokens.json"), "[]")?;
        assert_eq!(s.state(), TokenState::Empty);
        assert!(s.list().is_empty());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_revoking_the_last_token_is_refused_without_an_alternative,
    "tests/test_auth_fail_closed.py::test_revoking_the_last_token_is_refused_without_an_alternative",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let made = s.create("only", 1000.0)?;
        let e = s.revoke(&text(&made, "id"), true).expect_err("last token");
        assert!(e.to_string().contains("last"));
        assert_eq!(s.list().len(), 1);
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_a_second_token_makes_the_first_revocable,
    "tests/test_auth_fail_closed.py::test_a_second_token_makes_the_first_revocable",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let first = s.create("one", 1000.0)?;
        s.create("two", 1000.0)?;
        assert!(s.revoke(&text(&first, "id"), true)?);
        assert_eq!(s.list()[0]["name"], "two");
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_protection_can_be_turned_off_deliberately,
    "tests/test_auth_fail_closed.py::test_protection_can_be_turned_off_deliberately",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::remove_file(root.join("tokens.json"))?;
        assert!(s.protected());
        s.set_protected(false, 1001.0)?;
        assert!(!s.protected());
        Ok(())
    }
);
source_test!(
    auth_fail_closed_test_the_marker_is_not_the_token_file,
    "tests/test_auth_fail_closed.py::test_the_marker_is_not_the_token_file",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("ci", 1000.0)?;
        fs::remove_file(root.join("tokens.json"))?;
        assert!(s.protected());
        assert!(TokenStore::new(root.to_owned()).protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_falsey_policy_value_does_not_open_the_service_null_1a1ad1,
    "tests/test_auth_policy_strictness.py::test_a_falsey_policy_value_does_not_open_the_service[null]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(
            root.join("auth-policy.json"),
            format!("{{\"protected\":{}}}", "null"),
        )?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_falsey_policy_value_does_not_open_the_service_0_3db2a1,
    "tests/test_auth_policy_strictness.py::test_a_falsey_policy_value_does_not_open_the_service[0]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(
            root.join("auth-policy.json"),
            format!("{{\"protected\":{}}}", "0"),
        )?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_falsey_policy_value_does_not_open_the_service_cbab8e,
    "tests/test_auth_policy_strictness.py::test_a_falsey_policy_value_does_not_open_the_service[\"\"]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(
            root.join("auth-policy.json"),
            format!("{{\"protected\":{}}}", "\"\""),
        )?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_falsey_policy_value_does_not_open_the_service_84aa7e,
    "tests/test_auth_policy_strictness.py::test_a_falsey_policy_value_does_not_open_the_service[[]]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(
            root.join("auth-policy.json"),
            format!("{{\"protected\":{}}}", "[]"),
        )?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_falsey_policy_value_does_not_open_the_service_f091cc,
    "tests/test_auth_policy_strictness.py::test_a_falsey_policy_value_does_not_open_the_service[{}]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(
            root.join("auth-policy.json"),
            format!("{{\"protected\":{}}}", "{}"),
        )?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_falsey_policy_value_does_not_open_the_service_false_e8a12f,
    "tests/test_auth_policy_strictness.py::test_a_falsey_policy_value_does_not_open_the_service[\"false\"]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(
            root.join("auth-policy.json"),
            format!("{{\"protected\":{}}}", "\"false\""),
        )?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_falsey_policy_value_does_not_open_the_service_0_0_17df9c,
    "tests/test_auth_policy_strictness.py::test_a_falsey_policy_value_does_not_open_the_service[0.0]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(
            root.join("auth-policy.json"),
            format!("{{\"protected\":{}}}", "0.0"),
        )?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_only_a_real_boolean_false_opens_it,
    "tests/test_auth_policy_strictness.py::test_only_a_real_boolean_false_opens_it",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(root.join("auth-policy.json"), r#"{"protected":false}"#)?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Open);
        assert!(!s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_real_boolean_true_protects,
    "tests/test_auth_policy_strictness.py::test_a_real_boolean_true_protects",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.set_protected(true, 1000.0)?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Protected);
        assert!(s.protected());
        assert_eq!(read(root.join("auth-policy.json"))["protected"], true);
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_damaged_document_is_ambiguous_not_open_not_json_8da48b,
    "tests/test_auth_policy_strictness.py::test_a_damaged_document_is_ambiguous_not_open[{not json]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(root.join("auth-policy.json"), "{not json")?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_damaged_document_is_ambiguous_not_open_5e972b,
    "tests/test_auth_policy_strictness.py::test_a_damaged_document_is_ambiguous_not_open[[]]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(root.join("auth-policy.json"), "[]")?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_damaged_document_is_ambiguous_not_open_a_string_e9b2a7,
    "tests/test_auth_policy_strictness.py::test_a_damaged_document_is_ambiguous_not_open[\"a string\"]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(root.join("auth-policy.json"), "\"a string\"")?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_damaged_document_is_ambiguous_not_open_17_5c2049,
    "tests/test_auth_policy_strictness.py::test_a_damaged_document_is_ambiguous_not_open[17]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(root.join("auth-policy.json"), "17")?;
        assert_eq!(auth_policy::read(root), AuthPolicy::Ambiguous);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_no_document_is_absent_not_open,
    "tests/test_auth_policy_strictness.py::test_no_document_is_absent_not_open",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert_eq!(auth_policy::read(root), AuthPolicy::Absent);
        assert!(!s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_an_already_damaged_legacy_store_is_adopted,
    "tests/test_auth_policy_strictness.py::test_an_already_damaged_legacy_store_is_adopted",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        legacy(root, Some("{truncated"));
        assert_eq!(s.state(), TokenState::Unreadable);
        assert!(s.adopt(1000.0)?);
        assert_eq!(auth_policy::read(root), AuthPolicy::Protected);
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_deleting_a_damaged_legacy_file_after_adoption_stays_closed,
    "tests/test_auth_policy_strictness.py::test_deleting_a_damaged_legacy_file_after_adoption_stays_closed",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        legacy(root, Some("{truncated"));
        s.adopt(1000.0)?;
        fs::remove_file(root.join("tokens.json"))?;
        assert!(s.protected());
        assert!(TokenStore::new(root.to_owned()).protected());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_healthy_legacy_store_is_still_adopted,
    "tests/test_auth_policy_strictness.py::test_a_healthy_legacy_store_is_still_adopted",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        legacy(root, None);
        assert!(s.adopt(1000.0)?);
        assert_eq!(auth_policy::read(root), AuthPolicy::Protected);
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_adoption_leaves_an_existing_decision_alone,
    "tests/test_auth_policy_strictness.py::test_adoption_leaves_an_existing_decision_alone",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        legacy(root, None);
        s.set_protected(false, 1000.0)?;
        let before = fs::read(root.join("auth-policy.json"))?;
        assert!(!s.adopt(1001.0)?);
        assert_eq!(fs::read(root.join("auth-policy.json"))?, before);
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_new_install_is_not_adopted,
    "tests/test_auth_policy_strictness.py::test_a_new_install_is_not_adopted",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert!(!s.adopt(1000.0)?);
        assert!(!root.join("auth-policy.json").exists());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_a_damaged_legacy_root_stays_closed_across_a_restart,
    "tests/test_auth_policy_strictness.py::test_a_damaged_legacy_root_stays_closed_across_a_restart",
    {
        let t = fresh();
        let root = t.path();
        legacy(root, Some("{truncated"));
        let first = Http::new(root);
        assert_eq!(first.get("/api/health", None).status, 401);
        fs::remove_file(root.join("tokens.json"))?;
        let second = Http::new(root);
        assert_eq!(second.get("/api/health", None).status, 401);
        Ok(())
    }
);
source_test!(auth_policy_strictness_test_a_non_ascii_stored_hash_is_a_miss_not_an_exception_t_xf6k_xe9n_7158b5,"tests/test_auth_policy_strictness.py::test_a_non_ascii_stored_hash_is_a_miss_not_an_exception[t\\xf6k\\xe9n]",{
let t=fresh();let root=t.path();let s=TokenStore::new(root.to_owned());
fs::write(root.join("tokens.json"),json!([{"id":"aaaaaaaaaaaa","name":"hand-edited","hash":"tökén","created":1.0,"lastUsed":null}]).to_string())?;assert!(s.verify("nmr_anything",1000.0)?.is_none());
Ok(())
});
source_test!(auth_policy_strictness_test_a_non_ascii_stored_hash_is_a_miss_not_an_exception_u5bc6_u7801_f3c71a,"tests/test_auth_policy_strictness.py::test_a_non_ascii_stored_hash_is_a_miss_not_an_exception[\\u5bc6\\u7801]",{
let t=fresh();let root=t.path();let s=TokenStore::new(root.to_owned());
fs::write(root.join("tokens.json"),json!([{"id":"aaaaaaaaaaaa","name":"hand-edited","hash":"密码","created":1.0,"lastUsed":null}]).to_string())?;assert!(s.verify("nmr_anything",1000.0)?.is_none());
Ok(())
});
source_test!(auth_policy_strictness_test_a_non_ascii_stored_hash_is_a_miss_not_an_exception_plain_but_wrong_26bf26,"tests/test_auth_policy_strictness.py::test_a_non_ascii_stored_hash_is_a_miss_not_an_exception[plain-but-wrong]",{
let t=fresh();let root=t.path();let s=TokenStore::new(root.to_owned());
fs::write(root.join("tokens.json"),json!([{"id":"aaaaaaaaaaaa","name":"hand-edited","hash":"plain-but-wrong","created":1.0,"lastUsed":null}]).to_string())?;assert!(s.verify("nmr_anything",1000.0)?.is_none());
Ok(())
});
source_test!(
    auth_policy_strictness_test_a_damaged_row_does_not_hide_a_healthy_one,
    "tests/test_auth_policy_strictness.py::test_a_damaged_row_does_not_hide_a_healthy_one",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let made = s.create("real", 1000.0)?;
        let mut rows = s.list();
        rows.insert(
            0,
            json!({"id":"bbbbbbbbbbbb","name":"bad","hash":"tökén","created":1.0,"lastUsed":null}),
        );
        fs::write(root.join("tokens.json"), serde_json::to_vec(&rows)?)?;
        assert!(s.verify(&text(&made, "token"), 1001.0)?.is_some());
        Ok(())
    }
);
source_test!(
    auth_policy_strictness_test_the_right_token_still_authenticates,
    "tests/test_auth_policy_strictness.py::test_the_right_token_still_authenticates",
    {
        let t = fresh();
        let h = Http::configured(t.path(), "ascii-token-value", "", None);
        assert_eq!(h.get("/api/health", Some("ascii-token-value")).status, 200);
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_a_legacy_root_is_protected_to_begin_with,
    "tests/test_auth_policy_upgrade.py::test_a_legacy_root_is_protected_to_begin_with",
    {
        let t = fresh();
        legacy(t.path(), None);
        let h = Http::new(t.path());
        assert_eq!(h.get("/api/health", None).status, 401);
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_serving_a_legacy_root_records_the_policy,
    "tests/test_auth_policy_upgrade.py::test_serving_a_legacy_root_records_the_policy",
    {
        let t = fresh();
        legacy(t.path(), None);
        let _h = Http::new(t.path());
        assert_eq!(read(t.path().join("auth-policy.json"))["protected"], true);
        Ok(())
    }
);
source_test!(auth_policy_upgrade_test_losing_the_credentials_of_a_legacy_root_keeps_it_closed_lambda_deleting_tokens_json_20f327,"tests/test_auth_policy_upgrade.py::test_losing_the_credentials_of_a_legacy_root_keeps_it_closed[<lambda>-deleting tokens.json]",{
let t=fresh();let root=t.path();legacy(root,None);let h=Http::new(root);assert_eq!(h.get("/api/health",None).status,401);fs::remove_file(root.join("tokens.json"))?;assert_eq!(h.get("/api/health",None).status,401);
Ok(())
});
source_test!(auth_policy_upgrade_test_losing_the_credentials_of_a_legacy_root_keeps_it_closed_lambda_corrupting_tokens_json_310ab1,"tests/test_auth_policy_upgrade.py::test_losing_the_credentials_of_a_legacy_root_keeps_it_closed[<lambda>-corrupting tokens.json]",{
let t=fresh();let root=t.path();legacy(root,None);let h=Http::new(root);assert_eq!(h.get("/api/health",None).status,401);fs::write(root.join("tokens.json"),"{not json")?;assert_eq!(h.get("/api/health",None).status,401);
Ok(())
});
source_test!(auth_policy_upgrade_test_losing_the_credentials_of_a_legacy_root_keeps_it_closed_lambda_emptying_tokens_json_482c59,"tests/test_auth_policy_upgrade.py::test_losing_the_credentials_of_a_legacy_root_keeps_it_closed[<lambda>-emptying tokens.json]",{
let t=fresh();let root=t.path();legacy(root,None);let h=Http::new(root);assert_eq!(h.get("/api/health",None).status,401);fs::write(root.join("tokens.json"),"[]")?;assert_eq!(h.get("/api/health",None).status,401);
Ok(())
});
source_test!(auth_policy_upgrade_test_losing_the_credentials_of_a_legacy_root_keeps_it_closed_lambda_reshaping_tokens_json_1194f7,"tests/test_auth_policy_upgrade.py::test_losing_the_credentials_of_a_legacy_root_keeps_it_closed[<lambda>-reshaping tokens.json]",{
let t=fresh();let root=t.path();legacy(root,None);let h=Http::new(root);assert_eq!(h.get("/api/health",None).status,401);fs::write(root.join("tokens.json"),"{\"tokens\": []}")?;assert_eq!(h.get("/api/health",None).status,401);
Ok(())
});
source_test!(
    auth_policy_upgrade_test_the_policy_survives_a_restart_lambda_deleting_tokens_json_57ce8e,
    "tests/test_auth_policy_upgrade.py::test_the_policy_survives_a_restart[<lambda>-deleting tokens.json]",
    {
        let t = fresh();
        let root = t.path();
        legacy(root, None);
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 401);
        fs::remove_file(root.join("tokens.json"))?;
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 401);
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_the_policy_survives_a_restart_lambda_corrupting_tokens_json_cc4279,
    "tests/test_auth_policy_upgrade.py::test_the_policy_survives_a_restart[<lambda>-corrupting tokens.json]",
    {
        let t = fresh();
        let root = t.path();
        legacy(root, None);
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 401);
        fs::write(root.join("tokens.json"), "{not json")?;
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 401);
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_the_policy_survives_a_restart_lambda_emptying_tokens_json_cecb9b,
    "tests/test_auth_policy_upgrade.py::test_the_policy_survives_a_restart[<lambda>-emptying tokens.json]",
    {
        let t = fresh();
        let root = t.path();
        legacy(root, None);
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 401);
        fs::write(root.join("tokens.json"), "[]")?;
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 401);
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_an_unreadable_credential_list_is_not_an_open_configuration,
    "tests/test_auth_policy_upgrade.py::test_an_unreadable_credential_list_is_not_an_open_configuration",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        fs::write(root.join("tokens.json"), "{not json")?;
        assert_eq!(s.state(), TokenState::Unreadable);
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_a_damaged_policy_marker_is_not_an_open_configuration,
    "tests/test_auth_policy_upgrade.py::test_a_damaged_policy_marker_is_not_an_open_configuration",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.set_protected(true, 1000.0)?;
        fs::write(root.join("auth-policy.json"), "{not json")?;
        assert!(s.protected());
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_turning_protection_off_is_still_possible_and_explicit,
    "tests/test_auth_policy_upgrade.py::test_turning_protection_off_is_still_possible_and_explicit",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.set_protected(true, 1000.0)?;
        s.set_protected(false, 1001.0)?;
        assert!(!s.protected());
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 200);
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_a_genuinely_new_install_is_open_and_unmarked,
    "tests/test_auth_policy_upgrade.py::test_a_genuinely_new_install_is_open_and_unmarked",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 200);
        assert!(!root.join("auth-policy.json").exists());
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_an_environment_token_still_opens_a_locked_root,
    "tests/test_auth_policy_upgrade.py::test_an_environment_token_still_opens_a_locked_root",
    {
        let t = fresh();
        let root = t.path();
        legacy(root, None);
        let _first = Http::new(root);
        fs::remove_file(root.join("tokens.json"))?;
        let h = Http::configured(root, "env-token-value", "", None);
        assert_eq!(h.get("/api/health", None).status, 401);
        assert_eq!(h.get("/api/health", Some("env-token-value")).status, 200);
        Ok(())
    }
);
source_test!(
    auth_policy_upgrade_test_adoption_does_not_disturb_a_store_that_already_has_a_marker,
    "tests/test_auth_policy_upgrade.py::test_adoption_does_not_disturb_a_store_that_already_has_a_marker",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.set_protected(false, 1000.0)?;
        let before = fs::read(root.join("auth-policy.json"))?;
        let _h = Http::new(root);
        assert_eq!(fs::read(root.join("auth-policy.json"))?, before);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_minting_the_first_token_protects_the_running_app,
    "tests/test_auth_runtime.py::test_minting_the_first_token_protects_the_running_app",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 200);
        let made = h.post("/api/tokens", json!({"name":"CI"}), None);
        assert_eq!(h.get("/api/health", None).status, 401);
        assert_eq!(
            h.get("/api/health", Some(&text(&made.body, "token")))
                .status,
            200
        );
        Ok(())
    }
);
source_test!(
    auth_runtime_test_revoking_the_last_token_is_refused_rather_than_reopening,
    "tests/test_auth_runtime.py::test_revoking_the_last_token_is_refused_rather_than_reopening",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let made = h.post("/api/tokens", json!({"name":"CI"}), None);
        let secret = text(&made.body, "token");
        let r = h.delete(
            &format!("/api/tokens/{}", text(&made.body, "id")),
            Some(&secret),
        );
        assert_eq!(r.status, 409);
        assert!(r.text().contains("last API token"));
        assert_eq!(h.get("/api/health", None).status, 401);
        assert_eq!(h.get("/api/health", Some(&secret)).status, 200);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_the_env_token_makes_the_last_named_token_revocable,
    "tests/test_auth_runtime.py::test_the_env_token_makes_the_last_named_token_revocable",
    {
        let t = fresh();
        let h = Http::configured(t.path(), "env-secret", "", None);
        let made = h.post("/api/tokens", json!({"name":"CI"}), Some("env-secret"));
        assert_eq!(
            h.delete(
                &format!("/api/tokens/{}", text(&made.body, "id")),
                Some("env-secret")
            )
            .status,
            200
        );
        assert_eq!(h.get("/api/health", None).status, 401);
        assert_eq!(h.get("/api/health", Some("env-secret")).status, 200);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_a_revoked_token_stops_working_immediately,
    "tests/test_auth_runtime.py::test_a_revoked_token_stops_working_immediately",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let keep = h.post("/api/tokens", json!({"name":"keep"}), None).body;
        let secret = text(&keep, "token");
        let drop = h
            .post("/api/tokens", json!({"name":"drop"}), Some(&secret))
            .body;
        assert_eq!(
            h.delete(&format!("/api/tokens/{}", text(&drop, "id")), Some(&secret))
                .status,
            200
        );
        assert_eq!(
            h.get("/api/health", Some(&text(&drop, "token"))).status,
            401
        );
        assert_eq!(h.get("/api/health", Some(&secret)).status, 200);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_the_dashboard_itself_stays_reachable_when_the_api_is_locked,
    "tests/test_auth_runtime.py::test_the_dashboard_itself_stays_reachable_when_the_api_is_locked",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        token(root, "CI");
        assert!(matches!(h.get("/", None).status, 200 | 404));
        Ok(())
    }
);
source_test!(
    auth_runtime_test_a_locked_api_never_advertises_a_wildcard_origin,
    "tests/test_auth_runtime.py::test_a_locked_api_never_advertises_a_wildcard_origin",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        token(root, "CI");
        let r = h.raw(
            "OPTIONS",
            "/api/health",
            vec![],
            None,
            Some("https://untrusted.example"),
        );
        assert!(!r.headers.contains_key("access-control-allow-origin"));
        Ok(())
    }
);
source_test!(
    auth_runtime_test_the_default_is_same_origin_not_a_wildcard,
    "tests/test_auth_runtime.py::test_the_default_is_same_origin_not_a_wildcard",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let r = h.raw(
            "OPTIONS",
            "/api/health",
            vec![],
            None,
            Some("https://untrusted.example"),
        );
        assert!(!r.headers.contains_key("access-control-allow-origin"));
        Ok(())
    }
);
source_test!(
    auth_runtime_test_creating_the_first_token_cannot_leave_a_wildcard_behind,
    "tests/test_auth_runtime.py::test_creating_the_first_token_cannot_leave_a_wildcard_behind",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        h.post("/api/tokens", json!({"name":"CI"}), None);
        let r = h.raw(
            "OPTIONS",
            "/api/health",
            vec![],
            None,
            Some("https://untrusted.example"),
        );
        assert!(!r.headers.contains_key("access-control-allow-origin"));
        Ok(())
    }
);
source_test!(
    auth_runtime_test_an_explicit_origin_is_honoured,
    "tests/test_auth_runtime.py::test_an_explicit_origin_is_honoured",
    {
        let t = fresh();
        let h = Http::configured(t.path(), "", "http://localhost:5173", None);
        let r = h.raw(
            "GET",
            "/api/health",
            vec![],
            None,
            Some("http://localhost:5173"),
        );
        assert_eq!(
            r.headers["access-control-allow-origin"],
            "http://localhost:5173"
        );
        Ok(())
    }
);
source_test!(
    auth_runtime_test_ready_answers_while_the_api_is_locked,
    "tests/test_auth_runtime.py::test_ready_answers_while_the_api_is_locked",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        token(root, "CI");
        let r = h.get("/api/ready", None);
        assert_eq!(r.status, 200);
        assert_eq!(r.body["service"], "numera-browser");
        assert_eq!(r.body["authRequired"], true);
        assert_eq!(r.body["ok"], true);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_ready_says_when_no_token_is_needed,
    "tests/test_auth_runtime.py::test_ready_says_when_no_token_is_needed",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(h.get("/api/ready", None).body["authRequired"], false);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_ready_answers_with_an_env_token_too,
    "tests/test_auth_runtime.py::test_ready_answers_with_an_env_token_too",
    {
        let t = fresh();
        let h = Http::configured(t.path(), "secret", "", None);
        let r = h.get("/api/ready", None);
        assert_eq!(r.body["authRequired"], true);
        assert_eq!(r.body["ok"], true);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_ready_exposes_nothing_protected,
    "tests/test_auth_runtime.py::test_ready_exposes_nothing_protected",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        token(root, "CI");
        let body = h.get("/api/ready", None).body;
        assert_eq!(
            body.as_object()
                .expect("ready object")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            ["ok", "service", "version", "authRequired"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        Ok(())
    }
);
source_test!(
    auth_runtime_test_health_is_still_protected,
    "tests/test_auth_runtime.py::test_health_is_still_protected",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        token(root, "CI");
        assert_eq!(h.get("/api/health", None).status, 401);
        Ok(())
    }
);
source_test!(
    auth_runtime_test_a_non_loopback_bind_without_auth_is_refused_0_0_0_0_ea6e8a,
    "tests/test_auth_runtime.py::test_a_non_loopback_bind_without_auth_is_refused[0.0.0.0]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let refusal = numera_core::http::bind_policy::refusal("0.0.0.0", &s, "", "")
            .expect("refuse public bind");
        assert!(refusal.to_lowercase().contains("token"));
        Ok(())
    }
);
source_test!(
    auth_runtime_test_a_non_loopback_bind_without_auth_is_refused_d4f204,
    "tests/test_auth_runtime.py::test_a_non_loopback_bind_without_auth_is_refused[::]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let refusal =
            numera_core::http::bind_policy::refusal("::", &s, "", "").expect("refuse public bind");
        assert!(refusal.to_lowercase().contains("token"));
        Ok(())
    }
);
source_test!(
    auth_runtime_test_a_non_loopback_bind_without_auth_is_refused_192_168_1_10_200a5c,
    "tests/test_auth_runtime.py::test_a_non_loopback_bind_without_auth_is_refused[192.168.1.10]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        let refusal = numera_core::http::bind_policy::refusal("192.168.1.10", &s, "", "")
            .expect("refuse public bind");
        assert!(refusal.to_lowercase().contains("token"));
        Ok(())
    }
);
source_test!(
    auth_runtime_test_loopback_needs_no_token_127_0_0_1_1464d4,
    "tests/test_auth_runtime.py::test_loopback_needs_no_token[127.0.0.1]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert!(numera_core::http::bind_policy::refusal("127.0.0.1", &s, "", "").is_none());
        Ok(())
    }
);
source_test!(
    auth_runtime_test_loopback_needs_no_token_localhost_ddd46c,
    "tests/test_auth_runtime.py::test_loopback_needs_no_token[localhost]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert!(numera_core::http::bind_policy::refusal("localhost", &s, "", "").is_none());
        Ok(())
    }
);
source_test!(
    auth_runtime_test_loopback_needs_no_token_1_e50df1,
    "tests/test_auth_runtime.py::test_loopback_needs_no_token[::1]",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert!(numera_core::http::bind_policy::refusal("::1", &s, "", "").is_none());
        Ok(())
    }
);
source_test!(
    auth_runtime_test_a_non_loopback_bind_is_allowed_once_a_token_exists,
    "tests/test_auth_runtime.py::test_a_non_loopback_bind_is_allowed_once_a_token_exists",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        s.create("CI", 1000.0)?;
        assert!(numera_core::http::bind_policy::refusal("0.0.0.0", &s, "", "").is_none());
        Ok(())
    }
);
source_test!(
    auth_runtime_test_the_env_token_also_satisfies_the_bind_check,
    "tests/test_auth_runtime.py::test_the_env_token_also_satisfies_the_bind_check",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert!(numera_core::http::bind_policy::refusal("0.0.0.0", &s, "secret", "").is_none());
        Ok(())
    }
);
source_test!(
    auth_runtime_test_the_override_exists_but_has_to_be_asked_for,
    "tests/test_auth_runtime.py::test_the_override_exists_but_has_to_be_asked_for",
    {
        let t = fresh();
        let root = t.path();
        let s = TokenStore::new(root.to_owned());
        assert!(numera_core::http::bind_policy::refusal("0.0.0.0", &s, "", "1").is_none());
        assert!(numera_core::http::bind_policy::refusal("0.0.0.0", &s, "", "").is_some());
        Ok(())
    }
);
