//! Verify that manual signed-artifact review cannot publish or mutate refs.
use super::{Result, fail};
use serde_json::Value;

pub fn release_workflows(workflows: &Value) -> Result<()> {
    if let Some(build) = workflows.get("build-desktop") {
        let rows = build["jobs"]["desktop"]["strategy"]["matrix"]["include"]
            .as_array()
            .ok_or("Manual signing matrix missing")?;
        let actual = rows
            .iter()
            .filter_map(|row| {
                Some((
                    row["platform"].as_str()?,
                    row["os"].as_str()?,
                    row["arch"].as_str()?,
                ))
            })
            .collect::<std::collections::BTreeSet<_>>();
        if rows.len() != 3
            || actual
                != std::collections::BTreeSet::from([
                    ("linux-x64", "ubuntu-24.04", "X64"),
                    ("windows-x64", "windows-2025", "X64"),
                    ("macos-arm64", "macos-15", "ARM64"),
                ])
        {
            return fail("Manual signed artifact review requires Linux, Windows and macOS hosts");
        }
    }
    let ci = &workflows["ci"];
    let release = workflows
        .get("release-review")
        .unwrap_or(&workflows["release"]);
    if ci["on"]
        .as_object()
        .is_none_or(|on| on.len() != 1 || !on.contains_key("workflow_dispatch"))
        || release["on"].get("workflow_call").is_none()
        || release["on"].get("workflow_run").is_some()
    {
        return fail("Signed artifact review requires approved manual CI and reusable release");
    }
    let attest = &ci["jobs"]["attest"];
    let review = &ci["jobs"]["review"];
    if attest["needs"] != "required"
        || review["needs"] != serde_json::json!(["required", "attest"])
        || review["uses"] != "./.github/workflows/release-review.yml"
        || ci["permissions"]["contents"] != "read"
        || release["permissions"]["contents"] != "read"
        || ci["jobs"].get("promote").is_some()
        || ci["jobs"].get("publish").is_some()
        || release["jobs"].get("publish").is_some()
    {
        return fail("Signed artifact review requires verified evidence and read-only authority");
    }
    for (guard, terms) in [
        (
            &ci["jobs"]["coverage"]["if"],
            vec![
                "github.actor == 'KiaroSama'",
                "github.repository == 'KiaroSama/Numera-Browser'",
                "inputs.trusted_sha == github.sha",
                "github.ref == 'refs/heads/main'",
                "github.ref == 'refs/heads/migration/native-runtime-checkpoint-20261012'",
            ],
        ),
        (
            &attest["if"],
            vec![
                "github.event_name == 'workflow_dispatch'",
                "github.actor == 'KiaroSama'",
                "inputs.trusted_sha == github.sha",
                "needs.required.result == 'success'",
            ],
        ),
        (
            &review["if"],
            vec![
                "needs.required.result == 'success'",
                "needs.attest.result == 'success'",
            ],
        ),
        (
            &release["jobs"]["review"]["if"],
            vec![
                "github.event_name == 'workflow_dispatch'",
                "github.actor == 'KiaroSama'",
                "github.repository == 'KiaroSama/Numera-Browser'",
                "inputs.source_sha == github.sha",
                "inputs.run_id == github.run_id",
                "github.ref == 'refs/heads/main'",
                "github.ref == 'refs/heads/migration/native-runtime-checkpoint-20261012'",
            ],
        ),
    ] {
        if guard.as_str().is_none_or(|guard| {
            terms.iter().any(|term| !guard.contains(term)) || guard.contains("|| true")
        }) {
            return fail(
                "Manual artifact review owner, repository, source, run or prerequisite guard differs",
            );
        }
    }
    if review["with"]["source_sha"] != "${{ inputs.trusted_sha }}"
        || review["with"]["run_id"] != "${{ github.run_id }}"
    {
        return fail("Artifact review must consume its exact manual source and verification run");
    }
    let prerequisite = [
        "coverage",
        "dashboard",
        "desktop-contracts",
        "docker",
        "codeql",
        "dependencies",
    ];
    for (job, expected) in [
        ("distribution", prerequisite.to_vec()),
        (
            "required",
            prerequisite.into_iter().chain(["distribution"]).collect(),
        ),
    ] {
        let needs = ci["jobs"][job]["needs"]
            .as_array()
            .ok_or("Artifact review prerequisites missing")?;
        if needs.len() != expected.len()
            || expected
                .iter()
                .any(|name| !needs.iter().any(|value| value == name))
        {
            return fail("Artifact review must require every independent prerequisite");
        }
    }
    let secrets = ci["jobs"]["distribution"]["secrets"]
        .as_object()
        .ok_or("Named signing inputs missing")?;
    let named = [
        "NUMERA_RELEASE_SIGNING_KEY",
        "NUMERA_CODE_SIGNING_PFX",
        "NUMERA_CODE_SIGNING_PASSWORD",
    ];
    if secrets.len() != named.len()
        || named.iter().any(|name| {
            secrets
                .get(*name)
                .is_none_or(|value| value != &format!("${{{{ secrets.{name} }}}}"))
        })
    {
        return fail("Signed builds must receive only their explicitly named signing inputs");
    }
    let steps = release["jobs"]["review"]["steps"]
        .as_array()
        .ok_or("Independent release review missing")?;
    let checkouts: Vec<_> = steps
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|name| name.starts_with("actions/checkout@"))
        })
        .collect();
    if checkouts.len() != 1
        || checkouts[0]["with"]["ref"] != "${{ github.sha }}"
        || checkouts[0]["with"]["persist-credentials"] != false
    {
        return fail(
            "Artifact review must directly checkout its trusted run source without persisted credentials",
        );
    }
    let codeql = steps.iter().position(|step| {
        step["run"].as_str().is_some_and(|run| {
            run == "cargo run --manifest-path tools/xtask/Cargo.toml --locked -- ci codeql-verdict"
        })
    });
    let verification = steps.iter().position(|step| step["run"].as_str().is_some_and(|run| run == "cargo run --manifest-path tools/xtask/Cargo.toml --locked -- ci signed-product-review build/owner-review \"${{ inputs.source_sha }}\""));
    if !matches!((codeql, verification), (Some(codeql), Some(verification)) if codeql < verification)
    {
        return fail(
            "Signed artifact review requires independent exact-source CodeQL before signature and identity verification",
        );
    }
    let text = workflows.to_string();
    for required in [
        "workflow_dispatch",
        "KiaroSama",
        "inputs.trusted_sha == github.sha",
        "needs.attest.result == 'success'",
        "refs/heads/main",
        "codeql",
    ] {
        if !text.contains(required) {
            return fail(format!("Manual artifact review policy missing {required}"));
        }
    }
    fn authority(value: &Value) -> bool {
        match value {
            Value::Object(fields) => {
                if fields.contains_key("continue-on-error")
                    || fields.get("secrets").is_some_and(|v| v == "inherit")
                    || fields.get("permissions").is_some_and(|p| {
                        p == "write-all" || p["contents"] == "write" || p["actions"] == "write"
                    })
                {
                    return false;
                }
                fields.values().all(authority)
            }
            Value::Array(values) => values.iter().all(authority),
            Value::String(command) => ![
                "git push",
                "gh release create",
                "gh release upload",
                "gh release edit",
                "--method PATCH",
                "--method=PATCH",
                "-X PATCH",
                "force=true",
            ]
            .iter()
            .any(|forbidden| command.contains(forbidden)),
            _ => true,
        }
    }
    if !authority(workflows) {
        return fail(
            "Artifact review must not publish, mutate refs, inherit credentials or soften failures",
        );
    }
    Ok(())
}
