use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions, TryLockError},
    io,
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Condvar, Mutex, OnceLock},
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    owner: Option<ThreadId>,
    depth: usize,
    file: Option<File>,
}
struct Shared {
    state: Mutex<State>,
    released: Condvar,
}
static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Shared>>>> = OnceLock::new();

pub struct DataLock {
    path: PathBuf,
    shared: Arc<Shared>,
}
/// A guard must never move to another thread; ownership is the acquiring thread.
pub struct LockGuard<'a> {
    lock: &'a DataLock,
    owner: ThreadId,
    not_send: PhantomData<Rc<()>>,
}

fn poisoned() -> io::Error {
    io::Error::other("Data lock state poisoned; ownership is unverified")
}

/// Match Python's SHA256 pathname domain; never unlink a live lock file.
pub fn lock_name(key: &str, name: &str) -> String {
    let digest = Sha256::digest(key.as_bytes());
    let hash = digest
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    if safe.is_empty() {
        safe = "lock".into();
    }
    format!("{safe}.{hash}.lock")
}

fn open_lock(path: &Path) -> io::Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Lock path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub(crate) fn resolved(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let canonical = match absolute.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = absolute
                .parent()
                .ok_or_else(|| io::Error::other("Lock resource has no parent"))?;
            let name = absolute
                .file_name()
                .ok_or_else(|| io::Error::other("Lock resource has no name"))?;
            resolved(parent)?.join(name)
        }
        Err(error) => return Err(error),
    };
    #[cfg(windows)]
    {
        // Rust canonicalize adds a verbatim prefix; Python Path.resolve does not.
        // Hash the same spelling, including ordinary UNC paths, across runtimes.
        let text = canonical
            .to_str()
            .ok_or_else(|| io::Error::other("Lock path is not Unicode"))?;
        if let Some(tail) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{tail}")));
        }
        if let Some(tail) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(tail));
        }
    }
    Ok(canonical)
}

impl DataLock {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn owned_by_current_thread(&self) -> io::Result<bool> {
        let state = self.shared.state.lock().map_err(|_| poisoned())?;
        Ok(state.depth > 0 && state.owner == Some(thread::current().id()))
    }

    pub fn transaction(root: &Path) -> io::Result<Self> {
        let root = resolved(root)?;
        let key = root
            .to_str()
            .ok_or_else(|| io::Error::other("Lock root is not Unicode"))?;
        Self::new(root.join(".locks").join(lock_name(key, "transaction")))
    }

    pub fn for_file(path: &Path) -> io::Result<Self> {
        let path = resolved(path)?;
        let key = path
            .to_str()
            .ok_or_else(|| io::Error::other("Lock file is not Unicode"))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::other("Lock file has no Unicode name"))?;
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Lock file has no parent"))?;
        Self::new(parent.join(".locks").join(lock_name(key, name)))
    }

    pub fn new(path: PathBuf) -> io::Result<Self> {
        // The caller supplies the exact canonical source-compatible path.
        if !path.is_absolute() {
            return Err(io::Error::other("Data lock path must be absolute"));
        }
        let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
        let shared = locks
            .lock()
            .map_err(|_| poisoned())?
            .entry(path.clone())
            .or_insert_with(|| {
                Arc::new(Shared {
                    state: Mutex::new(State::default()),
                    released: Condvar::new(),
                })
            })
            .clone();
        Ok(Self { path, shared })
    }

    pub fn acquire(&self, limit: Duration) -> io::Result<LockGuard<'_>> {
        let deadline = Instant::now()
            .checked_add(limit)
            .ok_or_else(|| io::Error::other("Invalid lock timeout"))?;
        let owner = thread::current().id();
        let mut state = self.shared.state.lock().map_err(|_| poisoned())?;
        while state.owner.is_some_and(|id| id != owner) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(self.timed_out(limit));
            }
            state = self
                .shared
                .released
                .wait_timeout(state, remaining)
                .map_err(|_| poisoned())?
                .0;
        }
        if state.depth == 0 {
            // Reserve the local owner, then release the state mutex during OS
            // contention. Otherwise other threads block on an unbounded mutex
            // acquisition instead of sharing the finite condition/deadline.
            state.owner = Some(owner);
            drop(state);
            let acquired = (|| {
                let file = open_lock(&self.path)?;
                loop {
                    match file.try_lock() {
                        Ok(()) => return Ok(file),
                        Err(TryLockError::Error(error)) => return Err(error),
                        Err(TryLockError::WouldBlock) => {
                            let remaining = deadline.saturating_duration_since(Instant::now());
                            if remaining.is_zero() {
                                return Err(self.timed_out(limit));
                            }
                            thread::sleep(remaining.min(Duration::from_millis(50)));
                        }
                    }
                }
            })();
            state = self.shared.state.lock().map_err(|_| poisoned())?;
            match acquired {
                Ok(file) => state.file = Some(file),
                Err(error) => {
                    state.owner = None;
                    self.shared.released.notify_all();
                    return Err(error);
                }
            }
        }
        state.depth += 1;
        Ok(LockGuard {
            lock: self,
            owner,
            not_send: PhantomData,
        })
    }

    fn timed_out(&self, limit: Duration) -> io::Error {
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "another thread or process has held {} for more than {}s. Wait for the owner to finish or investigate the blocked operation; NUMERA_LOCK_TIMEOUT controls this wait. Keep the lock file in place while any instance may use it.",
                self.path.display(),
                limit.as_secs_f64()
            ),
        )
    }
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lock.shared.state.lock() {
            if state.owner != Some(self.owner) || state.depth == 0 {
                return;
            }
            state.depth -= 1;
            if state.depth == 0 {
                // Closing releases native locks even when explicit unlock fails.
                if let Some(file) = state.file.take() {
                    let _ = file.unlock();
                }
                state.owner = None;
                self.lock.shared.released.notify_all();
            }
        }
    }
}

pub struct SessionLease {
    _file: File,
}
impl SessionLease {
    pub fn path_for(root: &Path, profile_id: &str) -> io::Result<PathBuf> {
        let root = resolved(root)?;
        let key = format!(
            "{}\0{profile_id}",
            root.to_str()
                .ok_or_else(|| io::Error::other("Session root is not Unicode"))?
        );
        Ok(root
            .join(".locks")
            .join(lock_name(&key, &format!("session-{profile_id}"))))
    }

    pub fn acquire(path: &Path) -> io::Result<Self> {
        let file = open_lock(path)?;
        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => io::Error::new(io::ErrorKind::WouldBlock,"another process has this profile's browser session open. Close it there, or wait for it to finish."),
            TryLockError::Error(error) => error,
        })?;
        Ok(Self { _file: file })
    }
}
