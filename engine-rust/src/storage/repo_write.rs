use super::{
    atomic::Batch,
    config::ConfigStore,
    locks::DataLock,
    repo::{ProfileRepo, RepoError},
};
use crate::{
    conversion::to_dashboard,
    models::Profile,
    validation,
    vault::{
        self,
        policy::{VaultState, write_secret},
    },
};
use serde_json::Value;
use std::{collections::HashMap, fs};

impl ProfileRepo {
    /// Prepare both copies before the first write; no caller snapshot bypasses
    /// root ownership, name uniqueness, validation or credential decisions.
    pub fn save(&self, profile: Profile, now: f64) -> Result<Profile, RepoError> {
        self.save_many(vec![profile], serde_json::Map::new(), "reject", now)?
            .pop()
            .ok_or_else(|| RepoError::Message("Profile write returned no record".into()))
    }

    pub fn save_many(
        &self,
        profiles: Vec<Profile>,
        changes: serde_json::Map<String, Value>,
        policy: &str,
        now: f64,
    ) -> Result<Vec<Profile>, RepoError> {
        for profile in &profiles {
            Self::validate_write(profile)?;
        }
        let root = DataLock::transaction(&self.root)?;
        let _root = root.acquire_configured()?;
        self.require_settled()?;
        if !matches!(policy, "reject" | "skip") {
            return Err(RepoError::Message(format!(
                "unknown name-clash policy: {}",
                validation::repr(&Value::String(policy.into()))
            )));
        }
        let mut held = self.held_names()?;
        let mut accepted = Vec::new();
        for profile in profiles {
            let name = profile
                .name
                .as_str()
                .ok_or_else(|| RepoError::Message("Profile name is not text".into()))?;
            let already_held = held
                .get(&profile.id)
                .is_some_and(|names| names.iter().any(|held| held == name));
            let clash = !name.is_empty()
                && !already_held
                && held
                    .iter()
                    .any(|(id, names)| id != &profile.id && names.iter().any(|held| held == name));
            if clash {
                if policy == "skip" {
                    continue;
                }
                return Err(RepoError::NameTaken(format!(
                    "a profile called {} already exists",
                    validation::repr(&profile.name)
                )));
            }
            held.entry(profile.id.clone())
                .or_default()
                .push(name.into());
            accepted.push(profile);
        }
        let prepared = accepted
            .into_iter()
            .map(|profile| self.records(profile, now))
            .collect::<Result<Vec<_>, _>>()?;
        let mut lock_paths = Vec::new();
        for (profile, _, _) in &prepared {
            lock_paths.extend([
                self.path(&profile.id, "profiles")?,
                self.path(&profile.id, "dashboard")?,
            ]);
        }
        if !changes.is_empty() {
            lock_paths.push(self.root.join("config.json"));
        }
        lock_paths.sort();
        lock_paths.dedup();
        let locks = lock_paths
            .iter()
            .map(|path| DataLock::for_file(path))
            .collect::<Result<Vec<_>, _>>()?;
        let _guards = locks
            .iter()
            .map(|lock| lock.acquire_configured())
            .collect::<Result<Vec<_>, _>>()?;
        let mut batch = Batch::default();
        let result = (|| -> Result<(), RepoError> {
            for (profile, engine, dashboard) in &prepared {
                batch.write(&self.path(&profile.id, "profiles")?, engine)?;
                batch.write(&self.path(&profile.id, "dashboard")?, dashboard)?;
            }
            if !changes.is_empty() {
                let mut config = ConfigStore::new(self.root.clone()).read();
                config.extend(changes);
                batch.write(&self.root.join("config.json"), &Value::Object(config))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let failures = batch.rollback();
            if !failures.is_empty() {
                return Err(RepoError::Message(format!(
                    "Profile write failed; {} file(s) could not be restored: {error}",
                    failures.len()
                )));
            }
            return Err(error);
        }
        Ok(prepared
            .into_iter()
            .map(|(profile, _, _)| profile)
            .collect())
    }

    pub fn edit(
        &self,
        reference: &str,
        apply: impl FnOnce(&mut Profile) -> Result<Option<Profile>, RepoError>,
        now: f64,
    ) -> Result<Option<Profile>, RepoError> {
        let root = DataLock::transaction(&self.root)?;
        let _root = root.acquire_configured()?;
        self.require_settled()?;
        let Some(mut current) = self.resolve(reference, now)? else {
            return Ok(None);
        };
        let updated = apply(&mut current)?.unwrap_or(current);
        self.save(updated, now).map(Some)
    }

    pub fn patch_many(
        &self,
        references: &[String],
        mut apply: impl FnMut(&mut Profile) -> Result<bool, RepoError>,
        changes: serde_json::Map<String, Value>,
        policy: &str,
        now: f64,
    ) -> Result<Vec<Profile>, RepoError> {
        let root = DataLock::transaction(&self.root)?;
        let _root = root.acquire_configured()?;
        self.require_settled()?;
        let mut pending = Vec::new();
        for reference in references {
            if let Some(mut profile) = self.resolve(reference, now)?
                && apply(&mut profile)?
            {
                pending.push(profile);
            }
        }
        self.save_many(pending, changes, policy, now)
    }

    fn validate_write(profile: &Profile) -> Result<(), RepoError> {
        let problems = validation::profile_problems(
            &serde_json::to_value(profile)
                .map_err(|_| RepoError::Message("Profile cannot be serialized".into()))?,
            "engine record",
        )
        .map_err(|_| RepoError::Message("Field policy is unreadable".into()))?;
        if !problems.is_empty() {
            return Err(RepoError::Message(
                problems.into_iter().take(8).collect::<Vec<_>>().join("; "),
            ));
        }
        Ok(())
    }

    pub(crate) fn held_names(&self) -> Result<HashMap<String, Vec<String>>, RepoError> {
        let mut held: HashMap<String, Vec<String>> = HashMap::new();
        for directory in ["profiles", "dashboard"] {
            let entries = match fs::read_dir(self.root.join(directory)) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for entry in entries {
                let path = entry?.path();
                if !super::paths::json_filename(&path) {
                    continue;
                }
                if let Some(value) = Self::raw(&path)?.filter(Value::is_object)
                    && crate::conversion::truth(&value["name"])
                {
                    let id = value["id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .or_else(|| path.file_stem().and_then(|stem| stem.to_str()));
                    let name = match &value["name"] {
                        Value::String(name) => name.clone(),
                        Value::Bool(value) => value.to_string(),
                        Value::Number(value) => value.to_string(),
                        _ => String::new(),
                    };
                    if let Some(id) = id {
                        held.entry(id.into()).or_default().push(name);
                    }
                }
            }
        }
        Ok(held)
    }

    fn records(
        &self,
        mut profile: Profile,
        now: f64,
    ) -> Result<(Profile, Value, Value), RepoError> {
        let engine_path = self.path(&profile.id, "profiles")?;
        let dash_path = self.path(&profile.id, "dashboard")?;
        let previous = Self::raw(&dash_path)?.unwrap_or(Value::Null);
        let current = Self::raw(&engine_path)?.unwrap_or(Value::Null);
        let old = current["proxy"]["password"].as_str();
        let twin = previous["proxy"]["pass"].as_str();
        let config = Value::Object(self.config.read());
        let descriptor = &config["vault"];
        let salt = descriptor["salt"].as_str().unwrap_or("");
        let password = self.password.as_deref().unwrap_or("");
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
        let (left, right) = if let Some(proxy) = &profile.proxy {
            if proxy.password_intent() == "preserve" {
                self.refuse_unresolved_secret(&profile.id, old, twin)?;
                (old.map(str::to_owned), twin.map(str::to_owned))
            } else {
                let asked = proxy.to_record()["password"].as_str().map(str::to_owned);
                let value = write_secret(state, asked.as_deref(), old, seal)
                    .map_err(|e| RepoError::Message(e.to_string()))?;
                (value.clone(), value)
            }
        } else {
            for value in [old, twin] {
                write_secret(state, Some(""), value, seal)
                    .map_err(|e| RepoError::Message(e.to_string()))?;
            }
            (None, None)
        };
        profile.updated_at = now;
        let mut engine = serde_json::to_value(&profile)
            .map_err(|_| RepoError::Message("Profile cannot be serialized".into()))?;
        if !engine["proxy"].is_null() {
            engine["proxy"]["password"] = left.map_or(Value::Null, Value::String);
        }
        let mut dash = to_dashboard(&profile);
        dash["id"] = profile.id.clone().into();
        if !dash["proxy"].is_null() {
            dash["proxy"]["pass"] = right
                .filter(|v| !v.is_empty())
                .map_or(Value::String(String::new()), Value::String);
        }
        let mut merged = previous.as_object().cloned().unwrap_or_default();
        let fresh = dash.as_object().ok_or_else(|| {
            RepoError::Message("Dashboard conversion did not return an object".into())
        })?;
        merged.extend(fresh.clone());
        for key in [
            "folder",
            "_created",
            "_rev",
            "_last_active",
            "_trust",
            "status",
            "lastActive",
        ] {
            if let Some(value) = previous.get(key) {
                merged.insert(key.into(), value.clone());
            }
        }
        let revision = previous["_rev"]
            .as_i64()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| RepoError::Message("Profile revision overflow".into()))?;
        merged.insert("_rev".into(), revision.into());
        merged.entry("_created").or_insert(Value::from(now));
        Ok((profile, engine, Value::Object(merged)))
    }
}
