use super::{atomic::write_json, locks::DataLock};
use serde_json::{Map, Value};
use std::{error::Error, fmt, fs, io, path::PathBuf, time::Duration};

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    WrongContainer(&'static str),
}
impl ConfigError {
    pub fn source_kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "OSError",
            Self::WrongContainer(kind) => kind,
        }
    }
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "Settings operation failed: {error}"),
            Self::WrongContainer(_) => {
                f.write_str("Stored settings are not an object; nothing was written")
            }
        }
    }
}
impl Error for ConfigError {}
impl From<io::Error> for ConfigError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub struct ConfigStore {
    root: PathBuf,
}
impl ConfigStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Original read view is empty for missing/damaged/non-object settings.
    /// It does not rewrite that file or discard unknown fields in a valid one.
    pub fn read(&self) -> Map<String, Value> {
        fs::read(self.root.join("config.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    }

    pub fn set_many(&self, changes: Map<String, Value>) -> Result<Map<String, Value>, ConfigError> {
        if changes.is_empty() {
            return Ok(self.read());
        }
        let limit = std::env::var("NUMERA_LOCK_TIMEOUT")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 9_223_372_036.0)
            .unwrap_or(30.0);
        let root_lock = DataLock::transaction(&self.root)?;
        let _root = root_lock.acquire(Duration::from_secs_f64(limit))?;
        let path = self.root.join("config.json");
        let file_lock = DataLock::for_file(&path)?;
        let _file = file_lock.acquire(Duration::from_secs_f64(limit))?;
        let current = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or_else(|| Value::Object(Map::new()));
        let mut current = match current {
            Value::Object(value) => value,
            _ => return Err(ConfigError::WrongContainer("AttributeError")),
        };
        current.extend(changes);
        write_json(&path, &Value::Object(current.clone()))?;
        Ok(current)
    }
}
