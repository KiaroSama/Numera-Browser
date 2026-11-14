//! Exact historical source ownership and named semantic failure evidence.
use super::{Result, canonical_sha, capture, digest, fail, git, regular};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub commit: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub target: String,
    pub test: String,
    pub symptom: String,
    pub commit: String,
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub companions: BTreeMap<String, Source>,
    #[serde(default)]
    pub platform: Option<String>,
}
#[derive(Deserialize)]
pub struct Registry {
    pub base: Vec<Case>,
    pub current: Vec<Case>,
    pub storage_sources: BTreeMap<String, String>,
    pub storage_commit: String,
    pub settings_sources: BTreeMap<String, String>,
    pub settings_commit: String,
    pub added_settings: Vec<Case>,
    pub expected: Vec<Case>,
}
impl Registry {
    pub fn read() -> Result<Self> {
        Ok(serde_json::from_str(include_str!("history-cases.json"))?)
    }
    pub fn storage(&self) -> Vec<Case> {
        let mut cases = self.base.clone();
        for case in &mut cases {
            fill_companions(
                case,
                &self.storage_sources,
                &self.storage_commit,
                &self.storage_sources,
            );
        }
        cases.push(Case{target:"repo_parity".into(),test:"settings_errors::storage_settings_errors_match_original_lazy_reads_and_preserve_bytes".into(),symptom:"Storage settings admission differs".into(),commit:self.storage_commit.clone(),files:self.storage_sources.clone(),companions:BTreeMap::new(),platform:None});
        cases
    }
    pub fn settings(&self) -> Vec<Case> {
        let mut cases = self.storage();
        for case in &mut cases {
            fill_companions(
                case,
                &self.settings_sources,
                &self.settings_commit,
                &self.storage_sources,
            );
        }
        cases.extend(self.added_settings.clone());
        cases
    }
    pub fn extension(&self) -> Vec<Case> {
        let mut cases = self.settings();
        for case in &mut cases {
            if case
                .files
                .contains_key("engine-rust/src/storage/extensions.rs")
                && !case
                    .files
                    .contains_key("engine-rust/src/http/extension_routes.rs")
            {
                case.companions
                    .entry("engine-rust/src/http/extension_routes.rs".into())
                    .or_insert(Source {
                        commit: "edca9d09805ccdc2d29735c8977a4354c71223b7".into(),
                        sha256: "d6624367937929bd0e719243925d12a94787382525c01c0717e58e916dea2ac2"
                            .into(),
                    });
            }
        }
        cases
    }
    pub fn cases(&self) -> Result<Vec<Case>> {
        let mut cases = self.extension();
        cases.extend(self.current.clone());
        for case in &cases {
            case_sources(case)?;
        }
        if cases != self.expected {
            return fail(
                "Historical registry coherence differs from its reviewed source identities",
            );
        }
        Ok(cases)
    }
}
fn fill_companions(
    case: &mut Case,
    sources: &BTreeMap<String, String>,
    commit: &str,
    related: &BTreeMap<String, String>,
) {
    if case
        .files
        .keys()
        .chain(case.companions.keys())
        .any(|name| sources.contains_key(name) || related.contains_key(name))
    {
        for (name, hash) in sources {
            if !case.files.contains_key(name) {
                case.companions.entry(name.clone()).or_insert(Source {
                    commit: commit.into(),
                    sha256: hash.clone(),
                });
            }
        }
    }
}
pub fn proof_command(root: &Path, case: &Case, compile: bool, platform: &str) -> Command {
    let manifest = if case.target == "desktop-updater" {
        root.join("desktop/src-tauri/Cargo.toml")
    } else {
        root.join("engine-rust/Cargo.toml")
    };
    let mut command = Command::new("cargo");
    if platform == "win32" {
        // VsDevCmd's documented telemetry gate; ownership remains strict for
        // every process, including any helper started by another tool path.
        command.env("VSCMD_SKIP_SENDTELEMETRY", "1");
    }
    command
        .arg("test")
        .arg("--manifest-path")
        .arg(manifest)
        .args([
            "--locked",
            "--offline",
            "--no-default-features",
            "--config",
            "profile.dev.package.numera-core.opt-level=0",
            "--timings",
        ]);
    if !compile && platform == "win32" {
        command.arg("--message-format=json");
        super::compiler::configure(&mut command, true);
    }
    // This native HTTP fixture's clock seam is explicitly feature-gated in
    // its Cargo target. It does not enable an application or SDK default.
    if ["profile_read_parity", "first_run_failure"].contains(&case.target.as_str()) {
        command.args(["--features", "test-support"]);
    }
    if case.target == "lib" || case.target == "desktop-updater" {
        command.arg("--lib");
    } else {
        command.arg("--test").arg(&case.target);
    }
    if compile {
        command.arg("--no-run");
    } else {
        command.args([
            "--",
            &case.test,
            "--exact",
            "--test-threads=1",
            "--color=never",
        ]);
    }
    command
        .env("CARGO_INCREMENTAL", "0")
        .env("CARGO_BUILD_JOBS", "2")
        .env_remove("NUMERA_UPDATE_ACTIVATION");
    command
}
/// The same finite source-mutation boundary serves actual historical proof and
/// typed compiler/launcher fault controls. Input bytes are cached only within
/// one invocation, while every case still executes its named semantic proof.
pub fn prove_cases_with(
    root: &Path,
    cases: &[Case],
    platform: &str,
    inputs: &mut BTreeMap<(String, String, String), Vec<u8>>,
    execute: impl FnMut(bool, &Case, &mut Command) -> Result<super::owned::Output>,
) -> Result<()> {
    prove_cases_with_cleanup(root, cases, platform, inputs, execute, |seen| {
        super::compiler::sanitize(root, seen)
    })
}
/// Actual proof execution with an explicit cache-I/O dependency. A cleanup fault
/// cannot replace the original launch error or named semantic-proof failure.
struct CacheCleanup<
    'a,
    F: FnMut(&mut super::compiler::Seen) -> std::result::Result<u64, super::compiler::CacheError>,
> {
    cleanup: &'a mut F,
    seen: &'a mut super::compiler::Seen,
    pending: bool,
}
impl<F: FnMut(&mut super::compiler::Seen) -> std::result::Result<u64, super::compiler::CacheError>>
    CacheCleanup<'_, F>
{
    fn run(&mut self) -> std::result::Result<u64, super::compiler::CacheError> {
        self.pending = false;
        (self.cleanup)(self.seen)
    }
}
impl<F: FnMut(&mut super::compiler::Seen) -> std::result::Result<u64, super::compiler::CacheError>>
    Drop for CacheCleanup<'_, F>
{
    fn drop(&mut self) {
        if self.pending
            && let Err(error) = self.run()
        {
            eprintln!(
                "Historical compiler status=cache-cleanup-failed cause={} errno={} winerror={}",
                error.cause, error.errno, error.winerror
            );
        }
    }
}
pub fn prove_cases_with_cleanup(
    root: &Path,
    cases: &[Case],
    platform: &str,
    inputs: &mut BTreeMap<(String, String, String), Vec<u8>>,
    mut execute: impl FnMut(bool, &Case, &mut Command) -> Result<super::owned::Output>,
    mut cleanup: impl FnMut(
        &mut super::compiler::Seen,
    ) -> std::result::Result<u64, super::compiler::CacheError>,
) -> Result<()> {
    let selected = cases
        .iter()
        .filter(|case| {
            case.platform
                .as_deref()
                .is_none_or(|selected| selected == platform)
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return fail("Historical proof selected no owning platform cases");
    }
    let mut warmed = std::collections::BTreeSet::new();
    let mut seen = super::compiler::Seen::new();
    for (index, case) in selected.into_iter().enumerate() {
        let mut swap = SourceSwap::prepare(root, case, inputs)?;
        let mut negative_exit = None;
        let result = (|| -> Result<()> {
            if warmed.insert(case.target.clone()) {
                let output = execute(true, case, &mut proof_command(root, case, true, platform))?;
                if output.command.exit != 0 {
                    retain_compile_failure(root, case, &output.command)?;
                    return fail(
                        "Historical source must compile before named semantic failure proof",
                    );
                }
            }
            let enabled = platform == "win32";
            // Loaded Cargo caches are sanitized before a traced proof can replay
            // them; the second pass remains mandatory on every proof outcome.
            let before = if enabled { cleanup(&mut seen) } else { Ok(0) };
            let scope = super::compiler::Scope::new(root, index, enabled);
            let mut after_guard = CacheCleanup {
                cleanup: &mut cleanup,
                seen: &mut seen,
                pending: enabled,
            };
            let proof = (|| -> Result<()> {
                let output = execute(false, case, &mut proof_command(root, case, false, platform))?;
                negative_exit = Some(output.command.exit);
                scope.captured(&output.command.stdout, &output.command.stderr);
                let mut evidence = super::evidence::Failure::new(&case.test, &case.symptom);
                for raw in [&output.command.stdout, &output.command.stderr] {
                    let text = std::str::from_utf8(raw)?;
                    if !enabled {
                        for line in text.lines() {
                            evidence.accept(line);
                        }
                    }
                }
                if enabled {
                    scope.for_each_line(|line| evidence.accept(line));
                }
                evidence.verify(output.command.exit)
            })();
            let after = if enabled { after_guard.run() } else { Ok(0) };
            drop(after_guard);
            let cleaned = match (before, after) {
                (Err(error), _) | (_, Err(error)) => Err(error),
                (Ok(before), Ok(after)) => Ok(before + after),
            };
            let (proof, events) = scope.finish(proof, cleaned);
            for event in events {
                eprintln!("{event}");
            }
            proof
        })();
        swap.restore()?;
        result?;
        if !swap.adaptations.is_empty() {
            let output = execute(false, case, &mut proof_command(root, case, false, platform))?;
            let mut evidence = super::evidence::Current::new([case.test.clone()])?;
            for line in std::str::from_utf8(&output.command.stdout)?.lines() {
                evidence.accept(line);
            }
            if let Err(error) = evidence.verify(output.command.exit) {
                retain_compile_failure(root, case, &output.command)?;
                return Err(error);
            }
            for receipt in &mut swap.adaptations {
                receipt["negative_exit"] = serde_json::json!(negative_exit);
                receipt["restored_exit"] = serde_json::json!(output.command.exit);
                receipt["status"] = "named-negative-and-restored-positive-verified".into();
            }
            swap.publish_adaptations(root, case)?;
        }
    }
    Ok(())
}

fn retain_compile_failure(root: &Path, case: &Case, output: &super::capture::Output) -> Result<()> {
    let path = root.join("build/history").join(format!(
        "compile-{}.log",
        super::sha(format!("{}:{}", case.target, case.test).as_bytes())
    ));
    let (stdout, stderr) =
        super::evidence::retain_test_diagnostics(&path, &output.stdout, &output.stderr)?;
    for (name, text) in [("stdout", stdout), ("stderr", stderr)] {
        let mut first = text.len().saturating_sub(16 * 1024);
        while !text.is_char_boundary(first) {
            first += 1;
        }
        eprintln!(
            "Historical compile {name} (exit={}):\n{}",
            output.exit,
            &text[first..]
        );
    }
    eprintln!(
        "Retained historical compile diagnostics: {}",
        path.display()
    );
    Ok(())
}

pub fn proof_limits(compile: bool) -> numera_core::ownership::capture::Limits {
    super::owned::limits(
        Duration::from_secs(if compile { 900 } else { 180 }),
        Duration::from_secs(if compile { 300 } else { 90 }),
        8 * 1024 * 1024,
    )
}
pub fn native(root: &Path) -> Result<()> {
    require_hosted(root)?;
    let workflow = super::workflow::parse(&fs::read_to_string(
        root.join(".github/workflows/native-ci.yml"),
    )?)?;
    workflow_contract(&workflow)?;
    let registry = Registry::read()?;
    let cases = registry.cases()?;
    let temporary = std::env::var_os("NUMERA_TEST_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".ci-work/native-history"));
    let timings = Timings::prepare(root, &temporary)?;
    let result = prove_cases_with(
        root,
        &cases,
        if cfg!(windows) {
            "win32"
        } else if cfg!(target_os = "macos") {
            "darwin"
        } else {
            std::env::consts::OS
        },
        &mut BTreeMap::new(),
        |compile, _, command| super::owned::run(root, command, proof_limits(compile)),
    );
    if let Some(output) = std::env::var_os("GITHUB_OUTPUT") {
        let published = timings.publish(Path::new(&output));
        if result.is_ok() {
            published?;
        }
    }
    result
}

/// Both owning hosts are necessary: 91 shared controls plus six Linux-only
/// and two Windows-only controls form 99 distinct historical cases.
pub fn workflow_contract(workflow: &serde_json::Value) -> Result<()> {
    let job = &workflow["jobs"]["history"];
    if job["runs-on"] != "${{ matrix.os }}"
        || job["strategy"]
            != serde_json::json!({"fail-fast":false,"matrix":{"os":["ubuntu-24.04","windows-latest"]}})
        || job["needs"] != "coverage"
        || job["timeout-minutes"] != 60
    {
        return fail(
            "Historical controls require both owning platforms without fail-fast cancellation",
        );
    }
    let steps = &job["steps"];
    super::compiler::cache_policy(steps)?;
    let fetch = super::workflow::step_once(
        steps,
        "cargo fetch --manifest-path engine-rust/Cargo.toml --locked",
        true,
    )?;
    let proof = super::workflow::step_once(steps, "ci native-history", true)?;
    if fetch >= proof {
        return fail("Historical mutation must follow explicit native dependency acquisition");
    }
    let artifact = steps
        .as_array()
        .ok_or("Historical steps missing")?
        .iter()
        .find(|step| step["name"] == "Preserve actual historical compiler diagnostics")
        .ok_or("Historical failure artifact missing")?;
    let paths = artifact["with"]["path"]
        .as_str()
        .ok_or("Historical artifact paths missing")?;
    if artifact["if"] != "failure()"
        || [
            "build/history/",
            "build/owned-command-failure-*.log",
            "build/owned-members-*.json",
            ".ci-work/logs/ci-owned_*.log",
        ]
        .iter()
        .any(|path| !paths.lines().any(|line| line == *path))
    {
        return fail(
            "Historical failures must retain actual compiler and owned-process diagnostics",
        );
    }
    let mut counts = [0usize; 3];
    for case in Registry::read()?.cases()? {
        counts[match case.platform.as_deref() {
            None => 0,
            Some("linux") => 1,
            Some("win32") => 2,
            _ => return fail("Unreviewed historical owning platform"),
        }] += 1;
    }
    if counts != [91, 6, 2] {
        return fail("Original historical platform inventory differs");
    }
    Ok(())
}
pub fn hosted_identity(actions: &str, repository: &str, sha: &str, head: &str) -> Result<()> {
    if actions != "true"
        || repository != "KiaroSama/Numera-Browser"
        || !canonical_sha(sha)
        || head.trim() != sha
    {
        return fail("Historical regression execution requires the exact owning hosted source");
    }
    Ok(())
}
pub fn require_hosted(root: &Path) -> Result<String> {
    let sha = std::env::var("GITHUB_SHA").unwrap_or_default();
    let head = git(root, &["rev-parse", "HEAD"])?;
    hosted_clean(
        &std::env::var("GITHUB_ACTIONS").unwrap_or_default(),
        &std::env::var("GITHUB_REPOSITORY").unwrap_or_default(),
        &sha,
        std::str::from_utf8(&head)?,
        git(root, &["diff", "--exit-code", "HEAD"]).is_ok(),
    )?;
    Ok(sha)
}
pub fn hosted_clean(
    actions: &str,
    repository: &str,
    sha: &str,
    head: &str,
    clean: bool,
) -> Result<()> {
    hosted_identity(actions, repository, sha, head)?;
    if !clean {
        return fail("Historical regression execution requires clean reviewed sources");
    }
    Ok(())
}
pub fn case_sources(case: &Case) -> Result<BTreeMap<String, Source>> {
    if case.files.is_empty()
        || !canonical_sha(&case.commit)
        || case.test.is_empty()
        || case.symptom.is_empty()
    {
        return fail("Historical source case is incomplete");
    }
    let mut sources = BTreeMap::new();
    for (path, hash) in &case.files {
        sources.insert(
            path.clone(),
            Source {
                commit: case.commit.clone(),
                sha256: hash.clone(),
            },
        );
    }
    for (path, source) in &case.companions {
        if sources.insert(path.clone(), source.clone()).is_some() {
            return fail("Historical selected defect and companion overlap");
        }
    }
    for (path, source) in &sources {
        super::release::public_path(path, false)?;
        if !canonical_sha(&source.commit)
            || source.sha256.len() != 64
            || !source.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return fail("Historical source identity is invalid");
        }
    }
    Ok(sources)
}
pub struct SourceSwap {
    originals: BTreeMap<PathBuf, Vec<u8>>,
    adaptations: Vec<serde_json::Value>,
    restored: bool,
}
impl SourceSwap {
    pub fn prepare(
        root: &Path,
        case: &Case,
        inputs: &mut BTreeMap<(String, String, String), Vec<u8>>,
    ) -> Result<Self> {
        Self::prepare_with(root, case, inputs, |commit, name| {
            git(root, &["show", &format!("{commit}:{name}")])
        })
    }
    pub fn prepare_with(
        root: &Path,
        case: &Case,
        inputs: &mut BTreeMap<(String, String, String), Vec<u8>>,
        read: impl FnMut(&str, &str) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        Self::prepare_checked(root, case, inputs, read, |path| {
            regular(path, false)?;
            Ok(())
        })
    }
    pub fn prepare_checked(
        root: &Path,
        case: &Case,
        inputs: &mut BTreeMap<(String, String, String), Vec<u8>>,
        mut read: impl FnMut(&str, &str) -> Result<Vec<u8>>,
        mut check: impl FnMut(&Path) -> Result<()>,
    ) -> Result<Self> {
        let mut originals = BTreeMap::new();
        let mut replacements = BTreeMap::new();
        let mut adaptations = Vec::new();
        for (name, source) in case_sources(case)? {
            let key = (source.commit.clone(), name.clone(), source.sha256.clone());
            let bytes = match inputs.get(&key) {
                Some(bytes) => bytes.clone(),
                None => read(&source.commit, &name)?,
            };
            if bytes.len() > 32768
                || super::sha(&bytes) != source.sha256
                || std::str::from_utf8(&bytes).is_err()
            {
                return fail("Historical regression source integrity differs");
            }
            let path = root.join(&name);
            check(&path)?;
            let current = fs::read(&path)?;
            let adaptation = super::history_compat::replacement(case, &name, &source, &current)?
                .or(super::history_compat::fixture_replacement(
                    root, case, &name, &source, &current, &bytes,
                )?)
                .or(super::history_compat::companion_replacement(
                    case, &name, &source, &bytes,
                )?);
            let replacement = match adaptation {
                Some(mut replacement) => {
                    let assertion = root.join(format!("engine-rust/tests/{}.rs", case.target));
                    if assertion.is_file() {
                        replacement.receipt["assertion_source_sha256"] =
                            super::digest(&assertion)?.into();
                    }
                    adaptations.push(replacement.receipt);
                    replacement.bytes
                }
                None => bytes.clone(),
            };
            originals.insert(path.clone(), current);
            replacements.insert(path, replacement);
            inputs.insert(key, bytes);
        }
        for (name, replacement) in super::history_compat::additional_callers(root, case)? {
            let path = root.join(name);
            check(&path)?;
            if originals.insert(path.clone(), fs::read(&path)?).is_some() {
                return fail("Historical caller bridge overlaps original source inventory");
            }
            replacements.insert(path, replacement.bytes);
            adaptations.push(replacement.receipt);
        }
        let mut guard = Self {
            originals,
            adaptations,
            restored: false,
        };
        for (path, bytes) in replacements {
            if let Err(error) = fs::write(path, bytes) {
                guard.restore()?;
                return Err(error.into());
            }
        }
        guard.publish_adaptations(root, case)?;
        Ok(guard)
    }
    fn publish_adaptations(&self, root: &Path, case: &Case) -> Result<()> {
        if self.adaptations.is_empty() {
            return Ok(());
        }
        let directory = root.join("build/history");
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!(
            "adaptation-{}.json",
            super::sha(format!("{}:{}", case.target, case.test).as_bytes())
        ));
        fs::write(&path, serde_json::to_vec_pretty(&self.adaptations)?)?;
        eprintln!(
            "Historical proof uses an explicitly labeled native semantic adaptation: {}",
            path.display()
        );
        Ok(())
    }
    pub fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        let mut failed = Vec::new();
        for (path, bytes) in &self.originals {
            let result = regular(path, false).and_then(|_| {
                fs::write(path, bytes)?;
                if fs::read(path)? != *bytes {
                    return fail("Restoration readback differs");
                }
                Ok(())
            });
            if result.is_err() {
                failed.push(
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("source")
                        .to_owned(),
                );
            }
        }
        if !failed.is_empty() {
            return fail(format!(
                "Fixed native source restoration failed: {}",
                failed.join(", ")
            ));
        }
        self.restored = true;
        Ok(())
    }
}
impl Drop for SourceSwap {
    fn drop(&mut self) {
        if !self.restored && self.restore().is_err() {
            eprintln!(
                "Fixed native source restoration remains incomplete; further proof is blocked"
            );
        }
    }
}
pub fn swapped<T>(
    root: &Path,
    case: &Case,
    inputs: &mut BTreeMap<(String, String, String), Vec<u8>>,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let mut guard = SourceSwap::prepare(root, case, inputs)?;
    let result = operation();
    guard.restore()?;
    result
}
pub fn unix_host(platform: &str, runner: &str, trusted: bool, clean: bool) -> Result<()> {
    if !matches!((platform, runner), ("linux", "Linux") | ("darwin", "macOS")) || !trusted || !clean
    {
        return fail("Desktop historical proof requires exact clean owning hosted Unix source");
    }
    Ok(())
}
pub fn prove_case(root: &Path, case: &Case) -> Result<()> {
    require_hosted(root)?;
    let mut swap = SourceSwap::prepare(root, case, &mut BTreeMap::new())?;
    let result = (|| {
        let manifest = if case.target == "desktop-updater" {
            root.join("desktop/src-tauri/Cargo.toml")
        } else {
            root.join("engine-rust/Cargo.toml")
        };
        let command = || {
            let mut command = Command::new("cargo");
            command
                .arg("test")
                .arg("--manifest-path")
                .arg(&manifest)
                .args(["--locked", "--offline", "--no-default-features"]);
            if case.target == "lib" || case.target == "desktop-updater" {
                command.arg("--lib");
            } else {
                command.arg("--test").arg(&case.target);
            }
            command
                .env("CARGO_INCREMENTAL", "0")
                .env("CARGO_BUILD_JOBS", "2");
            command
        };
        let compiled = capture(
            command().arg("--no-run"),
            Duration::from_secs(900),
            Duration::from_secs(300),
            8 * 1024 * 1024,
        )?;
        if compiled.exit != 0 {
            retain_compile_failure(root, case, &compiled)?;
            return fail("Historical source must compile before named semantic failure proof");
        }
        let output = capture(
            command().args([
                "--",
                &case.test,
                "--exact",
                "--test-threads=1",
                "--color=never",
            ]),
            Duration::from_secs(180),
            Duration::from_secs(90),
            8 * 1024 * 1024,
        )?;
        let mut evidence = super::evidence::Failure::new(&case.test, &case.symptom);
        for bytes in [&output.stdout, &output.stderr] {
            for line in std::str::from_utf8(bytes)?.lines() {
                evidence.accept(line);
            }
        }
        evidence.verify(output.exit)
    })();
    let restoration = swap.restore();
    restoration?;
    result
}
pub fn desktop_case() -> Case {
    Case {
        target: "desktop-updater".into(),
        test: "updates::tests::rollback_preserves_an_occupied_recovery_destination".into(),
        symptom: "Rollback overwrote an occupied recovery destination".into(),
        commit: "09d6fd84b0cbe0c9214cf9bb3471598b8b748507".into(),
        files: BTreeMap::from([(
            "desktop/src-tauri/src/updates/files.rs".into(),
            "10c24cb52eb6ec786584772b6359b865d9060e3827edbada9f8754e846482bc3".into(),
        )]),
        companions: BTreeMap::new(),
        platform: None,
    }
}
pub fn desktop(root: &Path) -> Result<()> {
    let runner = std::env::var("RUNNER_OS").unwrap_or_default();
    if !matches!(
        (std::env::consts::OS, runner.as_str()),
        ("linux", "Linux") | ("macos", "macOS")
    ) {
        return fail("Desktop historical proof requires its owning hosted Unix platform");
    }
    prove_case(root, &desktop_case())
}
pub fn macos_host(actions: &str, repository: &str, runner: &str, system: &str) -> Result<()> {
    if actions != "true"
        || repository != "KiaroSama/Numera-Browser"
        || runner != "macOS"
        || system != "darwin"
    {
        return fail("Native ownership acceptance requires its owning hosted macOS job");
    }
    Ok(())
}
pub const MACOS_CURRENT: &[&str] = &[
    "ownership::macos::tests::macos_public_group_observation_retains_every_uncertain_claim",
    "ownership::macos::tests::macos_group_growth_is_bounded_and_never_calls_truncation_empty",
    "ownership::macos::tests::macos_public_identity_distinguishes_absence_from_observation_failure",
    "ownership::macos::tests::macos_birth_identity_refuses_malformed_records_and_reused_tokens",
];
pub fn macos_plan(root: &Path, phase: &str) -> Result<Vec<Vec<String>>> {
    let base = vec![
        "cargo".into(),
        "test".into(),
        "--manifest-path".into(),
        root.join("engine-rust/Cargo.toml")
            .to_string_lossy()
            .into_owned(),
        "--locked".into(),
    ];
    if phase == "build" {
        let mut fetch = base.clone();
        fetch[1] = "fetch".into();
        let mut compile = base;
        compile.extend(
            [
                "--lib",
                "--test",
                "ownership_identity_parity",
                "--test",
                "ownership_tree_parity",
                "--no-run",
            ]
            .map(str::to_owned),
        );
        Ok(vec![fetch, compile])
    } else if phase == "test" {
        let mut result = Vec::new();
        let mut current = base.clone();
        current.extend(
            [
                "--lib",
                "--",
                "ownership::macos::tests::",
                "--test-threads=2",
                "--color=never",
            ]
            .map(str::to_owned),
        );
        result.push(current);
        for _ in 0..4 {
            let mut command = base.clone();
            command.extend(
                [
                    "--test",
                    "ownership_identity_parity",
                    "--",
                    "native_birth_token_matches_original_runtime_for_the_same_live_process",
                    "--exact",
                    "--test-threads=1",
                    "--color=never",
                ]
                .map(str::to_owned),
            );
            result.push(command);
        }
        let mut tree = base;
        tree.extend(
            [
                "--test",
                "ownership_tree_parity",
                "--",
                "native_owned_tree_contains_descendants_before_their_first_capability",
                "--exact",
                "--test-threads=1",
                "--color=never",
            ]
            .map(str::to_owned),
        );
        result.push(tree);
        Ok(result)
    } else {
        fail("Unknown macOS ownership acceptance phase")
    }
}
pub fn macos(root: &Path, phase: &str) -> Result<()> {
    macos_host(
        &std::env::var("GITHUB_ACTIONS").unwrap_or_default(),
        &std::env::var("GITHUB_REPOSITORY").unwrap_or_default(),
        &std::env::var("RUNNER_OS").unwrap_or_default(),
        if cfg!(target_os = "macos") {
            "darwin"
        } else {
            std::env::consts::OS
        },
    )?;
    require_hosted(root)?;
    for (index, argv) in macos_plan(root, phase)?.iter().enumerate() {
        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]).env("CARGO_BUILD_JOBS", "2");
        if phase == "test" && (1..=4).contains(&index) {
            let locales = [
                ("C", "UTC"),
                ("en_US.UTF-8", "UTC"),
                ("en_US.UTF-8", "America/Los_Angeles"),
                ("numera_invalid_locale", "UTC"),
            ];
            let (locale, timezone) = locales[index - 1];
            command.env("LC_ALL", locale).env("TZ", timezone);
        }
        let output = capture(
            &mut command,
            Duration::from_secs(if phase == "build" { 900 } else { 600 }),
            Duration::from_secs(if phase == "build" { 300 } else { 120 }),
            8 * 1024 * 1024,
        )?;
        if output.exit != 0 {
            return fail("Native macOS ownership acceptance failed");
        }
        if phase == "build" && index == 0 {
            for case in macos_cases() {
                prove_case(root, &case)?;
            }
        }
        if phase == "test" {
            let names = if index == 0 {
                MACOS_CURRENT.iter().map(|name| (*name).into()).collect()
            } else {
                vec![if index == 5 {
                    "native_owned_tree_contains_descendants_before_their_first_capability".into()
                } else {
                    "native_birth_token_matches_original_runtime_for_the_same_live_process".into()
                }]
            };
            let mut evidence = super::evidence::Current::new(names)?;
            for line in std::str::from_utf8(&output.stdout)?.lines() {
                evidence.accept(line);
            }
            evidence.verify(output.exit)?;
        }
    }
    Ok(())
}
pub fn macos_cases() -> Vec<Case> {
    [("ownership::macos::tests::macos_public_identity_distinguishes_absence_from_observation_failure","macOS process identity differs","engine-rust/src/ownership/identity.rs","c5f41e97c748c392ac896dd835bb4d13bef3d92ceefa6240e106538672b44f56"),("ownership::macos::tests::macos_public_group_observation_retains_every_uncertain_claim","macOS group uncertainty differs","engine-rust/src/ownership/tree.rs","201260e01da09fea13951ba61ab0265faa68499554d4cfefaf712a179dc8b36b")].into_iter().map(|(test,symptom,path,hash)|Case{target:"lib".into(),test:test.into(),symptom:symptom.into(),commit:"897caadef85053d6d74896e901fc9a21f7baa6dd".into(),files:BTreeMap::from([(path.into(),hash.into())]),companions:BTreeMap::new(),platform:Some("darwin".into())}).collect()
}

pub fn validate_cached_source(path: &Path, expected: &str) -> Result<()> {
    regular(path, false)?;
    if digest(path)? != expected {
        return fail("Historical reused source integrity differs");
    }
    Ok(())
}
pub fn explicit_cases(cases: &[Case], platform: &str, clean: bool) -> Result<()> {
    if cases.is_empty()
        || !clean
        || cases
            .iter()
            .any(|case| case.platform.as_deref() != Some(platform))
    {
        return fail("Explicit historical proof cases must match the exact clean owning platform");
    }
    for case in cases {
        case_sources(case)?;
    }
    Ok(())
}
/// Preserve previous timing bytes before admitting a fresh compiler proof. A
/// failed rename or uncertain path never advertises previous evidence as new.
pub struct Timings {
    pub directory: PathBuf,
    pub previous: Option<PathBuf>,
}
impl Timings {
    pub fn prepare(root: &Path, temporary: &Path) -> Result<Self> {
        Self::prepare_with(root, temporary, |from, to| {
            fs::rename(from, to)?;
            Ok(())
        })
    }
    pub fn prepare_with(
        root: &Path,
        temporary: &Path,
        mut rename: impl FnMut(&Path, &Path) -> Result<()>,
    ) -> Result<Self> {
        Self::prepare_checked(
            root,
            temporary,
            |from, to| rename(from, to),
            |path| {
                regular(path, true)?;
                Ok(())
            },
        )
    }
    pub fn prepare_checked(
        root: &Path,
        temporary: &Path,
        mut rename: impl FnMut(&Path, &Path) -> Result<()>,
        mut check: impl FnMut(&Path) -> Result<()>,
    ) -> Result<Self> {
        let directory = root.join("engine-rust/target/cargo-timings");
        let previous = if directory.try_exists()? {
            check(&directory)?;
            fs::create_dir_all(temporary)?;
            let mut holder = None;
            for index in 0..1000 {
                let candidate = temporary.join(format!(
                    "native-history-timings-{}-{index}",
                    std::process::id()
                ));
                match fs::create_dir(&candidate) {
                    Ok(()) => {
                        holder = Some(candidate);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(error) => return Err(error.into()),
                }
            }
            let holder = holder.ok_or("Historical timing retention namespace exhausted")?;
            let previous = holder.join("previous");
            rename(&directory, &previous)?;
            Some(previous)
        } else {
            None
        };
        Ok(Self {
            directory,
            previous,
        })
    }
    pub fn publish(&self, output: &Path) -> Result<()> {
        use std::io::Write;
        if self.directory.try_exists()? {
            regular(&self.directory, true)?;
            regular(&self.directory.join("cargo-timing.html"), false)?;
            let mut output = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(output)?;
            writeln!(output, "timings_ready=true")?;
            output.sync_all()?;
        }
        Ok(())
    }
}
