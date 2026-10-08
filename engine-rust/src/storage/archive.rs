use super::{
    atomic::write_bytes,
    locks::DataLock,
    paths::safe_id,
    repo::{ProfileRepo, RepoError},
};
use crate::{conversion::value_text, vault};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

impl ProfileRepo {
    pub fn import_file(
        &self,
        source: &Path,
        now: f64,
    ) -> Result<crate::models::Profile, RepoError> {
        let bytes = fs::read(source)?;
        let record: Value = serde_json::from_slice(&bytes).map_err(|error| {
            RepoError::InvalidRecord(format!("Imported profile JSON cannot be decoded: {error}"))
        })?;
        let profile = crate::models::Profile::from_record(&record).map_err(|error| {
            RepoError::InvalidRecord(format!("Imported profile cannot be modeled: {error}"))
        })?;
        self.save(profile, now)
    }

    pub fn secret_withheld(&self, id: &str) -> Result<bool, RepoError> {
        if safe_id(id).is_err() {
            return Ok(false);
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        self.require_settled()?;
        let bytes = match fs::read(self.path(id, "profiles")?) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let Ok(raw) = serde_json::from_slice::<Value>(&bytes) else {
            return Ok(false);
        };
        Ok(raw["proxy"]["password"].as_str().is_some_and(|secret| {
            vault::is_envelope(secret) && self.readable_secret(Some(secret)).is_none()
        }))
    }

    /// Refuse incomplete credentials before serializing or touching the destination.
    pub fn export(&self, id: &str, destination: &Path) -> Result<Option<PathBuf>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let Some(profile) = self.get(id)? else {
            return Ok(None);
        };
        if self.secret_withheld(id)? {
            return Err(RepoError::VaultLocked(format!(
                "exporting '{}' would drop its proxy password: the vault is locked, so this needs the master password - set NUMERA_PASSWORD or pass --password",
                value_text(&profile.name)
            )));
        }
        let bytes = serde_json::to_vec_pretty(&profile)
            .map_err(|_| RepoError::Message("Profile cannot be serialized for export".into()))?;
        write_bytes(destination, &bytes)?;
        Ok(Some(destination.to_owned()))
    }
}
