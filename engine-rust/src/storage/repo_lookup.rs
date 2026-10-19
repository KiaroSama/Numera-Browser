use super::{
    locks::DataLock,
    paths::safe_id,
    repo::{ProfileRepo, RepoError},
};
use crate::{models::Profile, validation};
use serde_json::Value;
use std::{collections::BTreeSet, fs, time::Duration};

impl ProfileRepo {
    fn all_ids(&self) -> Result<BTreeSet<String>, RepoError> {
        let mut ids = BTreeSet::new();
        for directory in ["profiles", "dashboard"] {
            let entries = match fs::read_dir(self.root.join(directory)) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for entry in entries {
                let path = entry?.path();
                if path.extension().is_some_and(|v| v == "json")
                    && let Some(id) = path.file_stem().and_then(|v| v.to_str())
                {
                    ids.insert(id.into());
                }
            }
        }
        Ok(ids)
    }

    /// Credential-aware read never hands an encrypted blob to a browser as a password.
    pub fn get(&self, id: &str) -> Result<Option<Profile>, RepoError> {
        if safe_id(id).is_err() {
            return Ok(None);
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        self.require_settled()?;
        let bytes = match fs::read(self.path(id, "profiles")?) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut data: Value = serde_json::from_slice(&bytes)
            .map_err(|_| RepoError::InvalidRecord("Stored profile cannot be decoded".into()))?;
        if let Some(secret) = data["proxy"]["password"].as_str().map(str::to_owned) {
            data["proxy"]["password"] = self
                .readable_secret(Some(&secret))
                .map_or(Value::Null, Value::String);
        }
        Profile::from_record(&data)
            .map(Some)
            .map_err(|_| RepoError::InvalidRecord("Stored profile cannot be modeled".into()))
    }

    pub fn coherent(&self, id: &str, now: f64) -> Result<Option<Profile>, RepoError> {
        if safe_id(id).is_err() {
            return Ok(None);
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        self.require_settled()?;
        let engine = Self::raw(&self.path(id, "profiles")?)?;
        let dash = Self::raw(&self.path(id, "dashboard")?)?;
        if engine.is_none() && dash.is_none() {
            return Ok(None);
        }
        if dash.is_some() {
            self.materialize(id, now)?;
        } else {
            self.adopt(id, now)?;
        }
        self.get(id)
    }

    fn reconciled(&self, id: &str, now: f64) -> Result<Option<Profile>, RepoError> {
        if safe_id(id).is_ok()
            && let (Some(engine), Some(dashboard)) = (
                Self::raw(&self.path(id, "profiles")?)?,
                Self::raw(&self.path(id, "dashboard")?)?,
            )
            && !self.read_disagrees(&engine, &dashboard, now)
        {
            // Reading a healthy pair must not choose between equivalent ciphertexts.
            // Explicit launch coherence still performs its stricter materialization.
            return self.get(id);
        }
        match self.coherent(id, now) {
            Err(RepoError::Diverged(_)) | Err(RepoError::InvalidRecord(_)) => self.get(id),
            result => result,
        }
    }

    pub fn resolve(&self, reference: &str, now: f64) -> Result<Option<Profile>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        self.require_settled()?;
        // Case-insensitive filesystems can open an ID through a differently
        // cased name. Keep the stored spelling without bypassing twin repair.
        let mut canonical = None;
        if safe_id(reference).is_ok() {
            for kind in ["profiles", "dashboard"] {
                let Ok(path) = self.path(reference, kind) else {
                    continue;
                };
                if let Some(value) = Self::raw(&path)?
                    && let Some(id) = value["id"]
                        .as_str()
                        .filter(|id| id.eq_ignore_ascii_case(reference))
                {
                    canonical = Some(id.to_owned());
                    break;
                }
            }
        }
        if let Some(profile) = self.reconciled(canonical.as_deref().unwrap_or(reference), now)? {
            return Ok(Some(profile));
        }
        let mut matches = BTreeSet::new();
        for id in self.all_ids()? {
            if safe_id(&id).is_err() {
                continue;
            }
            for kind in ["profiles", "dashboard"] {
                if Self::raw(&self.path(&id, kind)?)?
                    .is_some_and(|value| value["name"] == reference)
                {
                    matches.insert(id.clone());
                }
            }
        }
        if matches.len() > 1 {
            return Err(RepoError::Message(format!(
                "{} profiles are called {}: {}. Use an id instead, or rename one of them - a command that changes or deletes 'by name' cannot be told which you mean.",
                matches.len(),
                validation::repr(&Value::String(reference.into())),
                matches.into_iter().collect::<Vec<_>>().join(", ")
            )));
        }
        let Some(id) = matches.into_iter().next() else {
            return Ok(None);
        };
        let profile = self.reconciled(&id, now)?;
        if profile
            .as_ref()
            .is_some_and(|profile| profile.name != reference)
        {
            return Err(Self::diverged(&id, "password"));
        }
        Ok(profile)
    }

    pub fn list(&self, now: f64) -> Result<Vec<Profile>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        self.require_settled()?;
        let mut profiles = Vec::new();
        for id in self.all_ids()? {
            if safe_id(&id).is_err() {
                continue;
            }
            let engine = Self::raw(&self.path(&id, "profiles")?)?;
            let dash = Self::raw(&self.path(&id, "dashboard")?)?;
            // Malformed engine-only records cannot be adopted and must not end
            // the entire listing. Filesystem failures still propagate.
            if dash.is_none()
                && engine
                    .as_ref()
                    .is_none_or(|value| Profile::from_record(value).is_err())
            {
                continue;
            }
            match self.reconciled(&id, now) {
                Ok(Some(profile)) => profiles.push(profile),
                Ok(None) | Err(RepoError::InvalidRecord(_)) => {}
                Err(error) => return Err(error),
            }
        }
        profiles.sort_by(|left, right| left.created_at.total_cmp(&right.created_at));
        Ok(profiles)
    }
}
