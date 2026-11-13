use super::completion::sites;
use std::{collections::BTreeSet, fs, io, path::Path};

fn tolerated(error: io::Error, directory: bool) -> io::Result<()> {
    // Rust exposes native Windows synchronization errors through ErrorKind;
    // Unix additionally preserves the original fsync errno compatibility.
    if matches!(
        error.kind(),
        io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
    ) {
        return Ok(());
    }
    #[cfg(unix)]
    if error.raw_os_error().is_some_and(|code| {
        [libc::EINVAL, libc::ENOTSUP, libc::EOPNOTSUPP].contains(&code)
            || directory && [libc::EACCES, libc::EBADF, libc::EPERM, libc::EISDIR].contains(&code)
    }) {
        return Ok(());
    }
    #[cfg(not(unix))]
    let _ = directory;
    Err(error)
}

pub fn sync_file(path: &Path) -> io::Result<()> {
    // Windows FlushFileBuffers requires write access, without truncation.
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| {
            #[cfg(feature = "test-support")]
            super::testing::event("fsync-file", path)?;
            file.sync_all()
        })
        .or_else(|error| tolerated(error, false))
}

pub fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)
            .and_then(|file| {
                #[cfg(feature = "test-support")]
                super::testing::event("fsync-directory", path)?;
                file.sync_all()
            })
            .or_else(|error| tolerated(error, true))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub fn sync_data(root: &Path) -> io::Result<()> {
    let mut directories = BTreeSet::new();
    for site in sites(root)? {
        sync_file(&site.path)?;
        if let Some(parent) = site.path.parent() {
            directories.insert(parent.to_owned());
        }
    }
    for directory in directories {
        sync_dir(&directory)?;
    }
    Ok(())
}

pub fn sync_config(root: &Path) -> io::Result<()> {
    let config = root.join("config.json");
    if config.exists() {
        sync_file(&config)?;
    }
    sync_dir(root)
}

pub fn end_journal(root: &Path) -> io::Result<()> {
    #[cfg(feature = "test-support")]
    super::testing::event("before-cleanup", &root.join("vault-migration.json"))?;
    match fs::remove_file(root.join("vault-migration.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    sync_dir(root)?;
    #[cfg(feature = "test-support")]
    super::testing::event("after-cleanup", &root.join("vault-migration.json"))?;
    Ok(())
}
