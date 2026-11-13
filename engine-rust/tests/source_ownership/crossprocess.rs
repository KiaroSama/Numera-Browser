use super::*;

fn cross_writers(root: &Path, operation: &str, count: usize) -> TestResult {
    let gate = root.join("writers-go");
    let mut owners = Vec::new();
    for who in 0..count {
        let mut child = command("cross-write")?;
        child
            .arg(root)
            .arg(who.to_string())
            .arg(&gate)
            .arg(operation)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        owners.push(OwnedTree::spawn(&mut child)?);
    }
    for who in 0..count {
        wait_file(&root.join(format!("writer-ready-{who}")))?;
    }
    fs::write(&gate, b"together")?;
    let end = Instant::now() + Duration::from_secs(15);
    for owner in &mut owners {
        let status = loop {
            if let Some(status) = owner.child_mut().try_wait()? {
                break status;
            }
            assert!(
                Instant::now() < end,
                "native concurrent writer exceeded supervision budget"
            );
            thread::sleep(Duration::from_millis(5));
        };
        owner.settle(end.saturating_duration_since(Instant::now()))?;
        let mut stderr = Vec::new();
        if let Some(mut pipe) = owner.child_mut().stderr.take() {
            use std::io::Read;
            pipe.read_to_end(&mut stderr)?;
        }
        assert!(
            status.success(),
            "native concurrent writer failed: {}",
            String::from_utf8_lossy(&stderr)
        );
    }
    Ok(())
}

source_test!(
    cross_process_locking_two_processes_editing_different_profiles_both_survive,
    "tests/test_cross_process_locking.py::test_two_processes_editing_different_profiles_both_survive",
    {
        let t = temp("process-profile-edits")?;
        for id in ["p0", "p1"] {
            profile(t.path(), id)?;
        }
        cross_writers(t.path(), "profile", 2)?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        for index in 0..2 {
            assert_eq!(
                repo.get(&format!("p{index}"))?.ok_or("profile lost")?.notes,
                format!("w{index}")
            );
        }
        Ok(())
    }
);
source_test!(
    cross_process_locking_concurrent_proxy_pool_adds_do_not_lose_entries,
    "tests/test_cross_process_locking.py::test_concurrent_proxy_pool_adds_do_not_lose_entries",
    {
        let t = temp("process-proxy-adds")?;
        cross_writers(t.path(), "pool", 4)?;
        let rows = numera_core::storage::pool::ProxyPool::new(t.path().into(), None).list()?;
        assert_eq!(rows.len(), 4);
        let mut hosts = rows
            .iter()
            .map(|row| row["host"].as_str().expect("host"))
            .collect::<Vec<_>>();
        hosts.sort();
        assert_eq!(hosts, ["10.0.0.0", "10.0.0.1", "10.0.0.2", "10.0.0.3"]);
        Ok(())
    }
);
source_test!(
    cross_process_locking_concurrent_schedule_writes_all_survive,
    "tests/test_cross_process_locking.py::test_concurrent_schedule_writes_all_survive",
    {
        let t = temp("process-schedule-adds")?;
        cross_writers(t.path(), "schedule", 4)?;
        let rows = numera_core::storage::schedules::ScheduleStore::new(t.path().into()).list()?;
        assert_eq!(rows.len(), 4);
        let ids = rows
            .iter()
            .map(|row| row["profileId"].as_str().expect("profile ID"))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            ids,
            std::collections::BTreeSet::from(["w0", "w1", "w2", "w3"])
        );
        Ok(())
    }
);
source_test!(
    cross_process_locking_a_vault_migration_is_not_interleaved_with_a_profile_write,
    "tests/test_cross_process_locking.py::test_a_vault_migration_is_not_interleaved_with_a_profile_write",
    {
        let t = temp("process-vault-vs-profile")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        for index in 0..6 {
            let mut value = profile(t.path(), &format!("p{index}"))?;
            value.proxy = Some(numera_core::proxy::Proxy::from_record(
                &json!({"server":"http://127.0.0.1:9","username":"source-user","password":"source-secret","country":null}),
                false,
            )?);
            repo.save(value, 1000.0)?;
        }
        cross_writers(t.path(), "vault", 2)?;
        assert!(t.path().join("migration-active").is_file());
        assert!(t.path().join("editor-started").is_file());
        assert!(t.path().join("editor-outcome").is_file());
        let unlocked = numera_core::storage::repo::ProfileRepo::new(
            t.path().into(),
            Some("source-master".into()),
        );
        assert!(unlocked.unlocked()?);
        let descriptor = unlocked.vault()?.ok_or("vault descriptor missing")?;
        assert!(
            numera_core::vault::completion::problems(t.path(), &descriptor, Some("source-master"))?
                .is_empty(),
            "migration left mixed credential generations"
        );
        for index in 0..6 {
            let value = unlocked.get(&format!("p{index}"))?.ok_or("profile lost")?;
            assert_eq!(
                value
                    .proxy
                    .as_ref()
                    .map(|proxy| proxy.to_record()["password"].clone()),
                Some(json!("source-secret"))
            );
            let raw: Value = serde_json::from_slice(&fs::read(
                t.path().join(format!("profiles/p{index}.json")),
            )?)?;
            assert!(
                raw["proxy"]["password"]
                    .as_str()
                    .is_some_and(numera_core::vault::is_envelope)
            );
        }
        Ok(())
    }
);
