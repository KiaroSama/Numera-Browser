use super::{
    VaultError,
    completion::{Site, sites},
    decrypt, encrypt, is_envelope,
};
use crate::storage::atomic::Batch;
use serde_json::Value;
use std::{error::Error, fmt, fs, io, path::PathBuf};

#[derive(Debug)]
pub enum MigrationError {
    Io(io::Error),
    Unreadable(PathBuf),
    Crypto(VaultError),
}
impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Vault file operation failed: {e}"),
            Self::Unreadable(path) => write!(
                f,
                "{} is unreadable, so its secrets cannot be migrated",
                path.display()
            ),
            Self::Crypto(e) => write!(f, "Vault secret could not be migrated: {e}"),
        }
    }
}
impl Error for MigrationError {}
impl From<io::Error> for MigrationError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<VaultError> for MigrationError {
    fn from(e: VaultError) -> Self {
        Self::Crypto(e)
    }
}

pub struct Migration {
    root: PathBuf,
    batch: Batch,
}
impl Migration {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            batch: Batch::default(),
        }
    }
    pub fn rollback(&mut self) -> Vec<(PathBuf, io::Error)> {
        self.batch.rollback()
    }
    pub fn remember_config(&mut self) -> io::Result<()> {
        self.batch.remember(&self.root.join("config.json"))
    }
    pub fn encrypt_all(&mut self, password: &str, salt: &str) -> Result<usize, MigrationError> {
        self.transform(|value| {
            if is_envelope(value) {
                Ok(None)
            } else {
                encrypt(password, salt, value).map(Some)
            }
        })
    }
    pub fn rekey_all(
        &mut self,
        old_password: &str,
        old_salt: &str,
        new_password: &str,
        new_salt: &str,
        resume: bool,
    ) -> Result<usize, MigrationError> {
        self.transform(|value| {
            if !is_envelope(value) || (resume && decrypt(new_password, new_salt, value).is_ok()) {
                return Ok(None);
            }
            let plain = decrypt(old_password, old_salt, value)?;
            encrypt(new_password, new_salt, &plain).map(Some)
        })
    }
    fn transform(
        &mut self,
        change: impl Fn(&str) -> Result<Option<String>, VaultError>,
    ) -> Result<usize, MigrationError> {
        let mut total = 0;
        for site in sites(&self.root)? {
            let mut data: Value = serde_json::from_slice(&fs::read(&site.path)?)
                .map_err(|_| MigrationError::Unreadable(site.path.clone()))?;
            let mut count = 0;
            Self::visit(&mut data, &site, |holder| {
                let Some(value) = holder
                    .get(site.field)
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty())
                else {
                    return Ok(());
                };
                if let Some(updated) = change(value)? {
                    holder[site.field] = updated.into();
                    count += 1;
                }
                Ok(())
            })?;
            if count > 0 {
                self.batch.write(&site.path, &data)?;
                total += count;
            }
        }
        Ok(total)
    }
    fn visit(
        data: &mut Value,
        site: &Site,
        mut change: impl FnMut(&mut Value) -> Result<(), VaultError>,
    ) -> Result<(), VaultError> {
        // This primitive matches original holders(): structural completeness is
        // separately proved before committing a descriptor, never inferred here.
        if site.many && data.is_array() {
            if let Some(rows) = data.as_array_mut() {
                for row in rows {
                    if row.is_object() {
                        change(row)?;
                    }
                }
            }
        } else if data.is_object() {
            let holder = match site.node {
                Some(key) => &mut data[key],
                None => data,
            };
            if holder.is_object() {
                change(holder)?;
            }
        }
        Ok(())
    }
}
