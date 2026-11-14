#[cfg(unix)]
use super::identity::started_at;
#[cfg(windows)]
use super::windows::{CleanupWitnesses, Job, resume};
use std::{
    io,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

pub struct OwnedTree {
    #[cfg(not(windows))]
    child: Child,
    #[cfg(windows)]
    windows: Option<WindowsOwner>,
    #[cfg(unix)]
    group: i32,
    token: u64,
    settled: bool,
}
#[cfg(windows)]
struct WindowsOwner {
    child: Child,
    job: Job,
    witnesses: CleanupWitnesses,
}

/// Passive observation boundary for Windows cleanup and portable fault controls.
#[cfg(any(windows, feature = "test-support"))]
#[doc(hidden)]
pub trait WindowsCleanup {
    fn capture_members(&mut self, deadline: Instant) -> io::Result<()>;
    fn complete_empty(&mut self) -> io::Result<bool>;
    fn terminate(&mut self) -> io::Result<()>;
    fn all_signaled(&mut self, deadline: Instant) -> io::Result<bool>;
    fn leader_exited(&mut self) -> io::Result<bool>;
}
#[cfg(any(windows, feature = "test-support"))]
fn windows_before(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Owned Windows cleanup exceeded its deadline",
        ))
    } else {
        Ok(())
    }
}
#[cfg(any(windows, feature = "test-support"))]
#[doc(hidden)]
pub fn settle_windows_with(deadline: Instant, kernel: &mut impl WindowsCleanup) -> io::Result<()> {
    windows_before(deadline)?;
    kernel.capture_members(deadline)?;
    windows_before(deadline)?;
    let empty = kernel.complete_empty()?;
    windows_before(deadline)?;
    if !empty {
        // One request only. A failed signal/query never becomes success merely
        // because a later accounting observation happens to be empty.
        kernel.terminate()?;
        windows_before(deadline)?;
    }
    loop {
        // Reconcile members born between the first snapshot and termination.
        // All handle waits are passive; no repeated termination is introduced.
        kernel.capture_members(deadline)?;
        windows_before(deadline)?;
        let signaled = kernel.all_signaled(deadline)?;
        windows_before(deadline)?;
        let empty = kernel.complete_empty()?;
        windows_before(deadline)?;
        let leader = kernel.leader_exited()?;
        windows_before(deadline)?;
        if signaled && empty && leader {
            return Ok(());
        }
        thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(20)),
        );
        windows_before(deadline)?;
    }
}
#[cfg(windows)]
impl WindowsCleanup for WindowsOwner {
    fn capture_members(&mut self, deadline: Instant) -> io::Result<()> {
        self.witnesses
            .retain_members(&self.job, &self.child, deadline)
    }
    fn complete_empty(&mut self) -> io::Result<bool> {
        #[cfg(feature = "test-support")]
        if let Some(result) = super::test_support::group_empty(self.child.id() as i32) {
            return result;
        }
        self.job.complete_empty()
    }
    fn terminate(&mut self) -> io::Result<()> {
        #[cfg(feature = "test-support")]
        if let Some(result) = super::test_support::signal(-(self.child.id() as i32), true) {
            return result;
        }
        self.job.terminate()
    }
    fn all_signaled(&mut self, deadline: Instant) -> io::Result<bool> {
        self.witnesses.all_signaled(deadline)
    }
    fn leader_exited(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
}
enum SpawnFailure {
    #[cfg(windows)]
    BeforeBirth(io::Error),
    Uncertain(io::Error),
    Born(io::Error, Box<super::pending_spawn::Unpublished>),
}
impl SpawnFailure {
    fn born(error: io::Error, child: Child, tree: super::pending_spawn::KernelTree) -> Self {
        Self::Born(
            error,
            Box::new(super::pending_spawn::Unpublished::new(child, tree)),
        )
    }
}
pub(super) struct SpawnError {
    pub(super) error: io::Error,
    pub(super) settled: bool,
}
impl From<io::Error> for SpawnError {
    fn from(error: io::Error) -> Self {
        Self {
            error,
            settled: false,
        }
    }
}
impl SpawnError {
    fn after_settlement(
        error: io::Error,
        intent: Option<&super::unit_report::SpawnIntent>,
    ) -> Self {
        // Receipt failure must retain the unresolved disposition without
        // replacing the original OS error, including its raw error number.
        let settled = intent.is_none_or(|intent| intent.cancel().is_ok());
        Self { error, settled }
    }
}
impl OwnedTree {
    /// Capability is withheld until the child belongs to its own kernel tree.
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        Self::spawn_classified(command).map_err(|failure| failure.error)
    }
    pub(super) fn spawn_classified(command: &mut Command) -> Result<Self, SpawnError> {
        let attempt = super::unit_report::begin_transfer()?;
        #[cfg(feature = "test-support")]
        if let Some(Err(error)) = super::test_support::before_spawn() {
            // This boundary precedes the OS spawn. Its intent cannot own a
            // child, unlike an uncertain failure after a spawn syscall.
            return Err(SpawnError::after_settlement(error, attempt.as_ref()));
        }
        // Failed creation can be uncertain after the OS accepted a child. Leave
        // the intent pending rather than inventing a zero-unit cancellation.
        let mut tree = match Self::spawn_tree(command) {
            Ok(tree) => tree,
            #[cfg(windows)]
            Err(SpawnFailure::BeforeBirth(error)) => {
                return Err(SpawnError::after_settlement(error, attempt.as_ref()));
            }
            Err(SpawnFailure::Uncertain(error)) => return Err(error.into()),
            Err(SpawnFailure::Born(error, child)) => {
                let settled = (*child).failed(attempt);
                return Err(SpawnError { error, settled });
            }
        };
        #[cfg(feature = "test-support")]
        if let Some(Err(error)) = super::test_support::spawned(&tree.record()) {
            let settled = tree.settle(Duration::from_secs(5)).is_ok();
            if settled {
                return Err(SpawnError::after_settlement(error, attempt.as_ref()));
            }
            super::capture::retain_tree(tree);
            return Err(error.into());
        }
        if let Err(error) = attempt
            .as_ref()
            .map_or(Ok(()), |intent| intent.record(&tree.record()))
        {
            let settled = tree.settle(Duration::from_secs(5)).is_ok();
            if settled {
                return Err(SpawnError::after_settlement(error, attempt.as_ref()));
            }
            super::capture::retain_tree(tree);
            return Err(error.into());
        }
        Ok(tree)
    }
    fn spawn_tree(command: &mut Command) -> Result<Self, SpawnFailure> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let job = Job::new().map_err(SpawnFailure::BeforeBirth)?;
            command.creation_flags(0x0000_0004 | 0x0800_0000); // suspended, no console
            let child = command.spawn().map_err(SpawnFailure::Uncertain)?;
            let token = match super::identity::started_at(child.id()) {
                Ok(Some(token)) => token,
                _ => {
                    return Err(SpawnFailure::born(
                        io::Error::other("Suspended tree leader has no birth identity"),
                        child,
                        super::pending_spawn::KernelTree::Job(job),
                    ));
                }
            };
            if let Err(error) = job.assign(&child).and_then(|_| resume(child.id())) {
                return Err(SpawnFailure::born(
                    error,
                    child,
                    super::pending_spawn::KernelTree::Job(job),
                ));
            }
            Ok(Self {
                windows: Some(WindowsOwner {
                    child,
                    job,
                    witnesses: CleanupWitnesses::default(),
                }),
                token,
                settled: false,
            })
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            let child = command.spawn().map_err(SpawnFailure::Uncertain)?;
            // A Unix PID is a positive pid_t. Keep conversion failure uncertain;
            // it cannot authorize a receipt cancellation.
            let group = match i32::try_from(child.id()) {
                Ok(group) if group > 0 => group,
                _ => {
                    return Err(SpawnFailure::born(
                        io::Error::other("Spawned tree leader PID is invalid"),
                        child,
                        super::pending_spawn::KernelTree::Unobservable,
                    ));
                }
            };
            let token = match started_at(child.id()) {
                Ok(Some(token)) => token,
                _ => {
                    let captured = super::pending_spawn::KernelTree::captured_group(&child, group);
                    return Err(SpawnFailure::born(
                        io::Error::other("Spawned tree leader has no birth identity"),
                        child,
                        captured,
                    ));
                }
            };
            Ok(Self {
                child,
                group,
                token,
                settled: false,
            })
        }
    }
    pub fn pid(&self) -> u32 {
        self.child().id()
    }
    pub fn record(&self) -> serde_json::Value {
        #[cfg(windows)]
        {
            serde_json::json!({"pid":self.pid(),"token":self.token,"tree":{"kind":"job","group":self.windows().job.name()}})
        }
        #[cfg(unix)]
        {
            serde_json::json!({"pid":self.pid(),"token":self.token,"tree":{"kind":"pgid","group":self.group}})
        }
    }
    pub fn child_mut(&mut self) -> &mut Child {
        #[cfg(windows)]
        {
            &mut self
                .windows
                .as_mut()
                .expect("Live owned Windows state")
                .child
        }
        #[cfg(not(windows))]
        {
            &mut self.child
        }
    }
    fn child(&self) -> &Child {
        #[cfg(windows)]
        {
            &self.windows().child
        }
        #[cfg(not(windows))]
        {
            &self.child
        }
    }
    #[cfg(windows)]
    fn windows(&self) -> &WindowsOwner {
        // Only a dying Drop shell may have taken this original state.
        self.windows.as_ref().expect("Live owned Windows state")
    }
    #[cfg(windows)]
    pub fn peak_job_memory(&self) -> io::Result<u64> {
        self.windows().job.peak_memory()
    }
    /// An accounting-settlement hint, never an empty-tree proof. Every member
    /// must already be signaled; inaccessible or uncertain membership is Err.
    #[cfg(windows)]
    pub fn all_job_members_exited(&self) -> io::Result<bool> {
        self.windows().job.all_members_exited(self.child())
    }
    /// Fresh complete membership and accounting from the original retained Job.
    /// This is passive; callers must separately prove retained-leader exit.
    #[cfg(windows)]
    pub fn complete_job_empty(&self) -> io::Result<bool> {
        self.windows().job.complete_empty()
    }
    /// A bounded passive snapshot for failure diagnostics, not a cleanup or
    /// ownership proof. Executable identities are limited to static classes.
    #[cfg(windows)]
    pub fn job_member_diagnostics(&self) -> io::Result<serde_json::Value> {
        self.windows().job.member_diagnostics(self.child())
    }
    #[cfg(windows)]
    pub(crate) fn job_handle(&self) -> std::os::windows::io::BorrowedHandle<'_> {
        self.windows().job.handle()
    }
    pub fn empty(&mut self) -> io::Result<bool> {
        let _ = self.child_mut().try_wait()?;
        #[cfg(windows)]
        {
            #[cfg(feature = "test-support")]
            if let Some(result) = super::test_support::group_empty(self.pid() as i32) {
                return result;
            }
            self.windows().job.empty()
        }
        #[cfg(unix)]
        {
            group_empty(self.group)
        }
    }
    pub fn settle(&mut self, budget: Duration) -> io::Result<()> {
        #[cfg(windows)]
        {
            if self.settled {
                return Ok(());
            }
            let deadline = Instant::now()
                .checked_add(budget)
                .ok_or_else(|| io::Error::other("Invalid tree cleanup budget"))?;
            let owner = self.windows.as_mut().expect("Live owned Windows state");
            settle_windows_with(deadline, owner)?;
            owner.witnesses.clear_proved();
            self.settled = true;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            if self.settled {
                return Ok(());
            }
            let deadline = Instant::now()
                .checked_add(budget)
                .ok_or_else(|| io::Error::other("Invalid tree cleanup budget"))?;
            #[cfg(unix)]
            let mut signal_error = None;
            if !self.observed_empty(deadline)? {
                #[cfg(unix)]
                {
                    // A live reused leader is not ours. Unknown identity is not
                    // permission to send a process-group signal.
                    let mut uncertainty = None;
                    let may_signal = loop {
                        if Instant::now() >= deadline
                            && let Some(error) = uncertainty.take()
                        {
                            return Err(error);
                        }
                        match started_at(self.child.id()) {
                            Ok(Some(token)) if token != self.token => {
                                return Err(io::Error::other(
                                    "Process group leader identity changed",
                                ));
                            }
                            Ok(Some(_)) => break true,
                            Ok(None) if uncertainty.is_none() => break true,
                            Ok(None) => {}
                            Err(error) => uncertainty = Some(error),
                        }
                        // An exiting leader can become temporarily unreadable
                        // after a nonempty group snapshot. A denied birth query is
                        // not absence or signal authority: retry only inside the
                        // original deadline, requiring a matching birth or a
                        // complete subsequent kernel proof that the group is empty.
                        if self.observed_empty(deadline)? {
                            break false;
                        }
                        if Instant::now() >= deadline {
                            return Err(uncertainty.unwrap_or_else(|| {
                                io::Error::other("Owned leader birth remains unknown")
                            }));
                        }
                        thread::sleep(
                            deadline
                                .saturating_duration_since(Instant::now())
                                .min(Duration::from_millis(10)),
                        );
                    };
                    if may_signal && let Err(error) = terminate_group(self.group) {
                        // Refusal is not emptiness. Preserve the exact error and
                        // observe only until the existing deadline; an exiting
                        // group can disappear after the signal snapshot. Never
                        // retry this signal or turn a live/unknown group free.
                        signal_error = Some(error);
                    }
                }
            }
            loop {
                #[cfg(unix)]
                if Instant::now() >= deadline
                    && let Some(error) = signal_error.take()
                {
                    return Err(error);
                }
                let observed = self.observed_empty(deadline);
                #[cfg(unix)]
                let observed = observed.map_err(|error| signal_error.take().unwrap_or(error));
                if observed? {
                    #[cfg(unix)]
                    if signal_error.is_some() {
                        // A complete group observation and this retained child's
                        // actual exit are both required. Do not wait unboundedly
                        // on a stale empty snapshot after a refused signal.
                        if Instant::now() >= deadline
                            && let Some(error) = signal_error.take()
                        {
                            return Err(error);
                        }
                        if matches!(self.child.try_wait(), Ok(Some(_))) {
                            self.settled = true;
                            return Ok(());
                        }
                        // An error here remains an unknown child observation.
                        // Retry only within the original window, preserving the
                        // first signal error rather than replacing its OS code.
                    } else {
                        self.child.wait()?;
                        self.settled = true;
                        return Ok(());
                    }
                }
                if Instant::now() >= deadline {
                    #[cfg(unix)]
                    if let Some(error) = signal_error.take() {
                        return Err(error);
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Owned process tree is not proved empty; retain its claim",
                    ));
                }
                thread::sleep(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(20)),
                );
            }
        }
    }
    #[cfg(not(windows))]
    fn observed_empty(&mut self, deadline: Instant) -> io::Result<bool> {
        loop {
            match self.empty() {
                Ok(empty) => return Ok(empty),
                Err(error) if Instant::now() >= deadline => return Err(error),
                // A procfs row can disappear or become unreadable while the
                // process exits. Unknown remains unknown; retry only inside
                // this caller's existing cleanup deadline.
                Err(_) => thread::sleep(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(10)),
                ),
            }
        }
    }
}
impl Drop for OwnedTree {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            if self.windows.is_none() {
                return;
            }
            if self.settle(Duration::from_secs(2)).is_err()
                && let Some(windows) = self.windows.take()
            {
                // Disarm the shell before moving ORIGINAL owners to the
                // existing retry queue, whose settlement runs unlocked.
                self.settled = true;
                super::capture::retain_tree(Self {
                    windows: Some(windows),
                    token: self.token,
                    settled: false,
                });
            }
        }
        #[cfg(not(windows))]
        let _ = self.settle(Duration::from_secs(2));
    }
}

pub fn record_empty(record: &serde_json::Value) -> io::Result<bool> {
    #[cfg(windows)]
    {
        let tree = &record["tree"];
        if tree["kind"] != "job" {
            return Err(io::Error::other("Tree kind cannot be evaluated on Windows"));
        }
        let name = tree["group"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| io::Error::other("Tree job name missing"))?;
        #[cfg(feature = "test-support")]
        if let Some(pid) = record["pid"]
            .as_u64()
            .and_then(|pid| i32::try_from(pid).ok())
            && let Some(result) = super::test_support::group_empty(pid)
        {
            return result;
        }
        super::windows::named_empty(name)
    }
    #[cfg(unix)]
    {
        let tree = &record["tree"];
        if tree["kind"] != "pgid" {
            return Err(io::Error::other("Tree kind cannot be evaluated on Unix"));
        }
        let group = tree["group"]
            .as_i64()
            .filter(|g| *g > 0)
            .and_then(|g| i32::try_from(g).ok())
            .ok_or_else(|| io::Error::other("Tree process group invalid"))?;
        group_empty(group)
    }
}

#[cfg(all(test, target_os = "linux"))]
std::thread_local! {
    static PROCFS_ROOT: std::cell::RefCell<std::path::PathBuf> =
        std::cell::RefCell::new(std::path::PathBuf::from("/proc"));
}

#[cfg(all(test, target_os = "linux"))]
#[path = "procfs_tests.rs"]
mod tests;

#[cfg(target_os = "linux")]
fn group_empty(group: i32) -> io::Result<bool> {
    #[cfg(feature = "test-support")]
    if let Some(result) = super::test_support::group_empty(group) {
        return result;
    }
    #[cfg(test)]
    let default_root = PROCFS_ROOT.with(|root| root.borrow().clone());
    #[cfg(not(test))]
    let default_root = std::path::PathBuf::from("/proc");
    #[cfg(feature = "test-support")]
    let root = super::test_support::procfs_root().unwrap_or(default_root);
    #[cfg(not(feature = "test-support"))]
    let root = default_root;
    struct Procfs(std::path::PathBuf);
    impl super::observation::GroupSource for Procfs {
        fn enumerate(&mut self, _group: i32) -> io::Result<super::observation::Enumeration> {
            let mut pids = Vec::new();
            for entry in std::fs::read_dir(&self.0)? {
                let entry = entry?;
                if let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|text| text.parse::<u32>().ok())
                {
                    pids.push(pid);
                }
            }
            Ok(super::observation::Enumeration {
                pids,
                complete: true,
                filtered: false,
            })
        }
        fn process(&mut self, pid: u32) -> io::Result<Option<super::observation::Row>> {
            let path = self.0.join(pid.to_string()).join("stat");
            #[cfg(feature = "test-support")]
            let text =
                super::test_support::stat(&path).unwrap_or_else(|| std::fs::read_to_string(&path));
            #[cfg(not(feature = "test-support"))]
            let text = std::fs::read_to_string(&path);
            match text {
                Ok(text) => super::observation::procfs_process(pid, &text),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        }
    }
    super::observation::observe_group(group, None, &mut Procfs(root))
}
#[cfg(target_os = "macos")]
fn group_empty(group: i32) -> io::Result<bool> {
    #[cfg(feature = "test-support")]
    if let Some(result) = super::test_support::group_empty(group) {
        return result;
    }
    super::macos::group_empty(group)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn group_empty(_group: i32) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Native process-group observation is not implemented for this platform",
    ))
}
#[cfg(unix)]
#[allow(unsafe_code)]
fn terminate_group(group: i32) -> io::Result<()> {
    if group <= 0 {
        return Err(io::Error::other("Invalid owned process group"));
    }
    #[cfg(feature = "test-support")]
    if let Some(result) = super::test_support::signal(-group, true) {
        return result;
    }
    // SAFETY: positive group created by this owner; birth identity checked.
    if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}
