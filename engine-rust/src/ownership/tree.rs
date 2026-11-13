#[cfg(unix)]
use super::identity::started_at;
#[cfg(windows)]
use super::windows::{Job, resume};
use std::{
    io,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

pub struct OwnedTree {
    child: Child,
    #[cfg(windows)]
    job: Job,
    #[cfg(unix)]
    group: i32,
    token: u64,
    settled: bool,
}
impl OwnedTree {
    /// Capability is withheld until the child belongs to its own kernel tree.
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        let attempt = super::unit_report::begin_spawn()?;
        #[cfg(feature = "test-support")]
        if let Some(Err(error)) = super::test_support::before_spawn() {
            // This boundary precedes the OS spawn. Its intent cannot own a
            // child, unlike an uncertain failure after a spawn syscall.
            super::unit_report::cancel_spawn(attempt.as_deref())?;
            return Err(error);
        }
        // Failed creation can be uncertain after the OS accepted a child. Leave
        // the intent pending rather than inventing a zero-unit cancellation.
        let mut tree = Self::spawn_tree(command)?;
        #[cfg(feature = "test-support")]
        if let Some(Err(error)) = super::test_support::spawned(&tree.record()) {
            if tree.settle(Duration::from_secs(5)).is_ok() {
                super::unit_report::cancel_spawn(attempt.as_deref())?;
            } else {
                super::capture::retain_tree(tree);
            }
            return Err(error);
        }
        if let Err(error) = super::unit_report::record(&tree.record(), attempt.as_deref()) {
            if tree.settle(Duration::from_secs(5)).is_ok() {
                super::unit_report::cancel_spawn(attempt.as_deref())?;
            } else {
                super::capture::retain_tree(tree);
            }
            return Err(error);
        }
        Ok(tree)
    }
    fn spawn_tree(command: &mut Command) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let job = Job::new()?;
            command.creation_flags(0x0000_0004 | 0x0800_0000); // suspended, no console
            let mut child = command.spawn()?;
            let token = match super::identity::started_at(child.id()) {
                Ok(Some(token)) => token,
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io::Error::other(
                        "Suspended tree leader has no birth identity",
                    ));
                }
            };
            if let Err(error) = job.assign(&child).and_then(|_| resume(child.id())) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            Ok(Self {
                child,
                job,
                token,
                settled: false,
            })
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            let mut child = command.spawn()?;
            let group = i32::try_from(child.id()).map_err(io::Error::other)?;
            let token = match started_at(child.id()) {
                Ok(Some(token)) => token,
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io::Error::other(
                        "Spawned tree leader has no birth identity",
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
        self.child.id()
    }
    pub fn record(&self) -> serde_json::Value {
        #[cfg(windows)]
        {
            serde_json::json!({"pid":self.pid(),"token":self.token,"tree":{"kind":"job","group":self.job.name()}})
        }
        #[cfg(unix)]
        {
            serde_json::json!({"pid":self.pid(),"token":self.token,"tree":{"kind":"pgid","group":self.group}})
        }
    }
    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }
    #[cfg(windows)]
    pub fn peak_job_memory(&self) -> io::Result<u64> {
        self.job.peak_memory()
    }
    #[cfg(windows)]
    pub(crate) fn job_handle(&self) -> std::os::windows::io::BorrowedHandle<'_> {
        self.job.handle()
    }
    pub fn empty(&mut self) -> io::Result<bool> {
        let _ = self.child.try_wait()?;
        #[cfg(windows)]
        {
            #[cfg(feature = "test-support")]
            if let Some(result) = super::test_support::group_empty(self.pid() as i32) {
                return result;
            }
            self.job.empty()
        }
        #[cfg(unix)]
        {
            group_empty(self.group)
        }
    }
    pub fn settle(&mut self, budget: Duration) -> io::Result<()> {
        if self.settled {
            return Ok(());
        }
        let deadline = Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Invalid tree cleanup budget"))?;
        if !self.observed_empty(deadline)? {
            #[cfg(windows)]
            {
                #[cfg(feature = "test-support")]
                let signalled = match super::test_support::signal(-(self.pid() as i32), true) {
                    Some(result) => result,
                    None => self.job.terminate(),
                };
                #[cfg(not(feature = "test-support"))]
                let signalled = self.job.terminate();
                if let Err(error) = signalled
                    && !self.observed_empty(deadline)?
                {
                    return Err(error);
                }
            }
            #[cfg(unix)]
            {
                // A live reused leader is not ours. Unknown identity is not
                // permission to send a process-group signal.
                match started_at(self.child.id()) {
                    Ok(Some(token)) if token != self.token => {
                        return Err(io::Error::other("Process group leader identity changed"));
                    }
                    Err(error) => return Err(error),
                    _ => {}
                }
                if let Err(error) = terminate_group(self.group)
                    && !self.observed_empty(deadline)?
                {
                    return Err(error);
                }
            }
        }
        loop {
            if self.observed_empty(deadline)? {
                self.child.wait()?;
                self.settled = true;
                return Ok(());
            }
            if Instant::now() >= deadline {
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
