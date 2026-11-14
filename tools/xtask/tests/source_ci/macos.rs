use super::*;
#[path = "macos_session.rs"]
mod session_bridge;
fn phase(phase: &str) -> TestResult {
    let plan = history::macos_plan(Path::new("/fixture-native-root"), phase)?;
    if phase == "build" {
        let cases = history::macos_cases();
        assert_eq!(cases.len(), 2);
        for case in cases {
            assert_eq!(case.platform.as_deref(), Some("darwin"));
            assert_eq!(case.target, "lib");
            assert_eq!(case.commit, "897caadef85053d6d74896e901fc9a21f7baa6dd");
            assert!(case.test.starts_with("ownership::macos::tests::"));
            for (path, hash) in case.files {
                assert_eq!(
                    ci::sha(&ci::git(
                        &root(),
                        &["show", &format!("{}:{path}", case.commit)]
                    )?),
                    hash
                );
            }
        }
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0][1], "fetch");
        let command = &plan[1];
        assert!(command.iter().any(|arg| arg == "--no-run"));
        assert_eq!(
            command
                .iter()
                .filter(|arg| arg.as_str() == "--test")
                .count(),
            2
        );
        assert!(command.iter().any(|arg| arg == "ownership_identity_parity"));
        assert!(command.iter().any(|arg| arg == "ownership_tree_parity"));
        session_bridge::verify()?;
    } else {
        assert_eq!(plan.len(), 6);
        assert!(plan[0].iter().any(|arg| arg == "ownership::macos::tests::"));
        for command in &plan[1..5] {
            assert!(command.iter().any(|arg| arg == "--exact"));
            assert!(command.iter().any(|arg| arg == "--test-threads=1"));
        }
        assert!(plan[5].iter().any(|arg| arg == "ownership_tree_parity"));
    }
    assert!(
        plan.iter()
            .all(|argv| argv.iter().any(|arg| arg == "--locked"))
    );
    Ok(())
}
source_cases!("tests/test_macos_ownership_ci.py::test_macos_ownership_phases_retain_public_seams_and_bounds",phase,[macos_phase_build=>("build","build"),macos_phase_test=>("test","test")]);
fn current(damage: &str) -> TestResult {
    let mut lines=vec!["running 2 tests".to_owned(),"test fixture_identity ... ok".into(),"test fixture_tree ... ok".into(),"test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.01s".into()];
    match damage{"zero"=>lines=vec!["running 0 tests".into(),"test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s".into()],"missing"=>{lines.remove(2);},"duplicate"=>lines[2]=lines[1].clone(),"ignored"=>lines[2]="test fixture_tree ... ignored".into(),"wrong-name"=>lines[2]="test unrelated ... ok".into(),"summary"=>lines[3]=lines[3].replace("0 ignored","1 ignored"),"incomplete"=>{lines.pop();},_=>{}}
    let mut evidence = evidence::Current::new(["fixture_identity".into(), "fixture_tree".into()])?;
    for line in lines {
        evidence.accept(&line);
    }
    verdict(
        evidence.verify(i32::from(damage == "exit")),
        damage == "None",
    );
    Ok(())
}
source_cases!("tests/test_macos_ownership_ci.py::test_macos_current_acceptance_rejects_false_green_output",current,[macos_current_good=>("None","None"),macos_current_zero=>("zero","zero"),macos_current_missing=>("missing","missing"),macos_current_duplicate=>("duplicate","duplicate"),macos_current_ignored=>("ignored","ignored"),macos_current_wrong=>("wrong-name","wrong-name"),macos_current_summary=>("summary","summary"),macos_current_exit=>("exit","exit"),macos_current_incomplete=>("incomplete","incomplete")]);
fn host(damage: &str) -> TestResult {
    let (mut actions, mut repository, mut runner, mut system) =
        ("true", "KiaroSama/Numera-Browser", "macOS", "darwin");
    match damage {
        "repository" => repository = "fixture/other",
        "actions" => actions = "",
        "runner" => runner = "Linux",
        "system" => system = "linux",
        _ => {}
    }
    assert!(history::macos_host(actions, repository, runner, system).is_err());
    Ok(())
}
source_cases!("tests/test_macos_ownership_ci.py::test_macos_ownership_refuses_foreign_hosts_before_any_execution",host,[macos_host_repository=>("repository","repository"),macos_host_actions=>("actions","actions"),macos_host_runner=>("runner","runner"),macos_host_system=>("system","system")]);
pub(super) fn contract_fixture() -> Value {
    json!({"jobs":{"contracts":{"steps":[{"uses":"actions/checkout@reviewed","with":{"fetch-depth":0}},{"if":"runner.os == 'macOS'","run":"cargo xtask ci owned --wall 900 --idle 300 -- cargo xtask ci macos-ownership build"},{"if":"runner.os == 'macOS'","run":"cargo xtask ci owned --wall 600 --idle 120 -- cargo xtask ci macos-ownership test"}]}}})
}
fn contract(damage: &str) -> TestResult {
    let mut workflow = contract_fixture();
    let steps = workflow["jobs"]["contracts"]["steps"]
        .as_array_mut()
        .ok_or("fixture steps")?;
    match damage {
        "build" => {
            steps.remove(1);
        }
        "test" => {
            steps.remove(2);
        }
        "platform" => steps[1]["if"] = "runner.os == 'Linux'".into(),
        "bound" => {
            steps[1]["run"] = steps[1]["run"]
                .as_str()
                .ok_or("fixture run")?
                .replace("--wall 900", "--wall 1800")
                .into()
        }
        "history" => steps[0]["with"]["fetch-depth"] = 1.into(),
        _ => {}
    }
    verdict(workflow::macos_contract(&workflow), damage == "None");
    Ok(())
}
source_cases!("tests/test_macos_ownership_ci.py::test_macos_ownership_is_required_in_existing_three_platform_contract_job",contract,[macos_workflow_good=>("None","None"),macos_workflow_build=>("build","build"),macos_workflow_test=>("test","test"),macos_workflow_platform=>("platform","platform"),macos_workflow_bound=>("bound","bound"),macos_workflow_history=>("history","history")]);
