//! Bind independent CodeQL results to the checked-out source.
use super::{Result, canonical_sha, capture, decode, evidence, fail};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

pub(super) fn codeql_output(
    command: &mut Command,
    deadline: Instant,
    bytes: usize,
) -> Result<Vec<u8>> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return fail("CodeQL source verification deadline expired");
    }
    let output = capture(command, remaining, remaining, bytes)?;
    if output.exit != 0 {
        return fail("CodeQL authenticated source verification command failed");
    }
    Ok(output.stdout)
}
fn codeql_api(endpoint: &str, deadline: Instant) -> Result<Value> {
    decode(&codeql_output(
        Command::new("gh").args([
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            endpoint,
        ]),
        deadline,
        4 * 1024 * 1024,
    )?)
}
pub fn codeql_local_source(root: &Path, deadline: Instant) -> Result<evidence::CodeqlLocalSource> {
    let git_text = |args: &[&str]| -> Result<String> {
        Ok(String::from_utf8(codeql_output(
            Command::new("git")
                .env("GIT_NO_REPLACE_OBJECTS", "1")
                .arg("-C")
                .arg(root)
                .args(args),
            deadline,
            4 * 1024 * 1024,
        )?)?
        .trim()
        .into())
    };
    Ok(evidence::CodeqlLocalSource {
        commit: git_text(&["rev-parse", "--verify", "HEAD"])?,
        tree: git_text(&["rev-parse", "--verify", "HEAD^{tree}"])?,
        clean: git_text(&["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty(),
    })
}
pub(super) fn codeql_pr_source(
    root: &Path,
    repository: &str,
    sha: &str,
    deadline: Instant,
) -> Result<Value> {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return fail("CodeQL PR source bridge requires the GitHub Actions context");
    }
    let event_path =
        PathBuf::from(std::env::var_os("GITHUB_EVENT_PATH").ok_or("PR event is missing")?);
    if fs::metadata(&event_path)?.len() > 4 * 1024 * 1024 {
        return fail("CodeQL PR event exceeds its bounded input size");
    }
    let event = decode(&fs::read(event_path)?)?;
    if event["repository"]["full_name"] != repository {
        return fail("CodeQL PR event repository differs from the owning repository");
    }
    let context = evidence::CodeqlPrContext {
        repository: repository.into(),
        event: std::env::var("GITHUB_EVENT_NAME")?,
        reference: std::env::var("GITHUB_REF")?,
        number: event["number"].as_u64().ok_or("PR number is missing")?,
        event_head: event["pull_request"]["head"]["sha"]
            .as_str()
            .ok_or("PR head is missing")?
            .into(),
        event_base: event["pull_request"]["base"]["sha"]
            .as_str()
            .ok_or("PR base is missing")?
            .into(),
        build_sha: sha.into(),
    };
    if !canonical_sha(&context.event_head) || !canonical_sha(&context.event_base) {
        return fail("CodeQL PR event source identifiers are invalid");
    }
    let local = codeql_local_source(root, deadline)?;
    let pull = codeql_api(
        &format!("repos/{repository}/pulls/{}", context.number),
        deadline,
    )?;
    let merge = codeql_api(&format!("repos/{repository}/git/commits/{sha}"), deadline)?;
    let head = codeql_api(
        &format!("repos/{repository}/git/commits/{}", context.event_head),
        deadline,
    )?;
    Ok(serde_json::to_value(evidence::codeql_pr_subject(
        &context, &pull, &merge, &head, &local,
    )?)?)
}
pub(super) fn retain_codeql_subject(subject: &Value, check: Option<u64>) -> Result<()> {
    let receipt = serde_json::json!({
        "schema":1, "source_subject":subject,
        "trusted_check_id":check,
        "trusted_verdict":if check.is_some() {"successful"} else {"pending_or_unsuccessful"}
    });
    let parent = std::env::var_os("RUNNER_TEMP")
        .map(PathBuf::from)
        .ok_or("CodeQL subject receipt requires the runner temporary directory")?
        .join("numera-codeql-verdict");
    fs::create_dir_all(&parent)?;
    fs::write(
        parent.join("source-subject.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!(
        "CodeQL source subject: {}",
        serde_json::to_string(&receipt)?
    );
    Ok(())
}
