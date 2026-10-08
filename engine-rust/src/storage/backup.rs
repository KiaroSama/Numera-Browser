use super::{
    atomic::write_stream,
    locks::{DataLock, SessionLease},
    paths::id_path,
    repo::{ProfileRepo, RepoError},
};
use crate::{conversion::value_text, ownership::claims};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn zip_error(error: zip::result::ZipError) -> io::Error {
    io::Error::other(format!("Backup archive operation failed: {error}"))
}

fn write_session(
    zip: &mut ZipWriter<&mut fs::File>,
    base: &Path,
    directory: &Path,
    depth: usize,
) -> io::Result<()> {
    if depth > 128 {
        return Err(io::Error::other(
            "Session directory nesting exceeds safe traversal limit",
        ));
    }
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let leaf = entry.file_name();
        if [
            ".ignoreme",
            "secrets.md",
            "explain-AI.md",
            ".ai",
            ".specify",
            "specs",
            ".claude",
            ".codex",
            ".kiro",
            ".cursor",
            ".cline",
            ".agents",
            "graphify-out",
            ".codebase-memory",
            "plans",
        ]
        .iter()
        .any(|protected| leaf == *protected)
        {
            return Err(io::Error::other(
                "Protected local-only content cannot be included in a backup",
            ));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::other("Session backup refuses symbolic links"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(io::Error::other("Session backup refuses reparse points"));
            }
        }
        if metadata.is_dir() {
            write_session(zip, base, &path, depth + 1)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(base).map_err(io::Error::other)?;
            let name = relative
                .to_str()
                .ok_or_else(|| io::Error::other("Session filename is not UTF8"))?
                .replace('\\', "/");
            zip.start_file(
                format!("user-data/{name}"),
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .large_file(metadata.len() >= u64::from(u32::MAX)),
            )
            .map_err(zip_error)?;
            io::copy(&mut fs::File::open(path)?, zip)?;
        } else {
            return Err(io::Error::other("Session backup refuses non-regular files"));
        }
    }
    Ok(())
}

impl ProfileRepo {
    pub fn backup(
        &self,
        id: &str,
        destination: &Path,
        host: &str,
    ) -> Result<Option<PathBuf>, RepoError> {
        let _lease = SessionLease::acquire(&SessionLease::path_for(&self.root, id)?)?;
        claims::refuse_if_busy(&self.root, id, host)
            .map_err(|error| RepoError::Message(error.to_string()))?;
        let profile_bytes = {
            let lock = DataLock::transaction(&self.root)?;
            let _guard = lock.acquire(Duration::from_secs(30))?;
            let Some(profile) = self.get(id)? else {
                return Ok(None);
            };
            if self.secret_withheld(id)? {
                return Err(RepoError::VaultLocked(format!(
                    "backing up '{}' would drop its proxy password: the vault is locked, so this needs the master password - set NUMERA_PASSWORD or pass --password",
                    value_text(&profile.name)
                )));
            }
            serde_json::to_vec_pretty(&profile)
                .map_err(|_| RepoError::Message("Profile cannot be serialized for backup".into()))?
        };
        let session = id_path(&self.root.join("user-data"), id, "")
            .map_err(|error| RepoError::Message(error.to_string()))?;
        let output = if destination.is_absolute() {
            destination.to_owned()
        } else {
            std::env::current_dir()?.join(destination)
        };
        write_stream(&output, |file| {
            let mut zip = ZipWriter::new(file);
            zip.start_file(
                "profile.json",
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(zip_error)?;
            zip.write_all(&profile_bytes)?;
            match fs::symlink_metadata(&session) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        if metadata.file_attributes() & 0x400 != 0 {
                            return Err(io::Error::other("Session backup refuses reparse points"));
                        }
                    }
                    // A destination inside this tree could cause the archive to consume itself.
                    let session_real = fs::canonicalize(&session)?;
                    let dest_parent =
                        fs::canonicalize(output.parent().ok_or_else(|| {
                            io::Error::other("Backup destination has no parent")
                        })?)?;
                    if dest_parent.starts_with(&session_real) {
                        return Err(io::Error::other(
                            "Backup destination must be outside the browser session",
                        ));
                    }
                    write_session(&mut zip, &session, &session, 0)?;
                }
                Ok(_) => {
                    return Err(io::Error::other(
                        "Session backup requires a regular directory",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            zip.finish().map_err(zip_error)?;
            Ok(())
        })?;
        Ok(Some(destination.to_owned()))
    }
}
