use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NONCE: AtomicU64 = AtomicU64::new(0);

fn temporary(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("JSON path has no filename"))?;
    let mut tmp = name.to_os_string();
    tmp.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(path.with_file_name(tmp))
}

/// Atomic replacement never substitutes for the caller's read-modify-write lock.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_stream(path, |file| file.write_all(bytes))
}

/// Publish a completed, synced sibling file without buffering an entire archive.
pub fn write_stream(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = temporary(path)?;
    let mut owned = false;
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        owned = true;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        for attempt in 0..5 {
            match fs::rename(&tmp, path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied && attempt < 4 => {
                    std::thread::sleep(Duration::from_millis(20 * (attempt + 1)));
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::other("Atomic replacement exhausted"))
    })();
    if owned {
        match fs::remove_file(&tmp) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(cleanup) => {
                return Err(io::Error::other(match &result {
                    Err(primary) => format!(
                        "Atomic write failed: {primary}; owned temporary cleanup failed: {cleanup}"
                    ),
                    Ok(()) => format!("Owned temporary cleanup failed: {cleanup}"),
                }));
            }
        }
    }
    result
}

pub fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    let encoded = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    let mut bytes = Vec::with_capacity(encoded.len());
    // Source JSON escapes non-ASCII text; text-mode writes translate structural LF.
    for character in encoded.chars() {
        if cfg!(windows) && character == '\n' {
            bytes.extend_from_slice(b"\r\n");
        } else if character.is_ascii() && character != '\u{7f}' {
            bytes.push(character as u8);
        } else {
            crate::identity_script::write_ascii_fragment(&mut bytes, &character.to_string())?;
        }
    }
    write_bytes(path, &bytes)
}

/// Snapshot bytes, never a decoded approximation, so failed changes are reversible.
#[derive(Default)]
pub struct Batch {
    undo: Vec<(PathBuf, Option<Vec<u8>>)>,
}
impl Batch {
    pub fn remember(&mut self, path: &Path) -> io::Result<()> {
        let bytes = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        self.undo.push((path.to_owned(), bytes));
        Ok(())
    }
    pub fn write(&mut self, path: &Path, value: &Value) -> io::Result<()> {
        self.remember(path)?;
        write_json(path, value)
    }
    pub fn rollback(&mut self) -> Vec<(PathBuf, io::Error)> {
        let mut failures = Vec::new();
        for (path, bytes) in self.undo.drain(..).rev() {
            let result = match bytes {
                Some(bytes) => write_bytes(&path, &bytes),
                None => match fs::remove_file(&path) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    result => result,
                },
            };
            if let Err(error) = result {
                failures.push((path, error));
            }
        }
        failures
    }
}
