use super::{claims, tree::OwnedTree};
use crate::storage::{
    locks::{DataLock, SessionLease},
    paths::safe_id,
};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// Admission owns the non-reentrant lease before any session helper is born.
/// SDK adapters must remain inert until this owner has published their claim.
pub struct Session {
    root: PathBuf,
    id: String,
    host: String,
    _lease: SessionLease,
    tree: Option<OwnedTree>,
    claim: Option<String>,
    admission: Option<super::update_admission::Reservation>,
    browser_units: Option<PathBuf>,
    shared_browser_units: bool,
}

impl Session {
    pub fn admit(root: &Path, id: &str, host: &str) -> io::Result<Self> {
        safe_id(id).map_err(io::Error::other)?;
        if host.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Session host identity is missing",
            ));
        }
        let lock = DataLock::transaction(root)?;
        let _guard = lock.acquire_configured()?;
        let admission = super::update_admission::Reservation::begin(root, "native-session")?;
        let lease = SessionLease::acquire(&SessionLease::path_for(root, id)?)?;
        if claims::state(&claims::read(root, id), host) != claims::State::Free {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "This profile remains owned by a busy or unverifiable session; stop or repair it before opening",
            ));
        }
        #[cfg(feature = "test-support")]
        if let Some(result) = super::test_support::admitted(root, id) {
            result?;
        }
        Ok(Self {
            root: root.into(),
            id: id.into(),
            host: host.into(),
            _lease: lease,
            tree: None,
            claim: None,
            admission: Some(admission),
            browser_units: None,
            shared_browser_units: false,
        })
    }

    #[cfg(unix)]
    pub(crate) fn own_browser_units(&mut self, command: &mut Command) -> io::Result<()> {
        if self.tree.is_some() || self.browser_units.is_some() {
            return Err(io::Error::other("Browser ownership was already prepared"));
        }
        let directory = self.root.join("sessions");
        if std::fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err(io::Error::other("Browser ownership directory is linked"));
        }
        std::fs::create_dir_all(&directory)?;
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(io::Error::other)?;
        let receipt = format!(
            "browser-{}",
            nonce
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let path = crate::storage::paths::id_path(&directory, &receipt, ".units")
            .map_err(io::Error::other)?;
        super::unit_report::initialize(&path)?;
        self.browser_units = Some(path.clone());
        command.env("NUMERA_BROWSER_UNITS", path);
        Ok(())
    }

    /// Start an inert adapter, not an arbitrary browser-capable command. The
    /// adapter's activation handshake is a separate required browser contract.
    pub fn spawn(&mut self, command: &mut Command, what: &str) -> io::Result<&mut OwnedTree> {
        if self.tree.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Session already owns a helper tree",
            ));
        }
        if self.admission.is_none() {
            return Err(io::Error::other("Session admission was already settled"));
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        super::update_admission::assert_open_locked(&self.root)?;
        self.tree = Some(match OwnedTree::spawn_classified(command) {
            Ok(tree) => tree,
            Err(failure) => {
                // Spawn failures may retain an uncertain tree in capture quarantine.
                // A positive no-child/empty proof permits the original owner
                // to release admission; error kinds alone never provide it.
                if !failure.settled
                    && let Some(admission) = &mut self.admission
                {
                    admission.retain();
                }
                return Err(failure.error);
            }
        });
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| io::Error::other("Session tree was not retained"))?;
        match claims::take_with_browser_units(
            &self.root,
            &self.id,
            tree,
            &self.host,
            what,
            self.browser_units.as_deref(),
        ) {
            Ok(claim) => self.claim = Some(claim),
            Err(error) => {
                // Retain the owned tree if cleanup fails; Drop retries rather
                // than losing its handle while an SDK child may still be alive.
                let cleanup = self.finish(Duration::from_secs(5));
                return Err(match cleanup {
                    Ok(()) => error,
                    Err(cleanup) => io::Error::other(format!(
                        "Session claim could not be published: {error}; helper cleanup failed: {cleanup}"
                    )),
                });
            }
        }
        // Detached job supervisors also need the session's browser receipts if
        // this manager dies before it can settle its own SDK worker.
        if let Some(session) = &self.browser_units {
            let inherited = std::env::var_os(super::unit_report::ENV)
                .map(|path| super::unit_report::validate(Path::new(&path)))
                .transpose()?;
            if inherited.as_ref().is_some_and(|parent| {
                session.parent().and_then(Path::parent) == parent.parent().and_then(Path::parent)
            }) {
                let attempt = super::unit_report::begin_spawn()?;
                self.shared_browser_units = attempt.is_some();
                if let Some(record) = self.tree_record() {
                    super::unit_report::record(&record, attempt.as_deref())?;
                }
            }
        }
        self.tree
            .as_mut()
            .ok_or_else(|| io::Error::other("Session tree was not retained"))
    }

    pub(crate) fn preparation_path(&self, id: &str) -> io::Result<PathBuf> {
        if id != self.id || self.tree.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Profile preparation requires its admitted lease before helper birth",
            ));
        }
        Ok(self.root.join("user-data").join(&self.id))
    }

    pub(crate) fn belongs_to(&self, root: &Path, id: &str, host: &str) -> bool {
        self.root == root && self.id == id && self.host == host
    }

    pub fn child_mut(&mut self) -> io::Result<&mut std::process::Child> {
        self.tree
            .as_mut()
            .map(OwnedTree::child_mut)
            .ok_or_else(|| io::Error::other("Session owns no helper tree"))
    }

    pub fn await_ready(
        &mut self,
        path: &Path,
        budget: Duration,
    ) -> Result<(), super::ready::StartupFailed> {
        let Some(tree) = self.tree.as_mut() else {
            return Err(super::ready::StartupFailed {
                kind: "failed".into(),
                message: "Session owns no helper tree".into(),
            });
        };
        let result = super::ready::await_ready(path, budget, || {
            tree.child_mut()
                .try_wait()
                .map(|status| status.map(|status| status.code().unwrap_or(-1)))
        });
        if let Err(failure) = result {
            if let Err(cleanup) = self.finish(Duration::from_secs(5)) {
                return Err(super::ready::StartupFailed {
                    message: format!("{failure}; helper cleanup failed: {cleanup}"),
                    kind: failure.kind,
                });
            }
            return Err(failure);
        }
        Ok(())
    }

    pub fn settled_exit(&mut self) -> io::Result<Option<i32>> {
        let Some(tree) = self.tree.as_mut() else {
            return Ok(None);
        };
        let Some(status) = tree.child_mut().try_wait()? else {
            return Ok(None);
        };
        if !tree.empty()? {
            return Ok(None);
        }
        Ok(Some(status.code().unwrap_or(-1)))
    }

    pub fn tree_record(&self) -> Option<serde_json::Value> {
        self.tree.as_ref().map(|tree| {
            let mut record = tree.record();
            if let Some(path) = &self.browser_units {
                record["browserUnits"] = path.to_string_lossy().into_owned().into();
            }
            record
        })
    }

    pub fn finish(&mut self, budget: Duration) -> io::Result<()> {
        self.finish_with_status(budget).map(|_| ())
    }

    pub fn finish_with_status(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        let deadline = std::time::Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Invalid session cleanup budget"))?;
        let Some(tree) = self.tree.as_mut() else {
            if let Some(path) = &self.browser_units {
                if !super::unit_report::empty(path)? {
                    return Err(io::Error::other("Browser ownership receipt is not empty"));
                }
                if !self.shared_browser_units {
                    std::fs::remove_file(path)?;
                }
                self.browser_units = None;
            }
            if let Some(admission) = &mut self.admission {
                admission.release()?;
                self.admission = None;
            }
            return Ok(None);
        };
        settle_owned(tree, self.browser_units.as_deref(), deadline)?;
        let code = tree
            .child_mut()
            .try_wait()?
            .map(|status| status.code().unwrap_or(-1));
        if let Some(claim) = &self.claim
            && !claims::release_before(&self.root, &self.id, claim, tree, deadline)?
        {
            return Err(io::Error::other(
                "Session tree is empty but its claim could not be released; ownership remains unverified",
            ));
        }
        self.claim = None;
        if let Some(path) = &self.browser_units {
            if !self.shared_browser_units {
                std::fs::remove_file(path)?;
            }
            self.browser_units = None;
        }
        self.tree = None;
        if let Some(admission) = &mut self.admission {
            admission.release()?;
            self.admission = None;
        }
        Ok(code)
    }
}

fn settle_owned(
    tree: &mut OwnedTree,
    browser_units: Option<&Path>,
    deadline: Instant,
) -> io::Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    tree.settle(if browser_units.is_some() {
        remaining / 2
    } else {
        remaining
    })?;
    if let Some(path) = browser_units {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let empty = if remaining.is_zero() {
            super::unit_report::empty_before(path, deadline)?
        } else {
            super::unit_report::settle(path, remaining)?
        };
        if !empty {
            return Err(io::Error::other(
                "Browser groups remain owned or their transfer is incomplete",
            ));
        }
    }
    Ok(())
}

struct Retained {
    root: PathBuf,
    tree: OwnedTree,
    browser_units: Option<PathBuf>,
    admission: Option<super::update_admission::Reservation>,
}
impl Retained {
    fn settle(&mut self, deadline: Instant) -> io::Result<()> {
        settle_owned(&mut self.tree, self.browser_units.as_deref(), deadline)?;
        if let Some(admission) = &mut self.admission {
            // Bound the outer acquisition. Reservation::release reenters this
            // same transaction while checking its exact original work record.
            let lock = DataLock::transaction(&self.root)?;
            let _guard = lock.acquire(deadline.saturating_duration_since(Instant::now()))?;
            admission.release()?;
            self.admission = None;
        }
        // Keep the durable claim and receipt available for explicit claim
        // clearing and independently retained adapter I/O completion proofs.
        Ok(())
    }
}
fn retained() -> &'static std::sync::Mutex<Vec<Retained>> {
    static OWNERS: std::sync::OnceLock<std::sync::Mutex<Vec<Retained>>> =
        std::sync::OnceLock::new();
    OWNERS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}
pub(super) fn retry_retained(deadline: Instant) -> Vec<String> {
    let owners = std::mem::take(
        &mut *retained()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let mut pending = Vec::new();
    let mut failures = Vec::new();
    for mut owner in owners {
        if let Err(error) = owner.settle(deadline) {
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

impl Drop for Session {
    fn drop(&mut self) {
        // A failed cleanup leaves both durable owners, never an invented free slot.
        if self.finish(Duration::from_secs(2)).is_err() {
            // Keep the original kernel owner and every settlement capability.
            // In particular, a retained Windows Job must not be replaced by
            // reopening a PID or a name after its last owned handle is lost.
            if let Some(tree) = self.tree.take() {
                retained()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(Retained {
                        root: self.root.clone(),
                        tree,
                        browser_units: self.browser_units.take(),
                        admission: self.admission.take(),
                    });
            } else if let Some(admission) = &mut self.admission {
                // An uncertain unpublished spawn has no complete owner to
                // transfer here. Its durable admission must remain blocked.
                admission.retain();
            }
        }
    }
}
