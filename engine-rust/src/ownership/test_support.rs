//! Explicit, thread-scoped fault injection for ownership contract tests.
//! This module and every hook call are absent without `test-support`.
use std::{cell::RefCell, io, sync::Arc};

/// Numeric fields from an existing macOS kernel read, never another query.
#[derive(Clone, Debug)]
pub struct MacosBsdSample {
    pub pid: u32,
    pub parent: u32,
    pub group: u32,
    pub status: u32,
    pub flags: u32,
    pub in_exit: bool,
    pub uids: [u32; 3], // pbi_uid, pbi_ruid, pbi_svuid
    pub gids: [u32; 3], // pbi_gid, pbi_rgid, pbi_svgid
    pub exit_status: u32,
    pub birth_seconds: u64,
    pub birth_micros: u64,
}

/// Passive test diagnostics. These observations cannot override an OS result.
#[derive(Clone, Debug)]
pub enum MacosObservation {
    Birth {
        pid: u32,
        token: Option<u64>,
        error: Option<(io::ErrorKind, Option<i32>)>,
    },
    Process {
        requested_pid: u32,
        returned_bytes: i32,
        errno: i32,
        fields_complete: bool,
        fields: MacosBsdSample,
    },
    Group {
        group: i32,
        capacity: usize,
        returned_bytes: i32,
        errno: i32,
        sampled_pids: Vec<i32>,
        omitted_pids: usize,
        incomplete_sample: bool,
    },
}

pub trait Backend: Send + Sync {
    fn macos_observed(&self, _sample: &MacosObservation) {}
    fn before_admit(&self, _id: &str) -> Option<io::Result<()>> {
        None
    }
    fn admitted(&self, _root: &std::path::Path, _id: &str) -> Option<io::Result<()>> {
        None
    }
    fn procfs_root(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn stat(&self, _path: &std::path::Path) -> Option<io::Result<String>> {
        None
    }
    fn before_spawn(&self) -> Option<io::Result<()>> {
        None
    }
    fn spawned(&self, _record: &serde_json::Value) -> Option<io::Result<()>> {
        None
    }
    fn pipe_prepared(&self, _index: usize) -> Option<io::Result<()>> {
        None
    }
    fn pipe_read(&self, _index: usize) -> Option<io::Result<()>> {
        None
    }
    fn before_os_lock(&self, _path: &std::path::Path) -> Option<io::Result<()>> {
        None
    }
    fn adapter_worker_end(&self, _index: usize) {}
    fn job_create(&self) -> Option<io::Result<()>> {
        None
    }
    fn job_assign(&self, _pid: u32) -> Option<io::Result<()>> {
        None
    }
    fn job_query(&self) -> Option<io::Result<()>> {
        None
    }
    fn birth(&self, _pid: u32) -> Option<io::Result<Option<u64>>> {
        None
    }
    fn alive(&self, _pid: u32, _token: Option<u64>) -> Option<io::Result<bool>> {
        None
    }
    fn group_empty(&self, _group: i32) -> Option<io::Result<bool>> {
        None
    }
    fn signal(&self, _target: i32, _force: bool) -> Option<io::Result<()>> {
        None
    }
}
thread_local! {static BACKEND:RefCell<Option<Arc<dyn Backend>>>=RefCell::new(None);}
fn current() -> Option<Arc<dyn Backend>> {
    BACKEND.with(|backend| backend.borrow().clone())
}
#[cfg(target_os = "macos")]
pub(crate) fn macos_observed(sample: impl FnOnce() -> MacosObservation) {
    if let Some(backend) = current() {
        backend.macos_observed(&sample());
    }
}
pub fn with_backend<T>(backend: Arc<dyn Backend>, operation: impl FnOnce() -> T) -> T {
    with_context(Context(Some(backend)), operation)
}
#[derive(Clone)]
pub struct Context(Option<Arc<dyn Backend>>);
pub fn capture() -> Context {
    Context(current())
}
pub fn with_context<T>(context: Context, operation: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<dyn Backend>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            BACKEND.with(|backend| *backend.borrow_mut() = self.0.take());
        }
    }
    let previous = BACKEND.with(|slot| slot.replace(context.0));
    let _restore = Restore(previous);
    operation()
}
pub(crate) fn adapter_worker_end(index: usize) {
    if let Some(backend) = current() {
        backend.adapter_worker_end(index);
    }
}
#[cfg(windows)]
pub(crate) fn job_create() -> Option<io::Result<()>> {
    current()?.job_create()
}
#[cfg(windows)]
pub(crate) fn job_assign(pid: u32) -> Option<io::Result<()>> {
    current()?.job_assign(pid)
}
#[cfg(windows)]
pub(crate) fn job_query() -> Option<io::Result<()>> {
    current()?.job_query()
}
pub(crate) fn birth(pid: u32) -> Option<io::Result<Option<u64>>> {
    current()?.birth(pid)
}
pub(crate) fn before_spawn() -> Option<io::Result<()>> {
    current()?.before_spawn()
}
pub(crate) fn before_admit(id: &str) -> Option<io::Result<()>> {
    current()?.before_admit(id)
}
pub(crate) fn admitted(root: &std::path::Path, id: &str) -> Option<io::Result<()>> {
    current()?.admitted(root, id)
}
pub(crate) fn spawned(record: &serde_json::Value) -> Option<io::Result<()>> {
    current()?.spawned(record)
}
pub(crate) fn pipe_prepared(index: usize) -> Option<io::Result<()>> {
    current()?.pipe_prepared(index)
}
pub(crate) fn pipe_read(index: usize) -> Option<io::Result<()>> {
    current()?.pipe_read(index)
}
pub(crate) fn before_os_lock(path: &std::path::Path) -> Option<io::Result<()>> {
    current()?.before_os_lock(path)
}
pub(crate) fn alive(pid: u32, token: Option<u64>) -> Option<io::Result<bool>> {
    current()?.alive(pid, token)
}
pub(crate) fn group_empty(group: i32) -> Option<io::Result<bool>> {
    current()?.group_empty(group)
}
pub(crate) fn signal(target: i32, force: bool) -> Option<io::Result<()>> {
    current()?.signal(target, force)
}
pub(crate) fn procfs_root() -> Option<std::path::PathBuf> {
    current()?.procfs_root()
}
pub(crate) fn stat(path: &std::path::Path) -> Option<io::Result<String>> {
    current()?.stat(path)
}
