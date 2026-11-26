use super::*;
fn sentinel() -> Value {
    serde_json::from_str(include_str!("sentinel.json")).expect("Source sentinel")
}
fn exported(app: &Http) -> Result<Value, Box<dyn std::error::Error>> {
    let (status, body) = app.request("POST", "/api/profiles", sentinel())?;
    assert_eq!(status, 200, "{body}");
    Ok(json!({"numeraProfiles":1,"profiles":app.list()?}))
}
fn roots() -> Result<(common::TempDirectory, common::TempDirectory), std::io::Error> {
    Ok((
        common::TempDirectory::new("export-source")?,
        common::TempDirectory::new("export-target")?,
    ))
}
original!(
    sentinel_covers_every_field,
    "tests/test_native_export_round_trip.py::test_the_sentinel_covers_every_field_a_client_may_write",
    {
        let body = sentinel();
        let mut fields = numera_core::http::profile_fields::client_fields()?
            .into_iter()
            .filter(|name| !["status", "lastActive"].contains(&name.as_str()))
            .collect::<Vec<_>>();
        fields.sort();
        let mut keys = body
            .as_object()
            .ok_or("Sentinel object missing")?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(fields, keys);
        Ok(())
    }
);
original!(
    every_editable_field_survives,
    "tests/test_native_export_round_trip.py::test_every_editable_field_survives_the_round_trip",
    {
        let (source, target) = roots()?;
        let original = exported(&Http::new(source.path(), None)?)?;
        let app = Http::new(target.path(), None)?;
        let (status, body) = app.import(&original)?;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["imported"], 1);
        let landed = app.list()?;
        for key in sentinel().as_object().ok_or("Sentinel missing")?.keys() {
            assert_eq!(
                landed[0][key], original["profiles"][0][key],
                "Lost field {key}"
            );
        }
        Ok(())
    }
);
original!(
    fingerprint_that_lands_is_the_one_that_left,
    "tests/test_native_export_round_trip.py::test_the_fingerprint_that_lands_is_the_one_that_left",
    {
        let (source, target) = roots()?;
        let original = exported(&Http::new(source.path(), None)?)?;
        let app = Http::new(target.path(), None)?;
        assert_eq!(app.import(&original)?.0, 200);
        let landed = app.list()?;
        assert_eq!(
            serde_json::to_value(conversion::to_engine(&landed[0], 1000.0)?.fingerprint)?,
            serde_json::to_value(
                conversion::to_engine(&original["profiles"][0], 1000.0)?.fingerprint
            )?
        );
        Ok(())
    }
);
original!(
    identity_and_bookkeeping_reminted,
    "tests/test_native_export_round_trip.py::test_the_id_and_the_stores_bookkeeping_are_reminted",
    {
        let (source, target) = roots()?;
        let original = exported(&Http::new(source.path(), None)?)?;
        let app = Http::new(target.path(), None)?;
        assert_eq!(app.import(&original)?.0, 200);
        let landed = app.list()?;
        assert_ne!(landed[0]["id"], original["profiles"][0]["id"]);
        assert_eq!(landed[0]["status"], "stopped");
        assert_eq!(landed[0]["lastActive"], "never");
        assert_eq!(landed[0]["_rev"], 1);
        Ok(())
    }
);
original!(
    newer_file_refused,
    "tests/test_native_export_round_trip.py::test_a_file_this_build_cannot_read_is_refused",
    {
        let (source, _) = roots()?;
        let app = Http::new(source.path(), None)?;
        let mut document = exported(&app)?;
        document["numeraProfiles"] = 2.into();
        let (status, body) = app.import(&document)?;
        assert_eq!(status, 400);
        assert!(
            body["detail"]
                .as_str()
                .ok_or("Detail missing")?
                .to_lowercase()
                .contains("newer")
        );
        Ok(())
    }
);
original!(
    unversioned_file_refused,
    "tests/test_native_export_round_trip.py::test_an_unversioned_native_export_is_refused_not_guessed_at",
    {
        let (source, target) = roots()?;
        let original = exported(&Http::new(source.path(), None)?)?;
        let app = Http::new(target.path(), None)?;
        let (status, body) = app.import(&original["profiles"])?;
        assert_eq!(status, 400);
        let message = body["detail"].as_str().ok_or("Detail missing")?;
        assert!(message.contains("Numera") && message.to_lowercase().contains("export"));
        assert_eq!(app.list()?, json!([]));
        Ok(())
    }
);
original!(
    vendor_export_still_imports,
    "tests/test_native_export_round_trip.py::test_a_vendor_export_still_imports",
    {
        let (source, _) = roots()?;
        let app = Http::new(source.path(), None)?;
        let(status,body)=app.import(&json!([{"name":"from gologin","os":"Windows","navigator":{"userAgent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/128.0.0.0","resolution":"1920x1080","hardwareConcurrency":8},"proxy":{"mode":"http","host":"1.2.3.4","port":8080}}]))?;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["format"], "vendor");
        let landed = app.list()?;
        assert_eq!(landed[0]["name"], "from gologin");
        assert_eq!(landed[0]["proxy"]["host"], "1.2.3.4");
        Ok(())
    }
);
original!(
    flat_vendor_not_mistaken_for_native,
    "tests/test_native_export_round_trip.py::test_a_flat_vendor_export_is_not_mistaken_for_a_native_one",
    {
        let (source, _) = roots()?;
        let app = Http::new(source.path(), None)?;
        let(status,body)=app.import(&json!([{"name":"flat vendor export","os":"Windows","mediaDevices":"real","maxTouchPoints":0,"devicePixelRatio":1,"user_agent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/128.0.0.0"}]))?;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["format"], "vendor");
        Ok(())
    }
);
original!(
    malformed_entry_refuses_whole_batch,
    "tests/test_native_export_round_trip.py::test_a_malformed_entry_refuses_the_whole_batch",
    {
        let (source, target) = roots()?;
        let mut original = exported(&Http::new(source.path(), None)?)?;
        let mut broken = original["profiles"][0].clone();
        broken["name"] = "broken".into();
        broken["tags"] = "not-a-list".into();
        original["profiles"]
            .as_array_mut()
            .ok_or("Profiles missing")?
            .push(broken);
        let app = Http::new(target.path(), None)?;
        let (status, body) = app.import(&original)?;
        assert_eq!(status, 400);
        assert!(
            body["detail"]
                .as_str()
                .ok_or("Detail missing")?
                .contains("profiles[1]")
        );
        assert_eq!(app.list()?, json!([]));
        Ok(())
    }
);
original!(
    profiles_must_be_list_of_objects,
    "tests/test_native_export_round_trip.py::test_profiles_is_not_a_list_of_objects",
    {
        let (source, _) = roots()?;
        let app = Http::new(source.path(), None)?;
        assert_eq!(
            app.import(&json!({"numeraProfiles":1,"profiles":["not an object"]}))?
                .0,
            400
        );
        Ok(())
    }
);
original!(
    unknown_engine_refused,
    "tests/test_native_export_round_trip.py::test_an_unknown_engine_is_refused",
    {
        let (source, target) = roots()?;
        let mut document = exported(&Http::new(source.path(), None)?)?;
        document["profiles"][0]["engine"] = "not-an-engine".into();
        let app = Http::new(target.path(), None)?;
        assert_eq!(app.import(&document)?.0, 422);
        assert_eq!(app.list()?, json!([]));
        Ok(())
    }
);
original!(
    taken_name_refuses_import,
    "tests/test_native_export_round_trip.py::test_a_name_that_is_already_taken_refuses_the_import",
    {
        let (source, _) = roots()?;
        let app = Http::new(source.path(), None)?;
        let document = exported(&app)?;
        let (status, body) = app.import(&document)?;
        assert_eq!(status, 409);
        assert!(
            body["detail"]
                .as_str()
                .ok_or("Detail missing")?
                .contains("sentinel profile")
        );
        assert_eq!(app.list()?.as_array().ok_or("List missing")?.len(), 1);
        Ok(())
    }
);
original!(
    withheld_password_reported_missing,
    "tests/test_native_export_round_trip.py::test_a_password_the_vault_was_holding_is_reported_missing",
    {
        let (source, target) = roots()?;
        let app = Http::new(source.path(), None)?;
        exported(&app)?;
        ProfileRepo::open(source.path().to_owned(), None, None)?.enable_vault("master")?;
        let locked = Http::new(source.path(), None)?;
        let document = json!({"numeraProfiles":1,"profiles":locked.list()?});
        assert!(document["profiles"][0]["proxy"]["pass"].is_null());
        let target = Http::new(target.path(), None)?;
        let (status, body) = target.import(&document)?;
        assert_eq!(status, 200, "{body}");
        assert!(
            body["results"][0]["issues"]
                .as_array()
                .ok_or("Issues missing")?
                .iter()
                .any(|v| v.as_str().is_some_and(|v| v.contains("password")))
        );
        let landed = target.list()?;
        assert_eq!(landed[0]["proxy"]["host"], "gw.sentinel.test");
        assert!(landed[0]["proxy"]["pass"].is_null() || landed[0]["proxy"]["pass"] == "");
        Ok(())
    }
);
original!(
    readable_password_comes_across,
    "tests/test_native_export_round_trip.py::test_a_readable_password_comes_across",
    {
        let (source, target) = roots()?;
        let document = exported(&Http::new(source.path(), None)?)?;
        let app = Http::new(target.path(), None)?;
        assert_eq!(app.import(&document)?.0, 200);
        assert_eq!(app.list()?[0]["proxy"]["pass"], "sentinel-pass");
        Ok(())
    }
);
original!(
    import_preserves_default_engine,
    "tests/test_native_export_round_trip.py::test_an_import_does_not_change_the_default_engine",
    {
        let (source, target) = roots()?;
        let document = exported(&Http::new(source.path(), None)?)?;
        let app = Http::new(target.path(), None)?;
        assert_eq!(
            app.request("PUT", "/api/config", json!({"default_engine":"cloak"}))?
                .0,
            200
        );
        let before = app.request("GET", "/api/config", Value::Null)?.1["default_engine"].clone();
        assert_ne!(document["profiles"][0]["engine"], before);
        assert_eq!(app.import(&document)?.0, 200);
        assert_eq!(
            app.request("GET", "/api/config", Value::Null)?.1["default_engine"],
            before
        );
        Ok(())
    }
);
original!(
    cli_import_refuses_native,
    "tests/test_native_export_round_trip.py::test_the_cli_importer_refuses_a_native_file_too",
    {
        let (source, _) = roots()?;
        let document = exported(&Http::new(source.path(), None)?)?;
        for form in [&document, &document["profiles"]] {
            assert!(
                numera_core::import_format::vendor(form)
                    .expect_err("Native export entered vendor importer")
                    .contains("Numera profile export")
            );
        }
        Ok(())
    }
);
original!(
    cli_vendor_file_still_reads,
    "tests/test_native_export_round_trip.py::test_the_cli_importer_still_reads_a_vendor_file",
    {
        let source = common::TempDirectory::new("vendor-file")?;
        let path = source.path().join("vendor.json");
        let document = json!([{"name":"from adspower","os":"Windows"}]);
        fs::write(&path, serde_json::to_vec(&document)?)?;
        let read: Value = serde_json::from_slice(&fs::read(path)?)?;
        assert_eq!(
            numera_core::import_format::vendor(&read)?,
            document.as_array().ok_or("Vendor rows missing")?.clone()
        );
        Ok(())
    }
);
original!(
    absent_engine_uses_saved_default,
    "tests/test_native_export_round_trip.py::test_an_entry_naming_no_engine_takes_the_saved_default",
    {
        let (source, target) = roots()?;
        let mut document = exported(&Http::new(source.path(), None)?)?;
        document["profiles"][0]
            .as_object_mut()
            .ok_or("Profile missing")?
            .remove("engine");
        let app = Http::new(target.path(), None)?;
        assert_eq!(
            app.request("PUT", "/api/config", json!({"default_engine":"cloak"}))?
                .0,
            200
        );
        let expected = app.request("GET", "/api/config", Value::Null)?.1["default_engine"].clone();
        assert_eq!(app.import(&document)?.0, 200);
        assert_eq!(app.list()?[0]["engine"], expected);
        Ok(())
    }
);
