//! Owned HTTP observations and retained paired evidence for the native CI gate.
use crate::ownership::tree::OwnedTree;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub const RESPONSE_LIMIT: usize = 1_048_576;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Memory {
    pub metric: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub elapsed_ns: u64,
    pub memory: Memory,
    pub payload: Value,
}
/// OS/process operations stay injectable without changing timing or acceptance.
pub trait Observation {
    fn spawn(&mut self) -> io::Result<()>;
    fn receipt(&mut self) -> io::Result<()>;
    fn exited(&mut self) -> io::Result<bool>;
    fn ready(&mut self) -> io::Result<()>;
    fn ticks(&mut self) -> u64;
    fn request(&mut self) -> io::Result<Value>;
    fn settle(&mut self) -> io::Result<()>;
    fn reap(&mut self) -> io::Result<()>;
    fn close_pipes(&mut self) -> io::Result<()>;
    fn peak(&mut self) -> io::Result<Memory>;
}
pub fn observe(host: &mut impl Observation) -> io::Result<Sample> {
    host.spawn()?;
    let result = (|| {
        host.receipt()?;
        if host.exited()? {
            return Err(io::Error::other(
                "HTTP benchmark server exited before readiness",
            ));
        }
        host.ready()?;
        let start = host.ticks();
        let payload = host.request()?;
        let elapsed_ns = host
            .ticks()
            .checked_sub(start)
            .ok_or_else(|| io::Error::other("Benchmark clock moved backwards"))?;
        if host.exited()? {
            return Err(io::Error::other(
                "HTTP benchmark server exited during request",
            ));
        }
        Ok((elapsed_ns, payload))
    })();
    let settled = host.settle();
    let reaped = host.reap();
    let closed = host.close_pipes();
    let (elapsed_ns, payload) = result?;
    settled?;
    reaped?;
    closed?;
    Ok(Sample {
        elapsed_ns,
        payload,
        memory: host.peak()?,
    })
}
pub async fn request(url: &str, token: &str, budget: Duration) -> io::Result<Value> {
    let client = crate::network::client(budget)?;
    let mut response = client
        .get(url)
        .bearer_auth(token)
        .header("connection", "close")
        .send()
        .await
        .map_err(crate::network::failure)?;
    if response.status() != reqwest::StatusCode::OK
        || !response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/json"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP benchmark response status or content type differs",
        ));
    }
    if response
        .content_length()
        .is_some_and(|n| n > RESPONSE_LIMIT as u64)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP benchmark response exceeds its bound",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(crate::network::failure)? {
        if chunk.len() > RESPONSE_LIMIT.saturating_sub(bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP benchmark response exceeds its bound",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP benchmark response is not UTF8",
        )
    })?;
    serde_json::from_str(text).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP benchmark response is not complete JSON",
        )
    })
}
struct NativeObservation {
    command: Command,
    tree: Option<OwnedTree>,
    units: Option<crate::ownership::capture_units::Units>,
    readers: Vec<JoinHandle<io::Result<()>>>,
    runtime: tokio::runtime::Runtime,
    origin: String,
    receipt: PathBuf,
    clock: Instant,
}
fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut total = 0;
        let mut bytes = [0; 8192];
        loop {
            let n = pipe.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            total += n;
        }
        if total > 65536 {
            Err(io::Error::other(
                "HTTP benchmark server output exceeds its bound",
            ))
        } else {
            Ok(())
        }
    })
}
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn child_peak() -> io::Result<u64> {
    // SAFETY: zeroed rusage is a valid initialized output buffer; getrusage
    // writes that exact buffer and the call takes no borrowed process memory.
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
    if unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage) } != 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(usage.ru_maxrss)
        .ok()
        .and_then(|n| n.checked_mul(1024))
        .ok_or_else(|| io::Error::other("Child peak memory is out of range"))
}
impl NativeObservation {
    fn new(command: Command, origin: String, receipt: PathBuf, root: &Path) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        if child_peak()? != 0 {
            return Err(io::Error::other(
                "Each Linux HTTP sample requires a fresh observer",
            ));
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (command, origin, receipt, root);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "HTTP benchmark requires Windows or Linux",
            ))
        }
        #[cfg(any(target_os = "linux", windows))]
        {
            let mut command = command;
            let units = crate::ownership::capture_units::Units::new(root, &mut command)?;
            Ok(Self {
                command,
                origin,
                receipt,
                tree: None,
                units: Some(units),
                readers: vec![],
                runtime: tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?,
                clock: Instant::now(),
            })
        }
    }
}
impl Observation for NativeObservation {
    fn spawn(&mut self) -> io::Result<()> {
        self.command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut tree = OwnedTree::spawn(&mut self.command)?;
        if let Some(pipe) = tree.child_mut().stdout.take() {
            self.readers.push(drain(pipe));
        }
        if let Some(pipe) = tree.child_mut().stderr.take() {
            self.readers.push(drain(pipe));
        }
        self.tree = Some(tree);
        Ok(())
    }
    fn receipt(&mut self) -> io::Result<()> {
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| io::Error::other("Benchmark tree missing"))?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.receipt)?;
        serde_json::to_writer(&mut output, &tree.record()).map_err(io::Error::other)?;
        output.write_all(b"\n")?;
        output.sync_all()
    }
    fn exited(&mut self) -> io::Result<bool> {
        Ok(self
            .tree
            .as_mut()
            .ok_or_else(|| io::Error::other("Benchmark tree missing"))?
            .child_mut()
            .try_wait()?
            .is_some())
    }
    fn ready(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.exited()? {
                return Err(io::Error::other(
                    "HTTP benchmark server exited before readiness",
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "HTTP benchmark readiness exceeded deadline",
                ));
            }
            if self
                .runtime
                .block_on(request(
                    &format!("{}/api/ready", self.origin),
                    "benchmark-only",
                    remaining.min(Duration::from_secs(2)),
                ))
                .is_ok_and(|v| v["ok"] == true && v["service"] == "numera-browser")
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn ticks(&mut self) -> u64 {
        u64::try_from(self.clock.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
    fn request(&mut self) -> io::Result<Value> {
        self.runtime.block_on(request(
            &format!("{}/api/profiles", self.origin),
            "benchmark-only",
            Duration::from_secs(2),
        ))
    }
    fn settle(&mut self) -> io::Result<()> {
        let tree = self
            .tree
            .as_mut()
            .ok_or_else(|| io::Error::other("Benchmark tree missing"))?
            .settle(Duration::from_secs(4));
        let units = match &mut self.units {
            Some(units) => units.settle(Duration::from_secs(5)),
            None => Ok(()),
        };
        tree?;
        units
    }
    fn reap(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let child = self
            .tree
            .as_mut()
            .ok_or_else(|| io::Error::other("Benchmark tree missing"))?
            .child_mut();
        loop {
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "HTTP benchmark child did not reap",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn close_pipes(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.readers.iter().any(|r| !r.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if self.readers.iter().any(|r| !r.is_finished()) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP benchmark pipes did not reach EOF",
            ));
        }
        for reader in self.readers.drain(..) {
            reader
                .join()
                .map_err(|_| io::Error::other("HTTP benchmark pipe task failed"))??;
        }
        Ok(())
    }
    fn peak(&mut self) -> io::Result<Memory> {
        #[cfg(target_os = "linux")]
        return Ok(Memory {
            metric: "peak-child-rss-bytes".into(),
            bytes: child_peak()?,
        });
        #[cfg(windows)]
        return Ok(Memory {
            metric: "peak-job-commit-bytes".into(),
            bytes: self
                .tree
                .as_ref()
                .ok_or_else(|| io::Error::other("Benchmark tree missing"))?
                .peak_job_memory()?,
        });
        #[cfg(not(any(target_os = "linux", windows)))]
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "HTTP benchmark requires Windows or Linux",
        ))
    }
}
impl Drop for NativeObservation {
    fn drop(&mut self) {
        if let Some(mut tree) = self.tree.take()
            && tree.settle(Duration::from_secs(5)).is_err()
        {
            crate::ownership::capture::retain_tree(tree);
        }
    }
}
pub fn observe_command(
    command: Command,
    origin: String,
    receipt: PathBuf,
    root: &Path,
) -> io::Result<Sample> {
    observe(&mut NativeObservation::new(command, origin, receipt, root)?)
}
fn protected(root: &Path) -> io::Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(base, &path, out)?;
            } else if path.extension().is_some_and(|e| e == "json") {
                out.insert(
                    path.strip_prefix(base)
                        .map_err(io::Error::other)?
                        .to_owned(),
                    fs::read(path)?,
                );
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result)?;
    Ok(result)
}
pub struct PairEvidence {
    root: PathBuf,
    ids: BTreeSet<String>,
    before: BTreeMap<PathBuf, Vec<u8>>,
}
impl PairEvidence {
    pub fn capture(root: &Path, ids: impl IntoIterator<Item = String>) -> io::Result<Self> {
        let ids = ids.into_iter().collect();
        Ok(Self {
            root: root.to_owned(),
            ids,
            before: protected(root)?,
        })
    }
    pub fn validate(&self, sample: Sample) -> io::Result<(Value, Value)> {
        if protected(&self.root)? != self.before {
            return Err(io::Error::other(
                "HTTP benchmark changed protected input bytes",
            ));
        }
        let rows = sample
            .payload
            .as_array()
            .ok_or_else(|| io::Error::other("HTTP benchmark profiles must be an array"))?;
        let mut ids = BTreeSet::new();
        for row in rows {
            let id = row["id"]
                .as_str()
                .ok_or_else(|| io::Error::other("HTTP benchmark profile identity must be text"))?;
            if !ids.insert(id.to_owned()) {
                return Err(io::Error::other(
                    "HTTP benchmark profiles contain duplicate identities",
                ));
            }
        }
        if ids != self.ids || rows.len() != self.ids.len() {
            return Err(io::Error::other("HTTP benchmark profiles are incomplete"));
        }
        Ok((
            json!({"elapsed_ns":sample.elapsed_ns,"memory":sample.memory}),
            sample.payload,
        ))
    }
}
fn verdict(native: &[u64], original: &[u64]) -> &'static str {
    if native.iter().max() < original.iter().min() {
        "no-regression"
    } else if native.iter().min() > original.iter().max() {
        "regression"
    } else {
        "inconclusive"
    }
}
/// Failed pairs and refused evidence writes remain on disk for review. Success
/// removes only the exclusively-created run directory after report acceptance.
pub fn run_pairs(
    parent: &Path,
    destination: &Path,
    mut observation: impl FnMut(&str, &Path) -> io::Result<Sample>,
) -> io::Result<Value> {
    fs::create_dir_all(parent)?;
    let mut nonce = [0; 8];
    getrandom::fill(&mut nonce).map_err(io::Error::other)?;
    let run = parent.join(format!(
        "benchmark-{}-{:016x}",
        std::process::id(),
        u64::from_le_bytes(nonce)
    ));
    fs::create_dir(&run)?;
    let mut expected = None;
    let mut samples = Vec::new();
    let mut native_time = Vec::new();
    let mut original_time = Vec::new();
    let mut native_memory = Vec::new();
    let mut original_memory = Vec::new();
    let mut metrics = BTreeSet::new();
    for index in -1i32..7 {
        let order = if index % 2 == 0 {
            ["original", "native"]
        } else {
            ["native", "original"]
        };
        let mut pair = serde_json::Map::new();
        pair.insert("order".into(), json!(order));
        for runtime in order {
            let directory = run.join(format!("pair-{index}-{runtime}"));
            fs::create_dir(&directory)?;
            let sample = observation(runtime, &directory)?;
            if expected
                .as_ref()
                .is_some_and(|value| value != &sample.payload)
            {
                return Err(io::Error::other(
                    "Benchmark output parity differs; evidence retained",
                ));
            }
            expected = Some(sample.payload.clone());
            if index >= 0 {
                metrics.insert(sample.memory.metric.clone());
                if runtime == "native" {
                    native_time.push(sample.elapsed_ns);
                    native_memory.push(sample.memory.bytes);
                } else {
                    original_time.push(sample.elapsed_ns);
                    original_memory.push(sample.memory.bytes);
                }
            }
            pair.insert(
                runtime.into(),
                json!({"elapsed_ns":sample.elapsed_ns,"memory":sample.memory}),
            );
        }
        if index >= 0 {
            samples.push(Value::Object(pair));
        }
    }
    if metrics.len() != 1 {
        return Err(io::Error::other(
            "Benchmark mixed incompatible memory metrics; evidence retained",
        ));
    }
    let time = verdict(&native_time, &original_time);
    let memory = verdict(&native_memory, &original_memory);
    let report = json!({"schema":1,"warmup_pairs":1,"repetitions":7,"results":[{"scenario":"http-list","time":time,"memory":memory,"latency_scope":"HTTP request after readiness","samples":samples}]});
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    serde_json::to_writer_pretty(&mut file, &report).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    if time != "no-regression" || memory != "no-regression" {
        return Err(io::Error::other(
            "Performance acceptance blocked: regression or inconclusive measurements; evidence retained",
        ));
    }
    fs::remove_dir_all(run)?;
    Ok(report)
}
pub fn main() -> io::Result<()> {
    let matches = clap::Command::new("numera-http-observer")
        .about("One owned native HTTP observation; measures the request after readiness")
        .arg(
            clap::Arg::new("runtime")
                .long("runtime")
                .required(true)
                .value_parser(["native", "original"]),
        )
        .arg(clap::Arg::new("candidate").long("candidate").required(true))
        .arg(clap::Arg::new("data-dir").long("data-dir").required(true))
        .arg(clap::Arg::new("receipt").long("receipt").required(true))
        .get_matches();
    let executable = PathBuf::from(
        matches
            .get_one::<String>("candidate")
            .ok_or_else(|| io::Error::other("Candidate missing"))?,
    )
    .canonicalize()?;
    let root = PathBuf::from(
        matches
            .get_one::<String>("data-dir")
            .ok_or_else(|| io::Error::other("Data directory missing"))?,
    )
    .canonicalize()?;
    let receipt = PathBuf::from(
        matches
            .get_one::<String>("receipt")
            .ok_or_else(|| io::Error::other("Ownership receipt missing"))?,
    );
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let mut command = Command::new(executable);
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
    let sample = observe_command(command, format!("http://127.0.0.1:{port}"), receipt, &root)?;
    serde_json::to_writer(io::stdout().lock(), &sample).map_err(io::Error::other)?;
    println!();
    Ok(())
}
