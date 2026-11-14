//! Shared portable-installation lifetime and activation ownership.
//! A development executable outside an installation's `engine` directory does
//! not claim an installation. Installed processes hold this lease until exit.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
};

pub struct RuntimeLease {
    _file: Option<File>,
}
/// Recognize the installed engine layout without probing a developer checkout.
pub fn installation_root(executable: &Path) -> Option<PathBuf> {
    let engine = executable.parent()?;
    (engine.file_name()? == "engine")
        .then(|| engine.parent().map(Path::to_owned))
        .flatten()
}
fn busy(detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        format!(
            "Recovery owns this installation; finish or repair the pending activation before starting Numera ({detail})"
        ),
    )
}
impl RuntimeLease {
    pub fn for_executable(executable: &Path, activation: Option<&str>) -> io::Result<Self> {
        Self::acquire(installation_root(executable).as_deref(), activation)
    }
    pub fn acquire(installation: Option<&Path>, activation: Option<&str>) -> io::Result<Self> {
        let Some(root) = installation else {
            return Ok(Self { _file: None });
        };
        let updates = root.join(".updates");
        fs::create_dir_all(&updates)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(updates.join("runtime.lock"))?;
        file.try_lock_shared().map_err(|error| match error {
            TryLockError::WouldBlock => busy("another process holds the installation write lease"),
            TryLockError::Error(error) => busy(error),
        })?;
        match fs::read(updates.join("activation.json")) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(busy(error)),
            Ok(bytes) => {
                let marker: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|_| busy("activation evidence is unreadable"))?;
                let nonce = activation
                    .filter(|nonce| !nonce.is_empty())
                    .ok_or_else(|| busy("activation nonce is required"))?;
                let digest = Sha256::digest(nonce.as_bytes())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                if marker["schema"] != 1 || marker["nonce_sha256"].as_str() != Some(digest.as_str())
                {
                    return Err(busy("activation evidence does not match this process"));
                }
            }
        }
        Ok(Self { _file: Some(file) })
    }
}
