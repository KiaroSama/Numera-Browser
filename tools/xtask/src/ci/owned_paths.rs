//! Child-visible spelling; the Temporary/Units owners retain their original path.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn temporary_environment_path(path: &Path) -> io::Result<PathBuf> {
    if !fs::metadata(path)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Owned command temporary path must be an existing directory",
        ));
    }
    #[cfg(windows)]
    {
        let path = dunce::canonicalize(path)?;
        if !matches!(path.components().next(), Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::UNC(..)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Owned command temporary directory requires a legacy-compatible Windows path",
            ));
        }
        Ok(path)
    }
    #[cfg(not(windows))]
    {
        // Keep Linux's deliberately short Chromium socket path unchanged.
        Ok(path.to_owned())
    }
}
