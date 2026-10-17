use super::{
    archive_paths::{Staging, member_path, random_hex},
    locks::{DataLock, SessionLease},
    paths::id_path,
    repo::{ProfileRepo, RepoError},
};
use crate::{models::Profile, ownership::claims};
use serde_json::Value;
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::{ZipArchive, result::ZipError};

fn zip_error(error: ZipError) -> RepoError {
    RepoError::Message(format!("Restore archive operation failed: {error}"))
}

fn read_profile(archive: &mut ZipArchive<fs::File>, source: &Path) -> Result<Profile, RepoError> {
    let name = source.file_name().unwrap_or_default().to_string_lossy();
    let mut file = archive
        .by_name("profile.json")
        .map_err(|error| match error {
            ZipError::FileNotFound => {
                RepoError::Message(format!("{name} has no profile.json - not a Numera backup"))
            }
            error => zip_error(error),
        })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let record: Value = serde_json::from_slice(&bytes).map_err(|error| {
        let message = if error.to_string().starts_with("key must be a string") {
            format!(
                "Expecting property name enclosed in double quotes: line {} column {} (char {})",
                error.line(),
                error.column(),
                error.column().saturating_sub(1)
            )
        } else {
            error.to_string()
        };
        RepoError::InvalidRecord(format!("{name} has an unreadable profile.json: {message}"))
    })?;
    Profile::from_record(&record).map_err(|error| {
        RepoError::InvalidRecord(format!("{name} does not contain a usable profile: {error}"))
    })
}

fn unpack(archive: &mut ZipArchive<fs::File>, staging: &Path) -> Result<(), RepoError> {
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(zip_error)?;
        let name = file.name().map_err(zip_error)?;
        let Some(member) = name.strip_prefix("user-data/") else {
            continue;
        };
        if file.is_dir() || name.ends_with('/') {
            continue;
        }
        // ZIP link metadata is never honored; only regular bytes enter staging.
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(RepoError::Message(
                "Restore refuses symbolic link archive members".into(),
            ));
        }
        let target =
            member_path(staging, member).map_err(|error| RepoError::Message(error.to_string()))?;
        fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| io::Error::other("Restore member has no parent"))?,
        )?;
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(target)?;
        io::copy(&mut file, &mut output)?;
        output.sync_all()?;
    }
    Ok(())
}

fn ordinary_directory(path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other(
            "Restore session target is not a regular directory",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("Restore refuses session reparse points"));
        }
    }
    Ok(true)
}

fn undo_session(
    session: &Path,
    aside: Option<&Path>,
    installed: bool,
    failure: RepoError,
) -> RepoError {
    if installed && let Err(error) = fs::remove_dir_all(session) {
        return RepoError::Message(format!(
            "Restore failed: {failure}; new session could not be removed: {error}; previous session retained at {}",
            aside.map_or_else(|| "none".into(), |path| path.display().to_string())
        ));
    }
    if let Some(aside) = aside
        && let Err(error) = fs::rename(aside, session)
    {
        return RepoError::Message(format!(
            "the restore failed ({failure}) and the previous session could not be put back: it is still on disk as {}. Move that directory to {} to finish recovering. ({error})",
            aside.display(),
            session.display()
        ));
    }
    failure
}

impl ProfileRepo {
    pub fn restore(
        &self,
        source: &Path,
        new_id: bool,
        host: &str,
        now: f64,
    ) -> Result<Profile, RepoError> {
        let mut archive = ZipArchive::new(fs::File::open(source)?).map_err(zip_error)?;
        let mut profile = read_profile(&mut archive, source)?;
        if new_id {
            profile.id = random_hex(6)?;
        }
        let mut staging = Staging::new(&self.root, "numera-restore-")?;
        let result = (|| -> Result<(Profile, Option<PathBuf>), RepoError> {
            unpack(&mut archive, staging.path()?)?;
            let _lease = SessionLease::acquire(&SessionLease::path_for(&self.root, &profile.id)?)?;
            claims::refuse_if_busy(&self.root, &profile.id, host)
                .map_err(|error| RepoError::Message(error.to_string()))?;
            let lock = DataLock::transaction(&self.root)?;
            let _root = lock.acquire(Duration::from_secs(30))?;
            self.require_settled()?;
            if new_id
                && (self.path(&profile.id, "profiles")?.try_exists()?
                    || self.path(&profile.id, "dashboard")?.try_exists()?)
            {
                return Err(RepoError::Message(
                    "Generated restore ID collided; nothing was replaced".into(),
                ));
            }
            let held = self.held_names()?;
            if let Some(name) = profile.name.as_str().filter(|name| !name.is_empty()) {
                let own = held
                    .get(&profile.id)
                    .is_some_and(|names| names.iter().any(|held| held == name));
                let taken = |candidate: &str| {
                    held.iter().any(|(id, names)| {
                        id != &profile.id && names.iter().any(|name| name == candidate)
                    })
                };
                if !own && taken(name) {
                    let original = name.to_owned();
                    let selected = (1..=held.values().map(Vec::len).sum::<usize>() + 1)
                        .map(|count| {
                            if count == 1 {
                                format!("{original} (copy)")
                            } else {
                                format!("{original} (copy {count})")
                            }
                        })
                        .find(|candidate| !taken(candidate))
                        .ok_or_else(|| {
                            RepoError::Message("No unclaimed restore name found".into())
                        })?;
                    profile.name = selected.into();
                }
            }
            let session = id_path(&self.root.join("user-data"), &profile.id, "")
                .map_err(|error| RepoError::Message(error.to_string()))?;
            fs::create_dir_all(
                session
                    .parent()
                    .ok_or_else(|| io::Error::other("Session has no parent"))?,
            )?;
            let aside = if ordinary_directory(&session)? {
                let path =
                    session.with_file_name(format!("{}.replaced-{}", profile.id, random_hex(4)?));
                if path.try_exists()? {
                    return Err(RepoError::Message(
                        "Restore replacement path already exists".into(),
                    ));
                }
                fs::rename(&session, &path)?;
                Some(path)
            } else {
                None
            };
            if let Err(error) = staging.transfer(&session) {
                return Err(undo_session(
                    &session,
                    aside.as_deref(),
                    false,
                    error.into(),
                ));
            }
            match self.save(profile, now) {
                Ok(saved) => Ok((saved, aside)),
                Err(error) => Err(undo_session(&session, aside.as_deref(), true, error)),
            }
        })();
        let cleanup = staging.cleanup();
        let (saved, aside) = match result {
            Ok(value) => {
                cleanup?;
                value
            }
            Err(error) => {
                if let Err(cleanup) = cleanup {
                    return Err(RepoError::Message(format!(
                        "{error}; restore staging cleanup failed: {cleanup}"
                    )));
                }
                return Err(error);
            }
        };
        if let Some(aside) = aside
            && let Err(error) = fs::remove_dir_all(&aside)
        {
            eprintln!(
                "WARNING: {} was restored, but its previous session remains at {}: {error}",
                saved.id,
                aside.display()
            );
        }
        Ok(saved)
    }
}
