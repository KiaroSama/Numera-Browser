//! Hosted browser measurement policy.
use super::{Result, fail};
use serde_json::{Value, json};
pub fn workflow_history(workflow: &Value, native: &Value) -> Result<()> {
    let live = &workflow["jobs"]["live"];
    if live["strategy"]["matrix"]["os"] != json!(["ubuntu-24.04", "windows-latest"]) {
        return fail("Browser comparisons require supported native ownership platforms");
    }
    let steps = live["steps"]
        .as_array()
        .ok_or("Browser workflow steps missing")?;
    let checkouts = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s["uses"]
                .as_str()
                .is_some_and(|v| v.starts_with("actions/checkout@"))
        })
        .collect::<Vec<_>>();
    if checkouts.len() != 1
        || checkouts[0].1["with"]["fetch-depth"] != 0
        || checkouts[0].1.get("if").is_some()
        || checkouts[0].0 != 0
    {
        return fail(
            "Browser comparison requires unconditional original history before acceptance",
        );
    }
    let steps = &native["jobs"]["tooling"]["steps"];
    let checks = steps
        .as_array()
        .ok_or("Native SDK steps missing")?
        .iter()
        .filter(|step| {
            step["run"].as_str().is_some_and(|run| {
                run.contains("cargo clippy")
                    && run.contains("--features native-browser")
                    && run.contains("--all-targets")
            })
        })
        .collect::<Vec<_>>();
    if checks.len() != 1
        || checks[0].get("if").is_some()
        || checks[0]
            .get("continue-on-error")
            .is_some_and(|v| v != false)
    {
        return fail("Native Rust checks must include required SDK all-target Clippy");
    }
    Ok(())
}
pub fn workflow_measurement(action: &Value) -> Result<()> {
    let steps = &action["runs"]["steps"];
    let build = super::super::workflow::step_once(steps, "build-candidate", true)?;
    if !steps[build]["run"]
        .as_str()
        .is_some_and(|run| run.contains("--release"))
    {
        return fail("Browser acceptance requires optimized candidate compilation");
    }
    let acceptance = super::super::workflow::step_once(steps, "ci acceptance ", true)?;
    let measure = super::super::workflow::step_once(steps, "ci browser-benchmark", false)?;
    if steps[acceptance]["id"] != "acceptance"
        || steps[measure]["if"] != "${{ !cancelled() && steps.acceptance.outcome == 'success' }}"
    {
        return fail("Each benchmark requires actual application acceptance");
    }
    if steps[measure]["env"]["NUMERA_HOSTED_PERFORMANCE_DIAGNOSTIC"] != "1" {
        return fail("Hosted browser performance comparisons must remain diagnostic");
    }
    if !steps[measure]["run"]
        .as_str()
        .is_some_and(|run| run.contains("\"$NUMERA_NATIVE_ENGINE\""))
    {
        return fail("Browser comparison must measure the actual native product binary");
    }
    let entry = super::super::workflow::step_once(steps, "ci entry-benchmark", false)?;
    if steps[entry]["if"] != "inputs.engine == 'playwright'"
        || steps[entry]["env"]["NUMERA_HOSTED_PERFORMANCE_DIAGNOSTIC"] != "1"
        || entry <= acceptance
        || entry >= measure
        || !steps[entry]["run"]
            .as_str()
            .is_some_and(|run| run.contains("--wall 180 --idle 90"))
    {
        return fail(
            "All five entry workloads require their bounded owning Windows/Linux acceptance legs",
        );
    }
    if build >= measure {
        return fail("Browser measurement cannot precede candidate compilation");
    }
    let uploads = steps
        .as_array()
        .ok_or("Browser evidence steps missing")?
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("actions/upload-artifact@"))
        })
        .collect::<Vec<_>>();
    if uploads.len() != 1
        || uploads[0]["with"]["if-no-files-found"] != "error"
        || !uploads[0]["with"]["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("browser-benchmark-"))
        || !uploads[0]["with"]["path"]
            .as_str()
            .is_some_and(|path| path.contains("browser-performance.json"))
    {
        return fail("Browser measurements must retain exact-source evidence");
    }
    Ok(())
}
