use super::{
    atomic::write_json,
    config::{ConfigError, ConfigStore},
    locks::DataLock,
    paths::id_path,
};
use crate::{
    conversion::{to_dashboard, to_engine},
    models::Profile,
    vault,
};
use serde_json::Value;
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum RepoError {
    Message(String),
    Diverged(String),
    InvalidRecord(String),
    NameTaken(String),
    StaleProfile(String),
    SessionBusy(String),
    UnconvertibleProfile(String),
    VaultLocked(String),
    Io(io::Error),
    Directory { message: String, cause: io::Error },
    Recovery(vault::recovery::RecoveryError),
}
impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message)
            | Self::Diverged(message)
            | Self::InvalidRecord(message)
            | Self::NameTaken(message)
            | Self::StaleProfile(message)
            | Self::SessionBusy(message)
            | Self::UnconvertibleProfile(message)
            | Self::VaultLocked(message) => f.write_str(message),
            Self::Io(error) if error.kind() == io::ErrorKind::TimedOut => {
                fmt::Display::fmt(error, f)
            }
            Self::Io(error) => write!(f, "Profile storage operation failed: {error}"),
            Self::Directory { message, .. } => f.write_str(message),
            Self::Recovery(error) => fmt::Display::fmt(error, f),
        }
    }
}
impl Error for RepoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Directory { cause, .. } => Some(cause),
            _ => None,
        }
    }
}
impl From<ConfigError> for RepoError {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::Io(error) => Self::Io(error),
            error => Self::Message(error.to_string()),
        }
    }
}
impl From<io::Error> for RepoError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub struct ProfileRepo {
    pub(crate) root: PathBuf,
    pub(crate) password: Option<String>,
    pub(crate) config: ConfigStore,
}
impl ProfileRepo {
    pub fn new(root: PathBuf, password: Option<String>) -> Self {
        Self {
            config: ConfigStore::new(root.clone()),
            root,
            password,
        }
    }
    pub fn open(
        root: PathBuf,
        password: Option<String>,
        old_password: Option<&str>,
    ) -> Result<Self, RepoError> {
        for directory in ["profiles", "user-data"] {
            let path = root.join(directory);
            fs::create_dir_all(&path).map_err(|cause| {
                if cause.kind() != io::ErrorKind::PermissionDenied {
                    return RepoError::Io(cause);
                }
                #[cfg(unix)]
                let owner = {
                    use std::os::unix::fs::MetadataExt;
                    fs::metadata(&root).map_or_else(|_| String::new(), |metadata| {
                        // SAFETY: getuid/getgid take no arguments and only read process identity.
                        #[allow(unsafe_code)]
                        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
                        format!(" (it is owned by uid {}:{}, this process runs as uid {uid}:{gid})",
                                metadata.uid(), metadata.gid())
                    })
                };
                #[cfg(not(unix))]
                let owner = "";
                RepoError::Directory {
                    message: format!(
                        "cannot create {} - the data directory {} is not writable by this process{owner}. Point --data-dir or NUMERA_DATA_DIR at a directory you can write, or fix the ownership of that one. In Docker, prefer the named volume the bundled docker-compose.yml uses; a bind-mounted host directory keeps its own ownership and must already be writable by the container's user.",
                        path.display(), root.display()
                    ),
                    cause,
                }
            })?;
        }
        vault::recovery::recover(&root, password.as_deref(), old_password)
            .map_err(RepoError::Recovery)?;
        Ok(Self::new(root, password))
    }
    pub fn vault(&self) -> Result<Option<Value>, RepoError> {
        self.require_settled()?;
        Ok(self
            .config
            .set_many(serde_json::Map::new())?
            .remove("vault")
            .filter(|value| !value.is_null()))
    }

    pub fn vault_enabled(&self) -> Result<bool, RepoError> {
        Ok(self.vault()?.is_some())
    }

    pub fn unlocked(&self) -> Result<bool, RepoError> {
        let Some(descriptor) = self.vault()?.filter(crate::conversion::truth) else {
            return Ok(true);
        };
        Ok(self
            .password
            .as_deref()
            .filter(|value| !value.is_empty())
            .is_some_and(|password| {
                match (descriptor["salt"].as_str(), descriptor["verifier"].as_str()) {
                    (Some(salt), Some(verifier)) => vault::check_password(password, salt, verifier),
                    _ => false,
                }
            }))
    }

    pub fn enable_vault(&mut self, password: &str) -> Result<usize, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        if self.vault_enabled()? {
            return Err(RepoError::Message("A vault is already set up. Use `numera vault change` to rotate the master password.".into()));
        }
        let count =
            vault::transaction::enable(&self.root, password).map_err(RepoError::Recovery)?;
        self.password = Some(password.to_owned());
        Ok(count)
    }

    pub fn change_vault_password(
        &mut self,
        old_password: &str,
        new_password: &str,
    ) -> Result<usize, RepoError> {
        if new_password.is_empty() {
            return Err(RepoError::Message(
                "the new master password cannot be empty".into(),
            ));
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.require_settled()?;
        let count = vault::transaction::rotate(&self.root, old_password, new_password)
            .map_err(RepoError::Recovery)?;
        self.password = Some(new_password.to_owned());
        Ok(count)
    }

    pub fn resume_vault(
        root: PathBuf,
        old_password: &str,
        new_password: &str,
    ) -> Result<Self, RepoError> {
        Self::open(root, Some(new_password.to_owned()), Some(old_password))
    }

    pub(crate) fn path(&self, id: &str, kind: &str) -> Result<PathBuf, RepoError> {
        id_path(&self.root.join(kind), id, ".json").map_err(|e| RepoError::Message(e.to_string()))
    }
    pub(crate) fn raw(path: &Path) -> io::Result<Option<Value>> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        // Undecodable text is damaged state, not an absent twin safe to replace.
        let text = std::str::from_utf8(&bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Stored profile {} is not valid UTF-8: {error}",
                    path.display()
                ),
            )
        })?;
        Ok(serde_json::from_str(text).ok())
    }
    fn secret(value: &Value, key: &str) -> Option<String> {
        value["proxy"][key].as_str().map(str::to_owned)
    }
    pub(crate) fn readable_secret(&self, value: Option<&str>) -> Result<Option<String>, RepoError> {
        let Some(value) = value else {
            return Ok(None);
        };
        if !vault::is_envelope(value) {
            return Ok(Some(value.into()));
        }
        // Envelope readers inspect vault state before a missing password short-circuits.
        let descriptor = self.vault()?.unwrap_or(Value::Null);
        let Some(password) = self.password.as_deref().filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let (Some(salt), Some(verifier)) =
            (descriptor["salt"].as_str(), descriptor["verifier"].as_str())
        else {
            return Ok(None);
        };
        if !vault::check_password(password, salt, verifier) {
            return Ok(None);
        }
        Ok(vault::decrypt(password, salt, value).ok())
    }
    pub(crate) fn same_secret(
        &self,
        left: Option<&str>,
        right: Option<&str>,
    ) -> Result<bool, RepoError> {
        if left == right {
            return Ok(true);
        }
        if left.is_none_or(str::is_empty) || right.is_none_or(str::is_empty) {
            return Ok(false);
        }
        let left = self.readable_secret(left)?;
        let right = self.readable_secret(right)?;
        Ok(left.is_some() && left == right)
    }
    pub fn refuse_unresolved_secret(
        &self,
        id: &str,
        stored: Option<&str>,
        twin: Option<&str>,
    ) -> Result<(), RepoError> {
        let missing = stored.is_none_or(str::is_empty) || twin.is_none_or(str::is_empty);
        if !self.same_secret(stored, twin)?
            && !(stored.is_none_or(str::is_empty) && twin.is_none_or(str::is_empty))
            && (missing
                || (self.readable_secret(stored)?.is_some()
                    && self.readable_secret(twin)?.is_some()))
        {
            return Err(Self::diverged(id, "password"));
        }
        Ok(())
    }

    pub fn refuse_conflicting_write(&self, id: &str, intent: &str) -> Result<(), RepoError> {
        if intent != "preserve" || super::paths::safe_id(id).is_err() {
            return Ok(());
        }
        let engine = Self::raw(&self.path(id, "profiles")?)?.unwrap_or(Value::Null);
        let dashboard = Self::raw(&self.path(id, "dashboard")?)?.unwrap_or(Value::Null);
        let stored = &engine["proxy"]["password"];
        let twin = &dashboard["proxy"]["pass"];
        if [stored, twin]
            .iter()
            .all(|value| value.is_null() || value.is_string())
        {
            return self.refuse_unresolved_secret(id, stored.as_str(), twin.as_str());
        }
        // Stored JSON may predate typed writes; a non-text secret is not absence.
        if super::repo_reconcile::equal(stored, twin)
            || (!crate::conversion::truth(stored) && !crate::conversion::truth(twin))
        {
            return Ok(());
        }
        let readable = |value: &Value| -> Result<bool, RepoError> {
            Ok(!value.as_str().is_some_and(vault::is_envelope)
                || self.readable_secret(value.as_str())?.is_some())
        };
        if !crate::conversion::truth(stored)
            || !crate::conversion::truth(twin)
            || (readable(stored)? && readable(twin)?)
        {
            return Err(Self::diverged(id, "password"));
        }
        Ok(())
    }

    pub(crate) fn diverged(id: &str, field: &str) -> RepoError {
        RepoError::Diverged(if field == "proxy" {
            format!(
                "the two stored copies of profile {id} differ: the engine has a proxy that the Dashboard did not show, so this operation cannot remove it. Set the profile's proxy explicitly before removing it."
            )
        } else {
            format!(
                "the two stored copies of profile {id} hold different proxy passwords, and neither can be shown to be the one you meant, so Numera will not choose. Set the proxy password on this profile again - one write moves both copies - or clear it. Launches have been using the engine copy."
            )
        })
    }
    pub(crate) fn require_settled(&self) -> Result<(), RepoError> {
        if self.root.join("vault-migration.json").try_exists()?
            && vault::recovery::needs_recovery(&self.root).map_err(RepoError::Recovery)?
        {
            return Err(RepoError::Message("another process left a vault migration unresolved under this data root while this store was open, so its secrets can no longer be read or written safely. Reopen the store (or restart the server) to finish or refuse that migration.".into()));
        }
        Ok(())
    }
    fn incoming_secret(
        &self,
        input: Option<&str>,
        descriptor: &Value,
    ) -> Result<Option<String>, RepoError> {
        let Some(input) = input else {
            return Ok(None);
        };
        if descriptor.is_null() || input.is_empty() || vault::is_envelope(input) {
            return Ok(Some(input.to_owned()));
        }
        let salt = descriptor["salt"]
            .as_str()
            .ok_or_else(|| RepoError::Message("Vault descriptor has no salt".into()))?;
        let verifier = descriptor["verifier"]
            .as_str()
            .ok_or_else(|| RepoError::Message("Vault descriptor has no verifier".into()))?;
        let password = self.password.as_deref()
            .filter(|password| vault::check_password(password, salt, verifier))
            .ok_or_else(|| RepoError::Message("the vault is locked, so this proxy password cannot be stored safely - set NUMERA_PASSWORD or pass --password first".into()))?;
        vault::encrypt(password, salt, input)
            .map(Some)
            .map_err(|error| RepoError::Message(error.to_string()))
    }

    pub fn materialize(&self, id: &str, now: f64) -> Result<Option<Profile>, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.require_settled()?;
        let Some(dash) = Self::raw(&self.path(id, "dashboard")?)? else {
            return Ok(None);
        };
        let path = self.path(id, "profiles")?;
        let current = Self::raw(&path)?;
        let mut profile =
            to_engine(&dash, now).map_err(|e| RepoError::InvalidRecord(e.to_string()))?;
        profile.id = id.into();
        if let Some(current) = &current {
            if let Some(v) = current["created_at"].as_f64() {
                profile.created_at = v;
            }
            if let Some(v) = current["updated_at"].as_f64() {
                profile.updated_at = v;
            }
            let fp = &current["fingerprint"];
            if fp["screen_width"].as_i64() == Some(profile.fingerprint.screen_width)
                && fp["screen_height"].as_i64() == Some(profile.fingerprint.screen_height)
            {
                if let Some(v) = fp["viewport_width"].as_i64().filter(|v| *v != 0) {
                    profile.fingerprint.viewport_width = v;
                }
                if let Some(v) = fp["viewport_height"].as_i64().filter(|v| *v != 0) {
                    profile.fingerprint.viewport_height = v;
                }
            }
        }
        let mut record = serde_json::to_value(&profile)
            .map_err(|_| RepoError::Message("Profile cannot be serialized".into()))?;
        let old = current.as_ref().and_then(|v| Self::secret(v, "password"));
        let input = Self::secret(&record, "password");
        let descriptor = if input.is_some() {
            self.vault()?.unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let fresh = match input.as_deref() {
            Some(input) => self.incoming_secret(Some(input), &descriptor),
            None => Ok(old
                .clone()
                .filter(|value| !value.is_empty())
                .or_else(|| Self::secret(&dash, "pass").filter(|value| !value.is_empty()))),
        };
        match fresh {
            Ok(fresh) => {
                if profile.proxy.is_some() {
                    let selected = if old.as_deref().is_none_or(str::is_empty)
                        || self.same_secret(old.as_deref(), fresh.as_deref())?
                    {
                        fresh
                    } else if fresh.as_deref().is_none_or(str::is_empty) {
                        return Err(Self::diverged(id, "password"));
                    } else if self.readable_secret(old.as_deref())?.is_none()
                        || self.readable_secret(fresh.as_deref())?.is_none()
                    {
                        old
                    } else {
                        return Err(Self::diverged(id, "password"));
                    };
                    record["proxy"]["password"] = selected.map_or(Value::Null, Value::String);
                } else if current
                    .as_ref()
                    .is_some_and(|v| v["proxy"].as_object().is_some_and(|p| !p.is_empty()))
                {
                    return Err(Self::diverged(id, "proxy"));
                }
                if current.as_ref() != Some(&record) {
                    let file = DataLock::for_file(&path)?;
                    let _file = file.acquire_configured()?;
                    write_json(&path, &record)?;
                }
                // Source materialization returns its derived model, not a decrypted
                // launch view. The launch gate must reread through the credential-aware
                // store after this transaction, just as source coherent() does.
                Ok(Some(profile))
            }
            Err(error) => Err(error),
        }
    }
    pub fn adopt(&self, id: &str, now: f64) -> Result<bool, RepoError> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        self.require_settled()?;
        let Some(engine) = Self::raw(&self.path(id, "profiles")?)? else {
            return Ok(false);
        };
        let path = self.path(id, "dashboard")?;
        if Self::raw(&path)?.is_some() {
            return Ok(false);
        }
        let profile = Profile::from_record(&engine).map_err(|_| {
            RepoError::InvalidRecord("Engine profile cannot be read for adoption".into())
        })?;
        let mut twin = to_dashboard(&profile);
        twin["id"] = id.into();
        if !twin["proxy"].is_null() {
            twin["proxy"]["pass"] = Self::secret(&engine, "password")
                .filter(|v| !v.is_empty())
                .map_or(Value::String(String::new()), Value::String);
        }
        twin["_rev"] = 1.into();
        twin["_created"] = engine
            .get("created_at")
            .filter(|v| v.as_f64().is_some_and(|v| v != 0.0))
            .cloned()
            .unwrap_or(Value::from(now));
        let file = DataLock::for_file(&path)?;
        let _file = file.acquire_configured()?;
        write_json(&path, &twin)?;
        Ok(true)
    }
}
