use super::{claims, tree::OwnedTree};
use crate::storage::{
    locks::{DataLock, SessionLease},
    paths::safe_id,
};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
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
        let lease = SessionLease::acquire(&SessionLease::path_for(root, id)?)?;
        let lock = DataLock::transaction(root)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        if claims::state(&claims::read(root, id), host) != claims::State::Free {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "This profile remains owned by a busy or unverifiable session; stop or repair it before opening",
            ));
        }
        Ok(Self {
            root: root.into(),
            id: id.into(),
            host: host.into(),
            _lease: lease,
            tree: None,
            claim: None,
        })
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
        self.tree = Some(OwnedTree::spawn(command)?);
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| io::Error::other("Session tree was not retained"))?;
        match claims::take(&self.root, &self.id, tree, &self.host, what) {
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
        self.tree.as_ref().map(OwnedTree::record)
    }

    pub fn finish(&mut self, budget: Duration) -> io::Result<()> {
        self.finish_with_status(budget).map(|_| ())
    }

    pub fn finish_with_status(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        let Some(tree) = self.tree.as_mut() else {
            return Ok(None);
        };
        tree.settle(budget)?;
        let code = tree
            .child_mut()
            .try_wait()?
            .map(|status| status.code().unwrap_or(-1));
        if let Some(claim) = &self.claim
            && !claims::release(&self.root, &self.id, claim, tree)?
        {
            return Err(io::Error::other(
                "Session tree is empty but its claim could not be released; ownership remains unverified",
            ));
        }
        self.claim = None;
        self.tree = None;
        Ok(code)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // A failed cleanup leaves the durable claim, never an invented free slot.
        let _ = self.finish(Duration::from_secs(2));
    }
}
