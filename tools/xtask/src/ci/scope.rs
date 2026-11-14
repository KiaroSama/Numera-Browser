//! Native source inventory, durable partition checkpoints and scanner scope.
use super::{Result, capture, decode, fail, git, regular};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const SCANNER_CONFIG_SHA256: &str =
    "e163e53b9e7e8a8511e77271e2b323ed057759542a6d988258afe3a1fa329caf";
pub const REVIEWED_RULES_SHA256: &str =
    "e8bacdc1cbd76ae1495ef4ec5a917030761093f793dc4a89bb512671600c5ca6";
const SIGNING_FIXTURE_DIRECTORY: &str = "tools/xtask/tests/fixtures/windows-signing";
const SIGNING_FIXTURE_RECORD: &str =
    "9373b346cbc88ab826f9fb59d526c96f3cd3aa720fa5a513958cdbfb632400ae";
const SIGNING_FIXTURES: [(&str, &str, u64); 2] = [
    (
        "public-fixture.der",
        "8fedb7633c7d447b6607375392c9bb02b549ed7c5fbaf7b562312f50d1edd7db",
        802,
    ),
    (
        "public-fixture.pfx",
        "4bc4ea7fa927262945f0eed6418d1a60b7f43e69d773007c79e6c7d129480782",
        2563,
    ),
];

fn signing_material(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| ["der", "pfx", "p12"].contains(&value.to_ascii_lowercase().as_str()))
}
fn validate_signing_fixture(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or("Binary fixture parent missing")?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("Signing fixture name is not UTF8")?;
    let (_, expected, length) = SIGNING_FIXTURES
        .iter()
        .find(|(allowed, _, _)| name == *allowed && parent.ends_with(SIGNING_FIXTURE_DIRECTORY))
        .ok_or("Unregistered binary signing material in tracked verification scope")?;
    regular(path, false)?;
    let record = parent.join("provenance.json");
    regular(&record, false)?;
    if fs::metadata(&record)?.len() > 16384 || super::digest(&record)? != SIGNING_FIXTURE_RECORD {
        return fail("Public signing fixture record differs from the reviewed pin");
    }
    let provenance: Value = decode(&fs::read(record)?)?;
    if provenance["schema"] != 1
        || provenance["files"]
            .as_object()
            .is_none_or(|files| files.len() != SIGNING_FIXTURES.len())
        || SIGNING_FIXTURES.iter().any(|(file, hash, size)| {
            provenance["files"][*file]["sha256"] != *hash
                || provenance["files"][*file]["bytes"] != *size
        })
        || fs::metadata(path)?.len() != *length
        || super::digest(path)? != *expected
    {
        return fail("Public signing fixture bytes differ from the reviewed pin");
    }
    Ok(())
}
fn validate_signing_directory(root: &Path) -> Result<()> {
    let directory = root.join(SIGNING_FIXTURE_DIRECTORY);
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if signing_material(&path) {
                validate_signing_fixture(&path)?;
            }
        }
    }
    Ok(())
}
fn validate_tracked_signing(root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(root)?;
    if !SIGNING_FIXTURES
        .iter()
        .any(|(name, _, _)| relative == Path::new(SIGNING_FIXTURE_DIRECTORY).join(name))
    {
        return fail("Signing material is outside its exact reviewed tracked path");
    }
    validate_signing_fixture(path)
}

/// Validate signing material at every path and revision before a history scan.
/// Current checkout validation alone cannot see an unknown blob later restored
/// to approved fixture bytes, or a deleted signing fixture directory.
pub fn history_signing_material(root: &Path) -> Result<usize> {
    use super::{gitleaks_contract as contract, release_signing};
    fn checked(root: &Path, args: &[&str], bytes: usize) -> Result<Vec<u8>> {
        let result = capture(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .env("GIT_NO_REPLACE_OBJECTS", "1")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    if cfg!(windows) { "NUL" } else { "/dev/null" },
                ),
            Duration::from_secs(30),
            Duration::from_secs(30),
            bytes,
        )?;
        if result.exit != 0 || !result.stderr.is_empty() {
            return fail("Signing-material Git history producer failed");
        }
        Ok(result.stdout)
    }
    if checked(root, &["rev-parse", "--is-shallow-repository"], 1024)? != b"false\n" {
        return fail("Complete signing-material history is unavailable");
    }
    let head = checked(root, &["rev-parse", "--verify", "HEAD^{commit}"], 1024)?;
    if head.len() != 41
        || head.last() != Some(&b'\n')
        || !std::str::from_utf8(&head[..40]).is_ok_and(super::canonical_sha)
    {
        return fail("Signing-material history has no canonical committed head");
    }
    let output = checked(
        root,
        &[
            "log",
            "--all",
            "--raw",
            "-z",
            "--no-abbrev",
            "--format=",
            "--full-history",
            "--root",
            "-m",
            "--no-renames",
            "--no-relative",
            "--no-notes",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
            "--",
            ":(icase,glob)**/*.pfx",
            ":(icase,glob)**/*.p12",
            ":(icase,glob)**/*.der",
            "tools/xtask/tests/fixtures/windows-signing/provenance.json",
        ],
        contract::HISTORY_BYTES,
    )?;
    let records = contract::history_records(&output)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut queries = BTreeSet::new();
    let mut environment = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    environment = release_signing::tool_environment(&environment, "git");
    environment.insert("GIT_NO_REPLACE_OBJECTS".into(), "1".into());
    environment.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
    environment.insert(
        "GIT_CONFIG_GLOBAL".into(),
        if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
    );
    for (path, old, new) in records {
        for oid in [old, new] {
            if oid.bytes().any(|byte| byte != b'0') && queries.insert((path.clone(), oid.clone())) {
                let packet = release_signing::execute_bounded(
                    &release_signing::ToolRequest {
                        program: "git".into(),
                        args: vec![
                            "-C".into(),
                            root.into(),
                            "cat-file".into(),
                            "--batch".into(),
                        ],
                        environment: environment.clone(),
                        input: format!("{oid}\n").into_bytes(),
                        seconds: 15,
                    },
                    16_384,
                )?;
                if packet.exit != 0 || !packet.stderr.is_empty() {
                    return fail("Historical signing-material blob read failed");
                }
                contract::history_blob(&path, &oid, &packet.stdout)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
        }
    }
    Ok(queries.len())
}

pub fn counted_tests(text: &str, expected: usize, exit: i32) -> Result<()> {
    counted_tests_format(text, expected, exit, false)
}
pub fn counted_tests_format(text: &str, expected: usize, exit: i32, wasm: bool) -> Result<()> {
    let names = text
        .lines()
        .filter_map(|line| {
            line.strip_prefix("test ")
                .and_then(|line| line.rsplit_once(" ... "))
        })
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    if expected == 0
        || names.len() != expected
        || names.iter().collect::<BTreeSet<_>>().len() != expected
    {
        return fail("Test report did not run exactly the maintained case inventory");
    }
    let mut evidence = if wasm {
        super::evidence::Current::wasm(names)?
    } else {
        super::evidence::Current::new(names)?
    };
    for line in text.lines() {
        evidence.accept(line);
    }
    evidence.verify(exit)
}

pub fn targets(root: &Path) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(root.join("engine-rust/tests"))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "rs") {
            regular(&path, false)?;
            result.push(
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .ok_or("Native target is not Unicode")?
                    .into(),
            );
        } else if path.is_dir() && path.join("main.rs").exists() {
            return fail("Nested Cargo entry points require explicit tier mapping");
        }
    }
    result.sort();
    if result.is_empty() {
        return fail("Native verification inventory is empty");
    }
    Ok(result)
}
pub fn target_registration(discovered: &[String]) -> Result<()> {
    let expected: BTreeSet<String> = serde_json::from_str(include_str!("native-targets.json"))?;
    let actual = discovered
        .iter()
        .filter(|name| !name.starts_with("source_"))
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual != expected || discovered.iter().collect::<BTreeSet<_>>().len() != discovered.len() {
        return fail("Rust integration suite discovery changed; map its source oracle and CI tier");
    }
    Ok(())
}
pub fn inventory(root: &Path) -> Result<Value> {
    let targets = targets(root)?;
    let source = targets
        .iter()
        .filter(|name| name.starts_with("source_"))
        .collect::<Vec<_>>();
    if source.is_empty() {
        return fail("Native source-case inventory is empty");
    }
    Ok(
        json!({"schema":1,"native":targets,"source":source,"dashboard":"dashboard/Cargo.toml","desktop":"desktop/src-tauri/Cargo.toml","build_tooling":"tools/xtask/Cargo.toml"}),
    )
}
pub fn partition(modules: &[String], index: usize, count: usize) -> Result<Vec<String>> {
    if !(1..=4).contains(&count) || index >= count {
        return fail("Invalid source verification partition");
    }
    if modules.iter().collect::<BTreeSet<_>>().len() != modules.len() {
        return fail("Source inventory contains duplicate modules");
    }
    let selected = modules
        .iter()
        .skip(index)
        .step_by(count)
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return fail("Source verification partition is empty");
    }
    Ok(selected)
}
pub fn notices(modules: &[String]) -> Result<Vec<Vec<String>>> {
    let pattern = regex::Regex::new(r"^(?:source_|test_)[A-Za-z0-9_]+(?:\.rs|\.py)?$")?;
    if modules.iter().any(|name| !pattern.is_match(name)) {
        return fail("Source diagnostic module name is invalid");
    }
    Ok(modules
        .chunks(modules.len().div_ceil(10).max(1))
        .map(|names| names.to_vec())
        .collect())
}
pub fn tracked(root: &Path) -> Result<Vec<PathBuf>> {
    let bytes = git(root, &["ls-files", "-z"])?;
    let result = std::str::from_utf8(&bytes)?
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(|name| root.join(name))
        .collect::<Vec<_>>();
    if result.is_empty() {
        return fail("Tracked verification scope is empty");
    }
    Ok(result)
}
pub fn check_utf8(paths: &[PathBuf]) -> Result<usize> {
    let binary = [
        "png", "ico", "jpg", "jpeg", "gif", "webp", "woff", "woff2", "ttf", "otf", "zip", "gz",
        "zst", "whl", "exe", "dll", "mmdb", "wasm",
    ];
    let mut checked = 0;
    for path in paths {
        if signing_material(path) {
            validate_signing_fixture(path)?;
            checked += 1;
            continue;
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| binary.contains(&value.to_ascii_lowercase().as_str()))
        {
            continue;
        }
        std::str::from_utf8(&fs::read(path)?)?;
        checked += 1;
    }
    Ok(checked)
}
pub fn stage_tracked(root: &Path, destination: &Path) -> Result<usize> {
    if destination.exists() {
        return fail("Tracked secret scan requires a new empty destination");
    }
    validate_signing_directory(root)?;
    let paths = tracked(root)?;
    for source in &paths {
        regular(source, false)?;
        if signing_material(source) {
            validate_tracked_signing(root, source)?;
        }
    }
    fs::create_dir_all(destination)?;
    let result = (|| {
        for source in &paths {
            regular(source, false)?;
            let target = destination.join(source.strip_prefix(root)?);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, target)?;
        }
        // Validate the actual scanner input again after publication, so a
        // source changed between the first hash and its copy cannot be hidden
        // behind the exact public-fixture filename exception.
        validate_signing_directory(destination)?;
        for source in &paths {
            if signing_material(source) {
                validate_tracked_signing(
                    destination,
                    &destination.join(source.strip_prefix(root)?),
                )?;
            }
        }
        Ok(paths.len())
    })();
    if result.is_err() {
        fs::remove_dir_all(destination)?;
    }
    result
}
pub fn sanitize_scanner(text: &str) -> Result<String> {
    let mut blocks = Vec::<String>::new();
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("[[") && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
    }
    blocks.push(current);
    let paths = regex::Regex::new(r"(?m)^\s*paths\s*=")?;
    let filtered = blocks
        .into_iter()
        .filter(|block| !(block.starts_with("[[rules.allowlists]]") && paths.is_match(block)))
        .collect::<String>();
    let inherited =
        regex::Regex::new(r"(?ms)^([ \t]*)paths = \[[ \t]*\r?\n.*?^[ \t]*\][ \t]*\r?$")?;
    if inherited.find_iter(&filtered).count() != 1 {
        return fail("Official scanner exclusion layout changed; inspect before scanning");
    }
    Ok(inherited.replace(&filtered, "${1}paths = []").into_owned())
}
pub fn prepare_secret_config(root: &Path, base: &Path, destination: &Path) -> Result<()> {
    prepare_secret_config_checked(root, base, destination, SCANNER_CONFIG_SHA256)
}
pub fn prepare_secret_config_checked(
    root: &Path,
    base: &Path,
    destination: &Path,
    expected_sha: &str,
) -> Result<()> {
    if destination.exists() {
        return fail("Scanner configuration destination must be new");
    }
    validate_signing_directory(root)?;
    let data = fs::read(base)?;
    if super::sha(&data) != expected_sha {
        return fail("Official scanner configuration checksum mismatch");
    }
    let text = sanitize_scanner(std::str::from_utf8(&data)?)?;
    let template = fs::read_to_string(root.join("scripts/ci/gitleaks.toml"))?;
    if super::sha(template.as_bytes()) != REVIEWED_RULES_SHA256
        || template.matches("useDefault = true").count() != 1
    {
        return fail("Exact reviewed scanner configuration changed");
    }
    let config = template.replace(
        "useDefault = true",
        &format!("path = {}", serde_json::to_string(&base.canonicalize()?)?),
    );
    fs::write(base, text)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    output.write_all(config.as_bytes())?;
    output.sync_all()?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModuleResult {
    pub module: String,
    pub exit: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub end: f64,
    pub next: usize,
    pub blocks: usize,
    pub modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub module_next: usize,
    #[serde(default)]
    pub module_results: Vec<ModuleResult>,
    #[serde(default)]
    pub slot_results: Vec<i32>,
}
impl Checkpoint {
    pub fn new(modules: Vec<String>, blocks: usize, now: f64) -> Result<Self> {
        if !(1..=4).contains(&blocks) || modules.len() < blocks || !now.is_finite() {
            return fail("Invalid source checkpoint block");
        }
        Ok(Self {
            end: now + 600.0,
            next: 0,
            blocks,
            modules,
            active: None,
            module_next: 0,
            module_results: Vec::new(),
            slot_results: Vec::new(),
        })
    }
    pub fn admit(
        &self,
        modules: &[String],
        block: usize,
        blocks: usize,
        now: f64,
    ) -> Result<(usize, usize)> {
        if self.next != block
            || self.blocks != blocks
            || self.modules != modules
            || !(1..=4).contains(&blocks)
            || block >= blocks
            || !self.end.is_finite()
            || now >= self.end
            || self.active.is_some()
        {
            return fail(
                "Source checkpoint is incomplete, expired or belongs to another partition",
            );
        }
        Ok((
            modules.len() * block / blocks,
            modules.len() * (block + 1) / blocks,
        ))
    }
    pub fn module_cursor(&self, index: usize, settled: bool) -> Result<&[ModuleResult]> {
        let first = self.modules.len() * 2 / 4;
        let last = self.modules.len() * 3 / 4;
        let count = last - first;
        let cursor = if settled {
            if index == 8 { count } else { index + 1 }
        } else {
            index
        };
        if count < 9
            || index >= 9
            || self.blocks != 4
            || self.next != if cursor == count { 3 } else { 2 }
            || self.active.is_some()
            || self.module_next != cursor
            || self.module_results.len() != cursor
            || self.slot_results.len() != index + usize::from(settled)
        {
            return fail(
                "Source module checkpoint is incomplete, repeated or belongs to another partition",
            );
        }
        for (offset, row) in self.module_results.iter().enumerate() {
            if row.module != self.modules[first + offset] || ![0, 1].contains(&row.exit) {
                return fail("Source module checkpoint has an incomplete verdict");
            }
        }
        let mut expected = self
            .module_results
            .iter()
            .take(8)
            .map(|row| row.exit)
            .collect::<Vec<_>>();
        if self.module_results.len() > 8 {
            expected.push(i32::from(
                self.module_results[8..].iter().any(|row| row.exit != 0),
            ));
        }
        if self.slot_results != expected {
            return fail("Source module checkpoint verdict does not match its settled modules");
        }
        Ok(&self.module_results)
    }
    pub fn begin_module(&mut self, module: &str) -> Result<()> {
        if self.active.is_some() {
            return fail("Unproved module cleanup blocks further execution");
        }
        self.active = Some(module.into());
        Ok(())
    }
    pub fn finish_module(&mut self, module: &str, exit: i32) -> Result<()> {
        if self.active.as_deref() != Some(module) || ![0, 1].contains(&exit) {
            return fail("Source module did not finish with a complete test verdict");
        }
        self.active = None;
        self.module_results.push(ModuleResult {
            module: module.into(),
            exit,
        });
        self.module_next = self.module_results.len();
        Ok(())
    }
}
pub fn save(path: &Path, state: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(state)?;
    numera_core::storage::atomic::write_bytes(path, &bytes)?;
    Ok(())
}

/// Durable source-module protocol used on hosted Windows and available for
/// diagnostics on every platform. An executor returns only complete test
/// verdicts (0/1); interruption or uncertain cleanup leaves `active` durable.
#[allow(clippy::too_many_arguments)] // Explicit durable protocol + independently replaceable clock/executor/notice boundaries.
pub fn execute_block(
    destination: &Path,
    state: &mut Checkpoint,
    modules: &[String],
    block: usize,
    blocks: usize,
    slot: Option<usize>,
    mut now: impl FnMut() -> f64,
    mut execute: impl FnMut(&str, f64, f64) -> Result<i32>,
    mut notify: impl FnMut(&str),
) -> Result<i32> {
    let (first, last) = state.admit(modules, block, blocks, now())?;
    let (start, end) = if let Some(slot) = slot {
        if block != 2 || blocks != 4 {
            return fail("Source module checkpoints require the third of four blocks");
        }
        state.module_cursor(slot, false)?;
        (
            first + slot,
            if slot == 8 { last } else { first + slot + 1 },
        )
    } else {
        if state.module_next != 0 && (block == 2 || state.module_cursor(8, true).is_err()) {
            return fail("Partial source module checkpoint cannot repeat or advance");
        }
        (first, last)
    };
    let checkpoint = destination.join("checkpoint.json");
    let mut progress = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(destination.join("progress.jsonl"))?;
    let mut rows = Vec::new();
    for module in &modules[start..end] {
        let remaining = state.end - now();
        if !remaining.is_finite() || remaining <= 0.0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Source checkpoint exhausted its single budget",
            )
            .into());
        }
        let previous = state
            .module_results
            .iter()
            .chain(rows.iter())
            .map(|row: &ModuleResult| format!("{}:{}", row.module, row.exit))
            .collect::<Vec<_>>()
            .join(",");
        notify(&format!(
            "##[notice]Source block starting modules={module} remaining={remaining:.1}s settled={}",
            if previous.is_empty() {
                "none"
            } else {
                &previous
            }
        ));
        state.begin_module(module)?;
        save(&checkpoint, state)?;
        writeln!(progress, "{}", json!({"module":module,"phase":"start"}))?;
        progress.sync_all()?;
        let exit = match execute(module, remaining, remaining.min(120.0)) {
            Ok(exit) if [0, 1].contains(&exit) => exit,
            result => {
                writeln!(
                    progress,
                    "{}",
                    json!({"module":module,"phase":"unproved","error":"NativeBoundary"})
                )?;
                progress.sync_all()?;
                notify(&format!(
                    "##[notice]Source module unproved name={module} type=NativeBoundary"
                ));
                return match result {
                    Err(error) => Err(error),
                    Ok(_) => fail("Source module interruption leaves cleanup unproved"),
                };
            }
        };
        state.active = None;
        rows.push(ModuleResult {
            module: module.clone(),
            exit,
        });
        if slot.is_some() {
            state.module_results.push(ModuleResult {
                module: module.clone(),
                exit,
            });
            state.module_next += 1;
        }
        writeln!(
            progress,
            "{}",
            json!({"module":module,"phase":"finish","exit":exit})
        )?;
        progress.sync_all()?;
        save(&checkpoint, state)?;
    }
    let exit = i32::from(rows.iter().any(|row| row.exit != 0));
    if let Some(slot) = slot {
        state.slot_results.push(exit);
        state.next = if slot == 8 { 3 } else { 2 };
    } else {
        state.next = block + 1;
    }
    save(&checkpoint, state)?;
    notify(&format!(
        "##[notice]Source block settled modules={}",
        rows.iter()
            .map(|row| format!("{}:{}", row.module, row.exit))
            .collect::<Vec<_>>()
            .join(",")
    ));
    Ok(if slot.is_some() { 0 } else { exit })
}
pub fn read_checkpoint(path: &Path) -> Result<Checkpoint> {
    Ok(serde_json::from_value(decode(&fs::read(path)?)?)?)
}
pub fn source_checkpoint(root: &Path, args: &[OsString]) -> Result<()> {
    if args.len() != 6 {
        return fail("source-checkpoint requires shard shards block blocks slot-or-all output");
    }
    let number = |index: usize| -> Result<usize> { Ok(args[index].to_string_lossy().parse()?) };
    let (shard, shards, block, blocks) = (number(0)?, number(1)?, number(2)?, number(3)?);
    let slot = if args[4] == "all" {
        None
    } else {
        Some(number(4)?)
    };
    let modules = partition(
        &targets(root)?
            .into_iter()
            .filter(|target| target.starts_with("source_"))
            .collect::<Vec<_>>(),
        shard,
        shards,
    )?;
    let destination = PathBuf::from(&args[5]);
    fs::create_dir_all(&destination)?;
    let clock = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(f64::INFINITY)
    };
    let checkpoint = destination.join("checkpoint.json");
    let mut state = if block == 0 && !checkpoint.exists() {
        Checkpoint::new(modules.clone(), blocks, clock())?
    } else {
        read_checkpoint(&checkpoint)?
    };
    let exit = execute_block(
        &destination,
        &mut state,
        &modules,
        block,
        blocks,
        slot,
        clock,
        |module, wall, idle| {
            let mut command = Command::new("cargo");
            command
                .arg("test")
                .arg("--manifest-path")
                .arg(root.join("engine-rust/Cargo.toml"))
                .args([
                    "--locked",
                    "--features",
                    "native-product,native-browser,test-support",
                    "--test",
                    module,
                    "--",
                    "--test-threads=2",
                    "--color=never",
                ]);
            let output = super::owned::run(
                root,
                &mut command,
                super::owned::limits(
                    Duration::from_secs_f64(wall),
                    Duration::from_secs_f64(idle),
                    8 * 1024 * 1024,
                ),
            )?
            .command;
            fs::write(destination.join(format!("{module}.stdout")), &output.stdout)?;
            let text = std::str::from_utf8(&output.stdout)?;
            if !text.lines().any(|line| line.starts_with("test result:")) {
                return fail("Source module has no completed test evidence");
            }
            if output.exit == 0 {
                super::coverage_inventory::validate_target_output(
                    root,
                    "engine-rust",
                    module,
                    std::env::consts::OS,
                    text,
                    0,
                )?;
            }
            match output.exit {
                0 => Ok(0),
                101 => Ok(1),
                _ => fail("Source module did not settle with a complete verdict"),
            }
        },
        |message| println!("{message}"),
    )?;
    if exit != 0 {
        return fail("Source checkpoint contains failed tests");
    }
    Ok(())
}
pub fn report_directory(root: &Path, directory: &Path, modules: &[String]) -> Result<usize> {
    report_directory_with(root, directory, modules, verify_report)
}
pub fn report_directory_with(
    root: &Path,
    directory: &Path,
    modules: &[String],
    mut verify: impl FnMut(&Path, &Value) -> Result<usize>,
) -> Result<usize> {
    let paths = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    let expected = modules
        .iter()
        .map(|module| format!("{module}.json"))
        .collect::<BTreeSet<_>>();
    let actual = paths
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return fail("Source report directory inventory differs");
    }
    let mut total = 0;
    for path in paths {
        total += verify(root, &decode(&fs::read(path)?)?)?;
    }
    Ok(total)
}

pub fn native_command(root: &Path, phase: &str, targets: &[String]) -> Result<Command> {
    if !["build", "test"].contains(&phase) {
        return fail("Native phase must be build or test");
    }
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command
        .arg("test")
        .arg("--manifest-path")
        .arg(root.join("engine-rust/Cargo.toml"))
        .args([
            "--locked",
            "--features",
            "native-product,native-browser,test-support",
            "--lib",
        ]);
    for target in targets {
        if target != "browser_adapter_live" {
            command.arg("--test").arg(target);
        }
    }
    command.args([
        "--no-fail-fast",
        "--message-format=json,json-render-diagnostics",
    ]);
    if phase == "build" {
        command.arg("--no-run");
    } else {
        command.args(["--", "--test-threads=2"]);
    }
    Ok(command)
}
pub fn native_phase(root: &Path, phase: &str) -> Result<()> {
    let inventory = super::coverage_inventory::validate(root)?;
    let registered = targets(root)?;
    target_registration(&registered)?;
    let targets = registered
        .into_iter()
        .filter_map(|target| {
            match inventory.applicable("engine-rust", &target, std::env::consts::OS) {
                Ok(true) => Some(Ok(target)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let mut command = native_command(root, phase, &targets)?;
    let output = capture(
        &mut command,
        Duration::from_secs(900),
        Duration::from_secs(300),
        8 * 1024 * 1024,
    )?;
    let report = root.join(format!("build/native-phase-{phase}.log"));
    super::evidence::retain_test_diagnostics(&report, &output.stdout, &output.stderr)?;
    let mut progress = super::evidence::CargoProgress::default();
    for line in std::str::from_utf8(&output.stdout)?.lines() {
        if let Some(line) = progress.project(line) {
            println!("{line}");
        }
    }
    if output.exit != 0 {
        return fail("Required native verification failed");
    }
    if phase == "test" {
        super::coverage_inventory::validate_native_output(
            root,
            std::str::from_utf8(&output.stdout)?,
        )?;
    }
    Ok(())
}
pub fn verify_report(root: &Path, report: &Value) -> Result<usize> {
    let inventory = super::coverage_inventory::validate(root)?;
    verify_report_with(root, report, |module, row| {
        let expected = inventory.expected(
            if module == "source_ci" {
                "tools/xtask"
            } else {
                "engine-rust"
            },
            module,
            std::env::consts::OS,
        )?;
        let stdout = row["stdout"]
            .as_str()
            .filter(|v| v.len() <= 8 * 1024 * 1024)
            .ok_or("Native report lacks bounded actual named execution evidence")?;
        super::coverage_inventory::validate_expected_output(&expected, stdout, 0)
    })
}
pub fn verify_report_with(
    root: &Path,
    report: &Value,
    mut verify: impl FnMut(&str, &Value) -> Result<usize>,
) -> Result<usize> {
    let expected = report["modules"]
        .as_array()
        .ok_or("Native report module inventory missing")?
        .iter()
        .map(|module| {
            module
                .as_str()
                .ok_or("Native report module is not text")
                .map(str::to_owned)
        })
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let known = targets(root)?
        .into_iter()
        .chain(["source_ci".into()])
        .collect::<BTreeSet<_>>();
    if expected.is_empty() || !expected.is_subset(&known) {
        return fail("Native report selected unknown or empty modules");
    }
    let results = report["results"]
        .as_array()
        .ok_or("Native report results missing")?;
    let mut seen = BTreeSet::new();
    let mut total = 0;
    for row in results {
        let module = row["module"]
            .as_str()
            .ok_or("Native report module missing")?;
        if !expected.contains(module)
            || !seen.insert(module.to_owned())
            || row["exit"] != 0
            || row["settled"] != true
        {
            return fail("Native report contains missing, duplicate or failed module evidence");
        }
        let count = row["passed"]
            .as_u64()
            .filter(|count| *count > 0)
            .ok_or("Native module has no passing evidence")?;
        if row["failed"] != 0 || row["ignored"] != 0 {
            return fail("Native source evidence failed or skipped cases");
        }
        if verify(module, row)? != count as usize {
            return fail("Native reported count differs from named execution evidence");
        }
        total += count as usize;
    }
    if seen != expected {
        return fail("Native report omits a maintained module");
    }
    Ok(total)
}
pub fn source_run(root: &Path, args: &[OsString]) -> Result<()> {
    if args.len() != 3 {
        return fail("source-run requires shard shards new-output-directory");
    }
    let shard = args[0].to_string_lossy().parse::<usize>()?;
    let shards = args[1].to_string_lossy().parse::<usize>()?;
    let modules = targets(root)?
        .into_iter()
        .filter(|target| target.starts_with("source_"))
        .collect::<Vec<_>>();
    let modules = partition(&modules, shard, shards)?;
    let output = PathBuf::from(&args[2]);
    fs::create_dir(&output)?;
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let mut checkpoint = Checkpoint::new(modules.clone(), 1, started)?;
    let mut results = Vec::new();
    let counts_pattern = regex::Regex::new(
        r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;",
    )?;
    for module in &modules {
        checkpoint.begin_module(module)?;
        save(&output.join("checkpoint.json"), &checkpoint)?;
        let remaining =
            checkpoint.end - SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
        if remaining <= 0.0 {
            return fail("Source partition exhausted its600second budget");
        }
        let mut command = Command::new("cargo");
        command
            .arg("test")
            .arg("--manifest-path")
            .arg(root.join("engine-rust/Cargo.toml"))
            .args([
                "--locked",
                "--features",
                "native-product,native-browser,test-support",
                "--test",
                module,
                "--",
                "--test-threads=2",
                "--color=never",
            ]);
        let result = capture(
            &mut command,
            Duration::from_secs_f64(remaining),
            Duration::from_secs_f64(remaining.min(120.0)),
            8 * 1024 * 1024,
        )?;
        fs::write(output.join(format!("{module}.stdout")), &result.stdout)?;
        let text = std::str::from_utf8(&result.stdout)?;
        if result.exit == 0 {
            super::coverage_inventory::validate_target_output(
                root,
                "engine-rust",
                module,
                std::env::consts::OS,
                text,
                0,
            )?;
        }
        let counts = counts_pattern
            .captures(text)
            .ok_or("Native module has no complete test verdict")?;
        let number = |index| -> Result<u64> {
            Ok(counts
                .get(index)
                .ok_or("Verdict field missing")?
                .as_str()
                .parse()?)
        };
        let exit = if result.exit == 0 {
            0
        } else if result.exit == 101 {
            1
        } else {
            return fail("Native module exit does not prove settled test completion");
        };
        checkpoint.finish_module(module, exit)?;
        save(&output.join("checkpoint.json"), &checkpoint)?;
        results.push(json!({"module":module,"exit":exit,"settled":true,"passed":number(1)?,"failed":number(2)?,"ignored":number(3)?,"stdout":text}));
    }
    let report = json!({"schema":1,"modules":modules,"results":results});
    save(&output.join("report.json"), &report)?;
    verify_report(root, &report)?;
    Ok(())
}

/// Maintenance selectors use the same native Cargo registration as full CI.
pub fn seam(root: &Path, scope: &str, phase: &str) -> Result<()> {
    if scope == "profiles" {
        return native_phase(root, phase);
    }
    let target = match scope {
        "identity-script" | "init-script" => "init_script_parity",
        "browser-options" => "browser_options_parity",
        "browser-admission" => "browser_launch_admission",
        "browser-resources" => "browser_resources_parity",
        "browser-ip" => "browser_ip_parity",
        "browser-hints" => "browser_hints_parity",
        "browser-cookies" => "browser_cookies_parity",
        "browser-extensions" => "browser_extensions_parity",
        "browser-startup" => "browser_startup_parity",
        "cookie-export" => "cookie_export_parity",
        "http-fields" => "http_fields_parity",
        "profile-write" => "profile_write_parity",
        "profile-overview" => "profile_overview_parity",
        "profile-batch" => "profile_batch_parity",
        "profile-http-import" => "profile_http_import_parity",
        "profile-http-delete" => "profile_http_delete_parity",
        "ready" => "ready_parity",
        _ => return fail("Unknown native maintenance seam"),
    };
    let registered = targets(root)?;
    target_registration(&registered)?;
    if !registered.iter().any(|name| name == target) {
        return fail("Native maintenance target is missing");
    }
    let output = super::owned::run(
        root,
        &mut native_command(root, phase, &[target.into()])?,
        super::owned::limits(
            Duration::from_secs(900),
            Duration::from_secs(300),
            8 * 1024 * 1024,
        ),
    )?;
    let mut projection = super::evidence::CargoProgress::default();
    for line in std::str::from_utf8(&output.command.stdout)?.lines() {
        if let Some(line) = projection.project(line) {
            println!("{line}");
        }
    }
    if output.command.exit != 0 {
        return fail("Required native maintenance seam failed");
    }
    Ok(())
}
