use super::*;

pub(super) fn verify() -> TestResult {
    const TREE: &str = "engine-rust/src/ownership/tree.rs";
    const SESSION: &str = "engine-rust/src/ownership/session.rs";
    const TESTS: &str = "engine-rust/src/ownership/macos_tests.rs";
    let cases = history::macos_cases();
    let case = cases
        .iter()
        .find(|case| case.files.contains_key(TREE))
        .expect("Mac tree proof");
    let temporary = Temp::new("macos-session-source-bridge")?;
    let tree = fs::read(root().join(TREE))?;
    let session = fs::read(root().join(SESSION))?;
    let tests = fs::read(root().join(TESTS))?;
    temporary.write(TREE, &tree)?;
    temporary.write(SESSION, &session)?;
    temporary.write(TESTS, &tests)?;
    let old = ci::git(&root(), &["show", &format!("{}:{TREE}", case.commit)])?;
    let mut cache = BTreeMap::new();
    let mut swapped =
        history::SourceSwap::prepare_with(&temporary.0, case, &mut cache, |commit, path| {
            assert_eq!(commit, case.commit);
            assert_eq!(path, TREE);
            Ok(old.clone())
        })?;
    assert_eq!(
        fs::read(temporary.0.join(TREE))?,
        old,
        "selected Mac defect was changed"
    );
    assert_eq!(
        fs::read(temporary.0.join(TESTS))?,
        tests,
        "Mac assertions were changed"
    );
    let changed = fs::read_to_string(temporary.0.join(SESSION))?;
    assert!(changed.contains("OwnedTree::spawn(command)"));
    assert!(!changed.contains("OwnedTree::spawn_classified(command)"));
    assert!(changed.contains(
        "if let Some(admission) = &mut self.admission {\n                    admission.retain();"
    ));
    let receipts = fs::read_dir(temporary.0.join("build/history"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    assert_eq!(receipts.len(), 1);
    let receipts: Value = serde_json::from_slice(&fs::read(&receipts[0])?)?;
    let receipts = receipts.as_array().expect("caller receipts");
    assert_eq!(receipts.len(), 1);
    let receipt = &receipts[0];
    assert_eq!(receipt["source_path"], SESSION);
    assert_eq!(receipt["historical_source_path"], TREE);
    assert_eq!(receipt["historical_source_commit"], case.commit);
    assert_eq!(receipt["historical_source_sha256"], case.files[TREE]);
    assert_eq!(receipt["before_sha256"], ci::sha(&session));
    assert_eq!(receipt["after_sha256"], ci::sha(changed.as_bytes()));
    assert_eq!(receipt["literal_historical_execution"], false);
    assert!(
        receipt["status"].is_null(),
        "materialization is not semantic verification"
    );
    swapped.restore()?;
    assert_eq!(fs::read(temporary.0.join(TREE))?, tree);
    assert_eq!(fs::read(temporary.0.join(SESSION))?, session);
    assert_eq!(fs::read(temporary.0.join(TESTS))?, tests);
    assert!(ci::history_compat::additional_callers(&temporary.0, &cases[0])?.is_empty());
    for damage in [
        "target",
        "test",
        "symptom",
        "commit",
        "platform",
        "selected-caller",
    ] {
        let mut wrong = case.clone();
        match damage {
            "target" => wrong.target = "source_browser".into(),
            "test" => wrong.test = "unreviewed_test".into(),
            "symptom" => wrong.symptom = "unreviewed failure".into(),
            "commit" => wrong.commit = "0".repeat(40),
            "platform" => wrong.platform = Some("linux".into()),
            _ => {
                wrong.files.insert(SESSION.into(), ci::sha(&session));
            }
        }
        assert!(
            ci::history_compat::additional_callers(&temporary.0, &wrong).is_err(),
            "{damage}"
        );
        assert_eq!(fs::read(temporary.0.join(TREE))?, tree);
        assert_eq!(fs::read(temporary.0.join(SESSION))?, session);
        assert_eq!(fs::read(temporary.0.join(TESTS))?, tests);
    }
    fs::write(
        temporary.0.join(SESSION),
        [session.as_slice(), b"\n// unreviewed caller\n"].concat(),
    )?;
    assert!(ci::history_compat::additional_callers(&temporary.0, case).is_err());
    Ok(())
}
