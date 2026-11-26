//! Real whole-command browser measurements with natural nested settlement.
use super::{Result, fail};
use numera_core::ownership::unit_report;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
#[path = "browser_benchmark_contract.rs"]
mod browser_benchmark_contract;
#[path = "browser_observer.rs"]
mod browser_observer;
pub use browser_benchmark_contract::{workflow_history, workflow_measurement};
pub use browser_observer::{
    Native, Observer, Sample, failure_projection, linux_peak, registration, verdict,
};
#[path = "browser_benchmark_fixture.rs"]
mod browser_benchmark_fixture;
pub use browser_benchmark_fixture::{
    Baseline, baseline, fixture_expiry, require_cookie_persistence,
};
pub(super) fn hosted_diagnostic() -> bool {
    std::env::var("NUMERA_HOSTED_PERFORMANCE_DIAGNOSTIC").as_deref() == Ok("1")
}
pub const SENTINEL: &[u8] = b"\0\xffretained session\r\n";
pub fn observe(host: &mut impl Observer, wall: Duration) -> Result<Sample> {
    let deadline = host
        .ticks()
        .checked_add(u64::try_from(wall.as_nanos())?)
        .ok_or("Browser benchmark deadline overflow")?;
    host.register_before(deadline)?;
    let start = host.ticks();
    if start >= deadline {
        return fail("Browser observer registration exceeded its original command deadline");
    }
    if let Err(error) = host.spawn() {
        if host.owned() {
            let cleanup = host.cleanup();
            let reap = host.reap();
            let close = host.close();
            cleanup?;
            reap?;
            close?;
        }
        return Err(error);
    }
    let result = (|| {
        let payload = host.communicate(deadline)?;
        loop {
            if host.ticks() >= deadline {
                return fail(
                    "Browser benchmark nested natural settlement exceeded its original deadline",
                );
            }
            let empty = host.natural(deadline)?;
            if host.ticks() >= deadline {
                return fail("Browser benchmark natural observation arrived after its deadline");
            }
            if empty {
                break;
            }
            host.pause()?;
        }
        let elapsed_ns = host
            .ticks()
            .checked_sub(start)
            .ok_or("Browser benchmark clock moved backwards")?;
        let (metric, bytes) = host.peak()?;
        if bytes == 0 || metric.is_empty() {
            return fail("Browser benchmark lifetime peak must be a positive integer");
        }
        let diagnostics = host.diagnostic()?;
        Ok(Sample {
            elapsed_ns,
            metric,
            bytes,
            payload,
            diagnostics,
        })
    })();
    let cleanup = host.cleanup();
    let reap = host.reap();
    let close = host.close();
    cleanup?;
    reap?;
    close?;
    result
}
pub fn positive_samples(value: &Value) -> Result<Vec<u64>> {
    browser_observer::positive_samples(value)
}
pub fn environment(
    parent: &BTreeMap<String, String>,
    workspace: &Path,
    worker: &Path,
    engine: &str,
    platform: &str,
) -> Result<BTreeMap<String, String>> {
    let mut env = parent
        .iter()
        .filter(|(key, _)| {
            !key.starts_with("NUMERA_") && !matches!(key.as_str(), "PYTHONHOME" | "PYTHONPATH")
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    super::candidate::bind_sdk_cache(&mut env, engine, platform, || {
        super::candidate::effective_windows_cache(parent)
    })?;
    let home = workspace.join("synthetic home");
    fs::create_dir(&home)?;
    for key in ["HOME", "USERPROFILE"] {
        env.insert(key.into(), home.to_string_lossy().into_owned());
    }
    for key in [
        "NUMERA_HELPER_UNITS",
        "NUMERA_HELPER_SCOPE",
        "NUMERA_BROWSER_DRIVERS",
        "NUMERA_DRIVERS_DIR",
        "NUMERA_DRIVER_SDK_ROOT",
    ] {
        if let Some(value) = parent.get(key) {
            env.insert(key.into(), value.clone());
        }
    }
    for (key, value) in [
        ("NUMERA_APP_DIR", workspace.to_owned()),
        ("NUMERA_LOG_DIR", workspace.join("logs")),
        ("NUMERA_BROWSER_ADAPTER", worker.to_owned()),
        ("NUMERA_BROWSER_RUNTIME", worker.to_owned()),
    ] {
        env.insert(key.into(), value.to_string_lossy().into_owned());
    }
    Ok(env)
}
pub fn metadata(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        let meta = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        if meta.is_symlink() {
            return fail("Browser benchmark protected metadata contains a link");
        }
        if meta.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_name() != ".locks" {
                    walk(root, &entry.path(), out)?;
                }
            }
        } else if meta.is_file() {
            out.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(path)?,
            );
        } else {
            return fail("Browser benchmark protected metadata is not regular");
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    for name in [
        "config.json",
        "dashboard-config.json",
        "proxies.json",
        "profiles",
        "dashboard",
    ] {
        walk(root, &root.join(name), &mut result)?;
    }
    Ok(result)
}
pub fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let meta = fs::symlink_metadata(&from)?;
        if meta.is_symlink() {
            let target = fs::read_link(&from)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &to)?;
            #[cfg(windows)]
            {
                if from.is_dir() {
                    std::os::windows::fs::symlink_dir(target, &to)?;
                } else {
                    std::os::windows::fs::symlink_file(target, &to)?;
                }
            }
        } else if meta.is_dir() {
            copy_tree(&from, &to)?;
        } else if meta.is_file() {
            fs::copy(&from, &to)?;
        } else {
            return fail("Browser benchmark input contains special file");
        }
    }
    Ok(())
}
pub fn observation(
    template: &Path,
    workspace: &Path,
    id: &str,
    mut execute: impl FnMut(&Path, &Path) -> Result<Sample>,
) -> Result<(Sample, Value)> {
    let data = workspace.join("data with spaces");
    copy_tree(template, &data)?;
    let before = metadata(&data)?;
    let marker = data.join("user-data").join(id).join("sentinel.bin");
    if fs::read(&marker)? != SENTINEL {
        return fail("Browser benchmark input session sentinel differs");
    }
    let destination = workspace.join("cookies.json");
    let result = execute(&data, &destination);
    for event in super::candidate::diagnostics(&workspace.join("logs"))? {
        eprintln!("{event}");
    }
    let sample = result?;
    crate::acceptance::require_equal(
        &sample.payload,
        &json!({"ok":true,"count":2,"file":destination,"format":"json"}),
        "Browser benchmark export metadata",
    )?;
    if metadata(&data)? != before {
        return fail("Browser benchmark changed protected metadata");
    }
    if fs::read(marker)? != SENTINEL {
        return fail("Browser benchmark changed session sentinel");
    }
    if data.join("sessions").is_dir()
        && fs::read_dir(data.join("sessions"))?
            .any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|v| v == "json")))
    {
        return fail("Browser benchmark retained a session claim");
    }
    if fs::metadata(&destination)?.len() > 131072 {
        return fail("Browser benchmark cookie export exceeds its bound");
    }
    let payload = super::decode(&fs::read(destination)?)?;
    Ok((sample, payload))
}
pub fn run_pairs(
    parent: &Path,
    destination: &Path,
    engine: &str,
    baseline: &Value,
    observe: impl FnMut(&str, &Path, i32, bool) -> Result<(Sample, Value)>,
) -> Result<Value> {
    run_pairs_with_policy(parent, destination, engine, baseline, false, observe)
}

pub fn run_pairs_with_policy(
    parent: &Path,
    destination: &Path,
    engine: &str,
    baseline: &Value,
    diagnostic: bool,
    mut observe: impl FnMut(&str, &Path, i32, bool) -> Result<(Sample, Value)>,
) -> Result<Value> {
    fs::create_dir_all(parent)?;
    let run = parent.join(format!(
        "browser-benchmark-{}-{}",
        std::process::id(),
        super::sha(format!("{:?}", Instant::now()).as_bytes())
    ));
    fs::create_dir(&run)?;
    let mut samples = Vec::new();
    let mut metrics = BTreeSet::new();
    for index in -1i32..7 {
        let order = if index % 2 == 0 {
            ["reference", "native"]
        } else {
            ["native", "reference"]
        };
        let mut pair = serde_json::Map::new();
        pair.insert("order".into(), json!(order));
        for runtime in order {
            let workspace = run.join(format!("browser-pair-{index}-{runtime}"));
            fs::create_dir(&workspace)?;
            let (sample, payload) = observe(runtime, &workspace, index, false)?;
            crate::acceptance::require_equal(&payload, baseline, "Browser cookie export parity")?;
            if index >= 0 {
                metrics.insert(sample.metric.clone());
            }
            pair.insert(runtime.into(), sample.summary());
        }
        if index >= 0 {
            samples.push(Value::Object(pair));
        }
    }
    if metrics.len() != 1 {
        return fail("Browser benchmark mixed incompatible memory metrics");
    }
    let values = |runtime: &str, memory: bool| {
        Value::Array(
            samples
                .iter()
                .map(|r| {
                    if memory {
                        r[runtime]["memory"]["bytes"].clone()
                    } else {
                        r[runtime]["elapsed_ns"].clone()
                    }
                })
                .collect(),
        )
    };
    let time = verdict(&values("native", false), &values("reference", false))?;
    let memory = verdict(&values("native", true), &values("reference", true))?;
    let report = json!({"schema":1,"performance_policy":if diagnostic {"diagnostic"} else {"required"},"comparison":"native-checkpoint-to-native-product","reference_source_sha":super::reference::SOURCE,"engine":engine,"repetitions":7,"warmup_pairs":1,"results":[{"scenario":"browser-cookie-export","time":time,"memory":memory,"latency_scope":"foreground command through natural SDK settlement","memory_scope":"lifetime OS peak; Linux largest waited child, not concurrent tree RSS","samples":samples}]});
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
    if time != "no-regression" || memory != "no-regression" {
        let diagnosis: Result<()> = (|| {
            if memory != "no-regression" {
                for runtime in ["reference", "native"] {
                    let workspace = run.join(format!("browser-pair-diagnostic-{runtime}"));
                    fs::create_dir(&workspace)?;
                    let (_, payload) = observe(runtime, &workspace, -2, true)?;
                    crate::acceptance::require_equal(
                        &payload,
                        baseline,
                        "Diagnostic cookie export parity",
                    )?;
                }
            }
            Ok(())
        })();
        for event in super::candidate::diagnostics(&run.join("logs"))? {
            eprintln!("{event}");
        }
        diagnosis?;
        if !diagnostic {
            return fail("Browser performance inconclusive or regression; input evidence retained");
        }
        eprintln!("Browser performance comparison is diagnostic: time={time} memory={memory}");
    }
    fs::remove_dir_all(run)?;
    Ok(report)
}

pub fn sample_command(arguments: &[std::ffi::OsString]) -> Result<()> {
    let boundary = arguments
        .iter()
        .position(|v| v == "--")
        .ok_or("Browser sample command boundary missing")?;
    if boundary < 1 || boundary + 1 >= arguments.len() {
        return fail("Browser sample receipt and command required");
    }
    let report = PathBuf::from(&arguments[0]);
    let diagnostic = arguments[1..boundary].iter().any(|v| v == "--diagnostics");
    let mut command = Command::new(&arguments[boundary + 1]);
    command.args(&arguments[boundary + 2..]);
    let sample = observe(
        &mut Native::new(command, report, diagnostic)?,
        Duration::from_secs(90),
    )?;
    let mut result = sample.summary();
    result["payload"] = sample.payload;
    if let Some(value) = sample.diagnostics {
        result["diagnostics"] = value;
    }
    println!("{result}");
    Ok(())
}
pub fn run(
    root: &Path,
    reference: &Path,
    candidate: &Path,
    worker: &Path,
    destination: &Path,
    engine: &str,
) -> Result<()> {
    if !matches!(engine, "playwright" | "patchright" | "camoufox" | "cloak")
        || std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || std::env::var("GITHUB_REPOSITORY").as_deref() != Ok("KiaroSama/Numera-Browser")
        || !cfg!(any(windows, target_os = "linux"))
    {
        return fail("Browser measurements require the owning hosted Windows/Linux SDK job");
    }
    let generation = std::env::var("GITHUB_RUN_ID")?.parse::<u64>()?;
    for (binary, sha, acceptance) in [
        (reference, super::reference::SOURCE.to_owned(), true),
        (candidate, std::env::var("GITHUB_SHA")?, false),
    ] {
        super::build_artifact::verify(binary, worker, &sha, acceptance)?;
        let output = crate::acceptance::execute(Command::new(binary).arg("--build-info"), None)?;
        let identity = super::decode(output.stdout.as_bytes())?;
        let expected_version = if acceptance {
            super::reference::VERSION
        } else {
            env!("CARGO_PKG_VERSION")
        };
        if output.exit != 0
            || !output.stderr.is_empty()
            || identity["source_sha"] != sha
            || identity["generation"] != generation
            || identity["schema"] != 1
            || identity["owner"] != "KiaroSama"
            || identity["product"] != "Numera Browser"
            || identity["version"] != expected_version
            || identity["acceptance"] != acceptance
            || identity["application_runtime"]
                != if acceptance {
                    "native-candidate"
                } else {
                    "native"
                }
        {
            return fail("Browser benchmark application source identity differs");
        }
    }
    let parent = root.join(".ci-work/browser-comparison");
    fs::create_dir_all(&parent)?;
    let setup = parent.join(format!(
        "setup-{}-{}",
        std::process::id(),
        super::sha(format!("{:?}", Instant::now()).as_bytes())
    ));
    fs::create_dir(&setup)?;
    let env = environment(
        &std::env::vars().collect(),
        &setup,
        worker,
        engine,
        if cfg!(windows) { "win32" } else { "linux" },
    )?;
    let seed = baseline(&setup, engine, |data, args| {
        let mut command = Command::new(reference);
        command
            .env_clear()
            .envs(&env)
            .arg("--data-dir")
            .arg(data)
            .args(args);
        crate::acceptance::execute_within(&mut command, None, Duration::from_secs(90))
    })?;
    let observer = std::env::current_exe()?;
    run_pairs_with_policy(
        &parent,
        destination,
        engine,
        &seed.payload,
        hosted_diagnostic(),
        |runtime, workspace, _, diagnostic| {
            observation(&seed.root, workspace, &seed.id, |data, destination| {
                let report = std::env::var_os(unit_report::ENV)
                    .ok_or("Browser measurement observer requires parent-owned receipt")?;
                let mut command = Command::new(&observer);
                command
                    .env_clear()
                    .envs(&env)
                    .env("NUMERA_LOG_DIR", workspace.join("logs"))
                    .args(["ci", "browser-sample"])
                    .arg(report);
                if diagnostic {
                    command.arg("--diagnostics");
                }
                command
                    .arg("--")
                    .arg(if runtime == "reference" {
                        reference
                    } else {
                        candidate
                    })
                    .arg("--data-dir")
                    .arg(data)
                    .args(["cookies", "export"])
                    .arg(&seed.id)
                    .arg(destination)
                    .args(["--format", "json", "--json"]);
                let output = super::owned::run(
                    root,
                    &mut command,
                    super::owned::limits(
                        Duration::from_secs(100),
                        Duration::from_secs(100),
                        131072,
                    ),
                )?;
                if output.command.exit != 0 {
                    return fail("Browser measured observer refused acceptance");
                }
                let value = super::decode(&output.command.stdout)?;
                Ok(Sample {
                    elapsed_ns: value["elapsed_ns"]
                        .as_u64()
                        .ok_or("Browser elapsed time missing")?,
                    metric: value["memory"]["metric"]
                        .as_str()
                        .ok_or("Browser memory metric missing")?
                        .into(),
                    bytes: value["memory"]["bytes"]
                        .as_u64()
                        .ok_or("Browser memory peak missing")?,
                    payload: value["payload"].clone(),
                    diagnostics: value.get("diagnostics").cloned(),
                })
            })
        },
    )?;
    fs::remove_dir_all(setup)?;
    Ok(())
}
