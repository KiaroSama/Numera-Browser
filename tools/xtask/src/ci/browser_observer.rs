//! Own the measured process tree and sample only after natural settlement.
use super::{Result, fail};
use numera_core::ownership::{
    capture::Pipe, identity, recorded_tree, tree::OwnedTree, unit_report,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
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
    fn register_before(&mut self, _deadline: u64) -> Result<()> {
        self.register()
    }
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
    command_deadline: u64,
    mut ticks: impl FnMut() -> u64,
    mut read: impl FnMut(u64) -> Result<unit_report::Report>,
    mut pause: impl FnMut(Duration) -> Result<()>,
    mut own_tree: impl FnMut(&Value) -> Result<bool>,
) -> Result<BTreeSet<(u64, u64)>> {
    let deadline = command_deadline.min(
        ticks()
            .checked_add(2_000_000_000)
            .ok_or("Browser observer registration deadline overflow")?,
    );
    loop {
        if ticks() >= deadline {
            return fail("Browser observer receipt registration or birth identity missing");
        }
        let report = read(deadline)?;
        let now = ticks();
        if now >= deadline {
            return fail("Browser observer receipt registration or birth identity missing");
        }
        if report.complete
            && report
                .trees
                .iter()
                .any(|record| record["pid"].as_u64() == Some(u64::from(pid)))
        {
            let mut matched = false;
            for observer in report
                .trees
                .iter()
                .filter(|record| key(record) == (u64::from(pid), birth))
            {
                if own_tree(observer)? {
                    matched = true;
                    break;
                }
            }
            if !matched {
                return fail("Browser observer ownership identity differs from its receipt");
            }
            if ticks() >= deadline {
                return fail(
                    "Browser observer registration exceeded its original command deadline",
                );
            }
            return Ok(report.trees.iter().map(key).collect());
        }
        pause(Duration::from_nanos((deadline - now).min(10_000_000)))?;
    }
}
pub struct Native {
    command: Command,
    report: PathBuf,
    prior: BTreeSet<(u64, u64)>,
    tree: Option<OwnedTree>,
    pipes: [Option<Pipe>; 2],
    started: Instant,
    diagnostics: bool,
    recorder: Option<super::super::memory::Recorder>,
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
        let deadline = self
            .ticks()
            .checked_add(2_000_000_000)
            .ok_or("Browser observer registration deadline overflow")?;
        self.register_before(deadline)
    }
    fn register_before(&mut self, command_deadline: u64) -> Result<()> {
        let deadline = command_deadline.min(
            self.ticks()
                .checked_add(2_000_000_000)
                .ok_or("Browser observer registration deadline overflow")?,
        );
        let end = self
            .started
            .checked_add(Duration::from_nanos(deadline))
            .ok_or("Browser observer registration deadline overflow")?;
        unit_report::validate(&self.report)?;
        let pid = u64::from(std::process::id());
        let birth = identity::started_at(std::process::id())?
            .ok_or("Browser observer birth identity missing")?;
        let started = self.started;
        let registration = registration(
            pid as u32,
            birth,
            deadline,
            || u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            |_| Ok(unit_report::read_before(&self.report, end)?),
            |duration| {
                std::thread::sleep(duration);
                Ok(())
            },
            |record| Ok(recorded_tree::current_record_matches(record)?),
        );
        match registration {
            Ok(prior) => self.prior = prior,
            Err(error) => {
                let diagnostic = unit_report::diagnostic(&self.report, Duration::ZERO, None)
                    .unwrap_or_else(
                        |error| json!({"available":false,"query_error_code":error.raw_os_error()}),
                    );
                eprintln!(
                    "Browser observer registration refusal {}",
                    json!({"pid":pid,"birth":birth,"receipt":diagnostic})
                );
                return Err(error);
            }
        }
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
            let owner = super::super::memory::Owner::from_record(
                &self.tree.as_ref().ok_or("Browser owner missing")?.record(),
            )?;
            self.recorder = Some(super::super::memory::Recorder::new(
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
        if exit != 0 || !output[1].is_empty() {
            return fail(format!(
                "Browser sample observation failed exit={exit} {}",
                failure_projection(&String::from_utf8_lossy(&output[1]))
            ));
        }
        let value = super::super::decode(&output[0])?;
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
