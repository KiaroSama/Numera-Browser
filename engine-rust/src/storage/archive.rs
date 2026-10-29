use super::{
    atomic::write_json,
    locks::DataLock,
    paths::safe_id,
    repo::{ProfileRepo, RepoError},
};
use crate::{conversion::value_text, vault};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
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
        let _guard = lock.acquire_configured()?;
        self.require_settled()?;
        let bytes = match fs::read(self.path(id, "profiles")?) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Stored profile is not valid UTF-8",
            )
        })?;
        let Ok(raw) = serde_json::from_str::<Value>(text) else {
            return Ok(false);
        };
        let Some(secret) = raw["proxy"]["password"]
            .as_str()
            .filter(|secret| vault::is_envelope(secret))
        else {
            return Ok(false);
        };
        Ok(self.readable_secret(Some(secret))?.is_none())
    }

    /// Refuse incomplete credentials before serializing or touching the destination.
    pub fn export(&self, id: &str, destination: &Path) -> Result<Option<PathBuf>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        let Some(profile) = self.get(id)? else {
            return Ok(None);
        };
        if self.secret_withheld(id)? {
            return Err(RepoError::VaultLocked(format!(
                "exporting '{}' would drop its proxy password: the vault is locked, so this needs the master password - set NUMERA_PASSWORD or pass --password",
                value_text(&profile.name)
            )));
        }
        let mut record = serde_json::to_value(&profile)
            .map_err(|_| RepoError::Message("Profile cannot be serialized for export".into()))?;
        let stored = Self::raw(&self.path(id, "profiles")?)?.unwrap_or(Value::Null);
        // The typed read view uses f64; exporting must not round stored integer times.
        for field in ["created_at", "updated_at"] {
            if let Some(value @ Value::Number(number)) = stored.get(field)
                && !number.to_string().contains(['.', 'e', 'E'])
            {
                record[field] = value.clone();
            }
        }
        write_json(destination, &record)?;
        Ok(Some(destination.to_owned()))
    }
}
