use super::{
    atomic::write_json,
    locks::DataLock,
    repo::{ProfileRepo, RepoError},
};
use crate::vault::{
    self,
    policy::{VaultState, write_secret},
};
use serde_json::{Map, Value};
use std::{fs, io, path::PathBuf};

pub struct ProxyPool {
    root: PathBuf,
    repo: ProfileRepo,
}
impl ProxyPool {
    pub fn new(root: PathBuf, password: Option<String>) -> Self {
        Self {
            repo: ProfileRepo::new(root.clone(), password),
            root,
        }
    }
    fn path(&self) -> PathBuf {
        self.root.join("proxies.json")
    }
    fn stored(&self) -> Result<Vec<Value>, RepoError> {
        let bytes = match fs::read(self.path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Stored proxy pool is not UTF-8; nothing was written",
            )
        })?;
        let Some(rows) = serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.as_array().cloned())
        else {
            return Ok(Vec::new());
        };
        if rows.iter().any(|value| !value.is_object()) {
            return Err(RepoError::InvalidRecord(
                "Stored pool entry is not an object; nothing was written".into(),
            ));
        }
        Ok(rows)
    }
    fn decrypted(&self, mut row: Value) -> Value {
        if let Some(secret) = row["pass"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
        {
            row["pass"] = self
                .repo
                .readable_secret(Some(&secret))
                .map_or(Value::Null, Value::String);
        }
        row
    }
    pub fn list(&self) -> Result<Vec<Value>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        Ok(self
            .stored()?
            .into_iter()
            .map(|row| self.decrypted(row))
            .collect())
    }
    pub fn get(&self, id: &str) -> Result<Option<Value>, RepoError> {
        Ok(self.list()?.into_iter().find(|row| row["id"] == id))
    }
    pub fn secret_withheld(&self, id: &str) -> Result<bool, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        let row = self.stored()?.into_iter().find(|row| row["id"] == id);
        Ok(row.is_some_and(|row| {
            row["pass"].as_str().is_some_and(|secret| {
                vault::is_envelope(secret) && self.repo.readable_secret(Some(secret)).is_none()
            })
        }))
    }
    fn save(&self, rows: &[Value], stored: &[Value]) -> Result<(), RepoError> {
        let config = Value::Object(self.repo.config.read());
        let descriptor = &config["vault"];
        let password = self.repo.password.as_deref().unwrap_or("");
        let salt = descriptor["salt"].as_str().unwrap_or("");
        let state = if descriptor.is_null() {
            VaultState::Disabled
        } else if !password.is_empty()
            && vault::check_password(
                password,
                salt,
                descriptor["verifier"].as_str().unwrap_or(""),
            )
        {
            VaultState::Unlocked
        } else {
            VaultState::Locked
        };
        // Seal every participating entry before replacing the file. A single
        // locked replacement refuses the whole mutation without partial writes.
        let sealed = rows
            .iter()
            .map(|row| {
                if !row["pass"].is_null() && !row["pass"].is_string() {
                    return Err(RepoError::Message("a proxy password must be text".into()));
                }
                let previous = stored.iter().find(|old| old["id"] == row["id"]);
                let secret = write_secret(
                    state,
                    row["pass"].as_str(),
                    previous.and_then(|old| old["pass"].as_str()),
                    |value| vault::encrypt(password, salt, value),
                )
                .map_err(|error| {
                    if state == VaultState::Locked {
                        RepoError::VaultLocked(error.to_string())
                    } else {
                        RepoError::Message(error.to_string())
                    }
                })?;
                let mut row = row.clone();
                if row["pass"].as_str() != secret.as_deref() {
                    row["pass"] = secret.map_or(Value::Null, Value::String);
                }
                Ok(row)
            })
            .collect::<Result<Vec<_>, RepoError>>()?;
        write_json(&self.path(), &Value::Array(sealed))?;
        Ok(())
    }
    pub fn add_many(&self, proxies: Vec<Value>) -> Result<Vec<Value>, RepoError> {
        let root = DataLock::transaction(&self.root)?;
        let _root = root.acquire_configured()?;
        self.repo.require_settled()?;
        let file = DataLock::for_file(&self.path())?;
        let _file = file.acquire_configured()?;
        let stored = self.stored()?;
        let mut rows: Vec<_> = stored
            .iter()
            .cloned()
            .map(|row| self.decrypted(row))
            .collect();
        let mut seen = stored
            .iter()
            .filter(|row| {
                !row["pass"].as_str().is_some_and(|secret| {
                    vault::is_envelope(secret) && self.repo.readable_secret(Some(secret)).is_none()
                })
            })
            .map(|row| crate::pool::identity(&self.decrypted(row.clone())))
            .collect::<Vec<_>>();
        let mut added = Vec::new();
        for mut proxy in proxies {
            if !proxy.is_object() {
                return Err(RepoError::Message(
                    "Pool import entry is not an object".into(),
                ));
            }
            let identity = crate::pool::identity(&proxy);
            if seen.contains(&identity) {
                continue;
            }
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes).map_err(|error| {
                RepoError::Message(format!("Pool ID randomness unavailable: {error}"))
            })?;
            bytes[6] = (bytes[6] & 0x0f) | 0x40;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            let id = bytes
                .iter()
                .take(6)
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            if rows.iter().any(|row| row["id"] == id) {
                return Err(RepoError::Message(
                    "Generated pool ID collided; nothing was written".into(),
                ));
            }
            proxy["id"] = id.into();
            proxy["status"] = "untested".into();
            seen.push(identity);
            rows.push(proxy.clone());
            added.push(proxy);
        }
        self.save(&rows, &stored)?;
        Ok(added)
    }

    pub fn update(
        &self,
        id: &str,
        changes: Map<String, Value>,
    ) -> Result<Option<Value>, RepoError> {
        let mut patches = Map::new();
        patches.insert(id.into(), Value::Object(changes));
        Ok(self.update_records(patches, true)?.into_iter().next())
    }
    pub fn update_many(&self, patches: Map<String, Value>) -> Result<Vec<Value>, RepoError> {
        self.update_records(patches, false)
    }
    fn update_records(
        &self,
        patches: Map<String, Value>,
        keep_empty: bool,
    ) -> Result<Vec<Value>, RepoError> {
        if patches.is_empty() {
            return Ok(Vec::new());
        }
        let root = DataLock::transaction(&self.root)?;
        let _root = root.acquire_configured()?;
        self.repo.require_settled()?;
        let file = DataLock::for_file(&self.path())?;
        let _file = file.acquire_configured()?;
        let stored = self.stored()?;
        let mut rows: Vec<_> = stored
            .iter()
            .cloned()
            .map(|row| self.decrypted(row))
            .collect();
        let mut touched = Vec::new();
        for row in &mut rows {
            if let Some(id) = row["id"].as_str()
                && let Some(patch) = patches
                    .get(id)
                    .filter(|patch| keep_empty || crate::conversion::truth(patch))
            {
                let patch = patch
                    .as_object()
                    .ok_or_else(|| RepoError::Message("Proxy patch is not an object".into()))?;
                row.as_object_mut()
                    .ok_or_else(|| RepoError::InvalidRecord("Pool entry is not an object".into()))?
                    .extend(patch.clone());
                touched.push(row.clone());
            }
        }
        if !touched.is_empty() {
            self.save(&rows, &stored)?;
        }
        Ok(touched)
    }
    pub fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let root = DataLock::transaction(&self.root)?;
        let _root = root.acquire_configured()?;
        self.repo.require_settled()?;
        let file = DataLock::for_file(&self.path())?;
        let _file = file.acquire_configured()?;
        let stored = self.stored()?;
        let kept: Vec<_> = stored
            .iter()
            .filter(|row| row["id"] != id)
            .cloned()
            .map(|row| self.decrypted(row))
            .collect();
        if kept.len() == stored.len() {
            return Ok(false);
        }
        self.save(&kept, &stored)?;
        Ok(true)
    }
}
