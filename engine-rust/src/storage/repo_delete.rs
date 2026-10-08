use super::{
    atomic::Batch,
    locks::{DataLock, SessionLease},
    paths::id_path,
    repo::{ProfileRepo, RepoError},
};
use crate::{ownership::claims, validation};
use serde_json::Value;
use std::{fs, io, path::PathBuf, time::Duration};

impl ProfileRepo {
    /// Candidate lookup happens before the lease; the target is re-read inside
    /// root ownership before detaching session bytes or deleting either record.
    pub fn delete(&self, reference: &str, host: &str, now: f64) -> Result<bool, RepoError> {
        let Some(candidate) = self.resolve(reference, now)? else {
            return Ok(false);
        };
        let id = candidate.id;
        let tomb: Option<PathBuf>;
        {
            let _lease = SessionLease::acquire(&SessionLease::path_for(&self.root, &id)?)?;
            // Keep source domain error text, not an opaque I/O prefix.
            claims::refuse_if_busy(&self.root, &id, host)
                .map_err(|e| RepoError::Message(e.to_string()))?;
            let lock = DataLock::transaction(&self.root)?;
            let _root = lock.acquire(Duration::from_secs(30))?;
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
            let session = id_path(&self.root.join("user-data"), &id, "")
                .map_err(|e| RepoError::Message(e.to_string()))?;
            tomb = if session.exists() {
                let mut nonce = [0u8; 4];
                getrandom::fill(&mut nonce).map_err(|e| {
                    RepoError::Message(format!("Session tombstone randomness unavailable: {e}"))
                })?;
                let suffix = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
                let path = session.with_file_name(format!("{id}.deleting-{suffix}"));
                if path.exists() {
                    return Err(RepoError::Message(
                        "Session tombstone already exists; nothing was deleted".into(),
                    ));
                }
                fs::rename(&session, &path)?;
                Some(path)
            } else {
                None
            };
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
