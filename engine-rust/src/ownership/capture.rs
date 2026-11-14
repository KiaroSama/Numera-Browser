//! Bounded command capture with one pipe owner, no blocking reader threads.
pub use super::capture_pipe::Pipe;
use super::tree::OwnedTree;
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
/// Kernel ownership observations for callers that require producers to finish
/// without leaving descendants. Observations precede cleanup; a successful
/// cleanup therefore cannot erase evidence of a residual process tree.
#[derive(Debug)]
pub enum Observation {
    Spawned {
        record: serde_json::Value,
    },
    /// At most one sample every 50 ms while capture is in progress. The
    /// accompanying borrowed owner retains the original Windows Job handle.
    Polling,
    /// Initial sample for passive diagnostic work. It is not the final verdict;
    /// all callbacks return before the final retained-owner proof and cleanup.
    BeforeCleanupSample {
        record: serde_json::Value,
        leader_exited: bool,
        natural_settlement: Option<bool>,
        natural_reason: &'static str,
    },
    BeforeCleanup {
        record: serde_json::Value,
        leader_exited: bool,
        /// `None` means that kernel observation failed, rather than emptiness.
        natural_settlement: Option<bool>,
        /// Static reason from the same observation, before any logging I/O.
        natural_reason: &'static str,
    },
}

/// Job accounting can lag already-signalled process handles briefly. This
/// observation never waits for a live or uncertain member, never kills one,
/// and cannot extend the original command wall deadline.
#[cfg(any(windows, feature = "test-support"))]
pub fn natural_settlement_with(
    deadline: Instant,
    leader_exited: bool,
    mut empty: impl FnMut() -> io::Result<bool>,
    mut all_exited: impl FnMut() -> io::Result<bool>,
) -> Option<bool> {
    natural_settlement_report_with(deadline, leader_exited, &mut empty, &mut all_exited).0
}
#[cfg(any(windows, feature = "test-support"))]
pub fn natural_settlement_report_with(
    deadline: Instant,
    leader_exited: bool,
    mut empty: impl FnMut() -> io::Result<bool>,
    mut all_exited: impl FnMut() -> io::Result<bool>,
) -> (Option<bool>, &'static str) {
    let observed = match empty() {
        Ok(observed) => observed,
        Err(_) => return (None, "empty-query-failed"),
    };
    if observed || !leader_exited {
        return (
            Some(observed),
            if observed {
                "empty-proof"
            } else {
                "leader-live"
            },
        );
    }
    let Some(cap) = Instant::now().checked_add(Duration::from_millis(249)) else {
        return (None, "deadline-overflow");
    };
    let end = cap.min(deadline);
    let exhausted = if deadline <= cap {
        "original-deadline"
    } else {
        "accounting-cap"
    };
    while Instant::now() < end {
        match all_exited() {
            Ok(true) => {}
            Ok(false) => return (Some(false), "live-member"),
            Err(_) => return (None, "member-query-failed"),
        }
        thread::sleep(
            end.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(5)),
        );
        if Instant::now() >= end {
            return (Some(false), exhausted);
        }
        match empty() {
            Ok(true) => return (Some(true), "empty-proof"),
            Ok(false) => {}
            Err(_) => return (None, "empty-query-failed"),
        }
    }
    (Some(false), exhausted)
}

#[cfg(any(windows, feature = "test-support"))]
/// Refresh only a known nonempty pre-cleanup sample. No waiting or cleanup is
/// permitted between these fresh complete-tree and retained-leader queries.
pub fn refine_natural_settlement_with(
    deadline: Instant,
    initial: (Option<bool>, &'static str),
    leader_exited: bool,
    complete_empty: impl FnOnce() -> io::Result<bool>,
    retained_leader_exited: impl FnOnce() -> io::Result<bool>,
) -> (Option<bool>, &'static str) {
    // An initial error is never turned into success by another observation.
    if initial.0 != Some(false) || !leader_exited {
        return initial;
    }
    if Instant::now() >= deadline {
        return (Some(false), "original-deadline");
    }
    let empty = complete_empty();
    if Instant::now() >= deadline {
        return (Some(false), "original-deadline");
    }
    match empty {
        Ok(true) => {}
        Ok(false) => return (Some(false), "final-nonempty-tree"),
        Err(_) => return (None, "final-empty-query-failed"),
    }
    let leader = retained_leader_exited();
    if Instant::now() >= deadline {
        return (Some(false), "original-deadline");
    }
    match leader {
        Ok(true) => (Some(true), "fresh-empty-proof"),
        Ok(false) => (Some(false), "final-leader-live"),
        Err(_) => (None, "final-leader-query-failed"),
    }
}

fn cleanup_observation(
    tree: &mut OwnedTree,
    deadline: Instant,
    leader_exited: bool,
    observe: &mut impl FnMut(Observation, &OwnedTree),
) {
    let initial = natural_settlement(tree, deadline, leader_exited);
    observe(
        Observation::BeforeCleanupSample {
            record: tree.record(),
            leader_exited,
            natural_settlement: initial.0,
            natural_reason: initial.1,
        },
        tree,
    );
    #[cfg(windows)]
    let final_observation = {
        let owner = std::cell::RefCell::new(&mut *tree);
        refine_natural_settlement_with(
            deadline,
            initial,
            leader_exited,
            || owner.borrow().complete_job_empty(),
            || Ok(owner.borrow_mut().child_mut().try_wait()?.is_some()),
        )
    };
    #[cfg(not(windows))]
    let final_observation = initial;
    observe(
        Observation::BeforeCleanup {
            record: tree.record(),
            leader_exited,
            natural_settlement: final_observation.0,
            natural_reason: final_observation.1,
        },
        tree,
    );
}

fn natural_settlement(
    tree: &mut OwnedTree,
    deadline: Instant,
    leader_exited: bool,
) -> (Option<bool>, &'static str) {
    #[cfg(windows)]
    {
        // Both callbacks borrow the same retained owner. RefCell serializes
        // the short passive queries without manufacturing another owner.
        let owner = std::cell::RefCell::new(tree);
        natural_settlement_report_with(
            deadline,
            leader_exited,
            || owner.borrow_mut().empty(),
            || owner.borrow().all_job_members_exited(),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (deadline, leader_exited);
        match tree.empty() {
            Ok(true) => (Some(true), "empty-proof"),
            Ok(false) => (Some(false), "nonempty-tree"),
            Err(_) => (None, "empty-query-failed"),
        }
    }
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
    pub(crate) fn context(mut self, error: io::Error) -> Self {
        self.error = io::Error::new(error.kind(), format!("{error}; {}", self.error));
        self
    }
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
    failures.extend(super::pending_spawn::retry_retained(
        end.saturating_duration_since(Instant::now()),
    ));
    failures
}
pub fn run(command: &mut Command, limits: Limits) -> Result<Output, Failure> {
    run_controlled(command, limits, || false)
}
pub fn run_controlled(
    command: &mut Command,
    limits: Limits,
    cancelled: impl FnMut() -> bool,
) -> Result<Output, Failure> {
    run_observed(command, limits, cancelled, |_, _| {})
}
pub fn run_observed(
    command: &mut Command,
    limits: Limits,
    cancelled: impl FnMut() -> bool,
    observe: impl FnMut(Observation, &OwnedTree),
) -> Result<Output, Failure> {
    run_observed_streamed(command, limits, cancelled, observe, |_, _| {})
}
/// Project accepted bytes as they arrive while retaining the same single pipe
/// owner, output limit, process-tree proof and completion semantics.
pub fn run_observed_streamed(
    command: &mut Command,
    limits: Limits,
    cancelled: impl FnMut() -> bool,
    observe: impl FnMut(Observation, &OwnedTree),
    on_chunk: impl FnMut(usize, &[u8]),
) -> Result<Output, Failure> {
    run_observed_then(command, limits, cancelled, observe, |_, _, _| {}, on_chunk)
}
pub(crate) fn run_observed_then(
    command: &mut Command,
    limits: Limits,
    mut cancelled: impl FnMut() -> bool,
    mut observe: impl FnMut(Observation, &OwnedTree),
    mut after_tree: impl FnMut(bool, &OwnedTree, Duration),
    mut on_chunk: impl FnMut(usize, &[u8]),
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
    observe(
        Observation::Spawned {
            record: tree.record(),
        },
        &tree,
    );
    let mut cleanup_failed = false;
    let mut cleanup_observed = false;
    let mut cleanup_reported = false;
    let mut cleanup_deadline = None;
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
        for (index, pipe) in pipes.iter().enumerate() {
            if let Some(pipe) = pipe {
                pipe.prepare()?;
                #[cfg(feature = "test-support")]
                if let Some(result) = super::test_support::pipe_prepared(index) {
                    result?;
                }
                #[cfg(not(feature = "test-support"))]
                let _ = index;
            }
        }
        let mut output = Output {
            exit: -1,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let mut bytes = [0u8; 8192];
        let mut progress = Instant::now();
        let mut next_sample = progress + Duration::from_millis(50);
        let mut leader_settled = false;
        loop {
            if cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Native command cancelled",
                ));
            }
            if let Some(status) = tree.child_mut().try_wait()? {
                // Descendants may keep both writers open after the command
                // exits. End and prove our tree before waiting for their EOF,
                // then drain every already-buffered output byte normally.
                if !leader_settled {
                    let cleanup_end = Instant::now()
                        .checked_add(limits.cleanup)
                        .ok_or_else(|| io::Error::other("Capture cleanup deadline overflow"))?;
                    cleanup_deadline = Some(cleanup_end);
                    cleanup_observation(&mut tree, end, true, &mut observe);
                    cleanup_observed = true;
                    if let Err(error) =
                        tree.settle(cleanup_end.saturating_duration_since(Instant::now()))
                    {
                        cleanup_failed = true;
                        after_tree(
                            false,
                            &tree,
                            cleanup_end.saturating_duration_since(Instant::now()),
                        );
                        cleanup_reported = true;
                        return Err(error);
                    }
                    after_tree(
                        true,
                        &tree,
                        cleanup_end.saturating_duration_since(Instant::now()),
                    );
                    cleanup_reported = true;
                    leader_settled = true;
                }
                if pipes.iter().all(Option::is_none) {
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
            if !leader_settled && Instant::now() >= next_sample {
                observe(Observation::Polling, &tree);
                next_sample = Instant::now() + Duration::from_millis(50);
            }
            let mut changed = false;
            for (index, pipe) in pipes.iter_mut().enumerate() {
                let Some(reader) = pipe.as_mut() else {
                    continue;
                };
                #[cfg(feature = "test-support")]
                if let Some(result) = super::test_support::pipe_read(index) {
                    result?;
                }
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
                        on_chunk(index, &bytes[..count]);
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
    if !cleanup_observed {
        cleanup_deadline = Instant::now().checked_add(limits.cleanup);
        let leader_exited = tree.child_mut().try_wait().ok().flatten().is_some();
        cleanup_observation(&mut tree, end, leader_exited, &mut observe);
    }
    if cleanup_failed {
        return Err(Failure {
            error: io::Error::other(format!(
                "Native command cleanup remains unverified: {}",
                result
                    .err()
                    .unwrap_or_else(|| io::Error::other("Owned tree did not settle"))
            )),
            pending: Some(Box::new(tree)),
        });
    }
    if let Err(error) = tree.settle(cleanup_deadline.map_or(Duration::ZERO, |deadline| {
        deadline.saturating_duration_since(Instant::now())
    })) {
        if !cleanup_reported {
            after_tree(
                false,
                &tree,
                cleanup_deadline.map_or(Duration::ZERO, |deadline| {
                    deadline.saturating_duration_since(Instant::now())
                }),
            );
        }
        return Err(Failure {
            error: io::Error::other(format!(
                "Native command cleanup remains unverified: {error}{}",
                result
                    .as_ref()
                    .err()
                    .map_or_else(String::new, |reason| format!("; command failed: {reason}")),
            )),
            pending: Some(Box::new(tree)),
        });
    }
    if !cleanup_reported {
        after_tree(
            true,
            &tree,
            cleanup_deadline.map_or(Duration::ZERO, |deadline| {
                deadline.saturating_duration_since(Instant::now())
            }),
        );
    }
    result.map_err(Failure::from)
}
