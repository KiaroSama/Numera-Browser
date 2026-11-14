use crate::validation::repr;
use serde_json::Value;
use std::{
    error::Error,
    fmt, io,
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub struct InvalidId(String);
impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for InvalidId {}

/// Match Python's platform-default `*.json`, including the hidden `.json` name.
pub(crate) fn json_filename(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().as_encoded_bytes();
    name.ends_with(b".json")
        || cfg!(windows)
            && (name[name.len().saturating_sub(5)..].eq_ignore_ascii_case(b".json")
                // Python Unicode IGNORECASE also matches long s in this suffix.
                || name[name.len().saturating_sub(6)..].eq_ignore_ascii_case(b".j\xc5\xbfon"))
}

/// IDs are portable opaque tokens, never caller-controlled path fragments.
pub fn safe_id(value: &str) -> Result<&str, InvalidId> {
    // The current reference uses fullmatch: an opaque portable ID may not
    // contain a trailing newline or any other filename-control character.
    let matched = value;
    let shown = repr(&Value::String(value.into()));
    if matched.is_empty()
        || matched.len() > 64
        || !matched
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(InvalidId(format!(
            "id {shown} is not a valid profile id: expected 1-64 characters from A-Z, a-z, 0-9, '_' or '-'"
        )));
    }
    let upper = value.to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|tail| tail.len() == 1 && tail.as_bytes()[0].is_ascii_digit());
    if reserved {
        return Err(InvalidId(format!(
            "id {shown} is not a valid profile id: {upper} is a reserved device name on Windows"
        )));
    }
    Ok(value)
}

/// Lexical containment must be checked before resolving caller components.
pub(crate) fn lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
pub(crate) fn physical(path: &Path) -> io::Result<PathBuf> {
    #[cfg(feature = "test-support")]
    if let Some(path) = crate::vault::testing::resolve_path(path)? {
        return Ok(path);
    }
    super::locks::resolved(path)
}
pub fn id_path(base: &Path, value: &str, suffix: &str) -> Result<PathBuf, InvalidId> {
    let id = safe_id(value)?;
    let base = super::locks::resolved(base).map_err(|error| InvalidId(error.to_string()))?;
    let target = base.join(format!("{id}{suffix}"));
    let absolute = lexical(&target);
    let refusal = || {
        InvalidId(format!(
            "Profile path escapes its folder or is linked: {}",
            target.display()
        ))
    };
    if absolute == base || !absolute.starts_with(&base) {
        return Err(refusal());
    }
    #[cfg(feature = "test-support")]
    crate::vault::testing::event("before-path-probe", &target)
        .map_err(|error| InvalidId(error.to_string()))?;
    if std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(refusal());
    }
    let physical = physical(&target).map_err(|error| InvalidId(error.to_string()))?;
    // Case-insensitive filesystems can return the stored spelling of this
    // same regular file (Fixture.json -> fixture.json). Keep the cross-ID
    // guard: only an actual matching filesystem identity permits that alias.
    #[cfg(unix)]
    let same_file = || {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(&target), std::fs::metadata(&physical)) {
            (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
            _ => false,
        }
    };
    #[cfg(not(unix))]
    let same_file = || match (target.canonicalize(), physical.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    };
    if physical.parent() != Some(base.as_path()) || physical != absolute && !same_file() {
        return Err(refusal());
    }
    Ok(target)
}

/// Session bytes remain owned at a retained directory after rollback failed.
#[derive(Debug)]
pub struct SessionStranded {
    pub retained: PathBuf,
    pub destination: PathBuf,
    pub cause: io::Error,
    pub primary: Option<String>,
}
impl fmt::Display for SessionStranded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(primary) = &self.primary {
            write!(f, "The operation failed ({primary}); ")?;
        }
        write!(
            f,
            "the previous session could not be put back: it is still on disk as {}. Move that directory to {} to finish recovering ({})",
            self.retained.display(),
            self.destination.display(),
            self.cause
        )
    }
}
impl Error for SessionStranded {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
