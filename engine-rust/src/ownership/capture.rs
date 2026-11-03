//! Bounded command capture with one pipe owner, no blocking reader threads.
use super::{capture_pipe::Pipe, tree::OwnedTree};
use std::{
    io,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Limits {
    pub wall: Duration,
    pub idle: Duration,
    pub bytes: usize,
    pub cleanup: Duration,
}
pub struct Output {
    pub exit: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
pub struct Failure {
    error: io::Error,
    pending: Option<Box<OwnedTree>>,
}
impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureFailure")
            .field("error", &self.error)
            .field("unsettled", &self.pending.is_some())
            .finish()
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for Failure {}
impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self {
            error,
            pending: None,
        }
    }
}
impl Failure {
    pub fn kind(&self) -> io::ErrorKind {
        self.error.kind()
    }
    pub fn retained_record(&self) -> Option<serde_json::Value> {
        self.pending.as_ref().map(|tree| tree.record())
    }
    pub fn settle(&mut self, budget: Duration) -> io::Result<()> {
        if let Some(tree) = self.pending.as_mut() {
            tree.settle(budget)?;
            self.pending = None;
        }
        Ok(())
    }
}
fn retained() -> &'static std::sync::Mutex<Vec<OwnedTree>> {
    static OWNERS: std::sync::OnceLock<std::sync::Mutex<Vec<OwnedTree>>> =
        std::sync::OnceLock::new();
    OWNERS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}
pub(crate) fn retain_tree(tree: OwnedTree) {
    retained()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(tree);
}
impl Drop for Failure {
    fn drop(&mut self) {
        if let Err(error) = self.settle(Duration::from_secs(2)) {
            eprintln!("Native command cleanup remains owned: {error}");
            if let Some(owner) = self.pending.take() {
                retained()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(*owner);
            }
        }
    }
}
pub fn retry_retained(budget: Duration) -> Vec<String> {
    let owners = std::mem::take(
        &mut *retained()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let end = Instant::now()
        .checked_add(budget)
        .unwrap_or_else(Instant::now);
    let mut pending = Vec::new();
    let mut failures = Vec::new();
    for mut owner in owners {
        if let Err(error) = owner.settle(end.saturating_duration_since(Instant::now())) {
            failures.push(error.to_string());
            pending.push(owner);
        }
    }
    retained()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(pending);
    failures
}
pub fn run(command: &mut Command, limits: Limits) -> Result<Output, Failure> {
    run_controlled(command, limits, || false)
}
pub fn run_controlled(
    command: &mut Command,
    limits: Limits,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Output, Failure> {
    if cancelled() {
        return Err(
            io::Error::new(io::ErrorKind::ConnectionAborted, "Native command cancelled").into(),
        );
    }
    if limits.wall.is_zero()
        || limits.idle.is_zero()
        || limits.cleanup.is_zero()
        || limits.bytes == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Capture limits must be positive",
        )
        .into());
    }
    let started = Instant::now();
    let end = started
        .checked_add(limits.wall)
        .ok_or_else(|| io::Error::other("Capture deadline overflow"))?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tree = OwnedTree::spawn(command)?;
    let result = (|| -> io::Result<Output> {
        let out = tree
            .child_mut()
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Captured stdout missing"))?;
        let err = tree
            .child_mut()
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("Captured stderr missing"))?;
        let mut pipes = [Some(Pipe::Out(out)), Some(Pipe::Err(err))];
        for pipe in pipes.iter().flatten() {
            pipe.prepare()?;
        }
        let mut output = Output {
            exit: -1,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let mut bytes = [0u8; 8192];
        let mut progress = Instant::now();
        loop {
            if cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Native command cancelled",
                ));
            }
            if pipes.iter().all(Option::is_none)
                && let Some(status) = tree.child_mut().try_wait()?
            {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    output.exit = status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(1));
                }
                #[cfg(not(unix))]
                {
                    output.exit = status.code().unwrap_or(-1);
                }
                return Ok(output);
            }
            let remaining = end.saturating_duration_since(Instant::now());
            let idle = limits.idle.saturating_sub(progress.elapsed());
            if remaining.is_zero() || idle.is_zero() {
                let leader_exited = match tree.child_mut().try_wait() {
                    Ok(Some(_)) => "true",
                    Ok(None) => "false",
                    Err(_) => "unknown",
                };
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Native command exceeded its deadline: deadline={} elapsed_ms={} idle_ms={} leader_exited={leader_exited} stdout_eof={} stderr_eof={} stdout_bytes={} stderr_bytes={}",
                        if remaining.is_zero() { "wall" } else { "idle" },
                        started.elapsed().as_millis(),
                        progress.elapsed().as_millis(),
                        pipes[0].is_none(),
                        pipes[1].is_none(),
                        output.stdout.len(),
                        output.stderr.len(),
                    ),
                ));
            }
            let mut changed = false;
            for (index, pipe) in pipes.iter_mut().enumerate() {
                let Some(reader) = pipe.as_mut() else {
                    continue;
                };
                match reader.available(&mut bytes)? {
                    Some(0) => {
                        *pipe = None;
                        changed = true;
                    }
                    Some(count) => {
                        if count
                            > limits
                                .bytes
                                .saturating_sub(output.stdout.len() + output.stderr.len())
                        {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Native command output exceeds capture limit",
                            ));
                        }
                        if index == 0 {
                            output.stdout.extend_from_slice(&bytes[..count]);
                        } else {
                            output.stderr.extend_from_slice(&bytes[..count]);
                        }
                        changed = true;
                    }
                    None => {}
                }
            }
            if changed {
                progress = Instant::now();
            } else {
                thread::sleep(remaining.min(idle).min(Duration::from_millis(10)));
            }
        }
    })();
    // Pipes close before settlement even when a writer escaped the POSIX group;
    // no reader/join can keep this call alive past its explicit cleanup budget.
    if let Err(error) = tree.settle(limits.cleanup) {
        return Err(Failure {
            error: io::Error::other(format!(
                "Native command cleanup remains unverified: {error}"
            )),
            pending: Some(Box::new(tree)),
        });
    }
    result.map_err(Failure::from)
}
