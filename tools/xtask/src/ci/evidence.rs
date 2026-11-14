use super::{Result, canonical_sha, fail};
use serde_json::Value;
use std::collections::BTreeSet;
pub fn retain_test_diagnostics(
    path: &std::path::Path,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(String, String)> {
    let stdout =
        numera_core::ownership::resources::redact_diagnostics(&String::from_utf8_lossy(stdout));
    let stderr =
        numera_core::ownership::resources::redact_diagnostics(&String::from_utf8_lossy(stderr));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &stdout)?;
    std::fs::write(path.with_extension("stderr.log"), &stderr)?;
    Ok((stdout, stderr))
}

pub fn codeql(pages: &Value, sha: &str) -> Result<u64> {
    if !canonical_sha(sha) {
        return fail("CodeQL source SHA is invalid");
    }
    let pages = pages
        .as_array()
        .filter(|pages| !pages.is_empty())
        .ok_or("CodeQL check response is missing")?;
    let mut selected = Vec::new();
    for page in pages {
        let rows = page
            .get("check_runs")
            .and_then(Value::as_array)
            .ok_or("CodeQL check response is malformed")?;
        selected.extend(rows.iter().filter(|row| {
            row["name"] == "CodeQL" && row["app"]["slug"] == "github-advanced-security"
        }));
    }
    let latest = selected
        .into_iter()
        .max_by_key(|row| row["id"].as_u64().unwrap_or(0))
        .ok_or("Independent CodeQL verdict is unavailable; signing is blocked")?;
    if latest["head_sha"] != sha
        || latest["status"] != "completed"
        || latest["conclusion"] != "success"
    {
        return fail("Independent CodeQL verdict is not successful for this exact source");
    }
    latest["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(|| std::io::Error::other("CodeQL check identity is invalid").into())
}

#[derive(Clone)]
pub struct CodeqlPrContext {
    pub repository: String,
    pub event: String,
    pub reference: String,
    pub number: u64,
    pub event_head: String,
    pub event_base: String,
    pub build_sha: String,
}

#[derive(Clone)]
pub struct CodeqlLocalSource {
    pub commit: String,
    pub tree: String,
    pub clean: bool,
}

#[derive(serde::Serialize)]
pub struct CodeqlSourceSubject {
    pub repository: String,
    pub build_commit: String,
    pub analysis_commit: String,
    pub identical_tree: String,
    pub base_parent: String,
    pub head_parent: String,
    pub pull_request: u64,
}

/// Validate independently fetched GitHub PR and Git-commit objects against the
/// event snapshot and actual clean checkout. This bridges only identical source
/// trees in the current PR's exact integration commit; it does not make a head
/// verdict a verdict about arbitrary merge changes or ancestor revisions.
pub fn codeql_pr_subject(
    context: &CodeqlPrContext,
    pull: &Value,
    merge: &Value,
    head: &Value,
    local: &CodeqlLocalSource,
) -> Result<CodeqlSourceSubject> {
    const REPOSITORY: &str = "KiaroSama/Numera-Browser";
    if context.repository != REPOSITORY
        || context.event != "pull_request"
        || context.number == 0
        || context.reference != format!("refs/pull/{}/merge", context.number)
        || !local.clean
        || local.commit != context.build_sha
        || [
            context.event_head.as_str(),
            context.event_base.as_str(),
            context.build_sha.as_str(),
            local.commit.as_str(),
            local.tree.as_str(),
        ]
        .iter()
        .any(|sha| !canonical_sha(sha))
    {
        return fail("CodeQL PR source context is invalid or checkout is not clean");
    }
    if pull["number"].as_u64() != Some(context.number)
        || pull["state"] != "open"
        || pull["base"]["repo"]["full_name"] != REPOSITORY
        || pull["head"]["repo"]["full_name"] != REPOSITORY
        || pull["head"]["sha"] != context.event_head
        || pull["base"]["sha"] != context.event_base
        || pull["merge_commit_sha"] != context.build_sha
        || merge["sha"] != context.build_sha
        || head["sha"] != context.event_head
    {
        return fail("CodeQL PR source metadata is missing, stale or from another repository");
    }
    let parents = merge["parents"]
        .as_array()
        .ok_or("CodeQL integration commit parents are missing")?;
    if parents.len() != 2
        || parents[0]["sha"] != context.event_base
        || parents[1]["sha"] != context.event_head
    {
        return fail("CodeQL integration commit does not have the exact ordered PR parents");
    }
    if merge["tree"]["sha"] != local.tree || head["tree"]["sha"] != local.tree {
        return fail("CodeQL build and trusted-analysis source trees differ");
    }
    Ok(CodeqlSourceSubject {
        repository: REPOSITORY.into(),
        build_commit: context.build_sha.clone(),
        analysis_commit: context.event_head.clone(),
        identical_tree: local.tree.clone(),
        base_parent: context.event_base.clone(),
        head_parent: context.event_head.clone(),
        pull_request: context.number,
    })
}

pub struct Current {
    expected: BTreeSet<String>,
    passed: BTreeSet<String>,
    started: usize,
    completed: usize,
    invalid: bool,
    wasm: bool,
}
impl Current {
    pub fn new(names: impl IntoIterator<Item = String>) -> Result<Self> {
        let expected = names.into_iter().collect::<BTreeSet<_>>();
        if expected.is_empty() {
            return fail("Native acceptance test inventory is empty");
        }
        Ok(Self {
            expected,
            passed: BTreeSet::new(),
            started: 0,
            completed: 0,
            invalid: false,
            wasm: false,
        })
    }
    pub fn wasm(names: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut evidence = Self::new(names)?;
        evidence.wasm = true;
        Ok(evidence)
    }
    pub fn accept(&mut self, line: &str) {
        if line.starts_with("running ") && line.split_whitespace().count() == 3 {
            self.started += 1;
            let count = line
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<usize>().ok());
            self.invalid |= count != Some(self.expected.len());
        } else if let Some((name, outcome)) = line
            .strip_prefix("test ")
            .and_then(|line| line.rsplit_once(" ... "))
        {
            self.invalid |= outcome != "ok"
                || !self.expected.contains(name)
                || !self.passed.insert(name.into());
        } else if line.starts_with("test result:") {
            self.completed += 1;
            let expression = format!(
                r"^test result: ok\. {} passed; 0 failed; 0 ignored; {}\d+ filtered out; finished in [0-9.]+s$",
                self.expected.len(),
                if self.wasm { "" } else { "0 measured; " }
            );
            self.invalid |=
                !regex::Regex::new(&expression).is_ok_and(|pattern| pattern.is_match(line));
        }
    }
    pub fn verify(&self, exit: i32) -> Result<()> {
        if exit != 0
            || self.started != 1
            || self.completed != 1
            || self.passed != self.expected
            || self.invalid
        {
            return fail("Native acceptance did not pass every required named test");
        }
        Ok(())
    }
}

pub struct Failure {
    name: String,
    symptom: String,
    started: usize,
    named: usize,
    symptomatic: bool,
    summary: usize,
    sections: usize,
    panics: usize,
    listed: usize,
    inside: bool,
    panic_message: bool,
    listing: bool,
    invalid: bool,
    required_values: Option<[String; 2]>,
    value_index: usize,
    values_started: bool,
}
impl Failure {
    pub fn new(name: &str, symptom: &str) -> Self {
        Self {
            name: name.into(),
            symptom: symptom.into(),
            started: 0,
            named: 0,
            symptomatic: false,
            summary: 0,
            sections: 0,
            panics: 0,
            listed: 0,
            inside: false,
            panic_message: false,
            listing: false,
            invalid: false,
            required_values: None,
            value_index: 0,
            values_started: false,
        }
    }
    /// Pin the immediate left/right diagnostics of this named assertion.
    /// Values printed before its panic or in another section cannot satisfy it.
    pub fn with_values(mut self, left: &str, right: &str) -> Self {
        self.required_values = Some([format!("left: {left}"), format!("right: {right}")]);
        self
    }
    pub fn accept(&mut self, line: &str) {
        if line.starts_with("running ") {
            self.started += 1;
            self.invalid |= line != "running 1 test";
        } else if line.starts_with("test result:") {
            self.summary += 1;
            self.invalid |= !regex::Regex::new(r"^test result: FAILED\. 0 passed; 1 failed; 0 ignored; 0 measured; \d+ filtered out; finished in [0-9.]+s$").is_ok_and(|pattern|pattern.is_match(line));
            self.listing = false;
            self.inside = false;
        } else if line.starts_with("test ")
            && (line.ends_with(" ... FAILED")
                || line.ends_with(" ... ok")
                || line.ends_with(" ... ignored"))
        {
            self.named += 1;
            self.invalid |= line != format!("test {} ... FAILED", self.name);
        } else if line.starts_with("---- ") && line.ends_with(" ----") {
            self.sections += 1;
            self.inside = line == format!("---- {} stdout ----", self.name);
            self.invalid |= !self.inside;
            self.panic_message = false;
        } else if line == "failures:" {
            self.listing = self.sections != 0;
            self.inside = false;
            self.panic_message = false;
        } else if self.inside && line.starts_with("thread '") && line.contains(" panicked at ") {
            self.panics += 1;
            self.invalid |= !line.starts_with(&format!("thread '{}'", self.name));
            self.panic_message = true;
        } else if self.inside && self.panic_message && !line.is_empty() {
            // The first panic diagnostic line contains the assertion message.
            // Earlier captured output, compiler messages and Debug left/right
            // values must not stand in for the required production assertion.
            self.symptomatic = line.contains(&self.symptom);
            self.panic_message = false;
            self.values_started = true;
        } else if self.inside
            && self.values_started
            && self.value_index < 2
            && let Some(values) = &self.required_values
            && !line.is_empty()
        {
            self.invalid |= line.trim() != values[self.value_index];
            self.value_index += 1;
        } else if self.listing && !line.is_empty() {
            self.listed += 1;
            self.invalid |= line != format!("    {}", self.name);
        }
    }
    pub fn verify(&self, exit: i32) -> Result<()> {
        if exit != 101
            || self.invalid
            || self.started != 1
            || self.named != 1
            || !self.symptomatic
            || self.summary != 1
            || self.sections != 1
            || self.panics != 1
            || self.listed != 1
            || (self.required_values.is_some() && self.value_index != 2)
        {
            return fail(
                "Historical native regression did not fail at its required production assertion",
            );
        }
        Ok(())
    }
}

pub fn artifact(
    messages: &[u8],
    name: &str,
    source: &str,
    release: bool,
) -> Result<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut finished = None;
    for line in messages
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: Value = serde_json::from_slice(line)?;
        if value["reason"] == "build-finished" {
            if finished.is_some() {
                return fail("Duplicate compiler completion record");
            }
            finished = value["success"].as_bool();
        }
        if value["reason"] == "compiler-artifact"
            && value["target"]["name"] == name
            && value["executable"].is_string()
        {
            let enabled = value["features"].as_array().is_some_and(|features| {
                features
                    .iter()
                    .any(|feature| feature == "native-candidate" || feature == "native-product")
            });
            let bin = value["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"));
            let optimized = value["profile"]["opt_level"]
                .as_str()
                .is_some_and(|level| level != "0")
                && value["profile"]["debug_assertions"] == false;
            if !enabled
                || !bin
                || value["profile"]["test"] != false
                || (release && !optimized)
                || value["package_id"]
                    .as_str()
                    .is_none_or(|id| !id.contains(source))
            {
                return fail("Compiler artifact is not the enabled current-source executable");
            }
            found.push(std::path::PathBuf::from(
                value["executable"].as_str().ok_or("Executable missing")?,
            ));
        }
    }
    if finished != Some(true) || found.len() != 1 {
        return fail("Cargo did not produce exactly one successful executable");
    }
    Ok(found.remove(0))
}

pub fn cargo_projection(line: &str) -> Option<String> {
    let event: Value = serde_json::from_str(line).ok()?;
    match event["reason"].as_str() {
        Some("compiler-artifact") => {
            let name = event["target"]["name"].as_str()?;
            let fresh = event["fresh"].as_bool()?;
            if !regex::Regex::new(r"^[A-Za-z0-9_-]{1,128}$")
                .ok()?
                .is_match(name)
            {
                return None;
            }
            Some(format!("Cargo artifact target={name} fresh={fresh}"))
        }
        Some("build-finished") => Some(format!(
            "Cargo build finished success={}",
            event["success"].as_bool()?
        )),
        _ => None,
    }
}

/// Cargo JSON can contain build-script environment values. Once Cargo emits a
/// typed completion record, subsequent stdout belongs to the test executable.
#[derive(Default)]
pub struct CargoProgress {
    finished: bool,
}
impl CargoProgress {
    pub fn project(&mut self, line: &str) -> Option<String> {
        if self.finished || !line.trim_start().starts_with('{') {
            return Some(line.into());
        }
        let projection = cargo_projection(line);
        if serde_json::from_str::<Value>(line)
            .ok()
            .is_some_and(|event| {
                event["reason"] == "build-finished" && event["success"].is_boolean()
            })
        {
            self.finished = true;
        }
        projection
    }
}
