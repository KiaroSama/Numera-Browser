use super::locks::DataLock;
use serde::Serialize;
use std::{
    fs::{self, File, FileTimes, OpenOptions},
    io,
    path::{Path, PathBuf},
};

const ITEMS: &[&str] = &[
    "profiles",
    "user-data",
    "dashboard",
    "extensions",
    "config.json",
    "dashboard-config.json",
    "proxies.json",
    "proxies.json.bak",
    "tokens.json",
    "schedules.json",
];

#[derive(Default, Serialize)]
pub struct Summary {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub copied: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub profiles: usize,
}

fn copy_file(source: &Path, destination: &Path) -> io::Result<()> {
    let metadata = fs::metadata(source)?;
    let mut input = File::open(source)?;
    // Exclusive creation is the final no-overwrite boundary, even if another
    // file-only writer does not participate in the root transaction.
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| {
        io::copy(&mut input, &mut output)?;
        output.set_times(
            FileTimes::new()
                .set_accessed(metadata.accessed()?)
                .set_modified(metadata.modified()?),
        )?;
        output.sync_all()?;
        drop(output);
        fs::set_permissions(destination, metadata.permissions())
    })();
    if result.is_err() {
        // This function created the file; a partial copy cannot become the
        // destination-wins input of the next retry.
        if let Err(cleanup) = fs::remove_file(destination) {
            return Err(io::Error::other(format!(
                "First-run copy failed; partial destination could not be removed: {cleanup}"
            )));
        }
    }
    result
}

fn merge_dir(source: &Path, destination: &Path, depth: usize) -> io::Result<(usize, usize)> {
    if depth > 128 {
        return Err(io::Error::other(
            "First-run source nesting exceeds the safe copy limit",
        ));
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let (mut added, mut kept) = (0, 0);
    for entry in entries {
        let name = entry.file_name();
        if name == ".locks" {
            continue;
        }
        if name == ".ignoreme" {
            return Err(io::Error::other(
                "First-run source contains a protected local-only entry; it was not copied",
            ));
        }
        let path = entry.path();
        if entry.file_type()?.is_symlink() {
            return Err(io::Error::other(
                "First-run source contains a symbolic link; copy requires explicit handling",
            ));
        }
        let target = destination.join(name);
        if target.try_exists()? {
            if fs::symlink_metadata(&target)?.is_symlink() {
                return Err(io::Error::other(
                    "First-run destination contains a symbolic link; it was not modified",
                ));
            }
            if path.is_dir() {
                let (a, k) = merge_dir(&path, &target, depth + 1)?;
                added += a;
                kept += k;
            } else {
                kept += 1;
            }
        } else if path.is_dir() {
            merge_dir(&path, &target, depth + 1)?;
            added += 1;
        } else {
            copy_file(&path, &target)?;
            added += 1;
        }
    }
    Ok((added, kept))
}

pub fn import_from(source: &Path, destination: &Path) -> io::Result<Summary> {
    fs::create_dir_all(destination)?;
    let mut summary = Summary {
        source: source.to_owned(),
        destination: destination.to_owned(),
        ..Summary::default()
    };
    for name in ITEMS {
        let src = source.join(name);
        let dst = destination.join(name);
        if !src.try_exists()? {
            continue;
        }
        let result = (|| -> io::Result<()> {
            if fs::symlink_metadata(&src)?.is_symlink() {
                return Err(io::Error::other("First-run source item is a symbolic link"));
            }
            if src.is_dir() {
                let (added, kept) = merge_dir(&src, &dst, 0)?;
                if added > 0 {
                    summary.copied.push(format!("{name} ({added})"));
                }
                if kept > 0 {
                    summary
                        .skipped
                        .push(format!("{name} ({kept} already here)"));
                }
            } else {
                let lock = DataLock::for_file(&dst)?;
                let _guard = lock.acquire_configured()?;
                if dst.try_exists()? {
                    summary.skipped.push((*name).into());
                } else {
                    copy_file(&src, &dst)?;
                    summary.copied.push((*name).into());
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            summary.failed.push(format!("{name}: {error}"));
        }
    }
    let profiles = destination.join("profiles");
    if profiles.is_dir() {
        for entry in fs::read_dir(profiles)? {
            if super::paths::json_filename(&entry?.path()) {
                summary.profiles += 1;
            }
        }
    }
    Ok(summary)
}

pub fn run_if_needed(
    destination: &Path,
    candidates: &[std::path::PathBuf],
) -> io::Result<Option<Summary>> {
    let marker = destination.join(".imported");
    if marker.try_exists()? {
        return Ok(None);
    }
    fs::create_dir_all(destination)?;
    let lock = DataLock::transaction(destination)?;
    let _guard = lock.acquire_configured()?;
    if marker.try_exists()? {
        return Ok(None);
    }
    let destination_identity = destination.canonicalize()?;
    let source = candidates.iter().find(|path| {
        path.canonicalize()
            .is_ok_and(|identity| identity != destination_identity)
            && (path.join("profiles").is_dir() || path.join("proxies.json").is_file())
    });
    let summary = source
        .map(|path| import_from(path, destination))
        .transpose()?;
    if summary
        .as_ref()
        .is_some_and(|summary| !summary.failed.is_empty())
    {
        return Ok(summary);
    }
    let text = source.map_or_else(
        || "nothing to import\n".into(),
        |source| format!("imported from {}\n", source.display()),
    );
    // Marker is last. A failed or partial import remains retryable.
    super::atomic::write_bytes(&marker, text.as_bytes())?;
    Ok(summary)
}
