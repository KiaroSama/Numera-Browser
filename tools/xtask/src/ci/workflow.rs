//! Required hosted job topology, independent compilation and credential scopes.
use super::{Result, fail};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};
pub fn parse(text: &str) -> Result<Value> {
    let value: serde_yaml::Value = serde_yaml::from_str(text)?;
    Ok(serde_json::to_value(value)?)
}
pub fn required_jobs(jobs: &Value, expected: &BTreeSet<String>) -> Result<()> {
    let required = &jobs["required"];
    if required["if"] != "always()" {
        return fail("Required verification must reject failed and skipped dependencies");
    }
    let needs = required["needs"]
        .as_array()
        .ok_or("Required dependencies must be explicit")?
        .iter()
        .map(|name| {
            name.as_str()
                .map(str::to_owned)
                .ok_or("Required dependency is not text")
        })
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    if &needs != expected {
        return fail("Required verification does not gate every maintained tier");
    }
    Ok(())
}
pub fn successful_needs(needs: &Value) -> Result<()> {
    let needs = needs
        .as_object()
        .filter(|needs| !needs.is_empty())
        .ok_or("Required verification dependency evidence missing")?;
    if needs.values().any(|job| job["result"] != "success") {
        return fail("Required verification failed or skipped a maintained tier");
    }
    Ok(())
}
pub fn bounded_jobs(workflow: &Value) -> Result<()> {
    for (name, job) in workflow["jobs"]
        .as_object()
        .ok_or("Workflow jobs missing")?
    {
        if job
            .get("continue-on-error")
            .is_some_and(|value| value != false)
            || job["runs-on"].to_string().contains("self-hosted")
        {
            return fail(format!("Unbounded or softened hosted tier: {name}"));
        }
        if job.get("runs-on").is_some()
            && job["timeout-minutes"]
                .as_u64()
                .is_none_or(|timeout| timeout == 0)
        {
            return fail("Missing hosted job deadline");
        }
        if job["steps"].as_array().is_some_and(|steps| {
            steps.iter().any(|step| {
                step.get("continue-on-error")
                    .is_some_and(|value| value != false)
            })
        }) {
            return fail("Softened required verification step");
        }
    }
    Ok(())
}
pub fn native_automatic(workflow: &Value) -> Result<()> {
    branch_concurrency(workflow)?;
    let events = workflow["on"]
        .as_object()
        .ok_or("Automatic CI triggers must be explicit")?
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if events
        != BTreeSet::from([
            "push".into(),
            "pull_request".into(),
            "workflow_dispatch".into(),
        ])
        || workflow["permissions"] != serde_json::json!({"contents":"read"})
    {
        return fail(
            "Native automatic verification requires push,PR,manual with read-only permissions",
        );
    }
    bounded_jobs(workflow)?;
    let build = &workflow["jobs"]["build"];
    if build.get("needs").is_some()
        || build.get("if").is_some()
        || build["strategy"]["fail-fast"] != false
    {
        return fail("Native compilation must run independently on every platform");
    }
    let matrix = build["strategy"]["matrix"]["include"]
        .as_array()
        .ok_or("Native build matrix missing")?;
    let platforms = matrix
        .iter()
        .filter_map(|row| row["platform"].as_str())
        .collect::<BTreeSet<_>>();
    if platforms != BTreeSet::from(["linux", "windows", "macos"]) || matrix.len() != 3 {
        return fail("Automatic native build must retain Linux,Windows,macOS");
    }
    if build.to_string().contains("secrets.") {
        return fail("Independent compilation cannot receive signing credentials");
    }
    let expected = workflow["jobs"]
        .as_object()
        .ok_or("Jobs missing")?
        .keys()
        .filter(|name| name.as_str() != "required")
        .cloned()
        .collect::<BTreeSet<_>>();
    required_jobs(&workflow["jobs"], &expected)
}
pub fn branch_concurrency(workflow: &Value) -> Result<()> {
    if workflow["concurrency"]
        != serde_json::json!({
            "group":"${{ github.workflow }}-${{ github.event.pull_request.head.repo.full_name || github.repository }}-${{ github.head_ref || github.ref_name }}",
            "cancel-in-progress":true
        })
    {
        return fail(
            "Native verification must replace obsolete runs of the same workflow and source branch",
        );
    }
    Ok(())
}
pub fn dependency_updates(document: &Value) -> Result<()> {
    let updates = document["updates"]
        .as_array()
        .ok_or("Dependency update inventory missing")?;
    let expected = BTreeSet::from([
        ("cargo", "/engine-rust"),
        ("cargo", "/dashboard"),
        ("cargo", "/browser-page-rust"),
        ("cargo", "/desktop/src-tauri"),
        ("cargo", "/tools/xtask"),
        ("cargo", "/tools/xtask/lifecycle-tests"),
        ("cargo", "/tools/security-scan"),
        ("github-actions", "/"),
        ("docker", "/"),
    ]);
    let actual = updates
        .iter()
        .map(|row| {
            Ok((
                row["package-ecosystem"]
                    .as_str()
                    .ok_or("Dependency ecosystem missing")?,
                row["directory"]
                    .as_str()
                    .ok_or("Dependency directory missing")?,
            ))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if document["version"] != 2 || actual != expected || actual.len() != updates.len() {
        return fail(
            "Dependency updates must cover all seven Cargo crates, Actions and Docker exactly once",
        );
    }
    Ok(())
}
pub fn step_once(steps: &Value, fragment: &str, required: bool) -> Result<usize> {
    let matches = steps
        .as_array()
        .ok_or("Steps missing")?
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains(fragment))
                || step["uses"].as_str() == Some(fragment)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1
        || matches[0]
            .1
            .get("continue-on-error")
            .is_some_and(|value| value != false)
        || (required && matches[0].1.get("if").is_some())
    {
        return fail("Required step is missing,duplicated,conditional or softened");
    }
    Ok(matches[0].0)
}
pub fn ordered(steps: &Value, earlier: &[&str], later: &str) -> Result<()> {
    let later = step_once(steps, later, true)?;
    for fragment in earlier {
        if step_once(steps, fragment, true)? >= later {
            return fail("Independent functional acceptance must precede performance");
        }
    }
    Ok(())
}
/// Keep compiler-produced page programs reproducible without moving compiler
/// or network dependencies into ordinary native application builds.
pub fn browser_page_compiler(native: &Value, advisory: &Value) -> Result<()> {
    let lane = &native["jobs"]["static"];
    if lane["runs-on"] != "ubuntu-24.04" || lane.get("if").is_some() {
        return fail("Browser compiler freshness requires the unconditional reviewed Linux host");
    }
    let steps = &lane["steps"];
    let toolchain = step_once(
        steps,
        "dtolnay/rust-toolchain@686976e191b89faba57d3206551f0f330d8cb249",
        true,
    )?;
    let configuration = &steps[toolchain]["with"];
    if configuration["toolchain"] != "1.99.1"
        || configuration["targets"] != "wasm32-unknown-unknown"
        || configuration["components"] != "rustfmt, clippy"
    {
        return fail("Browser compiler target, Rust version or strict checks changed");
    }
    let commands = [
        "cargo xtask ci owned --wall 120 --idle 60 -- cargo fmt --manifest-path browser-page-rust/Cargo.toml --all --check",
        "cargo xtask ci owned --wall 900 --idle 300 -- cargo clippy --manifest-path browser-page-rust/Cargo.toml --locked --bin browser-page-build -- -D warnings",
        "cargo xtask ci owned --wall 900 --idle 300 -- cargo clippy --manifest-path browser-page-rust/Cargo.toml --locked --release --target wasm32-unknown-unknown --lib -- -D warnings",
        "cargo xtask ci owned --wall 900 --idle 300 -- cargo install wasm-bindgen-cli --version 0.2.129 --locked",
        "cargo xtask ci owned --wall 900 --idle 300 -- cargo run --manifest-path browser-page-rust/Cargo.toml --locked --bin browser-page-build -- --check --wasm2js \"$RUNNER_TEMP/numera-binaryen-133/binaryen-version_133/bin/wasm2js\"",
    ];
    let mut last = toolchain;
    for command in commands {
        let index = step_once(steps, command, true)?;
        if steps[index]["run"].as_str() != Some(command) || index <= last {
            return fail("Browser compiler checks must remain exact, bounded and ordered");
        }
        last = index;
    }
    let setup = step_once(steps, "binaryen-version_133-x86_64-linux.tar.gz", true)?;
    let expected = concat!(
        "set -euo pipefail\n",
        "directory=\"$RUNNER_TEMP/numera-binaryen-133\"\n",
        "mkdir \"$directory\"\n",
        "curl --fail --location --proto '=https' --proto-redir '=https' --connect-timeout 10 --max-time 120 https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-linux.tar.gz --output \"$directory/download.tar.gz\"\n",
        "echo \"2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489  $directory/download.tar.gz\" | sha256sum --check --strict\n",
        "tar -xzf \"$directory/download.tar.gz\" -C \"$directory\" binaryen-version_133/bin/wasm2js\n",
        "echo \"52597efec0ad2416f8b5c245864960e775b8ffc94c751cc0b016c4b4986d3bcb  $directory/binaryen-version_133/bin/wasm2js\" | sha256sum --check --strict"
    );
    if steps[setup]["run"].as_str().map(str::trim) != Some(expected)
        || setup <= toolchain
        || setup >= last
    {
        return fail(
            "Browser compiler must use both reviewed upstream checksums before regeneration",
        );
    }
    let audit = &advisory["jobs"]["audit"]["steps"];
    for path in [
        "engine-rust/Cargo.lock",
        "dashboard/Cargo.lock",
        "desktop/src-tauri/Cargo.lock",
        "tools/xtask/Cargo.lock",
        "tools/xtask/lifecycle-tests/Cargo.lock",
        "tools/security-scan/Cargo.lock",
        "browser-page-rust/Cargo.lock",
    ] {
        step_once(audit, &format!("cargo audit --file {path}"), true)?;
    }
    Ok(())
}
pub fn coverage(root: &Path) -> Result<()> {
    super::coverage_inventory::validate(root)?;
    let native = parse(&fs::read_to_string(
        root.join(".github/workflows/native-ci.yml"),
    )?)?;
    native_automatic(&native)?;
    browser_page_compiler(
        &native,
        &parse(&fs::read_to_string(
            root.join(".github/workflows/dependency-review.yml"),
        )?)?,
    )?;
    let mut documents = serde_json::Map::new();
    for name in [
        "ci",
        "desktop-contracts",
        "build-desktop",
        "container",
        "release",
    ] {
        documents.insert(
            name.into(),
            parse(&fs::read_to_string(
                root.join(format!(".github/workflows/{name}.yml")),
            )?)?,
        );
    }
    let documents = Value::Object(documents);
    desktop_contracts(&documents)?;
    macos_contract(&documents["desktop-contracts"])?;
    super::release::release_workflows(&documents)?;
    let rust = parse(&fs::read_to_string(
        root.join(".github/workflows/rust-candidate.yml"),
    )?)?;
    branch_concurrency(&rust)?;
    dependency_updates(&parse(&fs::read_to_string(
        root.join(".github/dependabot.yml"),
    )?)?)?;
    super::browser_benchmark::workflow_history(&native, &rust)?;
    let action = parse(&fs::read_to_string(
        root.join(".github/actions/native-candidate-acceptance/action.yml"),
    )?)?;
    super::browser_benchmark::workflow_measurement(&action)?;
    super::container::definition(root)?;
    super::version::verify(root)?;
    super::scope::check_utf8(&super::scope::tracked(root)?)?;
    println!("Native hosted verification topology and tracked UTF8 scope verified");
    Ok(())
}

pub fn desktop_contracts(workflows: &Value) -> Result<()> {
    let jobs = &workflows["ci"]["jobs"];
    if jobs["desktop-contracts"]
        != serde_json::json!({"needs":"coverage","uses":"./.github/workflows/desktop-contracts.yml"})
    {
        return fail(
            "Desktop contract verification must precede signing without security dependencies or secrets",
        );
    }
    let child = &workflows["desktop-contracts"];
    if child["permissions"] != serde_json::json!({"contents":"read"})
        || child.to_string().contains("secrets.")
        || child["env"]["CARGO_BUILD_JOBS"] != "2"
        || child["env"]["NUMERA_MAX_TEST_WORKERS"] != "2"
    {
        return fail(
            "Desktop contract verification must retain private inputs and bounded workers",
        );
    }
    let contract = &child["jobs"]["contracts"];
    if child["jobs"].as_object().is_none_or(|jobs| jobs.len() != 1)
        || contract["runs-on"] != "${{ matrix.os }}"
        || contract.get("if").is_some()
        || contract["timeout-minutes"] != 45
        || contract["strategy"]
            != serde_json::json!({"fail-fast":false,"max-parallel":2,"matrix":{"os":["ubuntu-latest","windows-latest","macos-latest"]}})
    {
        return fail("Desktop contract verification requires all three bounded hosted platforms");
    }
    let steps = &contract["steps"];
    for command in [
        "cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --all --check",
        "cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --locked --all-targets --no-default-features -- -D warnings",
        "cargo test --manifest-path desktop/src-tauri/Cargo.toml --locked --no-default-features --no-run",
        "cargo test --manifest-path desktop/src-tauri/Cargo.toml --locked --no-default-features -- --test-threads=2",
    ] {
        step_once(steps, command, true).map_err(|_| {
            std::io::Error::other(
                "Desktop contract commands must remain required with unchanged bounds",
            )
        })?;
    }
    let history = step_once(steps, "ci desktop-history", false)
        .map_err(|_| std::io::Error::other("Desktop contract history must remain unique"))?;
    let history_step = &steps[history];
    if history_step["if"] != "runner.os != 'Windows'"
        || history_step["run"]
            != "cargo xtask ci owned --wall 900 --idle 300 -- cargo xtask ci desktop-history"
        || history >= step_once(steps, "--no-default-features --no-run", true)?
    {
        return fail(
            "Desktop contract history must prove the Unix defect before current compilation",
        );
    }
    let distribution = &jobs["distribution"];
    let needs = distribution["needs"]
        .as_array()
        .ok_or("Desktop contract distribution prerequisites missing")?;
    if !needs.iter().any(|name| name == "desktop-contracts")
        || !distribution["if"].as_str().is_some_and(|condition| {
            condition.contains("needs.desktop-contracts.result == 'success'")
        })
        || !jobs["required"]["needs"]
            .as_array()
            .is_some_and(|needs| needs.iter().any(|name| name == "desktop-contracts"))
    {
        return fail("Desktop contract verification must gate distribution and aggregate");
    }
    if workflows["build-desktop"]["jobs"]
        .as_object()
        .is_some_and(|jobs| {
            jobs.values().any(|job| {
                job["steps"].as_array().is_some_and(|steps| {
                    steps.iter().any(|step| {
                        step["run"].as_str().is_some_and(|run| {
                            ["cargo test", "cargo clippy", "cargo fmt"]
                                .iter()
                                .any(|command| run.contains(command))
                        })
                    })
                })
            })
        })
    {
        return fail("Desktop contract commands must not repeat in distribution");
    }
    if jobs["docker"]
        != serde_json::json!({"needs":"coverage","uses":"./.github/workflows/container.yml"})
    {
        return fail("Container verification must remain required through same-commit workflow");
    }
    Ok(())
}

pub fn macos_contract(workflow: &Value) -> Result<()> {
    let steps = workflow["jobs"]["contracts"]["steps"]
        .as_array()
        .ok_or("macOS contract steps missing")?;
    for (phase, wall, idle) in [("build", 900, 300), ("test", 600, 120)] {
        let command = format!(
            "cargo xtask ci owned --wall {wall} --idle {idle} -- cargo xtask ci macos-ownership {phase}"
        );
        let matches = steps
            .iter()
            .filter(|step| step["run"] == command)
            .collect::<Vec<_>>();
        if matches.len() != 1
            || matches[0]["if"] != "runner.os == 'macOS'"
            || matches[0]
                .get("continue-on-error")
                .is_some_and(|value| value != false)
        {
            return fail(
                "macOS ownership command is missing,softened or has changed bounds/platform",
            );
        }
    }
    let checkout = steps
        .iter()
        .find(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|name| name.starts_with("actions/checkout@"))
        })
        .ok_or("History checkout missing")?;
    if checkout["with"]["fetch-depth"] != 0 {
        return fail("macOS ownership requires complete original history");
    }
    Ok(())
}

pub fn source_module_action(action: &Value) -> Result<()> {
    if action["runs"]["using"] != "composite"
        || action["outputs"]["settled"]["value"]
            != "${{ steps.module_8.outputs.settled == 'true' && steps.checkpoint_8.outcome == 'success' }}"
    {
        return fail("Source module checkpoint action has ambiguous settlement");
    }
    let steps = action["runs"]["steps"]
        .as_array()
        .filter(|steps| steps.len() == 18)
        .ok_or("Source module checkpoint action must retain nine immutable uploads")?;
    for index in 0..9 {
        let (run, upload) = (&steps[index * 2], &steps[index * 2 + 1]);
        let expected = if index == 0 {
            "${{ !cancelled() }}".to_owned()
        } else {
            format!(
                "${{{{ !cancelled() && steps.module_{}.outputs.settled == 'true' && steps.checkpoint_{}.outcome == 'success' }}}}",
                index - 1,
                index - 1
            )
        };
        if run["id"] != format!("module_{index}")
            || run["shell"] != "pwsh"
            || run["if"] != expected
            || run
                .get("continue-on-error")
                .is_some_and(|value| value != false)
            || upload["id"] != format!("checkpoint_{index}")
            || upload["with"]["name"] != format!("source-windows-checkpoint-2-module-{index}")
            || upload["if"]
                != format!(
                    "${{{{ !cancelled() && steps.module_{index}.outputs.settled == 'true' }}}}"
                )
        {
            return fail("Source module checkpoint continuation or artifact identity differs");
        }
        let command = run["run"]
            .as_str()
            .ok_or("Source module checkpoint command missing")?;
        let expected = format!(
            "cargo xtask ci owned --wall 600 --idle 120 -- cargo xtask ci source-checkpoint ${{{{ inputs.shard }}}} ${{{{ inputs.shards }}}} 2 4 {index} '${{{{ inputs.output }}}}'\nif ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}\ncargo xtask ci owned --wall 30 --idle 15 -- cargo xtask ci settled-index '${{{{ inputs.output }}}}/checkpoint.json' {index}\nif ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}\n$state = Get-Content '${{{{ inputs.output }}}}/checkpoint.json' -Raw | ConvertFrom-Json\n$testExit = $state.slot_results[{index}]\n\"settled=true\" >> $env:GITHUB_OUTPUT\nexit [int]$testExit\n"
        );
        if command != expected {
            return fail(
                "Source module checkpoint command lost cleanup proof or complete test verdict",
            );
        }
    }
    Ok(())
}
