//! Retire only receipt capabilities nominated by completed native jobs.
use crate::{
    ownership::{recorded_tree, unit_report},
    storage::locks::DataLock,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const LIMIT: u64 = 1_048_576;
const SCAN_BYTES: u64 = 16 * LIMIT;
const SCAN_ENTRIES: usize = 4096;
const BUDGET: Duration = Duration::from_secs(2);

fn uncertain() -> io::Error {
    io::Error::other("Browser receipt cleanup lacks complete ownership evidence")
}

fn session_path(root: &Path, value: &Value) -> io::Result<PathBuf> {
    let path = Path::new(value.as_str().ok_or_else(uncertain)?);
    let directory = root.canonicalize()?.join("sessions");
    if path.parent() != Some(directory.as_path()) {
        return Err(uncertain());
    }
    let validated = unit_report::validate(path)?;
    if validated != path {
        return Err(uncertain());
    }
    Ok(validated)
}

/// The caller holds the job, root transaction and original receipt locks.
pub(super) fn collect(root: &Path, path: &Path) -> io::Result<BTreeSet<PathBuf>> {
    let report = unit_report::read(path)?;
    if !report.complete {
        return Err(uncertain());
    }
    let mut nominated = BTreeSet::new();
    for tree in report.trees {
        if !recorded_tree::empty(&tree)? {
            return Err(uncertain());
        }
        if let Some(value) = tree.get("browserUnits") {
            nominated.insert(session_path(root, value)?);
        }
    }
    Ok(nominated)
}

fn references(root: &Path, deadline: Instant) -> io::Result<BTreeSet<PathBuf>> {
    let mut referenced = BTreeSet::new();
    let mut entries = 0usize;
    let mut bytes = 0u64;
    for name in ["jobs", "sessions"] {
        let directory = root.join(name);
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(uncertain());
        }
        for entry in fs::read_dir(directory)? {
            if Instant::now() >= deadline {
                return Err(uncertain());
            }
            let path = entry?.path();
            let extension = path.extension().and_then(|value| value.to_str());
            let report = name == "jobs" && extension == Some("units");
            if !report
                && extension != Some("json")
                && !(name == "jobs" && extension == Some("exit"))
            {
                continue;
            }
            entries += 1;
            if entries > SCAN_ENTRIES {
                return Err(uncertain());
            }
            let metadata = fs::symlink_metadata(&path)?;
            bytes = bytes.saturating_add(metadata.len());
            if !metadata.is_file()
                || metadata.is_symlink()
                || metadata.len() > LIMIT
                || bytes > SCAN_BYTES
            {
                return Err(uncertain());
            }
            if report {
                let report = unit_report::read_before(&path, deadline)?;
                if !report.complete {
                    return Err(uncertain());
                }
                for tree in report.trees {
                    if let Some(value) = tree.get("browserUnits") {
                        referenced.insert(session_path(root, value)?);
                    }
                }
            } else {
                let mut data = Vec::new();
                fs::File::open(&path)?
                    .take(LIMIT + 1)
                    .read_to_end(&mut data)?;
                if data.len() as u64 > LIMIT {
                    return Err(uncertain());
                }
                let value: Value = serde_json::from_slice(&data).map_err(|_| uncertain())?;
                if !value.is_object() {
                    return Err(uncertain());
                }
                if let Some(value) = value.get("browserUnits") {
                    referenced.insert(session_path(root, value)?);
                }
                if name == "jobs" && value.get("units").is_some() {
                    let id = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .ok_or_else(uncertain)?;
                    unit_report::for_job(&value, root, id)?;
                }
            }
        }
    }
    Ok(referenced)
}

pub(super) fn retire(root: &Path, nominated: &BTreeSet<PathBuf>) -> io::Result<()> {
    if nominated.is_empty() {
        return Ok(());
    }
    let lock = DataLock::transaction(root)?;
    let Ok(_guard) = lock.acquire(BUDGET) else {
        return Ok(());
    };
    // Native session publication holds this transaction lock. Unknown metadata
    // retains every nominee; an absent reference is never inferred from damage.
    let Ok(referenced) = references(root, Instant::now() + BUDGET) else {
        return Ok(());
    };
    for path in nominated {
        if referenced.contains(path) {
            continue;
        }
        let deadline = Instant::now() + BUDGET;
        let Ok(lock) = DataLock::for_file(path) else {
            continue;
        };
        let Ok(_guard) = lock.acquire(deadline.saturating_duration_since(Instant::now())) else {
            continue;
        };
        if session_path(root, &Value::String(path.to_string_lossy().into_owned()))
            .as_ref()
            .ok()
            != Some(path)
            || !matches!(unit_report::empty_before(path, deadline), Ok(true))
            || Instant::now() >= deadline
        {
            continue;
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
