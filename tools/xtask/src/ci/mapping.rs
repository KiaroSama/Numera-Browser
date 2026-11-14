//! Build an honest source-case map from native test declarations. Pending IDs
//! stay pending; counting an ID never creates a test or authorizes cutover.
use super::{Result, fail};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
pub const FILES: &[&str] = &[
    "native_regressions",
    "native_history_phase",
    "native_history_inputs",
    "native_history_resources",
    "python_source_partition",
    "desktop_contract_ci",
    "macos_ownership_ci",
    "migration_gates",
    "native_candidate",
    "build_identity",
    "release_distribution",
    "build_preserves_data",
    "container_browser_contract",
    "camoufox_runtime",
    "browser_benchmark",
    "browser_memory_diagnostics",
    "desktop_history",
    "docker",
    "version_is_one_number",
];
fn string(text: &str, start: usize) -> Result<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return fail("Native case string literal missing");
    }
    let mut end = start + 1;
    let mut escaped = false;
    while end < bytes.len() {
        if bytes[end] == b'"' && !escaped {
            return Ok((serde_json::from_str(&text[start..=end])?, end + 1));
        }
        escaped = bytes[end] == b'\\' && !escaped;
        if bytes[end] != b'\\' {
            escaped = false;
        }
        end += 1;
    }
    fail("Native case literal is incomplete")
}
fn space(text: &str, mut index: usize) -> usize {
    while text
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    index
}
pub fn declarations(text: &str) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let single = regex::Regex::new(r#"source_test!\s*\(\s*([A-Za-z0-9_]+)\s*,\s*"#)?;
    for capture in single.captures_iter(text) {
        let found = capture.get(0).ok_or("Native test declaration missing")?;
        if text.as_bytes().get(found.end()) != Some(&b'"') {
            continue;
        }
        let (id, _) = string(text, found.end())?;
        let name = capture.get(1).ok_or("Native test name missing")?.as_str();
        if result.insert(id, name.into()).is_some() {
            return fail("Duplicate native source-case ID");
        }
    }
    let multi = regex::Regex::new(r#"source_cases!\s*\(\s*"#)?;
    let variant = regex::Regex::new(r#"([A-Za-z0-9_]+)\s*=>\s*\(\s*"#)?;
    for found in multi.find_iter(text) {
        let start = space(text, found.end());
        if text.as_bytes().get(start) != Some(&b'"') {
            continue;
        }
        let (prefix, mut index) = string(text, start)?;
        while text.as_bytes().get(index) != Some(&b'[') {
            index += 1;
            if index >= text.len() {
                return fail("Native case matrix is incomplete");
            }
        }
        index += 1;
        let body = index;
        let mut depth = 1;
        while depth > 0 {
            match text.as_bytes().get(index) {
                Some(b'"') => {
                    let (_, end) = string(text, index)?;
                    index = end;
                    continue;
                }
                Some(b'[') => depth += 1,
                Some(b']') => depth -= 1,
                Some(_) => {}
                None => return fail("Native case matrix is incomplete"),
            };
            index += 1;
        }
        let body = &text[body..index - 1];
        for capture in variant.captures_iter(body) {
            let found = capture.get(0).ok_or("Native matrix case missing")?;
            let (parameter, _) = string(body, found.end())?;
            let name = capture
                .get(1)
                .ok_or("Native matrix case name missing")?
                .as_str();
            if result
                .insert(format!("{prefix}[{parameter}]"), name.into())
                .is_some()
            {
                return fail("Duplicate native source-case ID");
            }
        }
    }
    Ok(result)
}
pub fn build(root: &Path) -> Result<Value> {
    let baseline: Value =
        serde_json::from_slice(&fs::read(root.join("docs/rust-test-baseline.json"))?)?;
    let mut declarations = BTreeMap::new();
    let entry = fs::read_to_string(root.join("tools/xtask/tests/source_ci.rs"))?;
    let module_pattern = regex::Regex::new(
        r#"#\[path\s*=\s*"source_ci/([^"]+)\.rs"\]\s*mod\s+([A-Za-z0-9_]+)\s*;"#,
    )?;
    let modules = module_pattern
        .captures_iter(&entry)
        .map(|capture| (capture[1].to_owned(), capture[2].to_owned()))
        .collect::<BTreeMap<_, _>>();
    let mut paths = vec![root.join("tools/xtask/tests/source_ci.rs")];
    paths.extend(
        fs::read_dir(root.join("tools/xtask/tests/source_ci"))?
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "rs")),
    );
    for path in paths {
        let module = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("Native case module name invalid")?;
        for (id, test) in self::declarations(&fs::read_to_string(&path)?)? {
            let name = if module == "source_ci" {
                test
            } else {
                format!(
                    "{}::{test}",
                    modules.get(module).ok_or(
                        "Source case file is not registered in its Rust integration target"
                    )?
                )
            };
            if declarations
                .insert(
                    id,
                    (
                        name,
                        path.strip_prefix(root)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    ),
                )
                .is_some()
            {
                return fail("Duplicate source case across native modules");
            }
        }
    }
    let mut cases = Vec::new();
    let mut implemented = 0;
    let mut types = BTreeMap::<String, usize>::new();
    for id in baseline["source_test_cases"]["python"]
        .as_array()
        .ok_or("Frozen source IDs missing")?
        .iter()
        .filter_map(Value::as_str)
    {
        let file = id.split("::").next().unwrap_or("");
        if !FILES
            .iter()
            .any(|name| file == format!("tests/test_{name}.py"))
        {
            continue;
        }
        let kind = if id.contains('[') {
            "parameterized"
        } else {
            "standard"
        };
        *types.entry(kind.into()).or_default() += 1;
        let mut row = json!({"python_id":id,"original_type":kind,"status":"pending"});
        if let Some((name, file)) = declarations.remove(id) {
            implemented += 1;
            row["rust_test"] = name.into();
            row["rust_file"] = file.into();
            row["native_type"] = "#[test]".into();
            row["status"] = "implemented".into();
            if id.starts_with("tests/test_browser_benchmark.py::") {
                row["adaptation"]="Compare the pinned native checkpoint application with the native product using the same current Rust SDK. The reviewed two-site transport bridge is recorded by Git blob and SHA256 receipts. This does not claim a Python performance comparison.".into();
            }
            if id.ends_with("test_release_workflow_rejects_failed_forks_and_unapproved_source_refs")
                || id.ends_with(
                    "test_manual_ci_secrets_are_named_and_publish_requires_verified_promotion",
                )
            {
                row["adaptation"]="The user forbids branch mutation and publication. Preserve exact owner/repository/SHA/run, CodeQL, named credentials and required-prerequisite gates while requiring manual artifact attestation and review; refuse promotion and publication commands.".into();
            }
            if id.starts_with("tests/test_migration_gates.py::") {
                row["adaptation"]="Native Rust AST occurrences, independently linked reference modules, owned kernel trees and isolated OS lifetime measurements replace interpreter imports and monkeypatches. Historical fixture hashes remain characterization provenance; no historical Python is executed.".into();
            }
            if id.ends_with(
                "::test_inventory_keeps_alternative_definitions_independently_addressable",
            ) {
                row["historical_program_sha256"] =
                    "22de18dea172c6f67a991fa53ed47dfeaae220f1a139a39f93f5f9f76c274314".into();
                row["adaptation"] = "The historical program hash is provenance metadata only. The real Rust AST inventory retains every conditional occurrence; a collapsed-ID mutation is rejected by the production coverage gate with every duplicate and missing ID asserted. No historical program file is read or executed.".into();
            }
            if id.ends_with(
                "::test_benchmark_requires_natural_settlement_inside_the_original_deadline",
            ) {
                row["historical_program_sha256"] =
                    "e9e65689ecff7dd714e7e1f349c442c02f4ad4992c77532d58c45a7bd2f2aaa1".into();
                row["adaptation"] = "The historical program hash is provenance metadata only. A native child compiles the actual Rust observer with its natural-settlement query bypassed and proves that mutant samples a live tree; the shipped observer rejects the same host. Transient, active, unknown, expired and late cases retain all original deadline, peak-order and cleanup assertions.".into();
            }
        }
        cases.push(row);
    }
    if !declarations.is_empty() {
        return fail(format!(
            "Native CI tests name unknown source IDs: {:?}",
            declarations.keys().collect::<Vec<_>>()
        ));
    }
    Ok(
        json!({"schema":1,"baseline":"docs/rust-test-baseline.json","baseline_source_commit":baseline["source_commit"],"suite":"source_ci","source_count":cases.len(),"implemented_count":implemented,"native_only":true,"case_types":types,"verification":{"status":"not-run"},"adaptations":["Python orchestration boundaries become Rust-authored source/module inventories, structured command plans, actual artifact validators and owned process capture. Every original ID remains independently addressable; pending IDs never authorize cutover.","Parameter matrices emit one #[test] for each original parameter expansion and inject the same missing/duplicate/failed/uncertain evidence into production validators. Actual OS-only observations retain explicit platform limits."],"cases":cases}),
    )
}
pub fn write(root: &Path) -> Result<()> {
    let document = build(root)?;
    fs::write(
        root.join("docs/source-ci-test-map.json"),
        serde_json::to_vec_pretty(&document)?,
    )?;
    println!(
        "Native CI source map: {}/{} implemented",
        document["implemented_count"], document["source_count"]
    );
    Ok(())
}
pub fn write_verified(root: &Path, log: &Path) -> Result<()> {
    let mut document = build(root)?;
    if document["implemented_count"] != document["source_count"] {
        return fail("Pending source CI IDs cannot be verified");
    }
    let names = document["cases"]
        .as_array()
        .ok_or("Native source cases missing")?
        .iter()
        .map(|row| {
            row["rust_test"]
                .as_str()
                .map(str::to_owned)
                .ok_or("Native source test name missing")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut evidence = super::evidence::Current::new(names)?;
    for line in std::str::from_utf8(&fs::read(log)?)?.lines() {
        evidence.accept(line);
    }
    evidence.verify(0)?;
    document["verification"] = json!({"status":"passed","platform":std::env::consts::OS,"reported_passed":document["source_count"],"failed":0,"ignored":0,"log_sha256":super::digest(log)?,"command":"cargo test --locked --manifest-path tools/xtask/Cargo.toml --test source_ci -- --test-threads=2","hosted_windows_macos":"required; not claimed by this local proof"});
    for row in document["cases"]
        .as_array_mut()
        .ok_or("Native source cases missing")?
    {
        row["status"] = "verified".into();
    }
    fs::write(
        root.join("docs/source-ci-test-map.json"),
        serde_json::to_vec_pretty(&document)?,
    )?;
    println!(
        "Verified every named source CI case: {}",
        document["source_count"]
    );
    Ok(())
}
