use super::*;
fn workflows() -> ci::Result<Value> {
    let mut all = serde_json::Map::new();
    for name in ["ci", "desktop-contracts", "build-desktop", "container"] {
        all.insert(
            name.into(),
            workflow::parse(&fs::read_to_string(
                root().join(format!(".github/workflows/{name}.yml")),
            )?)?,
        );
    }
    Ok(Value::Object(all))
}
fn independent(damage: &str) -> TestResult {
    let mut all = workflows()?;
    let jobs = &mut all["ci"]["jobs"];
    match damage {
        "caller" => {
            jobs["desktop-contracts"]["uses"] = json!("./.github/workflows/build-desktop.yml")
        }
        "gated" => jobs["desktop-contracts"]["needs"] = json!(["coverage", "codeql"]),
        "secret" => jobs["desktop-contracts"]["secrets"] = json!("inherit"),
        "aggregate" => jobs["required"]["needs"]
            .as_array_mut()
            .expect("fixture shape")
            .retain(|v| v != "desktop-contracts"),
        "distribution" => jobs["distribution"]["needs"]
            .as_array_mut()
            .expect("fixture shape")
            .retain(|v| v != "desktop-contracts"),
        "condition" => {
            jobs["distribution"]["if"] = json!(
                jobs["distribution"]["if"]
                    .as_str()
                    .expect("fixture shape")
                    .replace("needs.desktop-contracts.result == 'success'", "true")
            )
        }
        "container" => jobs["docker"]["uses"] = json!("./.github/workflows/desktop-contracts.yml"),
        _ => {}
    }
    let contract = &mut all["desktop-contracts"]["jobs"]["contracts"];
    match damage {
        "platform" => contract["strategy"]["matrix"]["os"]
            .as_array_mut()
            .expect("fixture shape")
            .retain(|v| v != "windows-latest"),
        "workers" => contract["strategy"]["max-parallel"] = json!(3),
        "compile" | "test" => {
            let fragment = if damage == "compile" {
                "--no-run"
            } else {
                "--test-threads=2"
            };
            let step = contract["steps"]
                .as_array_mut()
                .expect("fixture shape")
                .iter_mut()
                .find(|step| {
                    step["run"]
                        .as_str()
                        .is_some_and(|run| run.contains(fragment))
                })
                .expect("fixture shape");
            step["if"] = json!("runner.os == 'Linux'");
        }
        _ => {}
    }
    if damage == "duplicate" {
        all["build-desktop"]["jobs"]["desktop"]["steps"]
            .as_array_mut()
            .expect("fixture shape")
            .push(json!({"run":"cargo test --locked --no-default-features -- --test-threads=2"}));
    }
    if damage == "environment" {
        all["desktop-contracts"]["env"]["CARGO_BUILD_JOBS"] = json!("8");
    }
    verdict(workflow::desktop_contracts(&all), damage == "None");
    Ok(())
}
source_cases!("tests/test_desktop_contract_ci.py::test_desktop_contracts_are_independent_required_and_not_repeated",independent,[desktop_ci_normal=>("None","None"),desktop_ci_caller=>("caller","caller"),desktop_ci_gated=>("gated","gated"),desktop_ci_secret=>("secret","secret"),desktop_ci_platform=>("platform","platform"),desktop_ci_workers=>("workers","workers"),desktop_ci_aggregate=>("aggregate","aggregate"),desktop_ci_distribution=>("distribution","distribution"),desktop_ci_condition=>("condition","condition"),desktop_ci_compile=>("compile","compile"),desktop_ci_test=>("test","test"),desktop_ci_duplicate=>("duplicate","duplicate"),desktop_ci_environment=>("environment","environment"),desktop_ci_container=>("container","container")]);
fn history(damage: &str) -> TestResult {
    let mut all = workflows()?;
    let steps = all["desktop-contracts"]["jobs"]["contracts"]["steps"]
        .as_array_mut()
        .expect("fixture shape");
    let index = steps
        .iter()
        .position(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains("ci desktop-history"))
        })
        .ok_or("History proof missing")?;
    match damage {
        "missing" => {
            steps.remove(index);
        }
        "duplicate" => {
            steps.push(steps[index].clone());
        }
        "platform" => steps[index]["if"] = json!("runner.os == 'Linux'"),
        "bound" => {
            steps[index]["run"] = json!(
                steps[index]["run"]
                    .as_str()
                    .expect("fixture shape")
                    .replace("--wall 900", "--wall 1800")
            )
        }
        "order" => {
            let step = steps.remove(index);
            steps.push(step);
        }
        "ignored" => steps[index]["continue-on-error"] = json!(true),
        _ => {}
    }
    verdict(workflow::desktop_contracts(&all), damage == "None");
    Ok(())
}
source_cases!("tests/test_desktop_contract_ci.py::test_desktop_history_retains_the_named_unix_negative_proof_before_current_tests",history,[desktop_ci_history_normal=>("None","None"),desktop_ci_history_missing=>("missing","missing"),desktop_ci_history_duplicate=>("duplicate","duplicate"),desktop_ci_history_platform=>("platform","platform"),desktop_ci_history_bound=>("bound","bound"),desktop_ci_history_order=>("order","order"),desktop_ci_history_ignored=>("ignored","ignored")]);
