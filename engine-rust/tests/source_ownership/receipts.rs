use super::*;

source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_7c706355,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[17-a bare number]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("17")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "17\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_eeb5ab9e,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[[1]-a list]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("[1]")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "[1]\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_9b5cce5a,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[\"text\"-a bare string]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("\"text\"")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "\"text\"\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_016d968a,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": \"abc\"}-a non-numeric pid]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("{\"pid\":\"abc\"}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "{\"pid\":\"abc\"}\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_995947c9,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": -1}-a negative pid]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("{\"pid\":-1}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "{\"pid\":-1}\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_ed341481,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": 0}-a zero pid]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("{\"pid\":0}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "{\"pid\":0}\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_96c6d94d,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": true}-a boolean pid]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("{\"pid\":true}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "{\"pid\":true}\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_3401a378,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": 5, \"tree\": [1]}-a list where the tree belongs]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("{\"pid\":5,\"tree\":[1]}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "{\"pid\":5,\"tree\":[1]}\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_e73eae0e,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": 5, \"tree\": {\"kind\": \"weird\"}, \"pidToken\": 1}-an unsupported kind]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value =
            serde_json::from_str("{\"pid\":5,\"tree\":{\"kind\":\"weird\"},\"pidToken\":1}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(
                &path,
                "{\"pid\":5,\"tree\":{\"kind\":\"weird\"},\"pidToken\":1}\\n".as_bytes(),
            )?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_2bf6ffa5,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": 5, \"tree\": {\"kind\": 7}, \"pidToken\": 1}-a non-string kind]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value =
            serde_json::from_str("{\"pid\":5,\"tree\":{\"kind\":7},\"pidToken\":1}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(
                &path,
                "{\"pid\":5,\"tree\":{\"kind\":7},\"pidToken\":1}\\n".as_bytes(),
            )?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_245ce7a8,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": 5}-no durable identity at all]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("{\"pid\":5}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(&path, "{\"pid\":5}\\n".as_bytes())?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_untrustworthy_record_is_incomplete_evidence_81526171,
    "tests/test_helper_bounded_lifetime.py::test_an_untrustworthy_record_is_incomplete_evidence[{\"pid\": 5, \"tree\": {\"kind\": \"job\"}}-a job with no name]",
    {
        let t = temp("bad-unit")?;
        let path = receipt(t.path())?;
        let value: Value = serde_json::from_str("{\"pid\":5,\"tree\":{\"kind\":\"job\"}}")?;
        if value.is_object() {
            append(&path, &unit(value))?;
        } else {
            append(
                &path,
                "{\"pid\":5,\"tree\":{\"kind\":\"job\"}}\\n".as_bytes(),
            )?;
        }
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_completion_regression_report_requires_valid_tree_identity_e07f4451,
    "tests/test_helper_completion_regression.py::test_report_requires_valid_tree_identity[bad0]",
    {
        let t = temp("invalid-tree-identity")?;
        let path = receipt(t.path())?;
        let bad: Value = serde_json::from_str(
            "{\"pid\": 123, \"pidToken\": true, \"tree\": {\"kind\": \"pgid\", \"group\": 123}}",
        )?;
        append(&path, &unit(bad))?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_completion_regression_report_requires_valid_tree_identity_ae3b0e74,
    "tests/test_helper_completion_regression.py::test_report_requires_valid_tree_identity[bad1]",
    {
        let t = temp("invalid-tree-identity")?;
        let path = receipt(t.path())?;
        let bad: Value = serde_json::from_str(
            "{\"pid\": 123, \"pidToken\": [], \"tree\": {\"kind\": \"pgid\", \"group\": 123}}",
        )?;
        append(&path, &unit(bad))?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_completion_regression_report_requires_valid_tree_identity_1df50324,
    "tests/test_helper_completion_regression.py::test_report_requires_valid_tree_identity[bad2]",
    {
        let t = temp("invalid-tree-identity")?;
        let path = receipt(t.path())?;
        let bad: Value = serde_json::from_str(
            "{\"pid\": 123, \"pidToken\": 1, \"tree\": {\"kind\": \"pgid\", \"group\": \"123\"}}",
        )?;
        append(&path, &unit(bad))?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_completion_regression_report_requires_valid_tree_identity_dd4bb876,
    "tests/test_helper_completion_regression.py::test_report_requires_valid_tree_identity[bad3]",
    {
        let t = temp("invalid-tree-identity")?;
        let path = receipt(t.path())?;
        let bad: Value = serde_json::from_str(
            "{\"pid\": 123, \"pidToken\": 1, \"tree\": {\"kind\": \"pgid\", \"group\": -123}}",
        )?;
        append(&path, &unit(bad))?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_completion_regression_report_requires_valid_tree_identity_3118cebe,
    "tests/test_helper_completion_regression.py::test_report_requires_valid_tree_identity[bad4]",
    {
        let t = temp("invalid-tree-identity")?;
        let path = receipt(t.path())?;
        let bad: Value = serde_json::from_str(
            "{\"pid\": 123, \"pidToken\": 1, \"tree\": {\"kind\": \"pgid\", \"group\": 456}}",
        )?;
        append(&path, &unit(bad))?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_completion_regression_report_requires_valid_tree_identity_7797adb2,
    "tests/test_helper_completion_regression.py::test_report_requires_valid_tree_identity[bad5]",
    {
        let t = temp("invalid-tree-identity")?;
        let path = receipt(t.path())?;
        let bad: Value = serde_json::from_str(
            "{\"pid\": 123, \"pidToken\": 1, \"tree\": {\"kind\": \"job\", \"group\": {\"name\": \"x\"}}}",
        )?;
        append(&path, &unit(bad))?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_completion_regression_report_requires_valid_tree_identity_644d2494,
    "tests/test_helper_completion_regression.py::test_report_requires_valid_tree_identity[bad6]",
    {
        let t = temp("invalid-tree-identity")?;
        let path = receipt(t.path())?;
        let bad: Value = serde_json::from_str(
            "{\"pid\": 123, \"pidToken\": 1, \"tree\": {\"kind\": \"\", \"group\": null}}",
        )?;
        append(&path, &unit(bad))?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_a_valid_record_is_complete_db02cdf4,
    "tests/test_helper_bounded_lifetime.py::test_a_valid_record_is_complete[record0-pgid]",
    {
        let t = temp("valid-unit")?;
        let path = receipt(t.path())?;
        append(
            &path,
            &unit(serde_json::from_str(
                "{\"pid\": 4321, \"pidToken\": 99, \"tree\": {\"kind\": \"pgid\", \"group\": 4321}}",
            )?),
        )?;
        let report = unit_report::read(&path)?;
        assert!(report.complete);
        assert_eq!(report.trees.len(), 1);
        assert_eq!(report.trees[0]["tree"]["kind"], "pgid");
        assert!(recorded_tree::verified(&report.trees[0]));
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_a_valid_record_is_complete_79e3e53c,
    "tests/test_helper_bounded_lifetime.py::test_a_valid_record_is_complete[record1-job]",
    {
        let t = temp("valid-unit")?;
        let path = receipt(t.path())?;
        append(
            &path,
            &unit(serde_json::from_str(
                "{\"pid\": 4321, \"pidToken\": null, \"tree\": {\"kind\": \"job\", \"group\": \"numera-abc\"}}",
            )?),
        )?;
        let report = unit_report::read(&path)?;
        assert!(report.complete);
        assert_eq!(report.trees.len(), 1);
        assert_eq!(report.trees[0]["tree"]["kind"], "job");
        assert!(recorded_tree::verified(&report.trees[0]));
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_initialized_zero_unit_report_is_a_real_answer,
    "tests/test_helper_bounded_lifetime.py::test_an_initialized_zero_unit_report_is_a_real_answer",
    {
        let t = temp("initialized-empty")?;
        let path = receipt(t.path())?;
        let report = unit_report::read(&path)?;
        assert!(report.complete);
        assert!(report.trees.is_empty());
        assert!(unit_report::empty(&path)?);
        Ok(())
    }
);
source_test!(
    helper_ownership_an_empty_report_is_a_real_answer,
    "tests/test_helper_ownership.py::test_an_empty_report_is_a_real_answer",
    {
        let t = temp("initialized-empty")?;
        let path = receipt(t.path())?;
        let report = unit_report::read(&path)?;
        assert!(report.complete);
        assert!(report.trees.is_empty());
        assert!(unit_report::empty(&path)?);
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_erased_report_is_not_zero_units,
    "tests/test_helper_bounded_lifetime.py::test_an_erased_report_is_not_zero_units",
    {
        let t = temp("erased-units")?;
        let path = receipt(t.path())?;
        fs::write(&path, b"")?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.trees.is_empty());
        assert!(!unit_report::empty(&path)?);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_empty_file_does_not_prove_zero_units_but_initialized_report_does,
    "tests/test_helper_lifecycle_matrix.py::test_empty_file_does_not_prove_zero_units_but_initialized_report_does",
    {
        let t = temp("empty-vs-receipt")?;
        let path = receipt(t.path())?;
        fs::write(&path, b"")?;
        assert!(!unit_report::read(&path)?.complete);
        fs::remove_file(&path)?;
        unit_report::initialize(&path)?;
        assert!(unit_report::read(&path)?.complete);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_pending_transfer_is_not_zero_ownership,
    "tests/test_helper_lifecycle_matrix.py::test_pending_transfer_is_not_zero_ownership",
    {
        let t = temp("pending-transfer")?;
        let path = receipt(t.path())?;
        let attempt = unit_report::begin_at(&path)?;
        assert!(!unit_report::read(&path)?.complete);
        append(
            &path,
            format!("{{\"event\":\"cancelled\",\"attempt\":\"{attempt}\"}}\n").as_bytes(),
        )?;
        assert!(unit_report::read(&path)?.complete);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_interrupted_unit_line_keeps_pending_receipt,
    "tests/test_helper_lifecycle_matrix.py::test_interrupted_unit_line_keeps_pending_receipt",
    {
        let t = temp("interrupted-unit")?;
        let path = receipt(t.path())?;
        let _attempt = unit_report::begin_at(&path)?;
        append(&path, b"{\"event\":\"unit\"")?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert!(report.problem.is_some());
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_an_undecodable_report_is_incomplete_evidence,
    "tests/test_helper_bounded_lifetime.py::test_an_undecodable_report_is_incomplete_evidence",
    {
        let t = temp("utf8-unit")?;
        let path = receipt(t.path())?;
        append(
            &path,
            &unit(json!({"pid":4321,"pidToken":99,"tree":{"kind":"pgid","group":4321}})),
        )?;
        append(&path, &[255, 254])?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert_eq!(report.trees.len(), 1);
        assert_eq!(report.trees[0]["pid"], 4321);
        Ok(())
    }
);
source_test!(
    helper_completion_regression_invalid_utf8_tail_does_not_discard_a_valid_prior_unit,
    "tests/test_helper_completion_regression.py::test_invalid_utf8_tail_does_not_discard_a_valid_prior_unit",
    {
        let t = temp("utf8-unit")?;
        let path = receipt(t.path())?;
        append(
            &path,
            &unit(json!({"pid":4321,"pidToken":99,"tree":{"kind":"pgid","group":4321}})),
        )?;
        append(&path, &[255, 254])?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert_eq!(report.trees.len(), 1);
        assert_eq!(report.trees[0]["pid"], 4321);
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_a_partial_report_still_yields_the_records_it_did_have,
    "tests/test_helper_bounded_lifetime.py::test_a_partial_report_still_yields_the_records_it_did_have",
    {
        let t = temp("partial-unit")?;
        let path = receipt(t.path())?;
        append(
            &path,
            &unit(json!({"pid":4321,"pidToken":99,"tree":{"kind":"pgid","group":4321}})),
        )?;
        append(&path, b"{\"pid\":\"abc\"}\n")?;
        let report = unit_report::read(&path)?;
        assert!(!report.complete);
        assert_eq!(report.trees.len(), 1);
        assert_eq!(report.trees[0]["pid"], 4321);
        Ok(())
    }
);
source_test!(
    helper_ownership_a_missing_report_is_not_an_empty_one,
    "tests/test_helper_ownership.py::test_a_missing_report_is_not_an_empty_one",
    {
        let t = temp("missing-unit")?;
        let path = receipt(t.path())?;
        fs::remove_file(&path)?;
        assert!(unit_report::read(&path).is_err());
        assert!(unit_report::settle(&path, Duration::from_millis(30)).is_err());
        Ok(())
    }
);
source_test!(
    helper_ownership_no_report_path_at_all_is_not_an_empty_one,
    "tests/test_helper_ownership.py::test_no_report_path_at_all_is_not_an_empty_one",
    {
        let t = temp("absent-unit-path")?;
        assert!(unit_report::read(&t.path().join("jobs/absent.units")).is_err());
        Ok(())
    }
);
source_test!(
    helper_ownership_a_damaged_report_is_not_an_empty_one_75684af0,
    "tests/test_helper_ownership.py::test_a_damaged_report_is_not_an_empty_one[{\"pid\": 1,\\n-truncated]",
    {
        let t = temp("damaged-report")?;
        let path = receipt(t.path())?;
        append(&path, "{\"pid\": 1,\n".as_bytes())?;
        assert!(!unit_report::read(&path)?.complete);
        assert!(!unit_report::empty(&path)?);
        Ok(())
    }
);
source_test!(
    helper_ownership_a_damaged_report_is_not_an_empty_one_758477cb,
    "tests/test_helper_ownership.py::test_a_damaged_report_is_not_an_empty_one[garbage\\n-invalid JSON]",
    {
        let t = temp("damaged-report")?;
        let path = receipt(t.path())?;
        append(&path, "garbage\n".as_bytes())?;
        assert!(!unit_report::read(&path)?.complete);
        assert!(!unit_report::empty(&path)?);
        Ok(())
    }
);
source_test!(
    helper_ownership_a_damaged_report_is_not_an_empty_one_df6adfda,
    "tests/test_helper_ownership.py::test_a_damaged_report_is_not_an_empty_one[{\"tree\":{\"kind\":\"job\",\"group\":\"g\"}}\\n-missing pid]",
    {
        let t = temp("damaged-report")?;
        let path = receipt(t.path())?;
        append(
            &path,
            "{\"tree\":{\"kind\":\"job\",\"group\":\"g\"}}\n".as_bytes(),
        )?;
        assert!(!unit_report::read(&path)?.complete);
        assert!(!unit_report::empty(&path)?);
        Ok(())
    }
);
source_test!(
    helper_ownership_several_units_all_come_back,
    "tests/test_helper_ownership.py::test_several_units_all_come_back",
    {
        let t = temp("multiple-units")?;
        let path = receipt(t.path())?;
        for pid in [4321, 4322, 4323] {
            let attempt = unit_report::begin_at(&path)?;
            unit_report::record_at(
                &path,
                &json!({"pid":pid,"token":99,"tree":{"kind":"pgid","group":pid}}),
                &attempt,
            )?;
        }
        let report = unit_report::read(&path)?;
        assert!(report.complete);
        assert_eq!(
            report
                .trees
                .iter()
                .map(|v| v["pid"].as_i64().expect("pid"))
                .collect::<Vec<_>>(),
            vec![4321, 4322, 4323]
        );
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_settle_does_not_wait_past_its_deadline_for_the_finish_lock,
    "tests/test_helper_bounded_lifetime.py::test_settle_does_not_wait_past_its_deadline_for_the_finish_lock",
    {
        let t = temp("receipt-finish-deadline")?;
        let path = receipt(t.path())?;
        let held = Arc::new(DataLock::for_file(&path)?);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let lock = Arc::clone(&held);
        let owner = thread::spawn(move || {
            let _guard = lock.acquire(Duration::from_secs(1)).expect("owner lock");
            ready_tx.send(()).expect("ready");
            release_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("release");
        });
        ready_rx.recv_timeout(Duration::from_secs(2))?;
        let start = Instant::now();
        let result = unit_report::settle(&path, Duration::from_millis(40));
        let elapsed = start.elapsed();
        release_tx.send(())?;
        owner.join().expect("owner joined");
        assert!(result.is_err());
        assert!(
            elapsed < Duration::from_millis(400),
            "receipt lock exceeded cleanup budget: {elapsed:?}"
        );
        assert!(unit_report::settle(&path, Duration::from_secs(1))?);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_no_unbounded_wait_for_settlement_owner,
    "tests/test_helper_lifecycle_matrix.py::test_no_unbounded_wait_for_settlement_owner",
    {
        let t = temp("receipt-finish-deadline")?;
        let path = receipt(t.path())?;
        let held = Arc::new(DataLock::for_file(&path)?);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let lock = Arc::clone(&held);
        let owner = thread::spawn(move || {
            let _guard = lock.acquire(Duration::from_secs(1)).expect("owner lock");
            ready_tx.send(()).expect("ready");
            release_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("release");
        });
        ready_rx.recv_timeout(Duration::from_secs(2))?;
        let start = Instant::now();
        let result = unit_report::settle(&path, Duration::from_millis(40));
        let elapsed = start.elapsed();
        release_tx.send(())?;
        owner.join().expect("owner joined");
        assert!(result.is_err());
        assert!(
            elapsed < Duration::from_millis(400),
            "receipt lock exceeded cleanup budget: {elapsed:?}"
        );
        assert!(unit_report::settle(&path, Duration::from_secs(1))?);
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_the_finish_lock_is_released_for_the_next_caller,
    "tests/test_helper_bounded_lifetime.py::test_the_finish_lock_is_released_for_the_next_caller",
    {
        let t = temp("receipt-lock-released")?;
        let path = receipt(t.path())?;
        assert!(unit_report::settle(&path, Duration::from_millis(50))?);
        let lock = DataLock::for_file(&path)?;
        let _guard = lock.acquire(Duration::from_millis(50))?;
        Ok(())
    }
);
