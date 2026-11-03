use super::{
    archive_paths::random_hex,
    atomic::Batch,
    config::ConfigStore,
    folders::Folders,
    locks::DataLock,
    paths::safe_id,
    repo::{ProfileRepo, RepoError},
};
use crate::{
    conversion::{self, truth, value_text},
    validation,
    vault::{
        self,
        policy::{VaultState, write_secret},
    },
};
use serde_json::{Map, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn read_text(path: &Path) -> io::Result<String> {
    fs::read_to_string(path).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Stored dashboard file {} is not valid UTF-8",
                    path.display()
                ),
            )
        } else {
            error
        }
    })
}

pub struct DashboardStore {
    pub(crate) root: PathBuf,
    pub(crate) repo: ProfileRepo,
}
impl DashboardStore {
    pub fn new(root: PathBuf, password: Option<String>) -> Self {
        Self {
            repo: ProfileRepo::new(root.clone(), password),
            root,
        }
    }
    pub fn get_meta(&self) -> Result<Value, RepoError> {
        let text = match read_text(&self.root.join("dashboard-config.json")) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(serde_json::json!({}));
            }
            Err(error) => return Err(error.into()),
        };
        Ok(serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({})))
    }
    pub fn set_meta(&self, metadata: Value) -> Result<Value, RepoError> {
        let path = self.root.join("dashboard-config.json");
        let lock = DataLock::for_file(&path)?;
        let _guard = lock.acquire_configured()?;
        super::atomic::write_json(&path, &metadata)?;
        Ok(metadata)
    }
    pub fn delete(&self, id: &str, host: &str, now: f64) -> Result<bool, RepoError> {
        self.repo.delete(id, host, now)
    }
    fn decrypted(&self, mut record: Value) -> Result<Value, RepoError> {
        if record["proxy"].is_object() {
            let value = match record["proxy"]["pass"].as_str() {
                Some(secret) => self.repo.readable_secret(Some(secret))?,
                None => Some(String::new()),
            };
            record["proxy"]["pass"] = value.map_or(Value::Null, Value::String);
        }
        Ok(record)
    }
    pub fn get(&self, id: &str) -> Result<Option<Value>, RepoError> {
        if safe_id(id).is_err() {
            return Ok(None);
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        let text = match read_text(&self.repo.path(id, "dashboard")?) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let record = serde_json::from_str(&text).map_err(|_| {
            RepoError::InvalidRecord("Stored dashboard profile cannot be decoded".into())
        })?;
        self.decrypted(record).map(Some)
    }
    pub fn raw(&self, id: &str) -> Result<Option<Value>, RepoError> {
        if safe_id(id).is_err() {
            return Ok(None);
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        Ok(ProfileRepo::raw(&self.repo.path(id, "dashboard")?)?)
    }
    pub fn list(&self) -> Result<Vec<Value>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        let entries = match fs::read_dir(self.root.join("dashboard")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut paths = entries
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        let mut records = Vec::new();
        for path in paths {
            if super::paths::json_filename(&path)
                && let Ok(record) = serde_json::from_str::<Value>(&read_text(&path)?)
            {
                records.push(self.decrypted(record)?);
            }
        }
        records.sort_by(|left, right| {
            left["_created"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&right["_created"].as_f64().unwrap_or(0.0))
        });
        Ok(records)
    }
    fn prepare(
        &self,
        mut record: Value,
        intent: &str,
        now: f64,
        previous: Option<&(Value, Value)>,
    ) -> Result<(Value, Value), RepoError> {
        let id = record["id"]
            .as_str()
            .ok_or_else(|| RepoError::Message("Dashboard id must be text".into()))?
            .to_owned();
        let old = match previous {
            Some((_, engine)) => engine.clone(),
            None => ProfileRepo::raw(&self.repo.path(&id, "profiles")?)?.unwrap_or(Value::Null),
        };
        let stored = match previous {
            Some((dashboard, _)) => dashboard.clone(),
            None => ProfileRepo::raw(&self.repo.path(&id, "dashboard")?)?.unwrap_or(Value::Null),
        };
        let engine_secret = old["proxy"]["password"].as_str();
        let dash_secret = stored["proxy"]["pass"].as_str();
        if intent == "preserve" {
            self.repo
                .refuse_unresolved_secret(&id, engine_secret, dash_secret)?;
        }
        if intent == "remove"
            && old["proxy"]
                .as_object()
                .is_some_and(|proxy| !proxy.is_empty())
            && !truth(&stored["proxy"])
        {
            return Err(ProfileRepo::diverged(&id, "proxy"));
        }
        let descriptor = if intent == "remove"
            || record["proxy"].is_object()
                && intent != "preserve"
                && (intent == "clear" || record["proxy"]["pass"].is_string())
        {
            self.repo.vault()?.unwrap_or(Value::Null)
        } else {
            Value::Null
        };
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
        let seal = |value: &str| vault::encrypt(password, salt, value);
        if intent == "remove" {
            for secret in [engine_secret, dash_secret] {
                write_secret(state, Some(""), secret, seal)
                    .map_err(|error| RepoError::VaultLocked(error.to_string()))?;
            }
        }
        if record["proxy"].is_object() {
            let asked = match intent {
                "preserve" => None,
                "clear" => Some(""),
                "replace" => record["proxy"]["pass"].as_str(),
                _ => return Err(RepoError::Message("Unknown dashboard secret intent".into())),
            };
            let kept = write_secret(state, asked, dash_secret, seal)
                .map_err(|error| RepoError::VaultLocked(error.to_string()))?;
            if record["proxy"]["pass"].as_str() != kept.as_deref() {
                record["proxy"]["pass"] = kept.map_or(Value::Null, Value::String);
            }
        }
        if let Some(folder) = record["folder"]
            .as_str()
            .filter(|folder| !folder.is_empty())
        {
            record["folder"] = Folders::new(self.root.clone()).resolve_name(folder)?.into();
        }
        let problems = validation::flat_number_problems(&record)
            .map_err(|_| RepoError::Message("Field policy is unreadable".into()))?;
        if !problems.is_empty() {
            return Err(RepoError::UnconvertibleProfile(format!(
                "profile {id} cannot be converted for the engine, so neither copy was written: {}",
                problems.join("; ")
            )));
        }
        let profile=conversion::to_engine(&record,now).map_err(|error|RepoError::UnconvertibleProfile(format!("profile {id} cannot be converted for the engine, so neither copy was written: {error}")))?;
        // Mirroring reaches the engine secret gate even when no credential was supplied.
        self.repo.vault()?;
        let mut twin = serde_json::to_value(profile)
            .map_err(|_| RepoError::Message("Engine twin cannot be serialized".into()))?;
        if twin["proxy"].is_object() {
            let secret = if intent == "preserve" {
                engine_secret.map(str::to_owned)
            } else {
                write_secret(state, twin["proxy"]["password"].as_str(), None, seal)
                    .map_err(|error| RepoError::VaultLocked(error.to_string()))?
            };
            twin["proxy"]["password"] = secret.map_or(Value::Null, Value::String);
        }
        Ok((record, twin))
    }
    pub fn save(
        &self,
        mut record: Value,
        changes: Map<String, Value>,
        expect_rev: Option<&Value>,
        intent: Option<&str>,
        now: f64,
    ) -> Result<Value, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        let object = record.as_object_mut().ok_or_else(|| {
            RepoError::InvalidRecord("Dashboard profile must be an object".into())
        })?;
        if object.get("id").is_none_or(|id| !truth(id)) {
            object.insert("id".into(), random_hex(6)?.into());
        }
        let id = record["id"]
            .as_str()
            .ok_or_else(|| RepoError::InvalidRecord("Dashboard id must be text".into()))?
            .to_owned();
        let path = self.repo.path(&id, "dashboard")?;
        let file = DataLock::for_file(&path)?;
        let _file = file.acquire_configured()?;
        let held = self.repo.held_names()?;
        if let Some(name) = record["name"].as_str().filter(|name| !name.is_empty()) {
            let own = held
                .get(&id)
                .is_some_and(|names| names.iter().any(|held| held == name));
            if !own
                && held
                    .iter()
                    .any(|(other, names)| other != &id && names.iter().any(|held| held == name))
            {
                return Err(RepoError::NameTaken(format!(
                    "a profile called {} already exists",
                    validation::repr(&record["name"])
                )));
            }
        }
        record
            .as_object_mut()
            .ok_or_else(|| RepoError::InvalidRecord("Dashboard profile lost object shape".into()))?
            .entry("_created")
            .or_insert(now.into());
        let stored = ProfileRepo::raw(&path)?.unwrap_or(Value::Null);
        if let Some(expected) = expect_rev
            && let Some(current) = stored.get("_rev").filter(|value| !value.is_null())
            && value_text(current) != value_text(expected)
        {
            return Err(RepoError::StaleProfile(format!(
                "profile {id} changed since you loaded it (revision {}, you sent {})",
                value_text(current),
                value_text(expected)
            )));
        }
        let intent = intent.unwrap_or_else(|| conversion::secret_intent(&record));
        let (mut record, twin) = self.prepare(record, intent, now, None)?;
        record["_rev"] = super::dashboard_revision::next(&stored["_rev"])?;
        self.commit(vec![(record, twin)], changes)?
            .pop()
            .ok_or_else(|| RepoError::Message("Dashboard write returned no record".into()))
    }
    fn commit(
        &self,
        prepared: Vec<(Value, Value)>,
        changes: Map<String, Value>,
    ) -> Result<Vec<Value>, RepoError> {
        let mut paths = Vec::new();
        for (record, _) in &prepared {
            let id = record["id"]
                .as_str()
                .ok_or_else(|| RepoError::InvalidRecord("Dashboard id must be text".into()))?;
            paths.extend([
                self.repo.path(id, "dashboard")?,
                self.repo.path(id, "profiles")?,
            ]);
        }
        let config_path = self.root.join("config.json");
        if !changes.is_empty() {
            paths.push(config_path.clone());
        }
        paths.sort();
        paths.dedup();
        let locks = paths
            .iter()
            .map(|path| DataLock::for_file(path))
            .collect::<Result<Vec<_>, _>>()?;
        let _guards = locks
            .iter()
            .map(|lock| lock.acquire_configured())
            .collect::<Result<Vec<_>, _>>()?;
        let mut batch = Batch::default();
        let result = (|| -> Result<(), RepoError> {
            for (record, twin) in &prepared {
                let id = record["id"]
                    .as_str()
                    .ok_or_else(|| RepoError::InvalidRecord("Dashboard id must be text".into()))?;
                batch.write(&self.repo.path(id, "dashboard")?, record)?;
                batch.write(&self.repo.path(id, "profiles")?, twin)?;
            }
            if !changes.is_empty() {
                let mut config = ConfigStore::new(self.root.clone()).set_many(Map::new())?;
                config.extend(changes);
                batch.write(&config_path, &Value::Object(config))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let failures = batch.rollback();
            if !failures.is_empty() {
                return Err(RepoError::Message(format!(
                    "Dashboard write failed: {error}; {} file(s) could not be restored",
                    failures.len()
                )));
            }
            return Err(error);
        }
        Ok(prepared.into_iter().map(|(record, _)| record).collect())
    }
    pub fn save_many(
        &self,
        records: Vec<Value>,
        changes: Map<String, Value>,
        intent: Option<&str>,
        now: f64,
    ) -> Result<Vec<Value>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        let mut held = self.repo.held_names()?;
        let mut named = Vec::new();
        let mut prepared = Vec::new();
        // Source batches observe earlier writes to the same id. Keep that view
        // in memory so validation/sealing still finish before the first write.
        let mut pending: std::collections::HashMap<String, (Value, Value)> =
            std::collections::HashMap::new();
        for mut record in records {
            let object = record.as_object_mut().ok_or_else(|| {
                RepoError::InvalidRecord("Dashboard profile must be an object".into())
            })?;
            if object.get("id").is_none_or(|id| !truth(id)) {
                object.insert("id".into(), random_hex(6)?.into());
            }
            object.entry("_created").or_insert(now.into());
            let id = record["id"]
                .as_str()
                .ok_or_else(|| RepoError::InvalidRecord("Dashboard id must be text".into()))?
                .to_owned();
            if let Some(name) = record["name"].as_str().filter(|name| !name.is_empty()) {
                let own = held
                    .get(&id)
                    .is_some_and(|names| names.iter().any(|held| held == name));
                if !own
                    && held
                        .iter()
                        .any(|(other, names)| other != &id && names.iter().any(|held| held == name))
                {
                    return Err(RepoError::NameTaken(format!(
                        "a profile called {} already exists",
                        validation::repr(&record["name"])
                    )));
                }
                held.entry(id.clone()).or_default().push(name.into());
            }
            named.push(record);
        }
        // Name claims are checked for the whole batch before conversion, as in
        // the source. An invalid earlier record cannot hide a later collision.
        for record in named {
            let id = record["id"]
                .as_str()
                .ok_or_else(|| RepoError::InvalidRecord("Dashboard id must be text".into()))?
                .to_owned();
            let stored = match pending.get(&id) {
                Some((dashboard, _)) => dashboard.clone(),
                None => {
                    ProfileRepo::raw(&self.repo.path(&id, "dashboard")?)?.unwrap_or(Value::Null)
                }
            };
            let chosen = intent.unwrap_or_else(|| conversion::secret_intent(&record));
            let (mut record, twin) = self.prepare(record, chosen, now, pending.get(&id))?;
            record["_rev"] = super::dashboard_revision::next(&stored["_rev"])?;
            pending.insert(id, (record.clone(), twin.clone()));
            prepared.push((record, twin));
        }
        self.commit(prepared, changes)
    }
    pub fn patch(
        &self,
        id: &str,
        apply: impl FnOnce(Value) -> Result<(Value, Map<String, Value>), RepoError>,
        expect_rev: Option<&Value>,
        intent: Option<&str>,
        now: f64,
    ) -> Result<Option<Value>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        let path = self.repo.path(id, "dashboard")?;
        let file = DataLock::for_file(&path)?;
        let _file = file.acquire_configured()?;
        let Some(stored) = ProfileRepo::raw(&path)?.filter(truth) else {
            return Ok(None);
        };
        let (merged, changes) = apply(stored)?;
        self.save(merged, changes, expect_rev, intent, now)
            .map(Some)
    }
    pub fn patch_many(
        &self,
        ids: &[String],
        mut apply: impl FnMut(&str, Value) -> Result<Option<Value>, RepoError>,
        changes: Map<String, Value>,
        intent: Option<&str>,
        now: f64,
    ) -> Result<Vec<Value>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.repo.require_settled()?;
        let mut pending = Vec::new();
        for id in ids {
            if let Some(record) = self.get(id)?
                && let Some(changed) = apply(id, record)?
            {
                pending.push(changed);
            }
        }
        self.save_many(pending, changes, intent, now)
    }
}
