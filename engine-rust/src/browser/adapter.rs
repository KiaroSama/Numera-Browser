use crate::{
    ownership::session::Session,
    runtime_log::{NativePhase, RuntimeLog},
};
use serde_json::{Map, Value, json};
use std::{
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

pub(crate) const MAX_FRAME: usize = 1_048_576;
pub(crate) const PROTOCOL: u64 = 1;
/// The native owner acquires the lease and claim before any SDK operation.
/// Requests carry only effective SDK options, never a source-engine command.
struct PendingWrite {
    bytes: Vec<u8>,
    answer: mpsc::SyncSender<io::Result<()>>,
}
pub struct Adapter {
    session: Session,
    output: Option<Receiver<io::Result<Value>>>,
    input: Option<mpsc::SyncSender<PendingWrite>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    sequence: u64,
    metadata: Value,
    relay: Option<crate::network::relay::Relay>,
    log: Option<RuntimeLog>,
}
fn join_worker(
    worker: &mut Option<JoinHandle<()>>,
    deadline: std::time::Instant,
    label: &str,
) -> io::Result<()> {
    while worker.as_ref().is_some_and(|worker| !worker.is_finished()) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("Browser adapter {label} is still settling; its owner is retained"),
            ));
        }
        thread::sleep(remaining.min(Duration::from_millis(5)));
    }
    if let Some(worker) = worker.take() {
        worker
            .join()
            .map_err(|_| io::Error::other(format!("Browser adapter {label} failed")))?;
    }
    Ok(())
}
struct RetainedWorkers {
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    tree: Option<Value>,
}
impl RetainedWorkers {
    fn finish(&mut self, deadline: std::time::Instant) -> io::Result<()> {
        if let Some(tree) = &self.tree
            && !crate::ownership::recorded_tree::empty(tree)?
        {
            return Err(io::Error::other(
                "Browser adapter tree is not proved empty; I/O workers remain owned",
            ));
        }
        let writer = join_worker(&mut self.writer, deadline, "input writer");
        let reader = join_worker(&mut self.reader, deadline, "output reader");
        writer.and(reader)
    }
}
fn retained_workers() -> &'static std::sync::Mutex<Vec<RetainedWorkers>> {
    static WORKERS: std::sync::OnceLock<std::sync::Mutex<Vec<RetainedWorkers>>> =
        std::sync::OnceLock::new();
    WORKERS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}
/// Retry joining I/O workers whose adapter already requested shutdown. Their
/// handles remain owned across failed cleanup rather than becoming detached.
pub fn retry_retained(budget: Duration) -> Vec<String> {
    let workers = std::mem::take(
        &mut *retained_workers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let deadline = std::time::Instant::now()
        .checked_add(budget)
        .unwrap_or_else(std::time::Instant::now);
    let mut pending = Vec::new();
    let mut errors = Vec::new();
    for mut workers in workers {
        if let Err(error) = workers.finish(deadline) {
            errors.push(error.to_string());
            pending.push(workers);
        }
    }
    retained_workers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(pending);
    errors
}
pub fn frame(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    bounded_frame(reader, MAX_FRAME)
}
/// Exercise the same native framing boundary at a smaller contract limit.
#[cfg(feature = "test-support")]
pub fn frame_with_limit(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<Value>> {
    if limit == 0 || limit > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid adapter frame limit",
        ));
    }
    bounded_frame(reader, limit)
}
fn bounded_frame(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<Value>> {
    let mut bytes = Vec::new();
    loop {
        let (count, end, eof) = {
            let available = reader.fill_buf()?;
            let eof = available.is_empty();
            let count = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            if bytes.len() + count > limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Browser adapter frame exceeds limit",
                ));
            }
            bytes.extend_from_slice(&available[..count]);
            (
                count,
                available.get(count.saturating_sub(1)) == Some(&b'\n'),
                eof,
            )
        };
        reader.consume(count);
        if eof {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Browser adapter ended inside a frame",
                ))
            };
        }
        if end {
            break;
        }
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser adapter frame is not UTF8 JSON",
        )
    })?;
    if !value.is_object() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser adapter frame is not an object",
        ));
    }
    Ok(Some(value))
}
fn request_bytes(
    operation: &str,
    mut fields: Map<String, Value>,
    sequence: u64,
) -> io::Result<Vec<u8>> {
    fields.extend(Map::from_iter([
        ("protocol".into(), PROTOCOL.into()),
        ("id".into(), sequence.into()),
        ("operation".into(), operation.into()),
    ]));
    let mut bytes = serde_json::to_vec(&fields).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Browser adapter request exceeds frame limit",
        ));
    }
    Ok(bytes)
}
/// Remove application credentials and inherited ownership markers before the
/// SDK adapter is born. Its request carries the resolved browser options.
pub fn restrict_environment(command: &mut Command) {
    for key in [
        "NUMERA_API_TOKEN",
        "NUMERA_PASSWORD",
        "NUMERA_UPDATE_TOKEN",
        "NUMERA_UPDATE_TICKET",
        "NUMERA_UPDATE_ACTIVATION",
        "NUMERA_TREE",
        "NUMERA_READY_FILE",
    ] {
        command.env_remove(key);
    }
}
impl Adapter {
    /// Check the largest sequence envelope before any destructive SDK operation.
    pub(crate) fn preflight(operation: &str, fields: Map<String, Value>) -> io::Result<()> {
        request_bytes(operation, fields, u64::MAX).map(|_| ())
    }
    pub fn spawn(
        root: &Path,
        id: &str,
        host: &str,
        command: &mut Command,
        budget: Duration,
    ) -> io::Result<Self> {
        Self::spawn_admitted(Session::admit(root, id, host)?, command, budget)
    }
    pub fn spawn_admitted(
        mut session: Session,
        command: &mut Command,
        budget: Duration,
    ) -> io::Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        restrict_environment(command);
        let log = command
            .get_envs()
            .find(|(key, _)| *key == "NUMERA_ADAPTER_LOG_DIR")
            .and_then(|(_, value)| value)
            .filter(|directory| !directory.is_empty())
            .map(|directory| RuntimeLog::file_only(Path::new(directory), "native-browser"));
        let started = std::time::Instant::now();
        let spawned = session.spawn(command, "browser SDK adapter");
        if let Some(log) = &log {
            log.timing(NativePhase::BrowserSpawn, started.elapsed());
        }
        spawned?;
        let stdout = session
            .child_mut()?
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Browser adapter output missing"))?;
        let mut stdin = session
            .child_mut()?
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("Browser adapter input missing"))?;
        let (input, writes) = mpsc::sync_channel::<PendingWrite>(1);
        #[cfg(feature = "test-support")]
        let writer_context = crate::ownership::test_support::capture();
        let writer = thread::spawn(move || {
            while let Ok(PendingWrite { bytes, answer }) = writes.recv() {
                let result = stdin.write_all(&bytes).and_then(|_| stdin.flush());
                let failed = result.is_err();
                let _ = answer.send(result);
                if failed {
                    break;
                }
            }
            drop(stdin);
            #[cfg(feature = "test-support")]
            crate::ownership::test_support::with_context(writer_context, || {
                crate::ownership::test_support::adapter_worker_end(0);
            });
        });
        let (sender, output) = mpsc::sync_channel(8);
        #[cfg(feature = "test-support")]
        let reader_context = crate::ownership::test_support::capture();
        let reader = thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                match frame(&mut input) {
                    Ok(Some(value)) => {
                        if sender.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = sender.send(Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Browser adapter output ended",
                        )));
                        break;
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
            drop(input);
            #[cfg(feature = "test-support")]
            crate::ownership::test_support::with_context(reader_context, || {
                crate::ownership::test_support::adapter_worker_end(1);
            });
        });
        let mut adapter = Self {
            session,
            output: Some(output),
            input: Some(input),
            reader: Some(reader),
            writer: Some(writer),
            sequence: 0,
            metadata: Value::Null,
            relay: None,
            log,
        };
        let hello = adapter.call("hello", Map::new(), budget);
        match hello {
            Ok(metadata) => {
                adapter.metadata = metadata;
                Ok(adapter)
            }
            Err(error) => Err(match adapter.finish(budget) {
                Ok(()) => error,
                Err(cleanup) => io::Error::new(
                    error.kind(),
                    format!("{error}; adapter startup cleanup failed: {cleanup}"),
                ),
            }),
        }
    }
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }
    pub(crate) fn belongs_to(&self, root: &Path, id: &str, host: &str) -> bool {
        self.session.belongs_to(root, id, host)
    }
    pub(crate) fn own_relay(&mut self, relay: crate::network::relay::Relay) -> io::Result<()> {
        if self.relay.is_some() {
            return Err(io::Error::other("Browser relay already owned"));
        }
        self.relay = Some(relay);
        Ok(())
    }
    fn close_relay(&mut self, budget: Duration) -> io::Result<()> {
        if let Some(relay) = &mut self.relay {
            relay.close(budget)?;
        }
        self.relay = None;
        Ok(())
    }
    pub fn settled_exit(&mut self) -> io::Result<Option<i32>> {
        let exit = self.session.settled_exit()?;
        if exit.is_some() {
            self.close_relay(Duration::from_secs(5))?;
        }
        Ok(exit)
    }
    pub(crate) fn finish_with_status(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        self.finish_observed(budget, Session::settled_exit)
    }
    fn finish_observed(
        &mut self,
        budget: Duration,
        mut observe: impl FnMut(&mut Session) -> io::Result<Option<i32>>,
    ) -> io::Result<Option<i32>> {
        if self.session.tree_record().is_none() {
            return self.finish_inner(budget, observe);
        }
        let started = std::time::Instant::now();
        let result = self.finish_inner(budget, &mut observe);
        if let Some(log) = &self.log {
            log.timing(NativePhase::BrowserSettle, started.elapsed());
        }
        result
    }
    fn finish_inner(
        &mut self,
        budget: Duration,
        mut observe: impl FnMut(&mut Session) -> io::Result<Option<i32>>,
    ) -> io::Result<Option<i32>> {
        self.input.take();
        self.output.take();
        let deadline = std::time::Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Invalid browser cleanup budget"))?;
        // Reserve time to force and prove cleanup if cooperative EOF does not
        // arrive. Both phases consume this one caller deadline.
        let observation_end = std::time::Instant::now()
            .checked_add(budget / 2)
            .unwrap_or(deadline)
            .min(deadline);
        let mut observation = None;
        while self.session.tree_record().is_some() && std::time::Instant::now() < observation_end {
            match observe(&mut self.session) {
                Ok(None) => thread::sleep(
                    observation_end
                        .saturating_duration_since(std::time::Instant::now())
                        .min(Duration::from_millis(10)),
                ),
                Ok(Some(_)) => break,
                Err(error) => {
                    observation = Some(error);
                    break;
                }
            }
        }
        let cleanup = self
            .session
            .finish_with_status(deadline.saturating_duration_since(std::time::Instant::now()));
        if observation.is_some() && cleanup.is_ok() {
            eprintln!(
                "Browser tree observation failed; native owner subsequently settled the tree"
            );
        }
        let relay_cleanup =
            self.close_relay(deadline.saturating_duration_since(std::time::Instant::now()));
        let code = match (cleanup, relay_cleanup) {
            (Err(tree), Err(relay)) => {
                return Err(io::Error::other(format!(
                    "Browser tree cleanup failed: {tree}; relay cleanup failed: {relay}"
                )));
            }
            (Err(error), _) | (_, Err(error)) => return Err(error),
            (Ok(code), Ok(())) => code,
        };
        join_worker(&mut self.writer, deadline, "input writer")?;
        join_worker(&mut self.reader, deadline, "output reader")?;
        Ok(code)
    }
    pub(crate) fn close(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        let deadline = std::time::Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Invalid browser close budget"))?;
        let result = self.call("close", Map::new(), budget).and_then(|value| {
            if value["closing"] == true {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Browser adapter close was not confirmed",
                ))
            }
        });
        let cleanup =
            self.finish_with_status(deadline.saturating_duration_since(std::time::Instant::now()));
        match cleanup {
            Err(error) => Err(error),
            Ok(code) => {
                if result.is_err() {
                    eprintln!("Browser close failed; native owner settled its entire tree");
                    // A protocol failure cannot erase a concrete child exit.
                    // Zero/no exit still needs a failed-operation outcome.
                    Ok(Some(code.filter(|code| *code != 0).unwrap_or(-1)))
                } else {
                    Ok(code.or(Some(0)))
                }
            }
        }
    }
    pub fn open(
        &mut self,
        backend: &str,
        options: Map<String, Value>,
        scripts: Vec<String>,
        budget: Duration,
    ) -> io::Result<()> {
        self.open_configured(backend, options, scripts, None, budget)
    }
    pub fn open_configured(
        &mut self,
        backend: &str,
        options: Map<String, Value>,
        scripts: Vec<String>,
        cdp_policy: Option<Value>,
        budget: Duration,
    ) -> io::Result<()> {
        if !["playwright", "patchright", "camoufox", "cloak"].contains(&backend) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown browser backend",
            ));
        }
        if self.metadata["backends"][backend]["installed"] != true {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Requested {backend} browser SDK is not installed"),
            ));
        }
        let fields = Map::from_iter([
            ("backend".into(), backend.into()),
            ("options".into(), Value::Object(options)),
            ("init_scripts".into(), json!(scripts)),
            ("cdp_policy".into(), cdp_policy.unwrap_or(Value::Null)),
        ]);
        let value = self.call("open", fields, budget)?;
        if value["opened"] != true || value["backend"] != backend {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Browser adapter did not confirm the requested context",
            ));
        }
        Ok(())
    }
    pub fn call(
        &mut self,
        operation: &str,
        fields: Map<String, Value>,
        budget: Duration,
    ) -> io::Result<Value> {
        let phase = match operation {
            "hello" => Some(NativePhase::BrowserHello),
            "open" => Some(NativePhase::BrowserOpen),
            "cookies-read" => Some(NativePhase::BrowserCookiesRead),
            "close" => Some(NativePhase::BrowserClose),
            _ => None,
        };
        let started = std::time::Instant::now();
        let result = self.call_inner(operation, fields, budget);
        if let (Some(log), Some(phase)) = (&self.log, phase) {
            log.timing(phase, started.elapsed());
        }
        result
    }
    fn call_inner(
        &mut self,
        operation: &str,
        fields: Map<String, Value>,
        budget: Duration,
    ) -> io::Result<Value> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Browser adapter request sequence exhausted"))?;
        let bytes = request_bytes(operation, fields, self.sequence)?;
        let deadline = std::time::Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Browser adapter request budget is invalid"))?;
        let (done, written) = mpsc::sync_channel(1);
        self.input
            .as_ref()
            .ok_or_else(|| io::Error::other("Browser adapter input is closed"))?
            .try_send(PendingWrite {
                bytes,
                answer: done,
            })
            .map_err(|_| io::Error::other("Browser adapter writer is unavailable"))?;
        written
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Browser adapter request write did not complete",
                )
            })??;
        let value = self
            .output
            .as_ref()
            .ok_or_else(|| io::Error::other("Browser adapter output is closed"))?
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|error| {
                io::Error::new(
                    if error == mpsc::RecvTimeoutError::Timeout {
                        io::ErrorKind::TimedOut
                    } else {
                        io::ErrorKind::UnexpectedEof
                    },
                    "Browser adapter did not provide a complete response",
                )
            })??;
        if value["protocol"].as_u64() != Some(PROTOCOL)
            || value["id"].as_u64() != Some(self.sequence)
            || !value["ok"].is_boolean()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Browser adapter response envelope is invalid",
            ));
        }
        if value["ok"] != true {
            if value["error"]["kind"] == "sdk-cleanup" {
                return Err(io::Error::other(
                    "Browser SDK operation failed; SDK cleanup failed; inspect the sanitized adapter log",
                ));
            }
            return Err(io::Error::other(
                "Browser SDK operation failed; inspect the sanitized adapter log",
            ));
        }
        value.get("result").cloned().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Browser adapter result missing")
        })
    }
    pub fn finish(&mut self, budget: Duration) -> io::Result<()> {
        self.finish_with_status(budget).map(|_| ())
    }
}
impl Drop for Adapter {
    fn drop(&mut self) {
        if let Err(error) = self.finish(Duration::from_secs(2)) {
            eprintln!("Browser adapter cleanup remains unverified: {error}");
            if self.writer.is_some() || self.reader.is_some() {
                let tree = self.session.tree_record().map(|mut record| {
                    record["pidToken"] = record["token"].take();
                    record
                });
                retained_workers()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(RetainedWorkers {
                        reader: self.reader.take(),
                        writer: self.writer.take(),
                        tree,
                    });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as core;
    #[allow(dead_code)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/common/browser_fixture.rs"
        ));
    }
    #[test]
    fn polling_error_still_settles_sdk_tree_and_owned_relay() -> io::Result<()> {
        let mut nonce = [0; 8];
        getrandom::fill(&mut nonce).map_err(io::Error::other)?;
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("adapter-cleanup-{:x}", u64::from_le_bytes(nonce)));
        std::fs::create_dir_all(
            root.parent()
                .ok_or_else(|| io::Error::other("Fixture parent missing"))?,
        )?;
        std::fs::create_dir(&root)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut command = fixture::command(&root, "rpc", json!({}))?;
            let mut adapter = Adapter::spawn(
                &root,
                "a",
                "fixture-host",
                &mut command,
                Duration::from_secs(5),
            )?;
            let proxy = crate::proxy::Proxy::from_record(&json!({"server":"http://127.0.0.1:9","username":"fixture","password":null,"country":null}),false).map_err(io::Error::other)?;
            let relay = crate::network::relay::Relay::start(&proxy)?;
            let address = reqwest::Url::parse(&relay.url()).map_err(io::Error::other)?;
            let address = std::net::SocketAddr::new(
                "127.0.0.1".parse().map_err(io::Error::other)?,
                address
                    .port()
                    .ok_or_else(|| io::Error::other("Relay port missing"))?,
            );
            adapter.own_relay(relay)?;
            let called = std::cell::Cell::new(false);
            adapter.finish_observed(Duration::from_secs(5), |_| {
                called.set(true);
                Err(io::Error::other("Fixture process observation failure"))
            })?;
            assert!(called.get(), "Polling error path not exercised");
            assert!(adapter.relay.is_none());
            assert!(adapter.reader.is_none() && adapter.writer.is_none());
            assert!(
                std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err(),
                "Polling failure skipped relay closure"
            );
            assert!(matches!(
                crate::ownership::claims::read(&root, "a"),
                crate::ownership::claims::Claim::Absent
            ));
            Ok(())
        }));
        let cleanup = std::fs::remove_dir_all(&root);
        let result = match result {
            Ok(result) => result,
            Err(panic) => {
                if let Err(error) = cleanup {
                    eprintln!("Cleanup regression fixture removal failed: {error}");
                }
                std::panic::resume_unwind(panic)
            }
        };
        cleanup?;
        result
    }
}
