use super::*;
source_test!(
    creation_default_engine_test_a_minimal_create_uses_the_saved_default_camoufox_ea5121,
    "tests/test_creation_default_engine.py::test_a_minimal_create_uses_the_saved_default[camoufox]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        let made = h.post("/api/profiles", json!({"name":"minimal"}), None);
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(made.body["engine"], "camoufox");
        let id = text(&made.body, "id");
        for side in ["dashboard", "profiles"] {
            assert_eq!(
                read(root.join(side).join(format!("{id}.json")))["engine"],
                "camoufox"
            );
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_minimal_create_uses_the_saved_default_patchright_e36e57,
    "tests/test_creation_default_engine.py::test_a_minimal_create_uses_the_saved_default[patchright]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("patchright"))?;
        let made = h.post("/api/profiles", json!({"name":"minimal"}), None);
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(made.body["engine"], "patchright");
        let id = text(&made.body, "id");
        for side in ["dashboard", "profiles"] {
            assert_eq!(
                read(root.join(side).join(format!("{id}.json")))["engine"],
                "patchright"
            );
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_minimal_create_uses_the_saved_default_cloak_145575,
    "tests/test_creation_default_engine.py::test_a_minimal_create_uses_the_saved_default[cloak]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("cloak"))?;
        let made = h.post("/api/profiles", json!({"name":"minimal"}), None);
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(made.body["engine"], "cloak");
        let id = text(&made.body, "id");
        for side in ["dashboard", "profiles"] {
            assert_eq!(
                read(root.join(side).join(format!("{id}.json")))["engine"],
                "cloak"
            );
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_an_explicit_null_engine_is_the_same_as_omitting_it_camoufox_9f03e4,
    "tests/test_creation_default_engine.py::test_an_explicit_null_engine_is_the_same_as_omitting_it[camoufox]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        let made = h.post(
            "/api/profiles",
            json!({"name":"minimal","engine":null}),
            None,
        );
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(made.body["engine"], "camoufox");
        let id = text(&made.body, "id");
        for side in ["dashboard", "profiles"] {
            assert_eq!(
                read(root.join(side).join(format!("{id}.json")))["engine"],
                "camoufox"
            );
        }
        Ok(())
    }
);
source_test!(creation_default_engine_test_an_explicit_null_engine_is_the_same_as_omitting_it_patchright_3dfda0,"tests/test_creation_default_engine.py::test_an_explicit_null_engine_is_the_same_as_omitting_it[patchright]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let cfg=ConfigStore::new(root.to_owned());cfg.set_default_engine(json!("patchright"))?;let made=h.post("/api/profiles",json!({"name":"minimal","engine":null}),None);assert_eq!(made.status,200,"{}",made.text());assert_eq!(made.body["engine"],"patchright");let id=text(&made.body,"id");for side in ["dashboard","profiles"]{assert_eq!(read(root.join(side).join(format!("{id}.json")))["engine"],"patchright");}
Ok(())
});
source_test!(
    creation_default_engine_test_an_explicit_null_engine_is_the_same_as_omitting_it_cloak_a70bf3,
    "tests/test_creation_default_engine.py::test_an_explicit_null_engine_is_the_same_as_omitting_it[cloak]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("cloak"))?;
        let made = h.post(
            "/api/profiles",
            json!({"name":"minimal","engine":null}),
            None,
        );
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(made.body["engine"], "cloak");
        let id = text(&made.body, "id");
        for side in ["dashboard", "profiles"] {
            assert_eq!(
                read(root.join(side).join(format!("{id}.json")))["engine"],
                "cloak"
            );
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_minimal_batch_entry_uses_the_saved_default_camoufox_6e0a25,
    "tests/test_creation_default_engine.py::test_a_minimal_batch_entry_uses_the_saved_default[camoufox]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        let r = h.post(
            "/api/profiles/batch",
            json!({"profiles":[{"name":"one"},{"name":"two"}]}),
            None,
        );
        assert_eq!(r.status, 200, "{}", r.text());
        for id in r.body["ids"].as_array().expect("ids") {
            let id = id.as_str().expect("id");
            for side in ["dashboard", "profiles"] {
                assert_eq!(
                    read(root.join(side).join(format!("{id}.json")))["engine"],
                    "camoufox"
                );
            }
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_minimal_batch_entry_uses_the_saved_default_patchright_96436e,
    "tests/test_creation_default_engine.py::test_a_minimal_batch_entry_uses_the_saved_default[patchright]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("patchright"))?;
        let r = h.post(
            "/api/profiles/batch",
            json!({"profiles":[{"name":"one"},{"name":"two"}]}),
            None,
        );
        assert_eq!(r.status, 200, "{}", r.text());
        for id in r.body["ids"].as_array().expect("ids") {
            let id = id.as_str().expect("id");
            for side in ["dashboard", "profiles"] {
                assert_eq!(
                    read(root.join(side).join(format!("{id}.json")))["engine"],
                    "patchright"
                );
            }
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_minimal_batch_entry_uses_the_saved_default_cloak_093f7e,
    "tests/test_creation_default_engine.py::test_a_minimal_batch_entry_uses_the_saved_default[cloak]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("cloak"))?;
        let r = h.post(
            "/api/profiles/batch",
            json!({"profiles":[{"name":"one"},{"name":"two"}]}),
            None,
        );
        assert_eq!(r.status, 200, "{}", r.text());
        for id in r.body["ids"].as_array().expect("ids") {
            let id = id.as_str().expect("id");
            for side in ["dashboard", "profiles"] {
                assert_eq!(
                    read(root.join(side).join(format!("{id}.json")))["engine"],
                    "cloak"
                );
            }
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_mixed_batch_keeps_each_entry_s_own_answer,
    "tests/test_creation_default_engine.py::test_a_mixed_batch_keeps_each_entry_s_own_answer",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"implicit-first"},{"name":"explicit","engine":"playwright"},{"name":"implicit-last"}]}),None);
        assert_eq!(r.status, 200);
        for row in h.get("/api/profiles", None).body.as_array().expect("rows") {
            assert_eq!(
                row["engine"],
                if row["name"] == "explicit" {
                    "playwright"
                } else {
                    "camoufox"
                }
            );
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_an_explicit_engine_still_wins_over_the_default,
    "tests/test_creation_default_engine.py::test_an_explicit_engine_still_wins_over_the_default",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        let p = h.create("a");
        assert_eq!(p["engine"], "playwright");
        assert_eq!(
            read(
                root.join("dashboard")
                    .join(format!("{}.json", text(&p, "id")))
            )["engine"],
            "playwright"
        );
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_create_that_names_no_engine_leaves_the_default_alone,
    "tests/test_creation_default_engine.py::test_a_create_that_names_no_engine_leaves_the_default_alone",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.post("/api/profiles", json!({"name":"minimal"}), None)
                .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "camoufox");
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_create_that_names_an_engine_still_updates_the_default,
    "tests/test_creation_default_engine.py::test_a_create_that_names_an_engine_still_updates_the_default",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        h.create("a");
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "playwright");
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_batch_that_names_no_engine_leaves_the_default_alone,
    "tests/test_creation_default_engine.py::test_a_batch_that_names_no_engine_leaves_the_default_alone",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.post(
                "/api/profiles/batch",
                json!({"profiles":[{"name":"one"},{"name":"two"}]}),
                None
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "camoufox");
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_a_rename_does_not_move_an_existing_profile_to_the_default,
    "tests/test_creation_default_engine.py::test_a_rename_does_not_move_an_existing_profile_to_the_default",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let p = h.create("a");
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        let r = h.put(
            &format!("/api/profiles/{}", text(&p, "id")),
            json!({"name":"renamed"}),
        );
        assert_eq!(r.status, 200);
        assert_eq!(r.body["engine"], "playwright");
        assert_eq!(
            read(
                root.join("dashboard")
                    .join(format!("{}.json", text(&p, "id")))
            )["engine"],
            "playwright"
        );
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_an_unrelated_bulk_edit_does_not_re_engine_the_selection,
    "tests/test_creation_default_engine.py::test_an_unrelated_bulk_edit_does_not_re_engine_the_selection",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let a = h.create("a");
        let b = h.create("b");
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        let r = h.post(
            "/api/profiles/bulk",
            json!({"ids":[a["id"],b["id"]],"patch":{"addTags":["x"]}}),
            None,
        );
        assert_eq!(r.status, 200);
        for row in r.body.as_array().expect("rows") {
            assert_eq!(row["engine"], "playwright");
        }
        Ok(())
    }
);
source_test!(
    creation_default_engine_test_an_update_that_names_an_engine_still_changes_it,
    "tests/test_creation_default_engine.py::test_an_update_that_names_an_engine_still_changes_it",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let p = h.create("a");
        let r = h.put(
            &format!("/api/profiles/{}", text(&p, "id")),
            json!({"engine":"camoufox"}),
        );
        assert_eq!(r.status, 200);
        assert_eq!(
            read(
                root.join("dashboard")
                    .join(format!("{}.json", text(&p, "id")))
            )["engine"],
            "camoufox"
        );
        Ok(())
    }
);
source_test!(
    default_engine_test_renaming_a_profile_does_not_change_the_default,
    "tests/test_default_engine.py::test_renaming_a_profile_does_not_change_the_default",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let p = h.create("a");
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.put(
                &format!("/api/profiles/{}", text(&p, "id")),
                json!({"name":"renamed"})
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "camoufox");
        Ok(())
    }
);
source_test!(
    default_engine_test_an_explicit_engine_change_does_update_the_default,
    "tests/test_default_engine.py::test_an_explicit_engine_change_does_update_the_default",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let p = h
            .post(
                "/api/profiles",
                json!({"name":"a","engine":"camoufox"}),
                None,
            )
            .body;
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.put(
                &format!("/api/profiles/{}", text(&p, "id")),
                json!({"name":"a","engine":"playwright"})
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "playwright");
        Ok(())
    }
);
source_test!(
    default_engine_test_resaving_the_same_engine_is_not_a_change,
    "tests/test_default_engine.py::test_resaving_the_same_engine_is_not_a_change",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let p = h.create("a");
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.put(
                &format!("/api/profiles/{}", text(&p, "id")),
                json!({"name":"a","engine":"playwright"})
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "camoufox");
        Ok(())
    }
);
source_test!(
    default_engine_test_a_tag_only_bulk_edit_does_not_change_the_default,
    "tests/test_default_engine.py::test_a_tag_only_bulk_edit_does_not_change_the_default",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let a = h.create("a");
        let b = h.create("b");
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.post(
                "/api/profiles/bulk",
                json!({"ids":[a["id"],b["id"]],"patch":{"addTags":["x"]}}),
                None
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "camoufox");
        Ok(())
    }
);
source_test!(
    default_engine_test_a_bulk_patch_that_names_an_engine_still_updates_it,
    "tests/test_default_engine.py::test_a_bulk_patch_that_names_an_engine_still_updates_it",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let a = h.create("a");
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.post(
                "/api/profiles/bulk",
                json!({"ids":[a["id"]],"patch":{"engine":"playwright"}}),
                None
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "playwright");
        Ok(())
    }
);
source_test!(
    default_engine_test_an_auto_filled_batch_entry_cannot_override_an_explicit_one,
    "tests/test_default_engine.py::test_an_auto_filled_batch_entry_cannot_override_an_explicit_one",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.post(
                "/api/profiles/batch",
                json!({"profiles":[{"name":"one","engine":"playwright"},{"name":"two"}]}),
                None
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "playwright");
        Ok(())
    }
);
source_test!(
    default_engine_test_a_batch_that_names_no_engine_leaves_the_default_alone,
    "tests/test_default_engine.py::test_a_batch_that_names_no_engine_leaves_the_default_alone",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = ConfigStore::new(root.to_owned());
        cfg.set_default_engine(json!("camoufox"))?;
        assert_eq!(
            h.post(
                "/api/profiles/batch",
                json!({"profiles":[{"name":"one"},{"name":"two"}]}),
                None
            )
            .status,
            200
        );
        assert_eq!(cfg.default_engine(&BTreeMap::new())?, "camoufox");
        Ok(())
    }
);
source_test!(
    server_test_converts_os_and_screen,
    "tests/test_server.py::test_converts_os_and_screen",
    {
        let base = json!({"id":"abc123","name":"Test","os":"Windows","screen":"1920x1080","cores":8,"memory":16,"webglRenderer":"ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)","locale":"en-US","timezone":"America/New_York","browserVersion":"128.0.0.0","userAgent":"Mozilla/5.0 test"});
        let fp = conversion::to_engine(&base, 1000.0)?.fingerprint;
        assert_eq!(fp.os, "windows");
        assert_eq!((fp.screen_width, fp.screen_height), (1920, 1080));
        assert!(fp.viewport_height < fp.screen_height);
        Ok(())
    }
);
source_test!(
    server_test_maps_gpu_to_real_engine_string,
    "tests/test_server.py::test_maps_gpu_to_real_engine_string",
    {
        let base = json!({"id":"abc123","name":"Test","os":"Windows","screen":"1920x1080","cores":8,"memory":16,"webglRenderer":"ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)","locale":"en-US","timezone":"America/New_York","browserVersion":"128.0.0.0","userAgent":"Mozilla/5.0 test"});
        let fp = conversion::to_engine(&base, 1000.0)?.fingerprint;
        assert!(fp.webgl_renderer.contains("RTX 3060"));
        assert!(fp.webgl_vendor.starts_with("Google Inc."));
        Ok(())
    }
);
source_test!(
    server_test_converted_profile_is_coherent,
    "tests/test_server.py::test_converted_profile_is_coherent",
    {
        let base = json!({"id":"abc123","name":"Test","os":"Windows","screen":"1920x1080","cores":8,"memory":16,"webglRenderer":"ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)","locale":"en-US","timezone":"America/New_York","browserVersion":"128.0.0.0","userAgent":"Mozilla/5.0 test"});
        assert!(
            numera_core::identity::validate(&conversion::to_engine(&base, 1000.0)?.fingerprint)?
                .is_empty()
        );
        assert!(conversion::dashboard_coherence(&base, 1000.0).is_empty());
        Ok(())
    }
);
source_test!(
    server_test_proxy_is_converted,
    "tests/test_server.py::test_proxy_is_converted",
    {
        let base = json!({"id":"abc123","name":"Test","os":"Windows","screen":"1920x1080","cores":8,"memory":16,"webglRenderer":"ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)","locale":"en-US","timezone":"America/New_York","browserVersion":"128.0.0.0","userAgent":"Mozilla/5.0 test"});
        let mut data = base;
        data["proxy"] = json!({"type":"SOCKS5","host":"h.proxy.io","port":"1080","user":"u","pass":"p","country":"US"});
        let p = conversion::to_engine(&data, 1000.0)?.proxy.expect("proxy");
        assert_eq!(p.server, "socks5://h.proxy.io:1080");
        assert_eq!(p.username.as_deref(), Some("u"));
        Ok(())
    }
);
source_test!(
    server_test_validate_tolerates_missing_id,
    "tests/test_server.py::test_validate_tolerates_missing_id",
    {
        let base = json!({"id":"abc123","name":"Test","os":"Windows","screen":"1920x1080","cores":8,"memory":16,"webglRenderer":"ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)","locale":"en-US","timezone":"America/New_York","browserVersion":"128.0.0.0","userAgent":"Mozilla/5.0 test"});
        let mut data = base;
        data.as_object_mut().expect("object").remove("id");
        let issues = conversion::dashboard_coherence(&data, 1000.0);
        assert!(issues.is_empty());
        Ok(())
    }
);
source_test!(
    server_api_test_no_token_configured_keeps_the_api_open,
    "tests/test_server_api.py::test_no_token_configured_keeps_the_api_open",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(h.get("/api/config", None).status, 200);
        Ok(())
    }
);
source_test!(
    server_api_test_token_is_required_when_configured,
    "tests/test_server_api.py::test_token_is_required_when_configured",
    {
        let t = fresh();
        let h = Http::configured(t.path(), "s3cret", "", None);
        assert_eq!(h.get("/api/config", None).status, 401);
        assert_eq!(h.get("/api/config", Some("wrong")).status, 401);
        assert_eq!(h.get("/api/config", Some("s3cret")).status, 200);
        let request = Request::builder()
            .uri("/api/config")
            .header("X-Numera-Token", "s3cret")
            .body(Body::empty())?;
        let response = h.runtime.block_on(h.app.clone().oneshot(request))?;
        assert_eq!(response.status().as_u16(), 200);
        Ok(())
    }
);
source_test!(
    server_api_test_config_round_trips_the_new_settings,
    "tests/test_server_api.py::test_config_round_trips_the_new_settings",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let extension = root.join("ublock");
        fs::create_dir(&extension)?;
        fs::write(
            extension.join("manifest.json"),
            r#"{"manifest_version":3,"name":"u","version":"1.0"}"#,
        )?;
        let location = extension.to_string_lossy().into_owned();
        let r = h.put(
            "/api/config",
            json!({"default_extensions":[location],"headless_launch":true}),
        );
        assert_eq!(r.status, 200, "{}", r.text());
        assert_eq!(r.body["default_extensions"], json!([location]));
        assert_eq!(r.body["headless_launch"], true);
        assert_eq!(
            ConfigStore::new(root.to_owned()).default_extensions()?,
            vec![json!(location)]
        );
        Ok(())
    }
);
source_test!(
    server_api_test_created_profiles_inherit_default_extensions,
    "tests/test_server_api.py::test_created_profiles_inherit_default_extensions",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let extension = root.join("ublock");
        fs::create_dir(&extension)?;
        fs::write(
            extension.join("manifest.json"),
            r#"{"manifest_version":3,"name":"u","version":"1.0"}"#,
        )?;
        let location = extension.to_string_lossy().into_owned();
        assert_eq!(
            h.put("/api/config", json!({"default_extensions":[location]}))
                .status,
            200
        );
        let made = h.post(
            "/api/profiles",
            json!({"name":"inherits","os":"Windows"}),
            None,
        );
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(made.body["extensions"], location);
        Ok(())
    }
);
source_test!(
    server_api_test_an_explicit_extension_choice_wins,
    "tests/test_server_api.py::test_an_explicit_extension_choice_wins",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let extension = root.join("ublock");
        fs::create_dir(&extension)?;
        fs::write(
            extension.join("manifest.json"),
            r#"{"manifest_version":3,"name":"u","version":"1.0"}"#,
        )?;
        let location = extension.to_string_lossy().into_owned();
        assert_eq!(
            h.put("/api/config", json!({"default_extensions":[location]}))
                .status,
            200
        );
        let made = h.post(
            "/api/profiles",
            json!({"name":"explicit","os":"Windows","extensions":"/opt/ext/other"}),
            None,
        );
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(made.body["extensions"], "/opt/ext/other");
        Ok(())
    }
);
source_test!(
    server_health_test_schedule_crud,
    "tests/test_server_health.py::test_schedule_crud",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let p=h.post("/api/profiles",json!({"name":"Acct","os":"Windows","screen":"1920x1080","cores":8,"memory":16,"locale":"en-US","engine":"cloak"}),None);
        assert_eq!(p.status, 200, "{}", p.text());
        let id = text(&p.body, "id");
        let r = h.post(
            "/api/schedules",
            json!({"profileId":id,"everyHours":12,"minutes":6,"preset":"ads"}),
            None,
        );
        assert_eq!(r.status, 200, "{}", r.text());
        let sid = text(&r.body, "id");
        assert_eq!(r.body["profileName"], "Acct");
        assert_eq!(r.body["due"], true);
        assert!(
            h.get("/api/schedules", None)
                .body
                .as_array()
                .expect("schedule rows")
                .iter()
                .any(|s| s["id"] == sid)
        );
        let r = h.put(
            &format!("/api/schedules/{sid}"),
            json!({"enabled":false,"preset":"crypto"}),
        );
        assert_eq!(r.body["enabled"], false);
        assert_eq!(r.body["preset"], "crypto");
        assert_eq!(
            h.delete(&format!("/api/schedules/{sid}"), None).body["ok"],
            true
        );
        assert_eq!(h.get("/api/schedules", None).body, json!([]));
        Ok(())
    }
);
source_test!(
    server_health_test_schedule_rejects_unknown_profile,
    "tests/test_server_health.py::test_schedule_rejects_unknown_profile",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(
            h.post("/api/schedules", json!({"profileId":"nope"}), None)
                .status,
            404
        );
        Ok(())
    }
);
source_test!(
    server_health_test_health_overview,
    "tests/test_server_health.py::test_health_overview",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let one = h.create("One");
        h.create("Two");
        assert_eq!(
            h.post(
                "/api/schedules",
                json!({"profileId":one["id"],"everyHours":8}),
                None
            )
            .status,
            200
        );
        let r = h.get("/api/profiles/health", None);
        assert_eq!(r.status, 200);
        assert_eq!(r.body["summary"]["total"], 2);
        assert_eq!(r.body["summary"]["scheduled"], 1);
        let rows = r.body["profiles"].as_array().expect("health rows");
        let a = rows.iter().find(|r| r["name"] == "One").expect("one");
        let b = rows.iter().find(|r| r["name"] == "Two").expect("two");
        assert_eq!(a["schedule"]["everyHours"].as_f64(), Some(8.0));
        assert!(b["schedule"].is_null());
        assert_eq!(a["coherent"], true);
        assert!(a["trust"].is_null());
        assert_eq!(a["proxy"]["set"], false);
        Ok(())
    }
);
source_test!(
    server_health_test_attach_rejects_non_chromium_engine,
    "tests/test_server_health.py::test_attach_rejects_non_chromium_engine",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let p = h
            .post(
                "/api/profiles",
                json!({"name":"Fox","engine":"camoufox"}),
                None,
            )
            .body;
        let r = h.post(
            &format!("/api/profiles/{}/attach", text(&p, "id")),
            Value::Null,
            None,
        );
        assert_eq!(r.status, 400, "{}", r.text());
        assert!(r.text().contains("DevTools"));
        Ok(())
    }
);
source_test!(
    server_health_test_attach_unknown_profile_404,
    "tests/test_server_health.py::test_attach_unknown_profile_404",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(
            h.post("/api/profiles/nope/attach", Value::Null, None)
                .status,
            404
        );
        Ok(())
    }
);
source_test!(
    server_health_test_collisions_endpoint_and_health_flag,
    "tests/test_server_health.py::test_collisions_endpoint_and_health_flag",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        h.create("Twin-A");
        h.create("Twin-B");
        let solo = h.post(
            "/api/profiles",
            json!({"name":"Solo","os":"macOS","engine":"playwright"}),
            None,
        );
        assert_eq!(solo.status, 200);
        let r = h.get("/api/profiles/collisions", None);
        assert_eq!(r.body["total"], 3);
        assert_eq!(r.body["groups"].as_array().expect("groups").len(), 1);
        let names = r.body["groups"][0]["members"]
            .as_array()
            .expect("members")
            .iter()
            .map(|r| text(r, "name"))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            ["Twin-A", "Twin-B"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        let health = h.get("/api/profiles/health", None).body;
        assert_eq!(health["summary"]["colliding"], 2);
        for row in health["profiles"].as_array().expect("rows") {
            if row["name"] == "Solo" {
                assert_eq!(row["collides"], false);
            } else {
                assert_eq!(row["collides"], true);
                assert_eq!(row["needsAttention"], true);
            }
        }
        Ok(())
    }
);
source_test!(
    server_health_test_config_exposes_folders_root_and_version,
    "tests/test_server_health.py::test_config_exposes_folders_root_and_version",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let cfg = h.get("/api/config", None).body;
        assert!(cfg.get("default_engine").is_some());
        assert!(cfg["folders"].is_array());
        assert!(!text(&cfg, "root").is_empty());
        assert!(!text(&cfg, "version").is_empty());
        let r = h.post(
            "/api/profiles",
            json!({"name":"InFolder","folder":"Crypto"}),
            None,
        );
        assert_eq!(r.status, 200);
        assert!(
            h.get("/api/config", None).body["folders"]
                .as_array()
                .expect("folders")
                .contains(&json!("Crypto"))
        );
        Ok(())
    }
);
source_test!(
    server_health_test_folder_create_rename_delete,
    "tests/test_server_health.py::test_folder_create_rename_delete",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert!(
            h.post("/api/folders", json!({"name":"Ads"}), None).body["folders"]
                .as_array()
                .expect("folders")
                .contains(&json!("Ads"))
        );
        assert_eq!(h.post("/api/folders", json!({"name":""}), None).status, 400);
        let p = h
            .post(
                "/api/profiles",
                json!({"name":"Store","folder":"Ads"}),
                None,
            )
            .body;
        let renamed = h.post(
            "/api/folders/rename",
            json!({"from":"Ads","to":"Marketing"}),
            None,
        );
        assert_eq!(renamed.status, 200);
        assert!(
            renamed.body["folders"]
                .as_array()
                .expect("folders")
                .contains(&json!("Marketing"))
        );
        assert_eq!(h.get("/api/profiles", None).body[0]["folder"], "Marketing");
        let deleted = h.post("/api/folders/delete", json!({"name":"Marketing"}), None);
        assert_eq!(deleted.status, 200);
        let rows = h.get("/api/profiles", None).body;
        assert_eq!(rows.as_array().expect("rows").len(), 1);
        assert_eq!(rows[0]["id"], p["id"]);
        assert_eq!(rows[0]["folder"], "Unfiled");
        Ok(())
    }
);
