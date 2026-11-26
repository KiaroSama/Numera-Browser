//! Five real entry-point workloads, comparing an immutable native checkpoint
//! with the current native product. This does not measure a Python runtime.
use super::{Result, browser_benchmark as browser, fail};
use crate::acceptance::{self, Fixture};
use numera_core::{benchmark_http, ownership::unit_report};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{self, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[path = "entry_benchmark_fixture.rs"]
mod entry_benchmark_fixture;
use entry_benchmark_fixture::checked_output;
pub use entry_benchmark_fixture::{Baseline, baseline, observation, protected_bytes};

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

pub fn run_pairs(
    parent: &Path,
    destination: &Path,
    identity: &Value,
    observe: impl FnMut(Scenario, &str, &Path, i32) -> Result<(browser::Sample, Value)>,
) -> Result<Value> {
    run_pairs_with_policy(parent, destination, identity, false, observe)
}

pub fn run_pairs_with_policy(
    parent: &Path,
    destination: &Path,
    identity: &Value,
    diagnostic: bool,
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
        (
            "performance_policy".into(),
            json!(if diagnostic { "diagnostic" } else { "required" }),
        ),
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
        if !diagnostic {
            return fail(
                "Entry performance acceptance blocked: regression or inconclusive measurements; evidence retained",
            );
        }
        eprintln!(
            "Entry performance comparison found a regression or inconclusive result; see the report"
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
    let expected_version = if reference {
        super::reference::VERSION
    } else {
        env!("CARGO_PKG_VERSION")
    };
    if receipt["generation"] != generation
        || observed["generation"] != generation
        || observed["schema"] != 1
        || observed["source_sha"] != source
        || observed["owner"] != "KiaroSama"
        || observed["product"] != "Numera Browser"
        || observed["version"] != expected_version
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
        run_pairs_with_policy(
            &parent,
            destination,
            &report_identity,
            browser::hosted_diagnostic(),
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
