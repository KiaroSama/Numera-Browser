//! Five real entry-point workloads, comparing an immutable native checkpoint
//! with the current native product. This does not measure a Python runtime.
use super::{Result, browser_benchmark as browser, fail, migration};
use crate::acceptance::{self, Captured, Fixture};
use numera_core::{benchmark_http, ownership::unit_report, storage::locks::DataLock};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub const REPETITIONS: i32 = 7;
pub const SENTINEL: &[u8] = b"\0\xffbenchmark session bytes\r\n";
const FIRST: &str = "000000000000";
const SECRET: &str = "synthetic-benchmark-value";
const PASSWORD: &str = "synthetic-benchmark-master";
const OUTPUT_LIMIT: usize = 131_072;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    Create,
    List,
    VaultRead,
    Backup,
    HttpList,
}
pub const SCENARIOS: [Scenario; 5] = [
    Scenario::Create,
    Scenario::List,
    Scenario::VaultRead,
    Scenario::Backup,
    Scenario::HttpList,
];
impl Scenario {
    pub fn name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::List => "list",
            Self::VaultRead => "vault-read",
            Self::Backup => "backup",
            Self::HttpList => "http-list",
        }
    }
    fn arguments(self, destination: &Path) -> Vec<OsString> {
        match self {
            Self::Create => [
                "create",
                "Measured creation",
                "--seed",
                "43",
                "--engine",
                "playwright",
            ]
            .map(OsString::from)
            .to_vec(),
            Self::List => vec!["list".into()],
            Self::VaultRead => vec!["show".into(), FIRST.into()],
            Self::Backup => vec!["backup".into(), FIRST.into(), destination.into()],
            Self::HttpList => Vec::new(),
        }
    }
}

fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
/// Extend the shared migration snapshot with the source's lock/session bytes.
/// No lock files or session claims are silently discarded as measurement noise.
pub fn protected_bytes(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<String, Vec<u8>>,
        depth: usize,
    ) -> Result<()> {
        if depth > 128 || out.len() > 100_000 {
            return fail("Entry benchmark protected input exceeds traversal bound");
        }
        let meta = match fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if linked(&meta) {
            return fail("Entry benchmark contains a linked protected path");
        }
        if meta.is_dir() {
            for entry in fs::read_dir(path)? {
                walk(root, &entry?.path(), out, depth + 1)?;
            }
        } else if meta.is_file() {
            out.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(path)?,
            );
        } else {
            return fail("Entry benchmark protected input is not regular");
        }
        Ok(())
    }
    // Inspect before the shared helper follows any user-data path.
    let mut complete = BTreeMap::new();
    for name in [
        "config.json",
        "dashboard-config.json",
        "proxies.json",
        "profiles",
        "dashboard",
        "sessions",
        "user-data",
    ] {
        walk(root, &root.join(name), &mut complete, 0)?;
    }
    for (name, bytes) in migration::protected(root)? {
        if complete.get(&name) != Some(&bytes) {
            return fail("Entry benchmark protected snapshot changed while observed");
        }
    }
    Ok(complete)
}

pub struct Baseline {
    pub plain: PathBuf,
    pub encrypted: PathBuf,
    pub profile: Value,
}
/// Seed with the checkpoint's actual create/vault commands; only the source's
/// deterministic32-record expansion is performed by the runner itself.
pub fn baseline(
    parent: &Path,
    mut execute: impl FnMut(&Path, &[OsString], Option<&str>) -> Result<Captured>,
) -> Result<Baseline> {
    let plain = parent.join("baseline");
    let args = [
        "create",
        "Benchmark fixture",
        "--seed",
        "42",
        "--engine",
        "playwright",
        "--proxy",
        "http://fixture:synthetic-benchmark-value@127.0.0.1:8080",
        "--json",
    ]
    .map(OsString::from);
    let output = execute(&plain, &args, None)?;
    let created = checked_output(output, "baseline create")?;
    if created["ok"] != true {
        return fail("Entry benchmark baseline creation did not succeed");
    }
    let profile = &created["profile"];
    let id = profile_id(profile)?;
    for directory in ["profiles", "dashboard"] {
        let source = plain.join(directory).join(format!("{id}.json"));
        let raw = super::decode(&fs::read(&source)?)?;
        if directory == "profiles" {
            acceptance::require_equal(&raw, profile, "Entry baseline persisted profile")?;
            if raw["proxy"]["password"] != SECRET {
                return fail("Entry benchmark baseline lost its synthetic credential");
            }
        } else if raw["id"] != id || raw["name"] != profile["name"] {
            return fail("Entry benchmark baseline dashboard identity differs");
        }
        for index in 0..32 {
            let mut value = raw
                .as_object()
                .ok_or("Entry baseline record is not an object")?
                .clone();
            value.insert("id".into(), json!(format!("{index:012x}")));
            value.insert("name".into(), json!(format!("Benchmark {index:02}")));
            fs::write(
                source
                    .parent()
                    .ok_or("Baseline parent missing")?
                    .join(format!("{index:012x}.json")),
                serde_json::to_vec(&value)?,
            )?;
        }
        if !(0..32).any(|index| id == format!("{index:012x}")) {
            fs::remove_file(source)?;
        }
    }
    let marker = plain
        .join("user-data")
        .join(FIRST)
        .join("Default/synthetic.bin");
    fs::create_dir_all(marker.parent().ok_or("Baseline session parent missing")?)?;
    fs::write(marker, SENTINEL)?;
    let encrypted = parent.join("vault baseline");
    browser::copy_tree(&plain, &encrypted)?;
    let output = execute(
        &encrypted,
        &["vault".into(), "enable".into(), "--json".into()],
        Some(PASSWORD),
    )?;
    acceptance::require_equal(
        &checked_output(output, "baseline vault enable")?,
        &json!({"ok":true,"enabled":true,"encrypted":64}),
        "Entry benchmark encrypted baseline",
    )?;
    let profile = super::decode(&fs::read(
        plain.join("profiles").join(format!("{FIRST}.json")),
    )?)?;
    Ok(Baseline {
        plain,
        encrypted,
        profile,
    })
}
fn checked_output(output: Captured, operation: &str) -> Result<Value> {
    if output.exit != 0 || !output.stderr.is_empty() || output.stdout.len() > OUTPUT_LIMIT {
        return fail(format!(
            "Entry benchmark refused operation={operation} exit={} stdout_bytes={} stderr_bytes={}",
            output.exit,
            output.stdout.len(),
            output.stderr.len()
        ));
    }
    super::decode(output.stdout.as_bytes())
}
fn profile_id(profile: &Value) -> Result<&str> {
    profile["id"]
        .as_str()
        .filter(|id| {
            id.len() == 12
                && id
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        .ok_or_else(|| io::Error::other("Entry benchmark profile identity is invalid").into())
}
fn require_ids(payload: &Value) -> Result<()> {
    let rows = payload
        .as_array()
        .ok_or("Entry benchmark profile listing is not an array")?;
    let expected = (0..32)
        .map(|index| format!("{index:012x}"))
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    for row in rows {
        let id = row["id"]
            .as_str()
            .ok_or("Entry benchmark listed identity is not text")?;
        if !row.is_object() || !ids.insert(id.to_owned()) {
            return fail("Entry benchmark listing duplicated a profile");
        }
    }
    if rows.len() != 32 || ids != expected {
        return fail("Entry benchmark listing omitted a baseline profile");
    }
    Ok(())
}
fn creation(root: &Path, before: &BTreeMap<String, Vec<u8>>, payload: &Value) -> Result<Value> {
    if payload["ok"] != true {
        return fail("Entry benchmark measured creation did not succeed");
    }
    let profile = &payload["profile"];
    let id = numera_core::storage::paths::safe_id(
        profile["id"]
            .as_str()
            .ok_or("Entry benchmark creation identity is not text")?,
    )?;
    let stored = super::decode(&fs::read(root.join("profiles").join(format!("{id}.json")))?)?;
    migration::require_profile(&stored, profile)?;
    acceptance::require_equal(&stored, profile, "Entry measured creation persistence")?;
    let previous = before
        .get("config.json")
        .map(|bytes| super::decode(bytes))
        .transpose()?
        .unwrap_or(json!({}));
    let mut settings = previous
        .as_object()
        .ok_or("Entry benchmark previous config is not an object")?
        .clone();
    settings.insert("default_engine".into(), json!("playwright"));
    acceptance::require_equal(
        &super::decode(&fs::read(root.join("config.json"))?)?,
        &Value::Object(settings),
        "Entry benchmark requested settings",
    )?;
    let mut after = protected_bytes(root)?;
    match before.get("config.json") {
        Some(bytes) => {
            after.insert("config.json".into(), bytes.clone());
        }
        None => {
            after.remove("config.json");
        }
    }
    for directory in ["profiles", "dashboard"] {
        let name = format!("{directory}/{id}.json");
        if before.contains_key(&name) || after.remove(&name).is_none() {
            return fail("Entry benchmark creation overwrote or omitted a profile copy");
        }
        let record = root.join(&name);
        let lock = DataLock::for_file(&record)?;
        // DataLock hashes the source-compatible canonical spelling. On Windows
        // it removes the verbatim prefix that canonicalize adds to `root`.
        // The parent location is fixed here; only its actual lock name is used.
        let lock_name = lock
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Entry creation lock filename is not text")?;
        let relative = format!("{directory}/.locks/{lock_name}");
        if !before.contains_key(&relative) && after.get(&relative).is_some_and(Vec::is_empty) {
            after.remove(&relative);
        }
    }
    if after != *before {
        return fail("Entry benchmark creation changed an unrelated protected record");
    }
    let mut dashboard = super::decode(&fs::read(
        root.join("dashboard").join(format!("{id}.json")),
    )?)?
    .as_object()
    .ok_or("Entry benchmark dashboard is not an object")?
    .clone();
    if dashboard.get("id") != profile.get("id") {
        return fail("Entry benchmark creation dashboard identity differs");
    }
    dashboard.remove("id");
    dashboard.remove("_created");
    let mut profile = profile
        .as_object()
        .ok_or("Created profile is not an object")?
        .clone();
    for key in ["id", "created_at", "updated_at"] {
        profile.remove(key);
    }
    let mut normalized = payload
        .as_object()
        .ok_or("Creation payload is not an object")?
        .clone();
    normalized.insert("profile".into(), Value::Object(profile));
    normalized.insert("dashboard".into(), Value::Object(dashboard));
    Ok(Value::Object(normalized))
}
fn backup(path: &Path, payload: &Value, expected: &Value) -> Result<Value> {
    acceptance::require_equal(
        payload,
        &json!({"ok":true,"id":FIRST,"name":"Benchmark 00","file":path}),
        "Entry benchmark backup verdict",
    )?;
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut members = BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        if entry.size() > 1_048_576 || members.len() >= 2 {
            return fail("Entry benchmark backup contains unexpected session data");
        }
        let name = entry.name()?.into_owned();
        let mut bytes = Vec::new();
        entry.by_ref().take(1_048_577).read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 || members.insert(name, bytes).is_some() {
            return fail("Entry benchmark backup contains duplicate or oversized data");
        }
    }
    let profile = super::decode(
        &members
            .remove("profile.json")
            .ok_or("Entry benchmark backup profile missing")?,
    )?;
    acceptance::require_equal(&profile, expected, "Entry benchmark backed up profile")?;
    if members != BTreeMap::from([("user-data/Default/synthetic.bin".into(), SENTINEL.to_vec())]) {
        return fail("Entry benchmark backup lost or added session data");
    }
    Ok(profile)
}
/// Production mutation/parity validation is also callable by existing negative
/// controls, without replacing the measured command or manufacturing evidence.
pub fn observation(
    template: &Path,
    workspace: &Path,
    scenario: Scenario,
    expected: &Value,
    execute: impl FnOnce(&Path, &Path) -> Result<browser::Sample>,
) -> Result<(browser::Sample, Value)> {
    protected_bytes(template)?;
    let root = workspace.join("data with spaces");
    browser::copy_tree(template, &root)?;
    let root = root.canonicalize()?;
    let before = protected_bytes(&root)?;
    let destination = workspace.join("backup.zip");
    let sample = execute(&root, &destination)?;
    if sample.elapsed_ns == 0 || sample.bytes == 0 || sample.metric.is_empty() {
        return fail("Entry benchmark sample is not a positive lifetime observation");
    }
    let payload = if scenario == Scenario::Create {
        creation(&root, &before, &sample.payload)?
    } else {
        if protected_bytes(&root)? != before {
            return fail("Read-only entry benchmark changed protected input bytes");
        }
        match scenario {
            Scenario::Backup => backup(&destination, &sample.payload, expected)?,
            Scenario::HttpList => {
                require_ids(&sample.payload)?;
                sample.payload.clone()
            }
            Scenario::List => {
                if sample.payload["total"] != 32 {
                    return fail("Entry benchmark CLI count differs");
                }
                require_ids(&sample.payload["profiles"])?;
                sample.payload.clone()
            }
            Scenario::VaultRead => {
                acceptance::require_equal(&sample.payload, expected, "Entry decrypted profile")?;
                sample.payload.clone()
            }
            Scenario::Create => unreachable!(),
        }
    };
    acceptance::require_equal(&payload, &payload, "Entry benchmark finite payload")?;
    Ok((sample, payload))
}

pub fn run_pairs(
    parent: &Path,
    destination: &Path,
    identity: &Value,
    mut observe: impl FnMut(Scenario, &str, &Path, i32) -> Result<(browser::Sample, Value)>,
) -> Result<Value> {
    fs::create_dir_all(parent)?;
    let run = Fixture::create(&parent.join(format!(
        "entry-benchmark-{}-{}",
        std::process::id(),
        super::sha(format!("{:?}", Instant::now()).as_bytes())
    )))?;
    let mut results = Vec::new();
    for scenario in SCENARIOS {
        let mut samples = Vec::new();
        let mut metrics = BTreeSet::new();
        for repetition in -1..REPETITIONS {
            let order = if repetition % 2 == 0 {
                ["reference", "native"]
            } else {
                ["native", "reference"]
            };
            let mut pair = Map::from_iter([("order".into(), json!(order))]);
            let mut previous = None;
            for runtime in order {
                let workspace = run
                    .path()
                    .join(format!("{}-{repetition}-{runtime}", scenario.name()));
                fs::create_dir(&workspace)?;
                let (sample, payload) = observe(scenario, runtime, &workspace, repetition)?;
                if let Some(previous) = &previous {
                    acceptance::require_equal(
                        &payload,
                        previous,
                        "Entry benchmark cross-runtime payload",
                    )?;
                }
                previous = Some(payload);
                if repetition >= 0 {
                    metrics.insert(sample.metric.clone());
                }
                eprintln!(
                    "Measured scenario={} runtime={runtime} repetition={repetition} elapsed_ns={} peak_bytes={}",
                    scenario.name(),
                    sample.elapsed_ns,
                    sample.bytes
                );
                pair.insert(runtime.into(), sample.summary());
            }
            if repetition >= 0 {
                samples.push(Value::Object(pair));
            }
        }
        if metrics.len() != 1 {
            return fail("Entry benchmark mixed incompatible memory metrics");
        }
        let values = |runtime: &str, memory: bool| {
            Value::Array(
                samples
                    .iter()
                    .map(|row| {
                        if memory {
                            row[runtime]["memory"]["bytes"].clone()
                        } else {
                            row[runtime]["elapsed_ns"].clone()
                        }
                    })
                    .collect(),
            )
        };
        results.push(json!({"scenario":scenario.name(),"time":browser::verdict(&values("native",false),&values("reference",false))?,"memory":browser::verdict(&values("native",true),&values("reference",true))?,"latency_scope":if scenario==Scenario::HttpList {"HTTP request after readiness"}else{"foreground command lifetime"},"memory_scope":if scenario==Scenario::HttpList {"server lifetime including startup and request"}else{"foreground command lifetime"},"samples":samples}));
    }
    let mut report = identity
        .as_object()
        .ok_or("Entry benchmark identity is not an object")?
        .clone();
    report.extend(Map::from_iter([
        ("schema".into(), json!(1)),
        ("repetitions".into(), json!(REPETITIONS)),
        ("warmup_pairs".into(), json!(1)),
        ("results".into(), json!(results)),
    ]));
    let report = Value::Object(report);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    serde_json::to_writer_pretty(&mut file, &report)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);
    if results
        .iter()
        .any(|row| row["time"] != "no-regression" || row["memory"] != "no-regression")
    {
        return fail(
            "Entry performance acceptance blocked: regression or inconclusive measurements; evidence retained",
        );
    }
    run.finish(Ok(report))
}

/// A fresh process for each CLI sample preserves RUSAGE_CHILDREN isolation and
/// the original15-second command-through-natural-settlement deadline.
pub fn cli_sample(arguments: &[OsString]) -> Result<()> {
    if arguments.len() < 3 || arguments[1] != "--" {
        return fail("Entry sample requires receipt -- command");
    }
    let mut command = Command::new(&arguments[2]);
    command.args(&arguments[3..]);
    let sample = browser::observe(
        &mut browser::Native::new(command, PathBuf::from(&arguments[0]), false)?,
        Duration::from_secs(15),
    )?;
    let mut output = sample.summary();
    output["payload"] = sample.payload;
    println!("{output}");
    Ok(())
}
/// The request interval begins only after authenticated readiness. The core
/// observer retains the server owner, reaps it and measures its OS lifetime peak.
pub fn http_sample(arguments: &[OsString]) -> Result<()> {
    if arguments.len() != 3 {
        return fail("Entry HTTP sample requires binary data receipt");
    }
    let binary = PathBuf::from(&arguments[0]).canonicalize()?;
    let root = PathBuf::from(&arguments[1]).canonicalize()?;
    let receipt = PathBuf::from(&arguments[2]);
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let mut command = Command::new(binary);
    command
        .arg("--data-dir")
        .arg(&root)
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--json",
        ])
        .env("NUMERA_API_TOKEN", "benchmark-only");
    let sample = benchmark_http::observe_command(
        command,
        format!("http://127.0.0.1:{port}"),
        receipt,
        &root,
    )?;
    serde_json::to_writer(io::stdout().lock(), &sample)?;
    println!();
    Ok(())
}

fn measured(
    executable: &Path,
    data: &Path,
    destination: &Path,
    workspace: &Path,
    scenario: Scenario,
    environment: &BTreeMap<String, String>,
) -> Result<browser::Sample> {
    let observer = std::env::current_exe()?;
    let mut command = Command::new(observer);
    command
        .env_clear()
        .envs(environment)
        .env("NUMERA_LOG_DIR", workspace.join("logs"))
        .arg("ci");
    if scenario == Scenario::HttpList {
        command
            .arg("entry-http-sample")
            .arg(executable)
            .arg(data)
            .arg(workspace.join("server-owner.json"));
    } else {
        let report = std::env::var_os(unit_report::ENV)
            .ok_or("Entry observer requires a parent-owned receipt")?;
        command
            .arg("entry-sample")
            .arg(report)
            .arg("--")
            .arg(executable)
            .arg("--data-dir")
            .arg(data)
            .args(scenario.arguments(destination))
            .arg("--json");
        if scenario == Scenario::VaultRead {
            command.env("NUMERA_PASSWORD", PASSWORD);
        }
    }
    let result = super::owned::run(
        workspace,
        &mut command,
        super::owned::limits(
            Duration::from_secs(25),
            Duration::from_secs(25),
            OUTPUT_LIMIT,
        ),
    )?
    .command;
    if result.exit != 0 || result.stdout.len() > OUTPUT_LIMIT {
        return fail(format!(
            "Entry observation refused scenario={} exit={} {}",
            scenario.name(),
            result.exit,
            browser::failure_projection(&String::from_utf8_lossy(&result.stderr))
        ));
    }
    let value = super::decode(&result.stdout)?;
    let metric = value["memory"]["metric"]
        .as_str()
        .filter(|metric| {
            if cfg!(target_os = "linux") {
                *metric == "peak-child-rss-bytes"
            } else {
                matches!(
                    *metric,
                    "peak-job-committed-bytes" | "peak-job-commit-bytes"
                )
            }
        })
        .ok_or("Entry observation lifetime memory metric differs")?;
    Ok(browser::Sample {
        elapsed_ns: value["elapsed_ns"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("Entry observation elapsed time must be a positive integer")?,
        metric: metric.into(),
        bytes: value["memory"]["bytes"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("Entry observation lifetime peak must be a positive integer")?,
        payload: value
            .get("payload")
            .cloned()
            .ok_or("Entry observation payload missing")?,
        diagnostics: None,
    })
}

fn identity(
    binary: &Path,
    worker: &Path,
    source: &str,
    reference: bool,
    generation: u64,
) -> Result<Value> {
    let receipt = super::build_artifact::verify(binary, worker, source, reference)?;
    let result = acceptance::execute(Command::new(binary).arg("--build-info"), None)?;
    let observed = checked_output(result, "build identity")?;
    acceptance::require_equal(
        &observed,
        &receipt["identity"],
        "Entry actual binary build identity",
    )?;
    if receipt["generation"] != generation
        || observed["generation"] != generation
        || observed["schema"] != 1
        || observed["source_sha"] != source
        || observed["owner"] != "KiaroSama"
        || observed["product"] != "Numera Browser"
        || observed["version"] != env!("CARGO_PKG_VERSION")
    {
        return fail("Entry benchmark source, generation or product identity differs");
    }
    Ok(receipt)
}

/// Owning hosted jobs provide compiler-selected release binaries. Adjacent
/// receipts bind actual Cargo opt3/no-debug-assertions evidence, source/run,
/// executable hashes and the identical native SDK worker for both sides.
pub fn run(
    root: &Path,
    reference: &Path,
    product: &Path,
    worker: &Path,
    destination: &Path,
) -> Result<()> {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || std::env::var("GITHUB_REPOSITORY").as_deref() != Ok("KiaroSama/Numera-Browser")
        || !cfg!(any(target_os = "linux", windows))
    {
        return fail("Entry measurements require their owning hosted Windows/Linux job");
    }
    let source = std::env::var("GITHUB_SHA")?;
    let generation = std::env::var("GITHUB_RUN_ID")?.parse::<u64>()?;
    if !super::canonical_sha(&source) || generation == 0 {
        return fail("Entry benchmark hosted batch identity differs");
    }
    let reference = reference.canonicalize()?;
    let product = product.canonicalize()?;
    let worker = worker.canonicalize()?;
    let reference_receipt = identity(
        &reference,
        &worker,
        super::reference::SOURCE,
        true,
        generation,
    )?;
    let product_receipt = identity(&product, &worker, &source, false, generation)?;
    let worker_receipt = identity(&worker, &worker, &source, false, generation)?;
    let parent = root.join(".ci-work/entry-comparison");
    fs::create_dir_all(&parent)?;
    let setup = Fixture::create(&parent.join(format!(
        "setup-{}-{}",
        std::process::id(),
        super::sha(format!("{:?}", Instant::now()).as_bytes())
    )))?;
    let result = (|| {
        let environment = browser::environment(
            &std::env::vars().collect(),
            setup.path(),
            &worker,
            "playwright",
            if cfg!(windows) { "win32" } else { "linux" },
        )?;
        let seed = baseline(setup.path(), |data, args, password| {
            let mut command = Command::new(&reference);
            command
                .env_clear()
                .envs(&environment)
                .arg("--data-dir")
                .arg(data)
                .args(args);
            if let Some(password) = password {
                command.env("NUMERA_PASSWORD", password);
            }
            acceptance::execute(&mut command, None)
        })?;
        let report_identity = json!({
            "comparison":"native-checkpoint-to-native-product",
            "reference_source_sha":super::reference::SOURCE,
            "source_sha":source,"generation":generation,"profile":"release-opt3",
            "binary_sha256":super::digest(&product)?,
            "reference_binary_sha256":super::digest(&reference)?,
            "sdk_binary_sha256":super::digest(&worker)?,
            "compiler_receipts":{"reference":reference_receipt,"native":product_receipt,"worker":worker_receipt},
            "platform":if cfg!(windows) {"win32"} else {"linux"},
            "target_arch":std::env::consts::ARCH,
            "measurement_runtime":"native Rust observers",
            "memory_metric_scope":"Linux largest waited child lifetime RSS; Windows retained owned Job lifetime peak commit; no sampled-RSS substitution",
            "scope":"four foreground CLI flows and HTTP profile listing; not browser or full migration acceptance"
        });
        run_pairs(
            &parent,
            destination,
            &report_identity,
            |scenario, runtime, workspace, _| {
                let template = if scenario == Scenario::VaultRead {
                    &seed.encrypted
                } else {
                    &seed.plain
                };
                observation(
                    template,
                    workspace,
                    scenario,
                    &seed.profile,
                    |data, destination| {
                        measured(
                            if runtime == "reference" {
                                &reference
                            } else {
                                &product
                            },
                            data,
                            destination,
                            workspace,
                            scenario,
                            &environment,
                        )
                    },
                )
            },
        )?;
        Ok(())
    })();
    setup.finish(result)
}
