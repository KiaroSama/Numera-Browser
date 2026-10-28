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
        // Failed creation can be uncertain after the OS accepted a child. Leave
        // the intent pending rather than inventing a zero-unit cancellation.
        let mut tree = Self::spawn_tree(command)?;
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
    pub fn empty(&mut self) -> io::Result<bool> {
        let _ = self.child.try_wait()?;
        #[cfg(windows)]
        {
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
        if !self.empty()? {
            #[cfg(windows)]
            self.job.terminate()?;
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
                terminate_group(self.group)?;
            }
        }
        loop {
            if self.empty()? {
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
    #[cfg(test)]
    let root = PROCFS_ROOT.with(|root| root.borrow().clone());
    #[cfg(not(test))]
    let root = std::path::Path::new("/proc");
    let mut unknown = false;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_none_or(|s| s.parse::<u32>().is_err())
        {
            continue;
        }
        let text = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                unknown = true;
                continue;
            }
        };
        let Some((_, rest)) = text.rsplit_once(") ") else {
            unknown = true;
            continue;
        };
        let fields: Vec<_> = rest.split_whitespace().collect();
        if fields.len() < 3 {
            unknown = true;
            continue;
        }
        match fields[2].parse::<i32>() {
            Ok(member_group) if member_group == group && fields[0] != "Z" => return Ok(false),
            Ok(_) => {}
            Err(_) => unknown = true,
        }
    }
    if unknown {
        return Err(io::Error::other(
            "Process table incomplete; tree emptiness unknown",
        ));
    }
    Ok(true)
}
#[cfg(target_os = "macos")]
use super::macos::group_empty;

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
    // SAFETY: positive group created by this owner; birth identity checked.
    if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}
