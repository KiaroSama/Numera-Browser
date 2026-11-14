use super::*;
use numera_core::cli::profile_commands::{Outcome, Profiles};
fn made(root: &std::path::Path, name: &str) -> Value {
    let (status, body) = http(root, "POST", "/api/profiles", json!({"name":name}));
    assert!(status < 400, "{status}: {body}");
    body
}
fn dash_names(root: &std::path::Path) -> Vec<String> {
    let mut result = json_files(root, "dashboard")
        .into_iter()
        .map(|path| {
            let record: Value = serde_json::from_slice(&fs::read(path).expect("dashboard bytes"))
                .expect("dashboard JSON");
            record["name"].as_str().expect("dashboard name").to_owned()
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}
fn with_commands<T>(root: &std::path::Path, action: impl FnOnce(&Profiles<'_>) -> T) -> T {
    let repo = ProfileRepo::new(root.to_owned(), None);
    let config = ConfigStore::new(root.to_owned());
    let installed = BTreeMap::new();
    action(&Profiles {
        repo: &repo,
        config: &config,
        installed: &installed,
        host: "source-domain-host",
    })
}
fn cli_create(root: &std::path::Path, name: &str, seed: i64) -> Outcome {
    with_commands(root, |commands| {
        commands
            .create(
                json!({"name":name,"seed":seed,"engine":"playwright"})
                    .as_object()
                    .expect("arguments"),
                1000.0,
            )
            .expect("CLI create")
    })
}
fn backup(repo: &ProfileRepo, p: &Profile, root: &std::path::Path) -> std::path::PathBuf {
    let data = repo.user_data_path(&p.id).expect("user data path");
    fs::create_dir_all(&data).expect("session directory");
    fs::write(data.join("Cookies"), "session").expect("session bytes");
    repo.backup(&p.id, &root.join("b.zip"), "source-domain-host")
        .expect("backup")
        .expect("archive")
}
source_test!(
    a_second_create_with_a_taken_name_is_refused,
    "tests/test_name_contract.py::test_a_second_create_with_a_taken_name_is_refused",
    {
        let temp = common::TempDirectory::new("source-name-create").expect("temporary root");
        made(temp.path(), "alpha");
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles",
            json!({"name":"alpha"}),
        );
        assert_eq!(status, 409, "{response}");
        assert!(
            response["detail"]
                .as_str()
                .expect("detail")
                .contains("alpha")
        );
        assert_eq!(
            names(&ProfileRepo::new(temp.path().to_owned(), None)),
            ["alpha"]
        );
        assert_eq!(dash_names(temp.path()), ["alpha"]);
    }
);
source_test!(
    two_entries_of_one_batch_cannot_share_a_name,
    "tests/test_name_contract.py::test_two_entries_of_one_batch_cannot_share_a_name",
    {
        let temp = common::TempDirectory::new("source-name-twins").expect("temporary root");
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"twin"},{"name":"twin"}]}),
        );
        assert_eq!(status, 409, "{response}");
        assert!(names(&ProfileRepo::new(temp.path().to_owned(), None)).is_empty());
        assert!(dash_names(temp.path()).is_empty());
    }
);
source_test!(
    a_batch_cannot_claim_a_name_the_store_already_holds,
    "tests/test_name_contract.py::test_a_batch_cannot_claim_a_name_the_store_already_holds",
    {
        let temp =
            common::TempDirectory::new("source-name-existing-batch").expect("temporary root");
        made(temp.path(), "taken");
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"fresh"},{"name":"taken"}]}),
        );
        assert_eq!(status, 409, "{response}");
        assert_eq!(
            names(&ProfileRepo::new(temp.path().to_owned(), None)),
            ["taken"]
        );
    }
);
source_test!(
    renaming_a_profile_onto_a_taken_name_is_refused,
    "tests/test_name_contract.py::test_renaming_a_profile_onto_a_taken_name_is_refused",
    {
        let temp = common::TempDirectory::new("source-name-rename").expect("temporary root");
        made(temp.path(), "alpha");
        let mut other = made(temp.path(), "beta");
        let uri = format!("/api/profiles/{}", other["id"].as_str().expect("ID"));
        other["name"] = "alpha".into();
        let (status, response) = http(temp.path(), "PUT", &uri, other);
        assert_eq!(status, 409, "{response}");
        assert_eq!(
            names(&ProfileRepo::new(temp.path().to_owned(), None)),
            ["alpha", "beta"]
        );
    }
);
source_test!(
    a_profile_may_still_be_saved_under_the_name_it_already_has,
    "tests/test_name_contract.py::test_a_profile_may_still_be_saved_under_the_name_it_already_has",
    {
        let temp = common::TempDirectory::new("source-name-retain").expect("temporary root");
        let mut p = made(temp.path(), "alpha");
        let uri = format!("/api/profiles/{}", p["id"].as_str().expect("ID"));
        p["notes"] = "edited".into();
        let (status, response) = http(temp.path(), "PUT", &uri, p);
        assert!(status < 400, "{response}");
        assert_eq!(response["notes"], "edited");
    }
);
source_test!(
    a_bulk_rename_of_many_profiles_onto_one_name_is_refused,
    "tests/test_name_contract.py::test_a_bulk_rename_of_many_profiles_onto_one_name_is_refused",
    {
        let temp = common::TempDirectory::new("source-name-bulk-rename").expect("temporary root");
        let ids = ["one", "two", "three"].map(|name| made(temp.path(), name)["id"].clone());
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles/bulk",
            json!({"ids":ids,"patch":{"name":"merged"}}),
        );
        assert_eq!(status, 409, "{response}");
        assert_eq!(
            names(&ProfileRepo::new(temp.path().to_owned(), None)),
            ["one", "three", "two"]
        );
    }
);
source_test!(
    a_bulk_patch_that_does_not_touch_names_still_works,
    "tests/test_name_contract.py::test_a_bulk_patch_that_does_not_touch_names_still_works",
    {
        let temp = common::TempDirectory::new("source-name-bulk-notes").expect("temporary root");
        let ids = ["one", "two"].map(|name| made(temp.path(), name)["id"].clone());
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles/bulk",
            json!({"ids":ids,"patch":{"notes":"swept"}}),
        );
        assert!(status < 400, "{response}");
        assert_eq!(
            response
                .as_array()
                .expect("updated profiles")
                .iter()
                .map(|p| p["notes"].as_str().expect("notes"))
                .collect::<Vec<_>>(),
            ["swept", "swept"]
        );
    }
);
source_test!(
    importing_the_same_vendor_file_twice_is_refused,
    "tests/test_name_contract.py::test_importing_the_same_vendor_file_twice_is_refused",
    {
        let temp = common::TempDirectory::new("source-name-vendor-repeat").expect("temporary root");
        let payload = json!({"text":json!([{"name":"vendor-a","os":"win"},{"name":"vendor-b","os":"win"}]).to_string()});
        let (status, response) = http(temp.path(), "POST", "/api/profiles/import", payload.clone());
        assert!(status < 400, "{response}");
        let (status, response) = http(temp.path(), "POST", "/api/profiles/import", payload);
        assert_eq!(status, 409, "{response}");
        assert_eq!(
            names(&ProfileRepo::new(temp.path().to_owned(), None)),
            ["vendor-a", "vendor-b"]
        );
    }
);
source_test!(
    one_vendor_file_naming_a_profile_twice_is_refused,
    "tests/test_name_contract.py::test_one_vendor_file_naming_a_profile_twice_is_refused",
    {
        let temp = common::TempDirectory::new("source-name-vendor-twins").expect("temporary root");
        let payload = json!({"text":json!([{"name":"same","os":"win"},{"name":"same","os":"mac"}]).to_string()});
        let (status, response) = http(temp.path(), "POST", "/api/profiles/import", payload);
        assert_eq!(status, 409, "{response}");
        assert!(names(&ProfileRepo::new(temp.path().to_owned(), None)).is_empty());
    }
);
source_test!(
    the_api_refuses_the_name_the_cli_created,
    "tests/test_name_contract.py::test_the_api_refuses_the_name_the_cli_created",
    {
        let temp = common::TempDirectory::new("source-name-cli-first").expect("temporary root");
        assert_eq!(cli_create(temp.path(), "shared", 1).exit, 0);
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles",
            json!({"name":"shared"}),
        );
        assert_eq!(status, 409, "{response}");
    }
);
source_test!(
    the_cli_refuses_the_name_the_api_created,
    "tests/test_name_contract.py::test_the_cli_refuses_the_name_the_api_created",
    {
        let temp = common::TempDirectory::new("source-name-api-first").expect("temporary root");
        made(temp.path(), "shared");
        assert_ne!(cli_create(temp.path(), "shared", 1).exit, 0);
        assert_eq!(
            names(&ProfileRepo::new(temp.path().to_owned(), None)),
            ["shared"]
        );
    }
);
source_test!(
    listing_still_adopts_a_cli_made_profile,
    "tests/test_name_contract.py::test_listing_still_adopts_a_cli_made_profile",
    {
        let temp = common::TempDirectory::new("source-name-cli-adoption").expect("temporary root");
        let created = cli_create(temp.path(), "from-the-cli", 2);
        assert_eq!(created.exit, 0);
        // Reproduce a CLI-only legacy store so GET must actually adopt the engine
        // profile; current native CLI writes both copies as a transaction.
        let id = created.payload["profile"]["id"]
            .as_str()
            .expect("created ID");
        fs::remove_file(temp.path().join("dashboard").join(format!("{id}.json")))
            .expect("engine-only legacy record");
        let (status, response) = http(temp.path(), "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200, "{response}");
        assert_eq!(
            response
                .as_array()
                .expect("profiles")
                .iter()
                .map(|p| p["name"].as_str().expect("name"))
                .collect::<Vec<_>>(),
            ["from-the-cli"]
        );
        assert_eq!(dash_names(temp.path()), ["from-the-cli"]);
    }
);
source_test!(
    restoring_a_clone_takes_a_free_name,
    "tests/test_name_contract.py::test_restoring_a_clone_takes_a_free_name",
    {
        let temp = common::TempDirectory::new("source-name-clone").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = repo
            .save(profile("warmed", 3, "windows"), 1000.0)
            .expect("save");
        let archive = backup(&repo, &p, temp.path());
        let copy = repo
            .restore(&archive, true, "source-domain-host", 1001.0)
            .expect("clone");
        assert_ne!(copy.id, p.id);
        assert_eq!(copy.name, "warmed (copy)");
        assert_eq!(names(&repo), ["warmed", "warmed (copy)"]);
    }
);
source_test!(
    a_second_clone_does_not_collide_with_the_first,
    "tests/test_name_contract.py::test_a_second_clone_does_not_collide_with_the_first",
    {
        let temp = common::TempDirectory::new("source-name-second-clone").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = repo
            .save(profile("warmed", 3, "windows"), 1000.0)
            .expect("save");
        let archive = backup(&repo, &p, temp.path());
        repo.restore(&archive, true, "source-domain-host", 1001.0)
            .expect("first clone");
        let third = repo
            .restore(&archive, true, "source-domain-host", 1002.0)
            .expect("second clone");
        assert_eq!(third.name, "warmed (copy 2)");
        assert_eq!(names(&repo), ["warmed", "warmed (copy 2)", "warmed (copy)"]);
    }
);
source_test!(
    restoring_over_the_same_id_keeps_the_archived_name,
    "tests/test_name_contract.py::test_restoring_over_the_same_id_keeps_the_archived_name",
    {
        let temp =
            common::TempDirectory::new("source-name-restore-original").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = repo
            .save(profile("alpha", 4, "windows"), 1000.0)
            .expect("save");
        let archive = backup(&repo, &p, temp.path());
        repo.edit(
            &p.id,
            |current| {
                current.name = "renamed since".into();
                Ok(None)
            },
            1001.0,
        )
        .expect("rename");
        let restored = repo
            .restore(&archive, false, "source-domain-host", 1002.0)
            .expect("restore");
        assert_eq!(restored.id, p.id);
        assert_eq!(restored.name, "alpha");
        assert_eq!(names(&repo), ["alpha"]);
    }
);
source_test!(
    a_name_based_command_after_a_clone_targets_one_profile,
    "tests/test_name_contract.py::test_a_name_based_command_after_a_clone_targets_one_profile",
    {
        let temp = common::TempDirectory::new("source-name-clone-command").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = repo
            .save(profile("warmed", 5, "windows"), 1000.0)
            .expect("save");
        let archive = backup(&repo, &p, temp.path());
        let copy = repo
            .restore(&archive, true, "source-domain-host", 1001.0)
            .expect("clone");
        with_commands(temp.path(), |commands| {
            assert_eq!(commands.show("warmed", 1002.0).expect("show").exit, 0);
            assert_eq!(
                commands
                    .edit(
                        "warmed",
                        json!({"notes":"the original"})
                            .as_object()
                            .expect("arguments"),
                        1002.0
                    )
                    .expect("edit")
                    .exit,
                0
            );
        });
        assert_eq!(
            repo.get(&p.id)
                .expect("read original")
                .expect("original")
                .notes,
            "the original"
        );
        assert_ne!(
            repo.get(&copy.id)
                .expect("read clone")
                .expect("clone")
                .notes,
            "the original"
        );
        assert!(
            repo.delete("warmed (copy)", "source-domain-host", 1003.0)
                .expect("delete by name")
        );
        assert_eq!(names(&repo), ["warmed"]);
        assert!(repo.get(&p.id).expect("read original").is_some());
    }
);
source_test!(
    importing_an_export_whose_name_is_taken_is_refused,
    "tests/test_name_contract.py::test_importing_an_export_whose_name_is_taken_is_refused",
    {
        let temp = common::TempDirectory::new("source-name-export-clash").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = repo
            .save(profile("exported", 6, "windows"), 1000.0)
            .expect("save");
        let out = temp.path().join("p.json");
        repo.export(&p.id, &out).expect("export");
        let mut record: Value =
            serde_json::from_slice(&fs::read(&out).expect("export bytes")).expect("export JSON");
        record["id"] = "adifferentid".into();
        fs::write(
            &out,
            serde_json::to_vec(&record).expect("changed export JSON"),
        )
        .expect("changed export bytes");
        assert!(repo.import_file(&out, 1001.0).is_err());
        assert_eq!(names(&repo), ["exported"]);
    }
);
source_test!(
    reimporting_an_export_over_its_own_id_still_works,
    "tests/test_name_contract.py::test_reimporting_an_export_over_its_own_id_still_works",
    {
        let temp = common::TempDirectory::new("source-name-export-own").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = repo
            .save(profile("exported", 7, "windows"), 1000.0)
            .expect("save");
        let out = temp.path().join("p.json");
        repo.export(&p.id, &out).expect("export");
        repo.edit(
            &p.id,
            |current| {
                current.notes = "changed".into();
                Ok(None)
            },
            1001.0,
        )
        .expect("edit notes");
        let back = repo.import_file(&out, 1002.0).expect("reimport own ID");
        assert_eq!(back.id, p.id);
        assert_eq!(back.name, "exported");
        assert_eq!(
            repo.get(&p.id).expect("read saved").expect("profile").notes,
            ""
        );
    }
);
source_test!(
    a_hand_made_archive_can_still_be_restored_into_an_empty_store,
    "tests/test_name_contract.py::test_a_hand_made_archive_can_still_be_restored_into_an_empty_store",
    {
        use std::io::Write;
        let temp = common::TempDirectory::new("source-name-hand-archive").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let path = temp.path().join("hand.zip");
        let mut archive = zip::ZipWriter::new(fs::File::create(&path).expect("archive file"));
        archive
            .start_file("profile.json", zip::write::SimpleFileOptions::default())
            .expect("archive entry");
        archive
            .write_all(&serde_json::to_vec(&profile("fresh", 8, "windows")).expect("profile JSON"))
            .expect("write profile");
        archive.finish().expect("finish archive");
        assert_eq!(
            repo.restore(&path, false, "source-domain-host", 1000.0)
                .expect("restore hand made archive")
                .name,
            "fresh"
        );
    }
);
