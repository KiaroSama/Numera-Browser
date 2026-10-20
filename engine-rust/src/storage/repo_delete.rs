use super::{
    atomic::Batch,
    locks::{DataLock, SessionLease},
    paths::id_path,
    repo::{ProfileRepo, RepoError},
};
use crate::{ownership::claims, validation};
use serde_json::Value;
use std::{fs, io, path::PathBuf};

impl ProfileRepo {
    fn session_path(&self, id: &str) -> Result<PathBuf, RepoError> {
        let path = id_path(&self.root.join("user-data"), id, "")
            .map_err(|error| RepoError::Message(error.to_string()))?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let linked = false;
                if metadata.file_type().is_symlink() || linked {
                    return Err(RepoError::Message(
                        "profile path escapes its data directory or is linked".into(),
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(path)
    }

    pub fn user_data_path(&self, id: &str) -> Result<PathBuf, RepoError> {
        let path = self.session_path(id)?;
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn detach_session(&self, id: &str) -> Result<Option<PathBuf>, RepoError> {
        let session = self.session_path(id)?;
        if !session.try_exists()? {
            return Ok(None);
        }
        let suffix = super::archive_paths::random_hex(4)?;
        let tomb = session.with_file_name(format!("{id}.deleting-{suffix}"));
        if tomb.try_exists()? {
            return Err(RepoError::Message(
                "Session tombstone already exists; nothing was deleted".into(),
            ));
        }
        fs::rename(session, &tomb)?;
        Ok(Some(tomb))
    }

    pub fn delete_session(&self, id: &str, host: &str) -> Result<(), RepoError> {
        let tomb = {
            let _lease = SessionLease::acquire(&SessionLease::path_for(&self.root, id)?).map_err(
                |error| {
                    if error.kind() == io::ErrorKind::WouldBlock {
                        RepoError::SessionBusy(error.to_string())
                    } else {
                        error.into()
                    }
                },
            )?;
            claims::refuse_if_busy(&self.root, id, host)
                .map_err(|error| RepoError::SessionBusy(error.to_string()))?;
            if super::paths::safe_id(id).is_err() {
                return Ok(());
            }
            self.detach_session(id)?
        };
        // Detached bytes no longer belong to the active profile; reclaim outside the lease.
        if let Some(tomb) = tomb {
            let _ = fs::remove_dir_all(&tomb);
            if tomb.try_exists()? {
                return Err(RepoError::Message(format!(
                    "the session for {id} was detached to {} but could not be deleted - remove it by hand once nothing holds it",
                    tomb.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("<unreadable>")
                )));
            }
        }
        Ok(())
    }

    /// Candidate lookup happens before the lease; the target is re-read inside
    /// root ownership before detaching session bytes or deleting either record.
    pub fn delete(&self, reference: &str, host: &str, now: f64) -> Result<bool, RepoError> {
        let Some(candidate) = self.resolve(reference, now)? else {
            return Ok(false);
        };
        let id = candidate.id;
        let tomb: Option<PathBuf>;
        {
            let _lease = SessionLease::acquire(&SessionLease::path_for(&self.root, &id)?).map_err(
                |error| {
                    if error.kind() == io::ErrorKind::WouldBlock {
                        RepoError::SessionBusy(error.to_string())
                    } else {
                        error.into()
                    }
                },
            )?;
            // Keep source domain error text, not an opaque I/O prefix.
            claims::refuse_if_busy(&self.root, &id, host)
                .map_err(|e| RepoError::SessionBusy(e.to_string()))?;
            let lock = DataLock::transaction(&self.root)?;
            let _root = lock.acquire_configured()?;
            let Some(current) = self.resolve(reference, now)? else {
                return Ok(false);
            };
            if current.id != id {
                return Err(RepoError::Message(format!(
                    "{} meant profile {id} when the command started and means {} now - something renamed one of them in between. Nothing was deleted; re-run it against the id you meant.",
                    validation::repr(&Value::String(reference.into())),
                    current.id
                )));
            }
            let paths = [self.path(&id, "dashboard")?, self.path(&id, "profiles")?];
            let session = self.session_path(&id)?;
            tomb = self.detach_session(&id)?;
            let mut batch = Batch::default();
            let result = (|| -> io::Result<()> {
                for path in paths {
                    if path.exists() {
                        batch.remember(&path)?;
                        fs::remove_file(path)?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                let mut failures = batch.rollback();
                if let Some(tomb) = &tomb
                    && let Err(restore) = fs::rename(tomb, &session)
                {
                    failures.push((tomb.clone(), restore));
                }
                if !failures.is_empty() {
                    return Err(RepoError::Message(format!(
                        "Profile delete failed: {error}; {} path(s) could not be restored",
                        failures.len()
                    )));
                }
                return Err(error.into());
            }
        }
        // Logical deletion has committed. Expensive browser-state reclamation
        // must not block every unrelated writer under the root transaction.
        let _ = claims::clear(&self.root, &id, host)?;
        if let Some(tomb) = tomb
            && let Err(error) = fs::remove_dir_all(&tomb)
        {
            eprintln!(
                "WARNING: {id} is deleted, but its session is still on disk as {}; remove it once nothing holds it ({error})",
                tomb.display()
            );
        }
        Ok(true)
    }
}
