//! One portable installation is shared by every engine data root.
use super::{files, manifest};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
    thread,
    time::{Duration, Instant},
};
fn open(root: &Path) -> io::Result<File> {
    let directory = root.join(".updates");
    match fs::symlink_metadata(&directory) {
        Ok(meta) if !meta.is_dir() || files::linked(&meta) => {
            return Err(manifest::error("Installation lease directory is unsafe"))
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&directory)?,
        Err(error) => return Err(error),
    }
    let path = directory.join("runtime.lock");
    if fs::symlink_metadata(&path).is_ok_and(|meta| !meta.is_file() || files::linked(&meta)) {
        return Err(manifest::error("Installation lease is not a regular file"));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}
pub fn shared(root: &Path) -> io::Result<File> {
    let file = open(root)?;
    file.try_lock_shared().map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "An update owns this installation",
        )
    })?;
    let marker = root.join(".updates/activation.json");
    match files::read(&marker, 4096) {
        Ok(bytes) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|_| manifest::error("Installation activation marker is damaged"))?;
            let nonce = std::env::var("NUMERA_UPDATE_ACTIVATION").unwrap_or_default();
            if value["schema"] != 1
                || nonce.len() != 64
                || value["nonce_sha256"].as_str()
                    != Some(manifest::digest(nonce.as_bytes()).as_str())
            {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "A recoverable update is activating this installation",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(file)
}
pub fn exclusive(root: &Path, budget: Duration) -> io::Result<File> {
    let file = open(root)?;
    let end = Instant::now() + budget;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Another engine or desktop still uses this installation",
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}
pub fn mark(root: &Path, nonce: &str) -> io::Result<()> {
    manifest::decode_hex::<32>(nonce)?;
    files::json(
        &root.join(".updates/activation.json"),
        &serde_json::json!({"schema":1,"nonce_sha256":manifest::digest(nonce.as_bytes())}),
    )
}
pub fn clear(root: &Path, nonce: &str) -> io::Result<()> {
    let path = root.join(".updates/activation.json");
    let value: serde_json::Value = serde_json::from_slice(&files::read(&path, 4096)?)
        .map_err(|_| manifest::error("Activation marker damaged"))?;
    if value["schema"] != 1 || value["nonce_sha256"] != manifest::digest(nonce.as_bytes()) {
        return Err(manifest::error(
            "Activation marker belongs to another update",
        ));
    }
    fs::remove_file(path)
}
