//! Real whole-command browser measurements with natural nested settlement.
use super::{Result, fail};
use numera_core::ownership::{
    capture::Pipe, identity, recorded_tree, tree::OwnedTree, unit_report,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub const SENTINEL: &[u8] = b"\0\xffretained session\r\n";
pub fn linux_peak(platform: &str, rss_kib: &Value) -> Result<Value> {
    if platform != "linux" {
        return fail("Lifetime child RSS requires Linux");
    }
    let bytes = rss_kib
        .as_u64()
        .filter(|v| *v > 0)
        .and_then(|v| v.checked_mul(1024))
        .ok_or("Linux lifetime RSS must be a positive integer")?;
    Ok(json!({"metric":"peak-child-rss-bytes","bytes":bytes}))
}
#[derive(Clone, Debug)]
pub struct Sample {
    pub elapsed_ns: u64,
    pub metric: String,
    pub bytes: u64,
    pub payload: Value,
    pub diagnostics: Option<Value>,
}
impl Sample {
    pub fn summary(&self) -> Value {
        json!({"elapsed_ns":self.elapsed_ns,"memory":{"metric":self.metric,"bytes":self.bytes}})
    }
}
pub trait Observer {
    fn register(&mut self) -> Result<()>;
    fn ticks(&mut self) -> u64;
    fn spawn(&mut self) -> Result<()>;
    fn owned(&self) -> bool {
        true
    }
    fn communicate(&mut self, deadline: u64) -> Result<Value>;
    fn natural(&mut self, deadline: u64) -> Result<bool>;
    fn pause(&mut self) -> Result<()>;
    fn peak(&mut self) -> Result<(String, u64)>;
    fn diagnostic(&mut self) -> Result<Option<Value>> {
        Ok(None)
    }
    fn cleanup(&mut self) -> Result<()>;
    fn reap(&mut self) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}
pub fn observe(host: &mut impl Observer, wall: Duration) -> Result<Sample> {
    host.register()?;
    let start = host.ticks();
    let deadline = start
        .checked_add(u64::try_from(wall.as_nanos())?)
        .ok_or("Browser benchmark deadline overflow")?;
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
    value
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("Benchmark samples must be positive integers")?
        .iter()
        .map(|v| {
            v.as_u64().filter(|v| *v > 0).ok_or_else(|| {
                io::Error::other("Benchmark samples must be positive integers").into()
            })
        })
        .collect()
}
pub fn verdict(native: &Value, reference: &Value) -> Result<&'static str> {
    let n = positive_samples(native)?;
    let r = positive_samples(reference)?;
    Ok(if n.iter().max() < r.iter().min() {
        "no-regression"
    } else if n.iter().min() > r.iter().max() {
        "regression"
    } else {
        "inconclusive"
    })
}
pub fn failure_projection(stderr: &str) -> String {
    let pattern=regex::Regex::new(r"^\[[^\r\n]+ UTC\] \[ERROR\] \[numera\.migration-benchmark-sample\] Observation failed phase=(spawn|capture|receipt|settle|memory|reap|close) type=(OSError|ValueError|RuntimeError|TimeoutError|NativeSdkError) winerror=(None|[0-9]{1,10})$").expect("constant regex");
    stderr
        .lines()
        .find_map(|line| {
            pattern
                .captures(line)
                .map(|v| format!("phase={} type={} winerror={}", &v[1], &v[2], &v[3]))
        })
        .unwrap_or_else(|| "phase=unknown type=unknown winerror=None".into())
}
fn key(record: &Value) -> (u64, u64) {
    (
        record["pid"].as_u64().unwrap_or(0),
        record["pidToken"]
            .as_u64()
            .or_else(|| record["token"].as_u64())
            .unwrap_or(0),
    )
}
pub fn registration(
    pid: u32,
    birth: u64,
    mut read: impl FnMut() -> Result<unit_report::Report>,
    mut pause: impl FnMut() -> Result<()>,
) -> Result<BTreeSet<(u64, u64)>> {
    let mut report = read()?;
    if !report.complete {
        pause()?;
        report = read()?;
    }
    if !report.complete
        || !report
            .trees
            .iter()
            .any(|r| key(r) == (u64::from(pid), birth))
    {
        return fail("Browser observer receipt registration or birth identity missing");
    }
    Ok(report.trees.iter().map(key).collect())
}
pub struct Native {
    command: Command,
    report: PathBuf,
    prior: BTreeSet<(u64, u64)>,
    tree: Option<OwnedTree>,
    pipes: [Option<Pipe>; 2],
    started: Instant,
    diagnostics: bool,
    recorder: Option<super::memory::Recorder>,
    payload: Option<Value>,
}
impl Native {
    pub fn new(command: Command, report: PathBuf, diagnostics: bool) -> Result<Self> {
        #[cfg(target_os = "linux")]
        if numera_core::ownership::memory::child_peak_rss_bytes()? != 0 {
            return fail("Browser sample requires a fresh isolated lifetime peak observer");
        }
        Ok(Self {
            command,
            report,
            prior: BTreeSet::new(),
            tree: None,
            pipes: [None, None],
            started: Instant::now(),
            diagnostics,
            recorder: None,
            payload: None,
        })
    }
    fn nested(&self) -> Result<Vec<Value>> {
        let report = unit_report::read(&self.report)?;
        if !report.complete {
            return fail("Browser sample nested receipt is incomplete");
        }
        Ok(report
            .trees
            .into_iter()
            .filter(|r| !self.prior.contains(&key(r)))
            .collect())
    }
}
impl Observer for Native {
    fn owned(&self) -> bool {
        self.tree.is_some()
    }
    fn register(&mut self) -> Result<()> {
        unit_report::validate(&self.report)?;
        let pid = u64::from(std::process::id());
        let birth = identity::started_at(std::process::id())?
            .ok_or("Browser observer birth identity missing")?;
        self.prior = registration(
            pid as u32,
            birth,
            || Ok(unit_report::read(&self.report)?),
            || {
                std::thread::sleep(Duration::from_millis(10));
                Ok(())
            },
        )?;
        Ok(())
    }
    fn ticks(&mut self) -> u64 {
        u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
    fn spawn(&mut self) -> Result<()> {
        self.command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut tree = OwnedTree::spawn(&mut self.command)?;
        self.pipes = [
            tree.child_mut().stdout.take().map(Pipe::Out),
            tree.child_mut().stderr.take().map(Pipe::Err),
        ];
        self.tree = Some(tree);
        for pipe in self.pipes.iter().flatten() {
            pipe.prepare()?;
        }
        if self.diagnostics {
            let owner = super::memory::Owner::from_record(
                &self.tree.as_ref().ok_or("Browser owner missing")?.record(),
            )?;
            self.recorder = Some(super::memory::Recorder::new(
                owner,
                Default::default(),
                cfg!(windows),
            ));
        }
        Ok(())
    }
    fn communicate(&mut self, deadline: u64) -> Result<Value> {
        let mut output = [Vec::new(), Vec::new()];
        let mut buf = [0; 8192];
        let exit = loop {
            let tree = self.tree.as_mut().ok_or("Browser owner missing")?;
            let exit = tree.child_mut().try_wait()?;
            if self.pipes.iter().all(Option::is_none)
                && let Some(exit) = exit
            {
                break exit.code().unwrap_or(-1);
            }
            if self.started.elapsed().as_nanos() >= u128::from(deadline) {
                return fail("Browser sample communication deadline exceeded");
            }
            let mut progress = false;
            for (index, slot) in self.pipes.iter_mut().enumerate() {
                let Some(pipe) = slot.as_mut() else { continue };
                match pipe.available(&mut buf)? {
                    Some(0) => {
                        *slot = None;
                        progress = true;
                    }
                    Some(count) => {
                        if output[0].len() + output[1].len() + count > 131072 {
                            return fail("Browser sample output bound exceeded");
                        }
                        output[index].extend_from_slice(&buf[..count]);
                        progress = true;
                    }
                    None => {}
                }
            }
            if let Some(recorder) = self.recorder.as_mut() {
                recorder.sample_native(
                    self.started.elapsed().as_secs_f64(),
                    tree,
                    Some(&self.report),
                )?;
            }
            if !progress {
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        if exit != 0 {
            return fail(format!(
                "Browser sample observation failed exit={exit} {}",
                failure_projection(&String::from_utf8_lossy(&output[1]))
            ));
        }
        let value = super::decode(&output[0])?;
        self.payload = Some(value.clone());
        Ok(value)
    }
    fn natural(&mut self, _: u64) -> Result<bool> {
        let direct = self.tree.as_mut().ok_or("Browser owner missing")?.empty()?;
        let mut empty = direct;
        for record in self.nested()? {
            empty = recorded_tree::empty(&record)? && empty;
        }
        Ok(empty)
    }
    fn pause(&mut self) -> Result<()> {
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }
    fn peak(&mut self) -> Result<(String, u64)> {
        #[cfg(target_os = "linux")]
        {
            let peak = linux_peak(
                "linux",
                &json!(numera_core::ownership::memory::child_peak_rss_bytes()? / 1024),
            )?;
            Ok((
                "peak-child-rss-bytes".into(),
                peak["bytes"]
                    .as_u64()
                    .ok_or("Linux lifetime peak missing")?,
            ))
        }
        #[cfg(windows)]
        {
            Ok((
                "peak-job-committed-bytes".into(),
                self.tree
                    .as_ref()
                    .ok_or("Browser owner missing")?
                    .peak_job_memory()?,
            ))
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            fail("Browser benchmark supports hosted Linux and Windows lifetime OS peaks")
        }
    }
    fn diagnostic(&mut self) -> Result<Option<Value>> {
        Ok(self.recorder.take().map(|r| r.result()))
    }
    fn cleanup(&mut self) -> Result<()> {
        let direct = self
            .tree
            .as_mut()
            .ok_or("Browser owner missing")?
            .settle(Duration::from_secs(5));
        let report = unit_report::read(&self.report)?;
        let mut complete = report.complete;
        for record in report
            .trees
            .iter()
            .filter(|r| !self.prior.contains(&key(r)))
        {
            complete = recorded_tree::settle(record, Duration::from_secs(5))? && complete;
        }
        direct?;
        if !complete {
            return fail("Browser sample nested cleanup is incomplete");
        }
        Ok(())
    }
    fn reap(&mut self) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(5);
        let tree = self.tree.as_mut().ok_or("Browser owner missing")?;
        while tree.child_mut().try_wait()?.is_none() {
            if Instant::now() >= end {
                return fail("Browser sample leader reap exceeded deadline");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.pipes = [None, None];
        if let Some(mut tree) = self.tree.take() {
            drop(tree.child_mut().stdin.take());
            drop(tree.child_mut().stdout.take());
            drop(tree.child_mut().stderr.take());
        }
        Ok(())
    }
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
    let report = json!({"schema":1,"comparison":"native-checkpoint-to-native-product","reference_source_sha":super::reference::SOURCE,"engine":engine,"repetitions":7,"warmup_pairs":1,"results":[{"scenario":"browser-cookie-export","time":time,"memory":memory,"latency_scope":"foreground command through natural SDK settlement","memory_scope":"lifetime OS peak; Linux largest waited child, not concurrent tree RSS","samples":samples}]});
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
        return fail("Browser performance inconclusive or regression; input evidence retained");
    }
    fs::remove_dir_all(run)?;
    Ok(report)
}

pub struct Baseline {
    pub root: PathBuf,
    pub id: String,
    pub payload: Value,
}
pub fn baseline(
    parent: &Path,
    engine: &str,
    mut execute: impl FnMut(&Path, &[String]) -> Result<crate::acceptance::Captured>,
) -> Result<Baseline> {
    let root = parent.join("baseline");
    let mut command = |args: Vec<String>| -> Result<Value> {
        let result = execute(&root, &args)?;
        if result.exit != 0 || !result.stderr.is_empty() {
            let allow = [
                "ValueError",
                "RuntimeError",
                "OSError",
                "FileNotFoundError",
                "PermissionError",
                "TimeoutError",
                "Error",
                "TargetClosedError",
                "ImportError",
                "ModuleNotFoundError",
                "NativeSdkError",
            ];
            let kind = result
                .stderr
                .lines()
                .take(512)
                .filter_map(|l| l.split_once(':').map(|(kind, _)| kind))
                .filter(|kind| allow.contains(kind))
                .last()
                .unwrap_or("unknown");
            let operation = if args.first().is_some_and(|a| a == "cookies") {
                format!("cookies-{}", args[1])
            } else {
                "create".into()
            };
            return fail(format!(
                "Native checkpoint browser baseline failed operation={operation} exit={} type={kind} stdout_bytes={} stderr_bytes={}",
                result.exit,
                result.stdout.len(),
                result.stderr.len()
            ));
        }
        super::decode(result.stdout.as_bytes())
    };
    let created = command(vec![
        "create".into(),
        "Browser benchmark fixture".into(),
        "--seed".into(),
        "42".into(),
        "--engine".into(),
        engine.into(),
        "--json".into(),
    ])?;
    let id = created["profile"]["id"]
        .as_str()
        .filter(|v| {
            v.len() == 12
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("Native checkpoint profile identity invalid")?
        .to_owned();
    let cookie = json!({"name":"persistent-fixture","value":"synthetic","domain":".fixture.test","path":"/","expires":2524608000u64,"secure":false,"httpOnly":true,"sameSite":"Lax"});
    let mut second = cookie.clone();
    second["name"] = json!("second-fixture");
    second["httpOnly"] = json!(false);
    let cookies = json!([cookie, second]);
    let input = parent.join("cookie input.json");
    fs::write(&input, serde_json::to_vec(&json!({"cookies":cookies}))?)?;
    let seeded = command(vec![
        "cookies".into(),
        "import".into(),
        id.clone(),
        input.to_string_lossy().into_owned(),
        "--clear".into(),
        "--json".into(),
    ])?;
    crate::acceptance::require_equal(
        &seeded,
        &json!({"ok":true,"imported":2,"cleared":true}),
        "Native checkpoint cookie seeding",
    )?;
    let destination = parent.join("baseline cookies.json");
    let exported = command(vec![
        "cookies".into(),
        "export".into(),
        id.clone(),
        destination.to_string_lossy().into_owned(),
        "--format".into(),
        "json".into(),
        "--json".into(),
    ])?;
    crate::acceptance::require_equal(
        &exported,
        &json!({"ok":true,"count":2,"file":destination,"format":"json"}),
        "Native checkpoint export metadata",
    )?;
    let payload = super::decode(&fs::read(destination)?)?;
    let mut observed = payload["cookies"]
        .as_array()
        .cloned()
        .ok_or("Native checkpoint cookies missing")?;
    observed.sort_by_key(|row| row["name"].as_str().unwrap_or("").to_owned());
    crate::acceptance::require_equal(
        &Value::Array(observed),
        &cookies,
        "Native checkpoint cookie persistence",
    )?;
    if payload["origins"] != json!([]) {
        return fail("Native checkpoint cookie persistence origins differ");
    }
    let marker = root.join("user-data").join(&id).join("sentinel.bin");
    fs::create_dir_all(marker.parent().ok_or("Session parent missing")?)?;
    fs::write(marker, SENTINEL)?;
    Ok(Baseline { root, id, payload })
}
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
    let build = super::workflow::step_once(steps, "build-candidate", true)?;
    if !steps[build]["run"]
        .as_str()
        .is_some_and(|run| run.contains("--release"))
    {
        return fail("Browser acceptance requires optimized candidate compilation");
    }
    super::workflow::ordered(steps, &["ci acceptance "], "ci browser-benchmark")?;
    let measure = super::workflow::step_once(steps, "ci browser-benchmark", true)?;
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
        || !uploads[0]["with"]["path"]
            .as_str()
            .is_some_and(|path| path.contains("browser-performance.json"))
    {
        return fail("Browser measurements must retain exact-source evidence");
    }
    Ok(())
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
        let output = crate::acceptance::execute(Command::new(binary).arg("--build-info"), None)?;
        let identity = super::decode(output.stdout.as_bytes())?;
        if output.exit != 0
            || !output.stderr.is_empty()
            || identity["source_sha"] != sha
            || identity["generation"] != generation
            || identity["schema"] != 1
            || identity["owner"] != "KiaroSama"
            || identity["product"] != "Numera Browser"
            || identity["version"] != env!("CARGO_PKG_VERSION")
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
        crate::acceptance::execute(&mut command, None)
    })?;
    let observer = std::env::current_exe()?;
    run_pairs(
        &parent,
        destination,
        engine,
        &seed.payload,
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
