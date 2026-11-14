use super::*;
use std::cell::{Cell, RefCell};
fn names(size: usize) -> Vec<String> {
    (0..size)
        .map(|index| format!("source_{index:02}"))
        .collect()
}
fn repo(temp: &Temp, modules: &[String]) -> TestResult {
    for module in modules {
        temp.write(
            &format!("engine-rust/tests/{module}.rs"),
            b"#[test] fn contract() { assert_eq!(2 + 2, 4); }\n",
        )?;
    }
    Ok(())
}
fn report(modules: &[String]) -> Value {
    json!({"schema":1,"modules":modules,"results":modules.iter().map(|module|json!({"module":module,"exit":0,"settled":true,"passed":1,"failed":0,"ignored":0,"stdout":"running 1 test\ntest contract ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n"})).collect::<Vec<_>>()})
}
fn fixture_report(root: &Path, value: &Value) -> ci::Result<usize> {
    scope::verify_report_with(root, value, |_, row| {
        ci::coverage_inventory::validate_expected_output(
            &BTreeSet::from(["contract".into()]),
            row["stdout"].as_str().ok_or("Fixture stdout missing")?,
            0,
        )
    })
}
source_test!(
    partitions_cover_source,
    "tests/test_python_source_partition.py::test_all_four_partitions_are_disjoint_complete_and_nonempty",
    {
        secret_review::check()?;
        let modules = scope::targets(&root())?
            .into_iter()
            .filter(|module| module.starts_with("source_"))
            .collect::<Vec<_>>();
        let selected = (0..4)
            .map(|index| scope::partition(&modules, index, 4))
            .collect::<ci::Result<Vec<_>>>()?;
        let all = selected.iter().flatten().collect::<BTreeSet<_>>();
        assert_eq!(all.len(), modules.len());
        assert_eq!(
            selected.iter().map(Vec::len).max().expect("max")
                - selected.iter().map(Vec::len).min().expect("min"),
            1usize.min(modules.len() % 4)
        );
        // The native inventory covers actual Cargo object boundaries; an unregistered
        // nested integration entry cannot quietly disappear from every partition.
        let temp = Temp::new("entry-boundary")?;
        repo(&temp, &names(4))?;
        temp.write("engine-rust/tests/foreign/main.rs", b"fn main() {}")?;
        assert!(scope::targets(&temp.0).is_err());
        Ok(())
    }
);
fn invalid((index, count): (i32, usize)) -> TestResult {
    assert!(
        scope::partition(
            &names(8),
            usize::try_from(index).unwrap_or(usize::MAX),
            count
        )
        .is_err()
    );
    Ok(())
}
source_cases!("tests/test_python_source_partition.py::test_invalid_partition_never_becomes_an_empty_success",invalid,[partition_negative=>("-1-4",(-1,4)),partition_past=>("4-4",(4,4)),partition_zero=>("0-0",(0,0)),partition_five=>("0-5",(0,5))]);
fn module_report(damage: &str) -> TestResult {
    const WORKER: &str = "NUMERA_NATIVE_PHASE_DIAGNOSTIC_FIXTURE";
    if damage == "None"
        && let Ok(mode) = std::env::var(WORKER)
    {
        use std::io::Write;
        let pid = std::process::id();
        let token = numera_core::ownership::identity::started_at(pid)?.ok_or("worker birth")?;
        println!("native fixture stdout marker pid={pid} birth={token}");
        std::io::stdout().flush()?;
        std::io::stderr()
            .write_all(b"native compiler fixture marker\npassword=synthetic-native-")?;
        std::io::stderr().flush()?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::io::stderr().write_all(b"partial-value\n")?;
        std::io::stderr().flush()?;
        if mode == "timeout" {
            std::thread::sleep(std::time::Duration::from_secs(12));
        }
        std::process::exit(if mode == "success" { 0 } else { 7 });
    }
    let temp = Temp::new("report")?;
    let modules = names(4);
    repo(&temp, &modules)?;
    let selected = scope::partition(&modules, 0, 2)?;
    let mut value = report(&selected);
    if damage == "None" {
        let path = temp.0.join("redacted.log");
        let (stdout, stderr) = evidence::retain_test_diagnostics(
            &path,
            b"running 0 tests\n",
            b"Error: signing HRESULT=2147942487 token=synthetic-protected\n",
        )?;
        assert_eq!(stdout, "running 0 tests\n");
        assert!(stderr.contains("HRESULT=2147942487"));
        assert!(!stderr.contains("synthetic-protected"));
        assert_eq!(
            fs::read_to_string(path.with_extension("stderr.log"))?,
            stderr
        );
        // Exercise the production phase boundary with a real nonzero child
        // and a real silent timeout. Neither can erase its partial transcript
        // or become a successful command; redaction sees both flushed writes.
        for mode in ["success", "nonzero", "timeout"] {
            use std::{process::Command, time::Duration};
            let name = std::thread::current()
                .name()
                .ok_or("fixture test name")?
                .to_owned();
            let mut command = Command::new(std::env::current_exe()?);
            command
                .args(["--exact", &name, "--nocapture", "--test-threads=1"])
                .env(WORKER, mode);
            let report = temp.0.join(format!("native-phase-{mode}.log"));
            let captured = scope::capture_native_phase(
                &mut command,
                &report,
                Duration::from_secs(8),
                Duration::from_secs(4),
                65536,
            );
            if mode == "timeout" {
                assert!(
                    captured.is_err(),
                    "a timed-out producer acquired a clean verdict"
                );
            } else {
                let output = captured?;
                assert_eq!(
                    output.exit,
                    if mode == "success" { 0 } else { 7 },
                    "a failed producer acquired a clean verdict"
                );
                assert_eq!(
                    fs::read(&report)?,
                    output.stdout,
                    "complete stdout was not retained"
                );
            }
            let stdout = fs::read_to_string(&report)?;
            let stderr = fs::read_to_string(report.with_extension("stderr.log"))?;
            assert!(stdout.contains("native fixture stdout marker"));
            assert!(stderr.contains("native compiler fixture marker"));
            assert!(!stderr.contains("synthetic-native-") && !stderr.contains("partial-value"));
            let marker = stdout
                .lines()
                .find(|line| line.contains("native fixture stdout marker"))
                .ok_or("worker identity missing")?;
            let fields = marker.split_whitespace().collect::<Vec<_>>();
            let pid = fields
                .iter()
                .find_map(|field| field.strip_prefix("pid="))
                .ok_or("worker pid")?
                .parse()?;
            let token = fields
                .iter()
                .find_map(|field| field.strip_prefix("birth="))
                .ok_or("worker birth")?
                .parse()?;
            assert!(
                !numera_core::ownership::identity::alive(pid, Some(token))?,
                "diagnostic capture abandoned its producer"
            );
        }
    }
    match damage {
        "missing" => {
            value["results"].as_array_mut().expect("rows").pop();
        }
        "foreign" => value["results"][1]["module"] = json!("source_01"),
        "duplicate" => value["results"][1]["module"] = json!(&selected[0]),
        "failure" => value["results"][1]["failed"] = json!(1),
        _ => (),
    };
    let result = fixture_report(&temp.0, &value);
    if damage == "None" {
        assert_eq!(result?, 2);
    } else {
        assert!(result.is_err());
    }
    Ok(())
}
source_cases!("tests/test_python_source_partition.py::test_partition_report_requires_every_selected_module",module_report,[partition_report_good=>("None","None"),partition_report_missing=>("missing","missing"),partition_report_foreign=>("foreign","foreign"),partition_report_duplicate=>("duplicate","duplicate"),partition_report_failure=>("failure","failure")]);
fn directory(damage: &str) -> TestResult {
    let temp = Temp::new("report-directory")?;
    let modules = names(2);
    repo(&temp, &modules)?;
    let output = temp.0.join("reports");
    fs::create_dir(&output)?;
    for module in &modules {
        fs::write(
            output.join(format!("{module}.json")),
            serde_json::to_vec(&report(std::slice::from_ref(module)))?,
        )?;
    }
    if damage == "missing" {
        fs::remove_file(output.join("source_01.json"))?;
    }
    if damage == "extra" {
        fs::write(output.join("source_foreign.json"), b"{}")?;
    }
    let result = scope::report_directory_with(&temp.0, &output, &modules, fixture_report);
    if damage == "None" {
        assert_eq!(result?, 2);
    } else {
        assert!(result.is_err());
    }
    Ok(())
}
source_cases!("tests/test_python_source_partition.py::test_report_directory_inventory_matches_the_partition",directory,[partition_directory_good=>("None","None"),partition_directory_missing=>("missing","missing"),partition_directory_extra=>("extra","extra")]);
fn run_pair(unproved: bool) -> TestResult {
    let temp = Temp::new("partition-pair")?;
    let modules = names(2);
    let mut state = scope::Checkpoint::new(modules.clone(), 1, 0.0)?;
    let now = Cell::new(0.0);
    let calls = RefCell::new(Vec::new());
    let result = scope::execute_block(
        &temp.0,
        &mut state,
        &modules,
        0,
        1,
        None,
        || now.get(),
        |module, wall, idle| {
            calls.borrow_mut().push((module.to_owned(), wall, idle));
            now.set(now.get() + 10.0);
            if unproved {
                return Err(io::Error::other("fixture cleanup unproved").into());
            }
            Ok(i32::from(calls.borrow().len() == 1))
        },
        |_| {},
    );
    if unproved {
        assert!(result.is_err());
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(state.active.as_deref(), Some(modules[0].as_str()));
    } else {
        assert_eq!(result?, 1);
        assert_eq!(
            *calls.borrow(),
            vec![
                (modules[0].clone(), 600.0, 120.0),
                (modules[1].clone(), 590.0, 120.0)
            ]
        );
    }
    let rows = fs::read_to_string(temp.0.join("progress.jsonl"))?
        .lines()
        .map(|line| ci::decode(line.as_bytes()))
        .collect::<ci::Result<Vec<_>>>()?;
    assert_eq!(
        rows.last().expect("progress")["phase"],
        if unproved { "unproved" } else { "finish" }
    );
    Ok(())
}
source_test!(
    partition_collect_failure,
    "tests/test_python_source_partition.py::test_module_failure_is_collected_without_repeating_or_omitting_other_modules",
    { run_pair(false) }
);
source_test!(
    partition_unproved_stops,
    "tests/test_python_source_partition.py::test_unproved_module_cleanup_stops_the_partition",
    { run_pair(true) }
);
source_test!(
    partition_shared_deadline,
    "tests/test_python_source_partition.py::test_checkpoint_blocks_share_one_budget_and_never_repeat_modules",
    {
        let temp = Temp::new("four-blocks")?;
        let modules = names(8);
        let mut state = scope::Checkpoint::new(modules.clone(), 4, 0.0)?;
        let now = Cell::new(0.0);
        let mut calls = Vec::new();
        for block in 0..4 {
            assert_eq!(
                scope::execute_block(
                    &temp.0,
                    &mut state,
                    &modules,
                    block,
                    4,
                    None,
                    || now.get(),
                    |name, wall, _| {
                        calls.push((name.to_owned(), wall));
                        now.set(now.get() + 10.0);
                        Ok(i32::from(name == modules[0]))
                    },
                    |_| {}
                )?,
                i32::from(block == 0)
            );
            now.set(now.get() + 5.0);
        }
        assert_eq!(
            calls.iter().map(|row| row.0.clone()).collect::<Vec<_>>(),
            modules
        );
        assert_eq!(
            calls.iter().map(|row| row.1).collect::<Vec<_>>(),
            vec![600.0, 590.0, 575.0, 565.0, 550.0, 540.0, 525.0, 515.0]
        );
        assert_eq!(state.next, 4);
        assert_eq!(state.end, 600.0);
        assert!(state.admit(&modules, 3, 4, now.get()).is_err());
        Ok(())
    }
);
source_test!(
    partition_expired_checkpoint,
    "tests/test_python_source_partition.py::test_incomplete_or_expired_checkpoint_never_admits_another_block",
    {
        let modules = names(8);
        let mut state = scope::Checkpoint::new(modules.clone(), 4, 0.0)?;
        assert!(state.admit(&modules, 1, 4, 601.0).is_err());
        state.next = 1;
        assert!(state.admit(&modules, 1, 4, 601.0).is_err());
        Ok(())
    }
);
fn notices(damage: &str) -> TestResult {
    let temp = Temp::new("notices")?;
    let mut modules = names(37);
    modules[18..27].clone_from_slice(
        &[
            "source_profile_schema",
            "source_proxy_removal_intent",
            "source_public_surface",
            "source_release_distribution",
            "source_resilient_and_guarded",
            "source_rotate_transport",
            "source_server_api",
            "source_session_tree_mandatory",
            "source_startup_resources",
        ]
        .map(str::to_owned),
    );
    let mut state = scope::Checkpoint::new(modules.clone(), 4, 0.0)?;
    state.next = 2;
    let now = Cell::new(0.0);
    let messages = RefCell::new(Vec::<String>::new());
    let calls = Cell::new(0);
    let result = scope::execute_block(
        &temp.0,
        &mut state,
        &modules,
        2,
        4,
        None,
        || now.get(),
        |name, wall, idle| {
            let index = calls.get();
            let last = messages.borrow().last().expect("before spawn").clone();
            assert!(last.starts_with(&format!(
                "##[notice]Source block starting modules={name} remaining={:.1}s",
                600.0 - index as f64 * 10.0
            )));
            assert_eq!(wall, 600.0 - index as f64 * 10.0);
            assert_eq!(idle, 120.0);
            calls.set(index + 1);
            now.set(now.get() + 10.0);
            if damage == "unproved" && index == 2 {
                return Err(io::Error::other("fixture cleanup unproved").into());
            }
            Ok(i32::from(damage == "ordinary" && index == 0))
        },
        |message| messages.borrow_mut().push(message.into()),
    );
    if damage == "unproved" {
        assert!(result.is_err());
        assert_eq!(calls.get(), 3);
        assert_eq!(state.active.as_deref(), Some("source_public_surface"));
    } else {
        assert_eq!(result?, i32::from(damage == "ordinary"));
        assert_eq!(state.next, 3);
        assert_eq!(calls.get(), 9);
    }
    assert_eq!(messages.borrow().len(), calls.get() + 1);
    assert!(messages.borrow().len() <= 10);
    Ok(())
}
source_cases!("tests/test_python_source_partition.py::test_problem_block_notices_identify_each_admitted_module_before_execution",notices,[partition_notices_good=>("None","None"),partition_notices_failure=>("ordinary","ordinary"),partition_notices_unproved=>("unproved","unproved")]);
fn single((damage, size): (&str, usize)) -> TestResult {
    let temp = Temp::new("module-slots")?;
    let modules = names(size);
    let first = size * 2 / 4;
    let last = size * 3 / 4;
    let mut state = scope::Checkpoint::new(modules.clone(), 4, 0.0)?;
    state.next = 2;
    let now = Cell::new(20.0);
    let calls = RefCell::new(Vec::new());
    for index in 0..9 {
        let fail = (damage == "cleanup" || damage == "interrupted") && index == 2
            || damage == "tail-cleanup" && index == 8;
        let result = scope::execute_block(
            &temp.0,
            &mut state,
            &modules,
            2,
            4,
            Some(index),
            || now.get(),
            |name, wall, idle| {
                let saved = scope::read_checkpoint(&temp.0.join("checkpoint.json"))?;
                assert_eq!(saved.active.as_deref(), Some(name));
                let offset = calls.borrow().len();
                assert_eq!(name, modules[first + offset]);
                assert_eq!(
                    wall,
                    580.0 - 10.0 * offset as f64 - 5.0 * (offset.min(8)) as f64
                );
                assert_eq!(idle, 120.0);
                calls.borrow_mut().push(name.to_owned());
                now.set(now.get() + 10.0);
                if fail {
                    return if damage == "interrupted" {
                        Ok(2)
                    } else {
                        Err(io::Error::other("cleanup unproved").into())
                    };
                }
                Ok(i32::from(
                    damage == "ordinary" && offset == 0 || damage == "tail-failure" && offset == 8,
                ))
            },
            |_| {},
        );
        if fail {
            assert!(result.is_err());
            assert_eq!(calls.borrow().len(), index + 1);
            assert!(state.module_cursor(index, true).is_err());
            assert!(
                scope::execute_block(
                    &temp.0,
                    &mut state,
                    &modules,
                    2,
                    4,
                    Some(index),
                    || now.get(),
                    |_, _, _| panic!("uncertain repeat"),
                    |_| {}
                )
                .is_err()
            );
            break;
        }
        assert_eq!(result?, 0);
        state.module_cursor(index, true)?;
        assert_eq!(
            state.slot_results[index],
            i32::from(damage == "ordinary" && index == 0 || damage == "tail-failure" && index == 8)
        );
        assert_eq!(state.end, 600.0);
        if index < 8 {
            assert!(state.module_cursor(index, false).is_err());
            assert!(state.module_cursor(index + 2, false).is_err());
            assert!(
                scope::execute_block(
                    &temp.0,
                    &mut state,
                    &modules,
                    2,
                    4,
                    None,
                    || now.get(),
                    |_, _, _| panic!("partial full block"),
                    |_| {}
                )
                .is_err()
            );
        }
        now.set(now.get() + 5.0);
    }
    if ["None", "ordinary", "tail-failure"].contains(&damage) {
        assert_eq!(calls.borrow().len(), last - first);
        assert_eq!(state.next, 3);
        assert_eq!(state.module_next, last - first);
        let mut expired = scope::Checkpoint::new(modules.clone(), 4, 0.0)?;
        expired.next = 2;
        assert!(expired.admit(&modules, 2, 4, 601.0).is_err());
    }
    Ok(())
}
source_cases!("tests/test_python_source_partition.py::test_single_module_checkpoints_keep_one_deadline_and_stop_on_uncertain_cleanup",single,[
 partition_slots_good37=>("None-37",("None",37)),partition_slots_good39=>("None-39",("None",39)),partition_slots_good40=>("None-40",("None",40)),partition_slots_good60=>("None-60",("None",60)),
 partition_slots_ordinary37=>("ordinary-37",("ordinary",37)),partition_slots_ordinary39=>("ordinary-39",("ordinary",39)),partition_slots_ordinary40=>("ordinary-40",("ordinary",40)),partition_slots_ordinary60=>("ordinary-60",("ordinary",60)),
 partition_slots_cleanup37=>("cleanup-37",("cleanup",37)),partition_slots_cleanup39=>("cleanup-39",("cleanup",39)),partition_slots_cleanup40=>("cleanup-40",("cleanup",40)),partition_slots_cleanup60=>("cleanup-60",("cleanup",60)),
 partition_slots_interrupted37=>("interrupted-37",("interrupted",37)),partition_slots_interrupted39=>("interrupted-39",("interrupted",39)),partition_slots_interrupted40=>("interrupted-40",("interrupted",40)),partition_slots_interrupted60=>("interrupted-60",("interrupted",60)),
 partition_slots_tail37=>("tail-failure-37",("tail-failure",37)),partition_slots_tail39=>("tail-failure-39",("tail-failure",39)),partition_slots_tail40=>("tail-failure-40",("tail-failure",40)),partition_slots_tail60=>("tail-failure-60",("tail-failure",60)),
 partition_slots_tail_cleanup37=>("tail-cleanup-37",("tail-cleanup",37)),partition_slots_tail_cleanup39=>("tail-cleanup-39",("tail-cleanup",39)),partition_slots_tail_cleanup40=>("tail-cleanup-40",("tail-cleanup",40)),partition_slots_tail_cleanup60=>("tail-cleanup-60",("tail-cleanup",60))
]);
fn partial(pending: bool) -> TestResult {
    let temp = Temp::new("partial-tail")?;
    let modules = names(39);
    let first = 39 * 2 / 4;
    let mut state = scope::Checkpoint::new(modules.clone(), 4, 0.0)?;
    state.next = 2;
    state.module_next = 9;
    state.module_results = modules[first..first + 9]
        .iter()
        .map(|module| scope::ModuleResult {
            module: module.clone(),
            exit: 0,
        })
        .collect();
    state.slot_results = vec![0; 8];
    if pending {
        state.active = Some(modules[first + 9].clone());
    }
    assert!(state.module_cursor(8, true).is_err());
    for (block, slot) in [(2, Some(8)), (2, None), (3, None)] {
        assert!(
            scope::execute_block(
                &temp.0,
                &mut state,
                &modules,
                block,
                4,
                slot,
                || 0.0,
                |_, _, _| panic!("partial final slot spawned"),
                |_| {}
            )
            .is_err()
        );
    }
    Ok(())
}
source_cases!("tests/test_python_source_partition.py::test_partial_final_slot_never_authorizes_retry_or_next_block",partial,[partition_partial_false=>("False",false),partition_partial_true=>("True",true)]);
fn ambiguous(damage: &str) -> TestResult {
    let temp = Temp::new("ambiguous")?;
    let modules = names(if damage == "wrong-size" { 32 } else { 37 });
    let mut state = scope::Checkpoint::new(modules.clone(), 4, 0.0)?;
    state.next = 2;
    let index = usize::from(["bad-verdict", "slot-verdict", "missing-slot"].contains(&damage));
    if damage == "wrong-inventory" {
        state.modules[0] = "source_foreign".into();
    }
    if index == 1 {
        state.module_next = 1;
        state.module_results.push(scope::ModuleResult {
            module: modules[18].clone(),
            exit: if damage == "bad-verdict" {
                124
            } else {
                i32::from(damage == "slot-verdict")
            },
        });
        if damage != "missing-slot" {
            state.slot_results.push(0);
        }
    }
    if damage == "partial-json" {
        fs::write(temp.0.join("checkpoint.json"), b"{")?;
        assert!(scope::read_checkpoint(&temp.0.join("checkpoint.json")).is_err());
        return Ok(());
    }
    assert!(
        scope::execute_block(
            &temp.0,
            &mut state,
            &modules,
            2,
            4,
            Some(index),
            || 0.0,
            |_, _, _| panic!("ambiguous checkpoint spawned"),
            |_| {}
        )
        .is_err()
    );
    Ok(())
}
source_cases!("tests/test_python_source_partition.py::test_module_checkpoint_refuses_ambiguous_saved_state_before_spawn",ambiguous,[partition_ambiguous_size=>("wrong-size","wrong-size"),partition_ambiguous_json=>("partial-json","partial-json"),partition_ambiguous_inventory=>("wrong-inventory","wrong-inventory"),partition_ambiguous_verdict=>("bad-verdict","bad-verdict"),partition_ambiguous_slot=>("slot-verdict","slot-verdict"),partition_ambiguous_missing=>("missing-slot","missing-slot")]);
source_test!(
    partition_action_proves_cleanup,
    "tests/test_python_source_partition.py::test_module_checkpoint_action_keeps_failures_and_requires_previous_cleanup_and_upload",
    {
        let action = workflow::parse(&fs::read_to_string(
            root().join(".github/actions/source-module-checkpoints/action.yml"),
        )?)?;
        workflow::source_module_action(&action)?;
        for damage in [
            "repeat",
            "skip-proof",
            "ignored-exit",
            "read-owner",
            "softened",
            "artifact-collision",
            "false-settled",
            "scalar-exit",
        ] {
            let mut broken = action.clone();
            let run = &mut broken["runs"]["steps"][2];
            match damage {
                "repeat" => {
                    run["run"] = json!(
                        run["run"]
                            .as_str()
                            .expect("command")
                            .replace("2 4 1 '", "2 4 0 '")
                    )
                }
                "skip-proof" => run["if"] = json!("${{ !cancelled() }}"),
                "ignored-exit" => {
                    run["run"] = json!(
                        run["run"]
                            .as_str()
                            .expect("command")
                            .replace("exit [int]$testExit", "exit 0")
                    )
                }
                "read-owner" => {
                    run["run"] = json!(
                        run["run"]
                            .as_str()
                            .expect("command")
                            .replace("ci owned --wall 30 --idle 15", "ci owned")
                    )
                }
                "softened" => run["continue-on-error"] = json!(true),
                "false-settled" => broken["outputs"]["settled"]["value"] = json!("${{ true }}"),
                "scalar-exit" => {
                    run["run"] = json!(
                        run["run"]
                            .as_str()
                            .expect("command")
                            .replace("$state.slot_results[1]", "$state.slot_results[1].exit")
                    )
                }
                _ => {
                    broken["runs"]["steps"][3]["with"]["name"] =
                        json!("source-windows-checkpoint-2-module-0")
                }
            };
            assert!(workflow::source_module_action(&broken).is_err(), "{damage}");
        }
        Ok(())
    }
);
source_test!(
    partition_notice_bound,
    "tests/test_python_source_partition.py::test_durable_notice_blocks_cover_every_module_with_at_most_ten_annotations",
    {
        let modules = names(37);
        let groups = scope::notices(&modules)?;
        assert_eq!(groups.len(), 10);
        assert_eq!(groups.into_iter().flatten().collect::<Vec<_>>(), modules);
        Ok(())
    }
);
