//! Finite native worker resource gate. Measure inside an isolated worker so
//! unrelated concurrent tests cannot hide or create process-wide deltas.
use serde::{Deserialize, Serialize};
use std::{
    fmt, io,
    sync::{
        Once,
        atomic::{AtomicU64, Ordering},
    },
};

static PANICS: AtomicU64 = AtomicU64::new(0);
/// Preserve reviewable test/compiler diagnostics while filtering credential
/// syntax and explicitly supplied protected CI environment values.
pub fn redact_diagnostics(text: &str) -> String {
    let mut text = crate::log_redaction::redact(text);
    for (key, value) in std::env::vars() {
        if value.len() >= 4
            && [
                "TOKEN",
                "PASSWORD",
                "SECRET",
                "API_KEY",
                "LICENSE_KEY",
                "SIGNING_KEY",
                "PFX",
            ]
            .iter()
            .any(|suffix| key.ends_with(suffix))
        {
            text = text.replace(&value, "[REDACTED]");
        }
    }
    text
}
fn install_panic_observer() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |information| {
            PANICS.fetch_add(1, Ordering::SeqCst);
            previous(information);
        }));
    });
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceSnapshot {
    pub handles: u64,
    pub threads: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceEvidence {
    pub before: ResourceSnapshot,
    pub after: ResourceSnapshot,
    pub panics: u64,
}
impl ResourceEvidence {
    pub fn clean(&self) -> bool {
        self.after.handles <= self.before.handles
            && self.after.threads <= self.before.threads
            && self.panics == 0
    }
}
#[derive(Debug)]
pub enum ResourceFailure {
    Observation(io::Error),
    Residual(ResourceEvidence),
}
impl fmt::Display for ResourceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Observation(_) => {
                formatter.write_str("Native resource cleanup observation is incomplete")
            }
            Self::Residual(evidence) => write!(
                formatter,
                "Native resource cleanup failed handles_increased={} threads_increased={} panics={}",
                evidence.after.handles > evidence.before.handles,
                evidence.after.threads > evidence.before.threads,
                evidence.panics
            ),
        }
    }
}
impl std::error::Error for ResourceFailure {}
pub struct ResourceGuard {
    before: ResourceSnapshot,
    panics: u64,
}
impl ResourceGuard {
    pub fn new() -> io::Result<Self> {
        install_panic_observer();
        Ok(Self {
            before: snapshot()?,
            panics: PANICS.load(Ordering::SeqCst),
        })
    }
    /// Observe before process teardown; the OS would otherwise close forgotten
    /// files and erase the evidence the gate must reject.
    pub fn finish(self) -> Result<ResourceEvidence, ResourceFailure> {
        let after = snapshot().map_err(ResourceFailure::Observation)?;
        let evidence = ResourceEvidence {
            before: self.before,
            after,
            panics: PANICS.load(Ordering::SeqCst).saturating_sub(self.panics),
        };
        if evidence.clean() {
            Ok(evidence)
        } else {
            Err(ResourceFailure::Residual(evidence))
        }
    }
}
/// Shared finite-worker policy. Call this with the kernel observation emitted
/// before forced cleanup; successful cleanup cannot turn a leaked child into
/// a passing worker. The owner must still finish cleanup after this verdict.
pub fn require_natural_settlement(
    leader_exited: bool,
    natural_settlement: Option<bool>,
) -> io::Result<()> {
    match (leader_exited, natural_settlement) {
        (true, Some(true)) => Ok(()),
        (_, None) => Err(io::Error::other(
            "Native worker cleanup observation is incomplete",
        )),
        (false, _) => Err(io::Error::other(
            "Native worker did not finish within its boundary",
        )),
        (true, Some(false)) => Err(io::Error::other(
            "Native worker left a running owned subprocess",
        )),
    }
}
#[cfg(target_os = "linux")]
pub fn snapshot() -> io::Result<ResourceSnapshot> {
    fn count(path: &str) -> io::Result<u64> {
        std::fs::read_dir(path)?.try_fold(0, |count, entry| {
            entry?;
            Ok(count + 1)
        })
    }
    Ok(ResourceSnapshot {
        handles: count("/proc/self/fd")?,
        threads: count("/proc/self/task")?,
    })
}
#[cfg(target_os = "macos")]
pub fn snapshot() -> io::Result<ResourceSnapshot> {
    macos::snapshot()
}
#[cfg(windows)]
pub fn snapshot() -> io::Result<ResourceSnapshot> {
    windows::snapshot()
}
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn snapshot() -> io::Result<ResourceSnapshot> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Native resource observation is unsupported",
    ))
}

#[cfg(target_os = "macos")]
mod macos {
    #![allow(unsafe_code)]
    use super::*;
    pub fn snapshot() -> io::Result<ResourceSnapshot> {
        // SAFETY: zero initializes a POD output structure. proc_pidinfo writes
        // only the exact supplied byte length, and the current PID stays owned.
        let mut task: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&task);
        let returned = unsafe {
            libc::proc_pidinfo(
                std::process::id() as i32,
                libc::PROC_PIDTASKINFO,
                0,
                std::ptr::addr_of_mut!(task).cast(),
                size as i32,
            )
        };
        if returned != size as i32 || task.pti_threadnum < 1 {
            return Err(io::Error::other("Native thread observation is incomplete"));
        }
        let handles = std::fs::read_dir("/dev/fd")?.try_fold(0, |count, entry| {
            entry?;
            Ok::<_, io::Error>(count + 1)
        })?;
        Ok(ResourceSnapshot {
            handles,
            threads: task.pti_threadnum as u64,
        })
    }
}
#[cfg(windows)]
mod windows {
    #![allow(unsafe_code)]
    use super::*;
    use std::{
        mem,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_NO_MORE_FILES, GetLastError, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            Threading::{GetCurrentProcess, GetProcessHandleCount},
        },
    };
    pub fn snapshot() -> io::Result<ResourceSnapshot> {
        let mut handles = 0;
        // SAFETY: current-process pseudo handle is borrowed and the output is
        // initialized. The handle count is read before creating our snapshot.
        if unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut handles) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: returned snapshot is owned once and closed by OwnedHandle.
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let owner = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut entry = THREADENTRY32::default();
        entry.dwSize = mem::size_of_val(&entry) as u32;
        let mut threads = 0;
        let mut found = unsafe { Thread32First(owner.as_raw_handle(), &mut entry) };
        while found != 0 {
            if entry.th32OwnerProcessID == std::process::id() {
                threads += 1;
            }
            found = unsafe { Thread32Next(owner.as_raw_handle(), &mut entry) };
        }
        if unsafe { GetLastError() } != ERROR_NO_MORE_FILES || threads == 0 {
            return Err(io::Error::other("Native thread observation is incomplete"));
        }
        Ok(ResourceSnapshot {
            handles: u64::from(handles),
            threads,
        })
    }
}
