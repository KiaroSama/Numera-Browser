use super::*;

fn repo(root: &Path) -> ProfileRepo {
    ProfileRepo::new(root.to_owned(), None)
}
fn engine_only(root: &Path, p: &Profile) {
    fs::create_dir_all(root.join("profiles")).expect("engine records");
    numera_core::storage::atomic::write_json(
        &root.join("profiles").join(format!("{}.json", p.id)),
        &serde_json::to_value(p).expect("engine profile"),
    )
    .expect("native engine primitive");
}
fn twins(root: &Path) -> (Profile, Profile) {
    let first = repo(root)
        .save(profile("same"), 1000.0)
        .expect("first profile");
    let mut second = profile("same");
    second.id = "legacysecond".into();
    engine_only(root, &second);
    (first, second)
}
fn names(root: &Path) -> Vec<String> {
    let mut names = repo(root)
        .list(1001.0)
        .expect("profiles")
        .iter()
        .map(|p| p.name.as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}
source_test!(
    batch_names_test_a_batch_cannot_contain_two_of_one_name,
    "tests/test_batch_names.py::test_a_batch_cannot_contain_two_of_one_name",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        assert!(
            store
                .save_many(
                    vec![profile("twin"), profile("twin")],
                    Default::default(),
                    "reject",
                    1000.0
                )
                .is_err()
        );
        assert!(names(root).is_empty());
        Ok(())
    }
);
source_test!(
    batch_names_test_a_batch_cannot_claim_a_name_the_store_holds,
    "tests/test_batch_names.py::test_a_batch_cannot_claim_a_name_the_store_holds",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        store.save(profile("taken"), 1000.0)?;
        assert!(
            store
                .save_many(
                    vec![profile("fresh"), profile("taken")],
                    Default::default(),
                    "reject",
                    1001.0
                )
                .is_err()
        );
        assert_eq!(names(root), vec!["taken"]);
        Ok(())
    }
);
source_test!(
    batch_names_test_skip_drops_the_clash_and_keeps_the_rest,
    "tests/test_batch_names.py::test_skip_drops_the_clash_and_keeps_the_rest",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        store.save(profile("taken"), 1000.0)?;
        let saved = store.save_many(
            vec![profile("fresh"), profile("taken")],
            Default::default(),
            "skip",
            1001.0,
        )?;
        assert_eq!(
            saved
                .iter()
                .map(|p| p.name.as_str().expect("name"))
                .collect::<Vec<_>>(),
            vec!["fresh"]
        );
        assert_eq!(names(root), vec!["fresh", "taken"]);
        Ok(())
    }
);
source_test!(
    batch_names_test_keeping_the_name_you_already_have_is_not_a_claim,
    "tests/test_batch_names.py::test_keeping_the_name_you_already_have_is_not_a_claim",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let (mut first, mut second) = twins(root);
        first.notes = "bulk".into();
        second.notes = "bulk".into();
        store.save_many(vec![first, second], Default::default(), "reject", 1001.0)?;
        assert!(store.list(1002.0)?.iter().all(|p| p.notes == "bulk"));
        Ok(())
    }
);
source_test!(
    batch_names_test_renaming_onto_a_taken_name_is_refused,
    "tests/test_batch_names.py::test_renaming_onto_a_taken_name_is_refused",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        store.save(profile("a"), 1000.0)?;
        let mut b = store.save(profile("b"), 1000.0)?;
        b.name = json!("a");
        assert!(
            store
                .save_many(vec![b], Default::default(), "reject", 1001.0)
                .is_err()
        );
        assert_eq!(names(root), vec!["a", "b"]);
        Ok(())
    }
);
source_test!(
    batch_names_test_an_unnamed_profile_clashes_with_nothing,
    "tests/test_batch_names.py::test_an_unnamed_profile_clashes_with_nothing",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let saved = store.save_many(
            vec![profile(""), profile("")],
            Default::default(),
            "reject",
            1000.0,
        )?;
        assert_eq!(saved.len(), 2);
        Ok(())
    }
);
source_test!(
    batch_names_test_an_unknown_policy_is_refused,
    "tests/test_batch_names.py::test_an_unknown_policy_is_refused",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        assert!(
            store
                .save_many(vec![], Default::default(), "whatever", 1000.0)
                .is_err()
        );
        Ok(())
    }
);
source_test!(
    batch_names_test_two_processes_bulk_creating_one_prefix_make_no_duplicates,
    "tests/test_batch_names.py::test_two_processes_bulk_creating_one_prefix_make_no_duplicates",
    {
        use std::process::Stdio;
        let t = fresh();
        let start = || {
            Command::new(env!("CARGO_BIN_EXE_numera-engine"))
                .arg("--data-dir")
                .arg(t.path())
                .args(["bulk-create", "--count", "6", "--prefix", "Bulk", "--json"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        };
        let one = start()?;
        let two = start()?;
        for output in [one.wait_with_output()?, two.wait_with_output()?] {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(payload(&output).is_object());
        }
        let all = names(t.path());
        assert_eq!(all.len(), 6);
        assert_eq!(
            all.iter().collect::<std::collections::BTreeSet<_>>().len(),
            6
        );
        Ok(())
    }
);
source_test!(
    public_surface_test_the_quick_start_example_writes_both_copies,
    "tests/test_public_surface.py::test_the_quick_start_example_writes_both_copies",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let p = store.save(profile("acct-01"), 1000.0)?;
        assert!(
            root.join("profiles")
                .join(format!("{}.json", p.id))
                .is_file()
        );
        assert_eq!(
            read(root.join("dashboard").join(format!("{}.json", p.id)))["name"],
            "acct-01"
        );
        Ok(())
    }
);
source_test!(
    public_surface_test_updating_an_id_the_dashboard_knows_reaches_it,
    "tests/test_public_surface.py::test_updating_an_id_the_dashboard_knows_reaches_it",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let mut p = profile("acct-01");
        p.notes = "before".into();
        let mut p = store.save(p, 1000.0)?;
        assert_eq!(
            read(root.join("dashboard").join(format!("{}.json", p.id)))["notes"],
            "before"
        );
        p.notes = "after".into();
        store.save(p.clone(), 1001.0)?;
        for side in ["profiles", "dashboard"] {
            assert_eq!(
                read(root.join(side).join(format!("{}.json", p.id)))["notes"],
                "after"
            );
        }
        Ok(())
    }
);
source_test!(
    public_surface_test_delete_removes_both_copies,
    "tests/test_public_surface.py::test_delete_removes_both_copies",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let p = store.save(profile("acct-01"), 1000.0)?;
        assert!(store.delete(&p.id, "source-http-host", 1001.0)?);
        for side in ["profiles", "dashboard"] {
            assert!(!root.join(side).join(format!("{}.json", p.id)).exists());
        }
        Ok(())
    }
);
source_test!(
    public_surface_test_the_public_api_shares_the_name_contract,
    "tests/test_public_surface.py::test_the_public_api_shares_the_name_contract",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        store.save(profile("taken"), 1000.0)?;
        assert!(matches!(
            store.save(profile("taken"), 1001.0),
            Err(numera_core::storage::repo::RepoError::NameTaken(_))
        ));
        Ok(())
    }
);
source_test!(
    public_surface_test_the_engine_only_primitive_is_still_there_for_the_mirror,
    "tests/test_public_surface.py::test_the_engine_only_primitive_is_still_there_for_the_mirror",
    {
        let t = fresh();
        let p = profile("engine-only");
        engine_only(t.path(), &p);
        assert!(
            t.path()
                .join("profiles")
                .join(format!("{}.json", p.id))
                .is_file()
        );
        assert!(
            !t.path()
                .join("dashboard")
                .join(format!("{}.json", p.id))
                .exists()
        );
        assert_eq!(
            repo(t.path())
                .get(&p.id)?
                .expect("engine primitive record")
                .name,
            "engine-only"
        );
        Ok(())
    }
);
source_test!(
    public_surface_test_a_duplicate_name_refuses_instead_of_choosing,
    "tests/test_public_surface.py::test_a_duplicate_name_refuses_instead_of_choosing",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let (a, b) = twins(root);
        let error = store
            .resolve("same", 1001.0)
            .err()
            .expect("ambiguous name")
            .to_string();
        assert!(error.contains(&a.id));
        assert!(error.contains(&b.id));
        Ok(())
    }
);
source_test!(
    public_surface_test_an_id_still_resolves_in_an_ambiguous_store,
    "tests/test_public_surface.py::test_an_id_still_resolves_in_an_ambiguous_store",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let (a, b) = twins(root);
        for p in [a, b] {
            assert_eq!(store.resolve(&p.id, 1001.0)?.expect("explicit id").id, p.id);
        }
        Ok(())
    }
);
source_test!(
    public_surface_test_destructive_commands_by_name_refuse_and_change_nothing_argv0_143e17,
    "tests/test_public_surface.py::test_destructive_commands_by_name_refuse_and_change_nothing[argv0]",
    {
        let t = fresh();
        let root = t.path();
        let (a, b) = twins(root);
        let before = snapshot(root);
        let output = cli(root, &["show", "same"], true);
        assert_eq!(output.status.code(), Some(1));
        let value = payload(&output);
        let shown = value.to_string();
        assert!(shown.contains(&a.id));
        assert!(shown.contains(&b.id));
        assert_eq!(snapshot(root), before);
        Ok(())
    }
);
source_test!(
    public_surface_test_destructive_commands_by_name_refuse_and_change_nothing_argv1_77f681,
    "tests/test_public_surface.py::test_destructive_commands_by_name_refuse_and_change_nothing[argv1]",
    {
        let t = fresh();
        let root = t.path();
        let (a, b) = twins(root);
        let before = snapshot(root);
        let output = cli(root, &["edit", "same", "--notes", "hello"], true);
        assert_eq!(output.status.code(), Some(1));
        let value = payload(&output);
        let shown = value.to_string();
        assert!(shown.contains(&a.id));
        assert!(shown.contains(&b.id));
        assert_eq!(snapshot(root), before);
        Ok(())
    }
);
source_test!(
    public_surface_test_destructive_commands_by_name_refuse_and_change_nothing_argv2_20207b,
    "tests/test_public_surface.py::test_destructive_commands_by_name_refuse_and_change_nothing[argv2]",
    {
        let t = fresh();
        let root = t.path();
        let (a, b) = twins(root);
        let before = snapshot(root);
        let output = cli(root, &["delete", "same", "-y"], true);
        assert_eq!(output.status.code(), Some(1));
        let value = payload(&output);
        let shown = value.to_string();
        assert!(shown.contains(&a.id));
        assert!(shown.contains(&b.id));
        assert_eq!(snapshot(root), before);
        Ok(())
    }
);
source_test!(
    public_surface_test_destructive_commands_by_name_refuse_and_change_nothing_argv3_ea64fe,
    "tests/test_public_surface.py::test_destructive_commands_by_name_refuse_and_change_nothing[argv3]",
    {
        let t = fresh();
        let root = t.path();
        let (a, b) = twins(root);
        let before = snapshot(root);
        let output = cli(root, &["regen", "same"], true);
        assert_eq!(output.status.code(), Some(1));
        let value = payload(&output);
        let shown = value.to_string();
        assert!(shown.contains(&a.id));
        assert!(shown.contains(&b.id));
        assert_eq!(snapshot(root), before);
        Ok(())
    }
);
source_test!(
    public_surface_test_a_listing_still_works_in_an_ambiguous_store,
    "tests/test_public_surface.py::test_a_listing_still_works_in_an_ambiguous_store",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let (a, b) = twins(root);
        assert_eq!(store.list(1001.0)?.len(), 2);
        let h = Http::new(root);
        let r = h.get("/api/profiles", None);
        assert_eq!(r.status, 200);
        let ids = r
            .body
            .as_array()
            .expect("rows")
            .iter()
            .map(|p| text(p, "id"))
            .collect::<std::collections::BTreeSet<_>>();
        assert!(ids.contains(&a.id) && ids.contains(&b.id));
        Ok(())
    }
);
source_test!(
    public_surface_test_the_cli_refuses_a_delete_whose_target_moved,
    "tests/test_public_surface.py::test_the_cli_refuses_a_delete_whose_target_moved",
    {
        use numera_core::vault::testing::{self, Action};
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let p = store.save(profile("alpha"), 1000.0)?;
        let config = ConfigStore::new(root.to_owned());
        let installed = BTreeMap::new();
        let commands = numera_core::cli::profile_commands::Profiles {
            repo: &store,
            config: &config,
            installed: &installed,
            host: "source-http-host",
        };
        let observed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fired = observed.clone();
        let mut output = numera_core::cli::output::Output::new(Vec::new(), Vec::new(), true, false);
        let mutation_root = root.to_owned();
        let id = p.id.clone();
        let outcome = testing::with_observer(
            move |event| {
                if event.operation == "before-profile-delete-lease"
                    && !fired.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    testing::with_observer(
                        |_| Ok(Action::Continue),
                        || {
                            let fresh = repo(&mutation_root);
                            fresh.edit(
                                &id,
                                |p| {
                                    p.name = json!("beta");
                                    Ok(None)
                                },
                                1001.0,
                            )?;
                            fresh.save(profile("alpha"), 1001.0)?;
                            Ok::<(), numera_core::storage::repo::RepoError>(())
                        },
                    )
                    .map_err(io::Error::other)?;
                }
                Ok(Action::Continue)
            },
            || {
                commands.delete_with_output(
                    &["alpha".into()],
                    true,
                    1002.0,
                    |_| Ok(true),
                    &mut output,
                )
            },
        )?;
        assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(outcome.exit, 1);
        output.emit(&outcome.payload, outcome.exit)?;
        let (stdout, _) = output.into_parts();
        let value: Value = serde_json::from_slice(&stdout)?;
        assert!(
            value.to_string().contains("meant profile")
                || value.to_string().to_lowercase().contains("moved")
        );
        assert!(
            root.join("profiles")
                .join(format!("{}.json", p.id))
                .is_file()
        );
        assert_eq!(store.get(&p.id)?.expect("renamed profile").name, "beta");
        assert_eq!(names(root), vec!["alpha", "beta"]);
        Ok(())
    }
);
source_test!(
    public_surface_test_the_cli_deletes_one_profile_named_twice_once,
    "tests/test_public_surface.py::test_the_cli_deletes_one_profile_named_twice_once",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let p = store.save(profile("dup"), 1000.0)?;
        let output = cli(root, &["delete", "dup", &p.id, "-y"], true);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let shown = String::from_utf8(output.stdout)?;
        assert!(shown.matches(&p.id).count() <= 2);
        assert!(
            !root
                .join("profiles")
                .join(format!("{}.json", p.id))
                .exists()
        );
        Ok(())
    }
);
source_test!(
    public_surface_test_a_delete_that_found_nothing_is_not_reported_as_one,
    "tests/test_public_surface.py::test_a_delete_that_found_nothing_is_not_reported_as_one",
    {
        use numera_core::vault::testing::{self, Action};
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let p = store.save(profile("alpha"), 1000.0)?;
        let config = ConfigStore::new(root.to_owned());
        let installed = BTreeMap::new();
        let commands = numera_core::cli::profile_commands::Profiles {
            repo: &store,
            config: &config,
            installed: &installed,
            host: "source-http-host",
        };
        let observed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fired = observed.clone();
        let mut output = numera_core::cli::output::Output::new(Vec::new(), Vec::new(), true, false);
        let before = snapshot(root);
        let outcome = testing::with_observer(
            move |event| {
                if event.operation == "before-profile-delete-lease" {
                    fired.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(Action::Skip)
                } else {
                    Ok(Action::Continue)
                }
            },
            || {
                commands.delete_with_output(
                    &["alpha".into()],
                    true,
                    1001.0,
                    |_| Ok(true),
                    &mut output,
                )
            },
        )?;
        assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(outcome.exit, 1);
        output.emit(&outcome.payload, outcome.exit)?;
        let (stdout, _) = output.into_parts();
        let value: Value = serde_json::from_slice(&stdout)?;
        assert_eq!(value["removed"], json!([]));
        assert_eq!(snapshot(root), before);
        assert!(
            root.join("profiles")
                .join(format!("{}.json", p.id))
                .is_file()
        );
        Ok(())
    }
);
source_test!(
    public_surface_test_a_detached_command_reports_a_bad_reference_as_json,
    "tests/test_public_surface.py::test_a_detached_command_reports_a_bad_reference_as_json",
    {
        let t = fresh();
        twins(t.path());
        let output = cli(t.path(), &["show", "same", "--detach"], true);
        assert_eq!(output.status.code(), Some(1));
        let value = payload(&output);
        assert_eq!(value["ok"], false);
        assert!(text(&value, "error").contains("same"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Traceback"));
        let listed = payload(&cli(t.path(), &["jobs", "list"], true));
        assert!(listed["jobs"].as_array().expect("jobs").is_empty());
        Ok(())
    }
);
source_test!(
    public_surface_test_a_dashboard_only_twin_makes_a_name_ambiguous,
    "tests/test_public_surface.py::test_a_dashboard_only_twin_makes_a_name_ambiguous",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        let p = store.save(profile("same"), 1000.0)?;
        fs::create_dir_all(root.join("dashboard"))?;
        fs::write(
            root.join("dashboard/legacy00legacy.json"),
            json!({"id":"legacy00legacy","name":"same","os":"Windows"}).to_string(),
        )?;
        let error = store
            .resolve("same", 1001.0)
            .err()
            .expect("cross-store ambiguity")
            .to_string();
        assert!(error.contains(&p.id));
        assert!(error.contains("legacy00legacy"));
        Ok(())
    }
);
source_test!(
    public_surface_test_a_dashboard_only_profile_is_repaired_rather_than_denied,
    "tests/test_public_surface.py::test_a_dashboard_only_profile_is_repaired_rather_than_denied",
    {
        let t = fresh();
        let root = t.path();
        let store = repo(root);
        fs::create_dir_all(root.join("dashboard"))?;
        fs::write(
            root.join("dashboard/orphan00orphan.json"),
            json!({"id":"orphan00orphan","name":"orphan","os":"Windows"}).to_string(),
        )?;
        assert_eq!(
            store
                .resolve("orphan00orphan", 1001.0)?
                .expect("dashboard orphan")
                .id,
            "orphan00orphan"
        );
        assert!(root.join("profiles/orphan00orphan.json").exists());
        assert!(store.resolve("orphan", 1002.0)?.is_some());
        Ok(())
    }
);
