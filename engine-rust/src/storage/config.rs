use super::{atomic::write_json, locks::DataLock};
use serde_json::{Map, Value};
use std::{error::Error, fmt, fs, io, path::PathBuf, sync::Mutex, time::SystemTime};

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    WrongContainer(&'static str),
    TransactionRequired,
    InvalidExtensions(String),
}
impl ConfigError {
    pub fn source_kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "OSError",
            Self::WrongContainer(kind) => kind,
            Self::TransactionRequired => "RuntimeError",
            Self::InvalidExtensions(_) => "ValueError",
        }
    }
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) if error.kind() == io::ErrorKind::TimedOut => fmt::Display::fmt(error, f),
            Self::Io(error) => write!(f, "Settings operation failed: {error}"),
            Self::WrongContainer(_) => {
                f.write_str("Stored settings are not an object; nothing was written")
            }
            Self::TransactionRequired => f.write_str("Settings may only be replaced wholesale inside the root transaction that read them; use set_many for ordinary changes"),
            Self::InvalidExtensions(message) => f.write_str(message),
        }
    }
}
impl Error for ConfigError {}
impl From<io::Error> for ConfigError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

struct CachedConfig {
    key: (SystemTime, u64),
    values: Map<String, Value>,
}

pub struct ConfigStore {
    root: PathBuf,
    cache: Mutex<Option<CachedConfig>>,
}

impl ConfigStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            cache: Mutex::new(None),
        }
    }

    /// Cache successful object reads by the original size/mtime identity.
    /// Returns detached maps; read/parse failures retain the last successful cache.
    pub fn read(&self) -> Map<String, Value> {
        self.read_checked().unwrap_or_default()
    }

    fn read_checked(&self) -> Result<Map<String, Value>, ConfigError> {
        let mut cache = self.cache.lock().unwrap_or_else(|poisoned| {
            let mut cache = poisoned.into_inner();
            *cache = None;
            self.cache.clear_poison();
            cache
        });
        let path = self.root.join("config.json");
        let key = match fs::metadata(&path)
            .and_then(|metadata| Ok((metadata.modified()?, metadata.len())))
        {
            Ok(key) => key,
            Err(_) => {
                *cache = None;
                return Ok(Map::new());
            }
        };
        if let Some(cached) = cache.as_ref().filter(|cached| cached.key == key) {
            return Ok(cached.values.clone());
        }
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(Map::new()),
        };
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Stored settings are not UTF-8")
        })?;
        let Some(values) = serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.as_object().cloned())
        else {
            return Ok(Map::new());
        };
        *cache = Some(CachedConfig {
            key,
            values: values.clone(),
        });
        Ok(values)
    }

    pub fn default_engine(&self, installed: &std::collections::BTreeMap<String, bool>) -> Value {
        self.read()
            .get("default_engine")
            .filter(|value| crate::conversion::truth(value))
            .cloned()
            .unwrap_or_else(|| {
                ["camoufox", "patchright", "playwright"]
                    .into_iter()
                    .find(|name| installed.get(*name) == Some(&true))
                    .unwrap_or("playwright")
                    .into()
            })
    }

    pub fn default_extensions(&self) -> Result<Vec<Value>, ConfigError> {
        Self::extensions_from(&self.read_checked()?)
    }

    fn extensions_from(values: &Map<String, Value>) -> Result<Vec<Value>, ConfigError> {
        match values
            .get("default_extensions")
            .filter(|value| crate::conversion::truth(value))
        {
            None => Ok(Vec::new()),
            Some(Value::Array(rows)) => Ok(rows.clone()),
            Some(Value::String(text)) => Ok(text
                .chars()
                .map(|character| character.to_string().into())
                .collect()),
            Some(Value::Object(object)) => Ok(object.keys().cloned().map(Value::String).collect()),
            Some(value) => Err(ConfigError::WrongContainer(crate::validation::kind(value))),
        }
    }

    pub fn headless_launch(&self) -> bool {
        crate::conversion::truth(self.read().get("headless_launch").unwrap_or(&Value::Null))
    }

    pub fn set(&self, key: &str, value: Value) -> Result<Value, ConfigError> {
        self.change(Map::from_iter([(key.into(), value.clone())]), "TypeError")?;
        Ok(value)
    }

    pub fn set_default_engine(&self, name: Value) -> Result<Value, ConfigError> {
        self.set("default_engine", name)
    }

    pub fn set_headless_launch(&self, value: &Value) -> Result<bool, ConfigError> {
        let enabled = crate::conversion::truth(value);
        self.set("headless_launch", enabled.into())?;
        Ok(enabled)
    }

    pub fn set_default_extensions(&self, wanted: Vec<String>) -> Result<Vec<String>, ConfigError> {
        let root_lock = DataLock::transaction(&self.root)?;
        let _root = root_lock.acquire_configured()?;
        let record = self.read_for_change()?;
        let values = record.as_object().cloned().unwrap_or_default();
        let current = Self::extensions_from(&values).map_err(|error| match error {
            ConfigError::WrongContainer(_) => ConfigError::WrongContainer("TypeError"),
            error => error,
        })?;
        let fresh = wanted
            .iter()
            .filter(|entry| {
                !current
                    .iter()
                    .any(|old| old.as_str() == Some(entry.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        let problems = super::extensions::unloadable(&fresh);
        if !problems.is_empty() {
            return Err(ConfigError::InvalidExtensions(problems.join("; ")));
        }
        self.set("default_extensions", serde_json::json!(wanted))?;
        Ok(wanted)
    }

    /// A replacement must belong to the transaction that read its snapshot.
    pub fn replace_locked(
        &self,
        values: Map<String, Value>,
    ) -> Result<Map<String, Value>, ConfigError> {
        let root_lock = DataLock::transaction(&self.root)?;
        if !root_lock.owned_by_current_thread()? {
            return Err(ConfigError::TransactionRequired);
        }
        let path = self.root.join("config.json");
        let file_lock = DataLock::for_file(&path)?;
        let _file = file_lock.acquire_configured()?;
        write_json(&path, &Value::Object(values.clone()))?;
        Ok(values)
    }

    pub fn set_many(&self, changes: Map<String, Value>) -> Result<Map<String, Value>, ConfigError> {
        if changes.is_empty() {
            return self.read_checked();
        }
        self.change(changes, "AttributeError")
    }

    fn change(
        &self,
        changes: Map<String, Value>,
        wrong_container: &'static str,
    ) -> Result<Map<String, Value>, ConfigError> {
        let root_lock = DataLock::transaction(&self.root)?;
        let _root = root_lock.acquire_configured()?;
        let path = self.root.join("config.json");
        let file_lock = DataLock::for_file(&path)?;
        let _file = file_lock.acquire_configured()?;
        let current = self.read_for_change()?;
        let mut current = match current {
            Value::Object(value) => value,
            _ => return Err(ConfigError::WrongContainer(wrong_container)),
        };
        current.extend(changes);
        write_json(&path, &Value::Object(current.clone()))?;
        Ok(current)
    }

    fn read_for_change(&self) -> Result<Value, ConfigError> {
        Ok(match fs::read(self.root.join("config.json")) {
            Ok(bytes) => {
                let text = std::str::from_utf8(&bytes).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Stored settings are not UTF-8; nothing was written",
                    )
                })?;
                serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::Object(Map::new()))
            }
            Err(_) => Value::Object(Map::new()),
        })
    }
}
