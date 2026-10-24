use super::{
    archive_paths::{Staging, random_hex},
    extension_package,
    extensions::{describe, extension_id},
    locks::DataLock,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct Installation {
    pub id: String,
    pub path: PathBuf,
    pub name: Value,
    pub reused: bool,
}

fn regular_directory(path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other(
            "Extension target must be a regular directory",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other(
                "Extension target must not be a reparse point",
            ));
        }
    }
    Ok(true)
}

fn outcome(id: String, path: PathBuf, reused: bool) -> io::Result<Installation> {
    let name = describe(&path)?.name;
    Ok(Installation {
        id,
        path,
        name,
        reused,
    })
}

/// The network adapter supplies only the package; native storage owns admission,
/// staging, validation, replacement and rollback. A failed download is not a write.
pub fn install(
    root: &Path,
    reference: &str,
    force: bool,
    download: impl FnOnce(&str) -> io::Result<Vec<u8>>,
) -> io::Result<Installation> {
    let id = extension_id(reference)?;
    let directory = root.join("extensions");
    fs::create_dir_all(&directory)?;
    if !regular_directory(&directory)? {
        return Err(io::Error::other("Extension root unavailable"));
    }
    let destination = directory.join(&id);
    {
        let lock = DataLock::transaction(root)?;
        let _guard = lock.acquire_configured()?;
        if regular_directory(&destination)? && destination.join("manifest.json").is_file() && !force
        {
            return outcome(id, destination, true);
        }
    }
    let package = download(&id)?;
    let mut staging = Staging::new(root, "extension-install-")?;
    let result = (|| {
        extension_package::unpack(&package, staging.path()?, true)?;
        if !staging.path()?.join("manifest.json").is_file() {
            return Err(io::Error::other(format!(
                "'{id}' unpacked without a manifest.json, so Chromium would refuse it."
            )));
        }
        let replaced = {
            let lock = DataLock::transaction(root)?;
            let _guard = lock.acquire_configured()?;
            let existing = regular_directory(&destination)?;
            if existing && destination.join("manifest.json").is_file() && !force {
                return outcome(id, destination, true);
            }
            let aside = if existing {
                let aside = root.join(format!("extension-{id}.replaced-{}", random_hex(4)?));
                if aside.try_exists()? {
                    return Err(io::Error::other(
                        "Extension replacement path already exists",
                    ));
                }
                fs::rename(&destination, &aside)?;
                Some(aside)
            } else {
                None
            };
            if let Err(error) = staging.transfer(&destination) {
                if let Some(aside) = &aside
                    && let Err(restore) = fs::rename(aside, &destination)
                {
                    return Err(io::Error::other(format!(
                        "Extension install failed: {error}; old installation remains at {} because rollback failed: {restore}",
                        aside.display()
                    )));
                }
                return Err(error);
            }
            aside
        };
        if let Some(replaced) = replaced
            && let Err(error) = fs::remove_dir_all(&replaced)
        {
            eprintln!(
                "WARNING: Extension installation committed; old files remain at {}: {error}",
                replaced.display()
            );
        }
        outcome(id, destination, false)
    })();
    if let Err(cleanup) = staging.cleanup() {
        return Err(io::Error::other(match result {
            Err(error) => format!("{error}; extension staging cleanup failed: {cleanup}"),
            Ok(_) => format!("Extension staging cleanup failed: {cleanup}"),
        }));
    }
    result
}

pub fn uninstall(root: &Path, reference: &str) -> io::Result<bool> {
    let id = extension_id(reference)?;
    let directory = root.join("extensions");
    if !regular_directory(&directory)? {
        return Ok(false);
    }
    let destination = directory.join(&id);
    let tomb = {
        let lock = DataLock::transaction(root)?;
        let _guard = lock.acquire_configured()?;
        if !regular_directory(&destination)? {
            return Ok(false);
        }
        let tomb = root.join(format!("extension-{id}.deleting-{}", random_hex(4)?));
        if tomb.try_exists()? {
            return Err(io::Error::other("Extension deletion path already exists"));
        }
        fs::rename(&destination, &tomb)?;
        tomb
    };
    if let Err(error) = fs::remove_dir_all(&tomb) {
        return Err(io::Error::other(format!(
            "Extension detached but files remain at {}: {error}",
            tomb.display()
        )));
    }
    Ok(true)
}
