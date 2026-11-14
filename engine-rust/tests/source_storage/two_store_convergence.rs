use super::*;
source_test!(
    a_dashboard_only_profile_is_listed,
    "tests/test_two_store_convergence.py::test_a_dashboard_only_profile_is_listed",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        fs::remove_file(root.join(format!("profiles/{id}.json"))).expect("remove one twin");
        let listed = store(root).list(1001.0).expect("list");
        assert_eq!(
            listed.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            [id.as_str()]
        );
        assert_eq!(listed[0].name, "alpha");
        assert!(root.join(format!("profiles/{id}.json")).is_file());
    }
);
source_test!(
    an_engine_only_profile_is_still_listed,
    "tests/test_two_store_convergence.py::test_an_engine_only_profile_is_still_listed",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        fs::remove_file(root.join(format!("dashboard/{id}.json"))).expect("remove one twin");
        let listed = store(root).list(1001.0).expect("list");
        assert_eq!(
            listed.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            [id.as_str()]
        );
        assert_eq!(listed[0].name, "alpha");
        assert!(root.join(format!("profiles/{id}.json")).is_file());
    }
);
source_test!(
    a_dashboard_only_profile_resolves_with_its_secret_readable,
    "tests/test_two_store_convergence.py::test_a_dashboard_only_profile_resolves_with_its_secret_readable",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        fs::remove_file(root.join(format!("profiles/{id}.json"))).expect("remove engine twin");
        let opened = ProfileRepo::new(root.to_owned(), Some("master".into()));
        for reference in [id.as_str(), "alpha"] {
            assert_eq!(
                opened
                    .resolve(reference, 1001.0)
                    .expect("resolve")
                    .expect("profile")
                    .proxy
                    .expect("proxy")
                    .to_record()["password"],
                "s3cret"
            );
        }
    }
);
source_test!(
    a_name_disagreement_converges_on_the_dashboard,
    "tests/test_two_store_convergence.py::test_a_name_disagreement_converges_on_the_dashboard",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        let mut d = raw(root, &format!("dashboard/{id}.json"));
        d["name"] = "beta".into();
        write(root, &format!("dashboard/{id}.json"), d);
        assert_eq!(
            store(root)
                .resolve(id, 1001.0)
                .expect("resolve")
                .expect("profile")
                .name,
            "beta"
        );
        assert_eq!(names(root), ["beta"]);
        assert_eq!(
            store(root)
                .resolve("beta", 1001.0)
                .expect("by name")
                .expect("profile")
                .name,
            "beta"
        );
        assert!(
            store(root)
                .resolve("alpha", 1001.0)
                .expect("old name")
                .is_none()
        );
        assert_eq!(raw(root, &format!("profiles/{id}.json"))["name"], "beta");
    }
);
source_test!(
    a_shared_field_disagreement_converges_too,
    "tests/test_two_store_convergence.py::test_a_shared_field_disagreement_converges_too",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        let mut d = raw(root, &format!("dashboard/{id}.json"));
        d["notes"] = "new".into();
        d["tags"] = json!(["y", "z"]);
        write(root, &format!("dashboard/{id}.json"), d);
        let got = store(root)
            .resolve(id, 1001.0)
            .expect("resolve")
            .expect("profile");
        assert_eq!(got.notes, "new");
        assert_eq!(got.tags, ["y", "z"]);
        assert_eq!(raw(root, &format!("profiles/{id}.json"))["notes"], "new");
    }
);
source_test!(
    a_healthy_profile_is_never_rewritten_by_a_listing,
    "tests/test_two_store_convergence.py::test_a_healthy_profile_is_never_rewritten_by_a_listing",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        let path = root.join(format!("profiles/{id}.json"));
        let before = fs::read(&path).expect("before");
        let stamp = fs::metadata(&path)
            .expect("metadata")
            .modified()
            .expect("mtime");
        store(root).list(1001.0).expect("list");
        store(root).resolve(id, 1001.0).expect("resolve");
        store(root).materialize(id, 1001.0).expect("materialize");
        assert_eq!(fs::read(&path).expect("after"), before);
        assert_eq!(
            fs::metadata(&path)
                .expect("metadata")
                .modified()
                .expect("mtime"),
            stamp
        );
        let record: Value = serde_json::from_slice(&before).expect("record");
        assert_ne!(
            record["fingerprint"]["viewport_width"], record["fingerprint"]["screen_width"],
            "fixture has viewport inset"
        );
    }
);
source_test!(
    a_repair_does_not_reset_a_field_only_the_engine_models,
    "tests/test_two_store_convergence.py::test_a_repair_does_not_reset_a_field_only_the_engine_models",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        let viewport =
            raw(root, &format!("profiles/{id}.json"))["fingerprint"]["viewport_width"].clone();
        let mut d = raw(root, &format!("dashboard/{id}.json"));
        d["name"] = "beta".into();
        write(root, &format!("dashboard/{id}.json"), d);
        store(root).resolve(id, 1001.0).expect("repair");
        let e = raw(root, &format!("profiles/{id}.json"));
        assert_eq!(e["name"], "beta");
        assert_eq!(e["fingerprint"]["viewport_width"], viewport);
    }
);
source_test!(
    two_different_proxy_passwords_are_reported_not_resolved,
    "tests/test_two_store_convergence.py::test_two_different_proxy_passwords_are_reported_not_resolved",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let error = store(root)
            .materialize(id, 1001.0)
            .err()
            .expect("divergence");
        assert!(matches!(
            error,
            numera_core::storage::repo::RepoError::Diverged(_)
        ));
        assert!(error.to_string().contains("different"));
        assert_eq!(
            store(root)
                .list(1001.0)
                .expect("still listable")
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            [id.as_str()]
        );
        assert_eq!(
            raw(root, &format!("profiles/{id}.json"))["proxy"]["password"],
            "engine"
        );
        assert_eq!(
            store(root).divergence(id, 1001.0).expect("divergence"),
            ["proxy.password"]
        );
    }
);
source_test!(
    a_locked_vault_keeps_both_ciphertexts_exactly,
    "tests/test_two_store_convergence.py::test_a_locked_vault_keeps_both_ciphertexts_exactly",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        let before = pair(root, id);
        let mut d = before.1.clone();
        d["name"] = "beta".into();
        write(root, &format!("dashboard/{id}.json"), d);
        assert_eq!(
            store(root)
                .resolve(id, 1001.0)
                .expect("locked repair")
                .expect("profile")
                .name,
            "beta"
        );
        let after = pair(root, id);
        assert_eq!(after.0["proxy"]["password"], before.0["proxy"]["password"]);
        assert_eq!(after.1["proxy"]["pass"], before.1["proxy"]["pass"]);
        let mut edited = after.1.clone();
        edited["name"] = "HTTP locked repair".into();
        write(root, &format!("dashboard/{id}.json"), edited);
        let (status, rows) = http(root, "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(rows[0]["name"], "HTTP locked repair");
        let http_after = pair(root, id);
        assert_eq!(http_after.0["name"], "HTTP locked repair");
        assert_eq!(
            http_after.0["proxy"]["password"],
            before.0["proxy"]["password"]
        );
        assert_eq!(http_after.1["proxy"]["pass"], before.1["proxy"]["pass"]);
        assert_eq!(
            ProfileRepo::new(root.to_owned(), Some("master".into()))
                .get(id)
                .expect("get")
                .expect("profile")
                .proxy
                .expect("proxy")
                .to_record()["password"],
            "s3cret"
        );
    }
);
source_test!(
    the_same_secret_encrypted_twice_is_not_a_divergence,
    "tests/test_two_store_convergence.py::test_the_same_secret_encrypted_twice_is_not_a_divergence",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        let before = pair(root, id);
        let opened = ProfileRepo::new(root.to_owned(), Some("master".into()));
        let descriptor = opened.vault().expect("vault").expect("descriptor");
        let mut d = before.1;
        d["name"] = "beta".into();
        d["proxy"]["pass"] = numera_core::vault::encrypt(
            "master",
            descriptor["salt"].as_str().expect("salt"),
            "s3cret",
        )
        .expect("fresh encryption")
        .into();
        assert_ne!(d["proxy"]["pass"], before.0["proxy"]["password"]);
        write(root, &format!("dashboard/{id}.json"), d);
        assert_eq!(
            opened
                .resolve(id, 1001.0)
                .expect("repair")
                .expect("profile")
                .name,
            "beta"
        );
        assert_eq!(
            opened
                .get(id)
                .expect("get")
                .expect("profile")
                .proxy
                .expect("proxy")
                .to_record()["password"],
            "s3cret"
        );
        let (engine_before, mut dashboard_before) = pair(root, id);
        let dashboard_secret = dashboard_before["proxy"]["pass"].clone();
        dashboard_before["name"] = "HTTP unlocked repair".into();
        write(root, &format!("dashboard/{id}.json"), dashboard_before);
        let (status, rows) = http_password(
            root,
            "GET",
            "/api/profiles",
            Value::Null,
            Some("master".into()),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(rows[0]["name"], "HTTP unlocked repair");
        assert_eq!(rows[0]["proxy"]["pass"], "s3cret");
        let http_after = pair(root, id);
        assert_eq!(http_after.0["name"], "HTTP unlocked repair");
        assert_eq!(
            http_after.0["proxy"]["password"],
            engine_before["proxy"]["password"]
        );
        assert_eq!(http_after.1["proxy"]["pass"], dashboard_secret);
    }
);
source_test!(
    the_api_listing_shows_the_reconciled_profile,
    "tests/test_two_store_convergence.py::test_the_api_listing_shows_the_reconciled_profile",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        let mut d = raw(root, &format!("dashboard/{id}.json"));
        d["name"] = "beta".into();
        write(root, &format!("dashboard/{id}.json"), d);
        let (status, rows) = http(root, "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(
            rows.as_array()
                .expect("rows")
                .iter()
                .map(|p| p["name"].as_str().expect("name"))
                .collect::<Vec<_>>(),
            ["beta"]
        );
        assert_eq!(raw(root, &format!("profiles/{id}.json"))["name"], "beta");
    }
);
source_test!(
    a_direct_launch_is_refused_before_the_browser_starts,
    "tests/test_two_store_convergence.py::test_a_direct_launch_is_refused_before_the_browser_starts",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let registry =
            Registry::new(root.to_owned(), "source-storage-host".into(), None).expect("registry");
        let error = registry
            .open_browser(id, "alpha", |_, _| {
                panic!("unresolved credential reached browser activation")
            })
            .expect_err("launch refused");
        assert_eq!(error.kind, "diverged");
        assert_eq!(
            numera_core::ownership::claims::state(
                &numera_core::ownership::claims::read(root, id),
                "source-storage-host"
            ),
            numera_core::ownership::claims::State::Free
        );
        assert!(!registry.running(id).expect("running"));
    }
);
source_test!(
    the_refusal_leaves_no_claim_behind,
    "tests/test_two_store_convergence.py::test_the_refusal_leaves_no_claim_behind",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let registry =
            Registry::new(root.to_owned(), "source-storage-host".into(), None).expect("registry");
        let error = registry
            .open_browser(id, "alpha", |_, _| {
                panic!("unresolved credential reached browser activation")
            })
            .expect_err("launch refused");
        assert_eq!(error.kind, "diverged");
        assert_eq!(
            numera_core::ownership::claims::state(
                &numera_core::ownership::claims::read(root, id),
                "source-storage-host"
            ),
            numera_core::ownership::claims::State::Free
        );
        assert!(!registry.running(id).expect("running"));
    }
);
source_test!(
    an_ordinary_edit_cannot_settle_the_conflict,
    "tests/test_two_store_convergence.py::test_an_ordinary_edit_cannot_settle_the_conflict",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let before = pair(root, id);
        assert!(matches!(
            store(root).edit(
                id,
                |p| {
                    p.notes = "an unrelated note".into();
                    Ok(None)
                },
                1001.0
            ),
            Err(numera_core::storage::repo::RepoError::Diverged(_))
        ));
        assert_eq!(pair(root, id), before);
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            ""
        );
    }
);
source_test!(
    a_dashboard_edit_cannot_settle_it_either,
    "tests/test_two_store_convergence.py::test_a_dashboard_edit_cannot_settle_it_either",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let (status, _body) = http(
            root,
            "PUT",
            &format!("/api/profiles/{id}"),
            json!({"notes":"hello"}),
        );
        assert_eq!(status, 409, "HTTP response status mismatch");
        assert_secrets(root, id, "engine", "dashboard");
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            ""
        );
    }
);
source_test!(
    replacing_the_password_resolves_it,
    "tests/test_two_store_convergence.py::test_replacing_the_password_resolves_it",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        store(root)
            .edit(
                id,
                |p| {
                    p.proxy
                        .as_mut()
                        .expect("proxy")
                        .set_password(Some("chosen".into()));
                    Ok(None)
                },
                1001.0,
            )
            .expect("explicit resolution");
        assert_secrets(root, id, "chosen", "chosen");
        assert!(
            store(root)
                .divergence(id, 1001.0)
                .expect("settled")
                .is_empty()
        );
    }
);
source_test!(
    removing_the_proxy_resolves_it_too,
    "tests/test_two_store_convergence.py::test_removing_the_proxy_resolves_it_too",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        store(root)
            .edit(
                id,
                |p| {
                    p.proxy = None;
                    Ok(None)
                },
                1001.0,
            )
            .expect("explicit resolution");
        let (a, b) = pair(root, id);
        assert!(a["proxy"].is_null());
        assert!(b["proxy"].is_null() || b["proxy"] == json!({}));
        assert!(
            store(root)
                .divergence(id, 1001.0)
                .expect("settled")
                .is_empty()
        );
    }
);
source_test!(
    a_name_lookup_never_answers_with_a_different_name,
    "tests/test_two_store_convergence.py::test_a_name_lookup_never_answers_with_a_different_name",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let mut d = raw(root, &format!("dashboard/{id}.json"));
        d["name"] = "beta".into();
        write(root, &format!("dashboard/{id}.json"), d);
        assert!(matches!(
            store(root).resolve("beta", 1001.0),
            Err(numera_core::storage::repo::RepoError::Diverged(_))
        ));
        assert_eq!(
            store(root)
                .resolve(id, 1001.0)
                .expect("ID read")
                .expect("profile")
                .name,
            "alpha"
        );
    }
);
source_test!(
    an_engine_only_profile_gets_its_twin,
    "tests/test_two_store_convergence.py::test_an_engine_only_profile_gets_its_twin",
    {
        let temp = temp();
        let root = temp.path();
        let mut p = profile("alpha", 1);
        p.notes = "kept".into();
        p.tags = vec!["t".into()];
        let p = store(root).save(p, 1000.0).expect("save");
        let id = &p.id;
        let engine = raw(root, &format!("profiles/{id}.json"));
        fs::remove_file(root.join(format!("dashboard/{id}.json"))).expect("remove twin");
        assert_eq!(
            store(root)
                .list(1001.0)
                .expect("adopt")
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            [id.as_str()]
        );
        let d = raw(root, &format!("dashboard/{id}.json"));
        assert_eq!(d["id"], id.as_str());
        assert_eq!(d["name"], "alpha");
        assert_eq!(d["notes"], "kept");
        assert_eq!(d["tags"], json!(["t"]));
        assert_eq!(raw(root, &format!("profiles/{id}.json")), engine);
        assert!(
            store(root)
                .divergence(id, 1001.0)
                .expect("settled")
                .is_empty()
        );
    }
);
source_test!(
    adopting_an_engine_record_keeps_its_secret_encrypted,
    "tests/test_two_store_convergence.py::test_adopting_an_engine_record_keeps_its_secret_encrypted",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        let before = pair(root, id);
        fs::remove_file(root.join(format!("dashboard/{id}.json"))).expect("remove dashboard");
        store(root).list(1001.0).expect("adopt");
        assert_eq!(
            raw(root, &format!("dashboard/{id}.json"))["proxy"]["pass"],
            before.0["proxy"]["password"]
        );
        assert_eq!(
            ProfileRepo::new(root.to_owned(), Some("master".into()))
                .get(id)
                .expect("get")
                .expect("profile")
                .proxy
                .expect("proxy")
                .to_record()["password"],
            "s3cret"
        );
    }
);
source_test!(
    resolving_an_engine_only_profile_repairs_it_too,
    "tests/test_two_store_convergence.py::test_resolving_an_engine_only_profile_repairs_it_too",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "alpha", 1);
        let id = &p.id;
        fs::remove_file(root.join(format!("dashboard/{id}.json"))).expect("remove twin");
        assert_eq!(
            store(root)
                .resolve("alpha", 1001.0)
                .expect("resolve")
                .expect("profile")
                .id,
            *id
        );
        assert!(root.join(format!("dashboard/{id}.json")).is_file());
    }
);
source_test!(
    a_secret_the_dashboard_has_lost_is_not_erased_by_an_unrelated_edit,
    "tests/test_two_store_convergence.py::test_a_secret_the_dashboard_has_lost_is_not_erased_by_an_unrelated_edit",
    {
        let temp = temp();
        let root = temp.path();
        let p = one_sided(root, Some("working-secret"), "");
        let id = &p.id;
        let before = pair(root, id);
        assert!(matches!(
            store(root).edit(
                id,
                |p| {
                    p.notes = "unrelated".into();
                    Ok(None)
                },
                1001.0
            ),
            Err(numera_core::storage::repo::RepoError::Diverged(_))
        ));
        assert_eq!(pair(root, id), before);
    }
);
source_test!(
    a_secret_only_the_dashboard_has_is_propagated_not_refused,
    "tests/test_two_store_convergence.py::test_a_secret_only_the_dashboard_has_is_propagated_not_refused",
    {
        let temp = temp();
        let root = temp.path();
        let p = one_sided(root, None, "working-secret");
        let id = &p.id;
        let before = pair(root, id);
        edit_note(root, id, "unrelated");
        assert_secrets(root, id, "working-secret", "working-secret");
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            "unrelated"
        );
        assert_ne!(pair(root, id), before);
    }
);
source_test!(
    a_dashboard_edit_cannot_settle_a_one_sided_secret_either,
    "tests/test_two_store_convergence.py::test_a_dashboard_edit_cannot_settle_a_one_sided_secret_either",
    {
        let temp = temp();
        let root = temp.path();
        let p = one_sided(root, Some("working-secret"), "");
        let id = &p.id;
        let before = pair(root, id);
        let (status, _body) = http(
            root,
            "PUT",
            &format!("/api/profiles/{id}"),
            json!({"notes":"hello"}),
        );
        assert_eq!(status, 409, "HTTP response status mismatch");
        assert_eq!(pair(root, id), before);
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            ""
        );
    }
);
source_test!(
    a_locked_edit_leaves_each_ciphertext_exactly_as_it_found_it,
    "tests/test_two_store_convergence.py::test_a_locked_edit_leaves_each_ciphertext_exactly_as_it_found_it",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        let descriptor = store(root).vault().expect("vault").expect("descriptor");
        let mut d = raw(root, &format!("dashboard/{id}.json"));
        d["proxy"]["pass"] = numera_core::vault::encrypt(
            "master",
            descriptor["salt"].as_str().expect("salt"),
            "second",
        )
        .expect("second secret")
        .into();
        write(root, &format!("dashboard/{id}.json"), d);
        let before = pair(root, id);
        edit_note(root, id, "while locked");
        let after = pair(root, id);
        assert_eq!(after.0["proxy"]["password"], before.0["proxy"]["password"]);
        assert_eq!(after.1["proxy"]["pass"], before.1["proxy"]["pass"]);
        let opened = ProfileRepo::new(root.to_owned(), Some("master".into()));
        assert_eq!(
            opened.divergence(id, 1001.0).expect("divergence"),
            ["proxy.password"]
        );
        assert!(matches!(
            opened.edit(
                id,
                |p| {
                    p.notes = "now".into();
                    Ok(None)
                },
                1002.0
            ),
            Err(numera_core::storage::repo::RepoError::Diverged(_))
        ));
    }
);
source_test!(
    a_password_can_be_cleared_while_the_proxy_stays,
    "tests/test_two_store_convergence.py::test_a_password_can_be_cleared_while_the_proxy_stays",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root)
            .edit(
                id,
                |p| {
                    p.proxy
                        .as_mut()
                        .expect("proxy")
                        .set_password(Some(String::new()));
                    Ok(None)
                },
                1001.0,
            )
            .expect("clear");
        let (e, d) = pair(root, id);
        assert!(e["proxy"]["password"].as_str().is_none_or(str::is_empty));
        assert!(d["proxy"]["pass"].as_str().is_none_or(str::is_empty));
        assert_eq!(e["proxy"]["server"], "http://1.2.3.4:8080");
        assert_eq!(e["proxy"]["username"], "u");
    }
);
source_test!(
    an_explicit_clear_resolves_a_real_conflict,
    "tests/test_two_store_convergence.py::test_an_explicit_clear_resolves_a_real_conflict",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        store(root)
            .edit(
                id,
                |p| {
                    p.proxy
                        .as_mut()
                        .expect("proxy")
                        .set_password(Some(String::new()));
                    Ok(None)
                },
                1001.0,
            )
            .expect("explicit resolution");
        let (a, b) = pair(root, id);
        assert!(a["proxy"]["password"].as_str().is_none_or(str::is_empty));
        assert!(b["proxy"]["pass"].as_str().is_none_or(str::is_empty));
        assert!(
            store(root)
                .divergence(id, 1001.0)
                .expect("settled")
                .is_empty()
        );
    }
);
source_test!(
    clearing_a_password_needs_an_unlocked_vault,
    "tests/test_two_store_convergence.py::test_clearing_a_password_needs_an_unlocked_vault",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        let before = raw(root, &format!("profiles/{id}.json"))["proxy"]["password"].clone();
        assert!(matches!(
            store(root).edit(
                id,
                |p| {
                    p.proxy
                        .as_mut()
                        .expect("proxy")
                        .set_password(Some(String::new()));
                    Ok(None)
                },
                1001.0
            ),
            Err(numera_core::storage::repo::RepoError::VaultLocked(_))
        ));
        assert_eq!(
            raw(root, &format!("profiles/{id}.json"))["proxy"]["password"],
            before
        );
        ProfileRepo::new(root.to_owned(), Some("master".into()))
            .edit(
                id,
                |p| {
                    p.proxy
                        .as_mut()
                        .expect("proxy")
                        .set_password(Some(String::new()));
                    Ok(None)
                },
                1002.0,
            )
            .expect("unlocked clear");
        assert!(
            raw(root, &format!("profiles/{id}.json"))["proxy"]["password"]
                .as_str()
                .is_none_or(str::is_empty)
        );
    }
);
source_test!(
    the_api_answers_a_divergence_with_409,
    "tests/test_two_store_convergence.py::test_the_api_answers_a_divergence_with_409",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        for (method, uri, body) in [
            ("PUT", format!("/api/profiles/{id}"), json!({"notes":"x"})),
            (
                "POST",
                "/api/profiles/bulk".into(),
                json!({"ids":[id],"patch":{"notes":"x"}}),
            ),
        ] {
            let (status, answer) = http(root, method, &uri, body);
            assert_eq!(status, 409, "HTTP response status mismatch");
            assert!(
                answer["detail"]
                    .as_str()
                    .expect("detail")
                    .contains("proxy password")
            );
        }
        assert_secrets(root, id, "engine", "dashboard");
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            ""
        );
    }
);
source_test!(
    the_api_answers_a_locked_clear_with_423,
    "tests/test_two_store_convergence.py::test_the_api_answers_a_locked_clear_with_423",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        let before = pair(root, id);
        let (status, _body) = http(
            root,
            "PUT",
            &format!("/api/profiles/{id}"),
            json!({"proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":""}}),
        );
        assert_eq!(status, 423, "HTTP response status mismatch");
        assert_eq!(pair(root, id), before);
    }
);
source_test!(
    a_bulk_edit_cannot_settle_a_conflict_and_stays_atomic,
    "tests/test_two_store_convergence.py::test_a_bulk_edit_cannot_settle_a_conflict_and_stays_atomic",
    {
        let temp = temp();
        let root = temp.path();
        let healthy = saved(root, "healthy", 3);
        let p = diverged(root, "alpha", 1);
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[healthy.id,p.id],"patch":{"tags":["t"]}}),
        );
        assert_eq!(status, 409, "HTTP response status mismatch");
        assert!(
            store(root)
                .get(&healthy.id)
                .expect("get healthy")
                .expect("healthy")
                .tags
                .is_empty()
        );
        assert_secrets(root, &p.id, "engine", "dashboard");
    }
);
source_test!(
    a_locked_bulk_edit_leaves_each_ciphertext_alone,
    "tests/test_two_store_convergence.py::test_a_locked_bulk_edit_leaves_each_ciphertext_alone",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        store(root).enable_vault("master").expect("vault");
        let descriptor = store(root).vault().expect("vault").expect("descriptor");
        let mut d = raw(root, &format!("dashboard/{id}.json"));
        d["proxy"]["pass"] = numera_core::vault::encrypt(
            "master",
            descriptor["salt"].as_str().expect("salt"),
            "second",
        )
        .expect("second secret")
        .into();
        write(root, &format!("dashboard/{id}.json"), d);
        let before = pair(root, id);
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[id],"patch":{"notes":"bulk"}}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let after = pair(root, id);
        assert_eq!(after.0["proxy"]["password"], before.0["proxy"]["password"]);
        assert_eq!(after.1["proxy"]["pass"], before.1["proxy"]["pass"]);
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            "bulk"
        );
    }
);
source_test!(
    a_bulk_edit_cannot_erase_a_one_sided_secret_either,
    "tests/test_two_store_convergence.py::test_a_bulk_edit_cannot_erase_a_one_sided_secret_either",
    {
        let temp = temp();
        let root = temp.path();
        let p = one_sided(root, Some("working-secret"), "");
        let id = &p.id;
        let before = pair(root, id);
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[id],"patch":{"notes":"bulk"}}),
        );
        assert_eq!(status, 409, "HTTP response status mismatch");
        assert_eq!(pair(root, id), before);
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            ""
        );
    }
);
source_test!(
    a_healthy_bulk_edit_still_works,
    "tests/test_two_store_convergence.py::test_a_healthy_bulk_edit_still_works",
    {
        let temp = temp();
        let root = temp.path();
        let a = saved(root, "a", 1);
        let b = saved(root, "b", 2);
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/bulk",
            json!({"ids":[a.id,b.id],"patch":{"tags":["ok"]}}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        for id in [&a.id, &b.id] {
            assert_eq!(
                store(root).get(id).expect("get").expect("profile").tags,
                ["ok"]
            );
        }
    }
);
source_test!(
    either_side_of_a_conflict_can_be_chosen_explicitly,
    "tests/test_two_store_convergence.py::test_either_side_of_a_conflict_can_be_chosen_explicitly",
    {
        let temp = temp();
        let root = temp.path();
        for (i, chosen) in ["engine", "dashboard"].into_iter().enumerate() {
            let p = diverged(root, &format!("pick-{chosen}"), i as i64 + 1);
            store(root)
                .edit(
                    &p.id,
                    |p| {
                        p.proxy
                            .as_mut()
                            .expect("proxy")
                            .set_password(Some(chosen.into()));
                        Ok(None)
                    },
                    1001.0,
                )
                .expect("choose side");
            assert_secrets(root, &p.id, chosen, chosen);
            assert!(
                store(root)
                    .divergence(&p.id, 1001.0)
                    .expect("settled")
                    .is_empty()
            );
        }
    }
);
source_test!(
    the_shape_the_dashboard_sends_preserves_the_secret,
    "tests/test_two_store_convergence.py::test_the_shape_the_dashboard_sends_preserves_the_secret",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let rows = http(root, "GET", "/api/profiles", Value::Null).1;
        let mut edited = rows
            .as_array()
            .expect("rows")
            .iter()
            .find(|p| p["id"] == id.as_str())
            .expect("loaded")
            .clone();
        assert_eq!(edited["proxy"]["pass"], "dashboard");
        edited["notes"] = "just a note".into();
        edited["proxy"]
            .as_object_mut()
            .expect("proxy")
            .remove("pass");
        let (status, _body) = http(root, "PUT", &format!("/api/profiles/{id}"), edited);
        assert_eq!(status, 409, "HTTP response status mismatch");
        assert_secrets(root, id, "engine", "dashboard");
        assert_eq!(
            store(root).get(id).expect("get").expect("profile").notes,
            ""
        );
    }
);
source_test!(
    the_dashboards_other_proxy_edits_still_go_through,
    "tests/test_two_store_convergence.py::test_the_dashboards_other_proxy_edits_still_go_through",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "alpha", 1, Some("s3cret"));
        let id = &p.id;
        let rows = http(root, "GET", "/api/profiles", Value::Null).1;
        let mut edited = rows[0].clone();
        edited["proxy"]["host"] = "moved.example".into();
        edited["proxy"]
            .as_object_mut()
            .expect("proxy")
            .remove("pass");
        let (status, _body) = http(root, "PUT", &format!("/api/profiles/{id}"), edited);
        assert_eq!(status, 200, "HTTP response status mismatch");
        let after = store(root)
            .get(id)
            .expect("get")
            .expect("profile")
            .proxy
            .expect("proxy")
            .to_record();
        assert!(
            after["server"]
                .as_str()
                .expect("server")
                .contains("moved.example")
        );
        assert_eq!(after["password"], "s3cret");
    }
);
source_test!(
    the_api_can_choose_a_side_too,
    "tests/test_two_store_convergence.py::test_the_api_can_choose_a_side_too",
    {
        let temp = temp();
        let root = temp.path();
        let p = diverged(root, "alpha", 1);
        let id = &p.id;
        let (status, _body) = http(
            root,
            "PUT",
            &format!("/api/profiles/{id}"),
            json!({"proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"dashboard"}}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_secrets(root, id, "dashboard", "dashboard");
    }
);
