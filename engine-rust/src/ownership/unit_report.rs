//! Versioned receipts keep interrupted trusted spawn transfers visibly pending.
use crate::storage::locks::DataLock;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const ENV: &str = "NUMERA_HELPER_UNITS";
pub const SCOPE_ENV: &str = "NUMERA_HELPER_SCOPE";
const LIMIT: u64 = 1_048_576;
pub struct Report {
    pub trees: Vec<Value>,
    pub complete: bool,
    pub problem: Option<String>,
}
fn report_path() -> io::Result<Option<PathBuf>> {
    std::env::var_os(ENV)
        .map(|path| validate(Path::new(&path)))
        .transpose()
}
pub fn validate(path: &Path) -> io::Result<PathBuf> {
    if path
        .extension()
        .is_none_or(|extension| extension != "units")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ownership report is not a unit receipt",
        ));
    }
    let id = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("Ownership report ID is not UTF-8"))?;
    crate::storage::paths::safe_id(id).map_err(io::Error::other)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Ownership report directory missing"))?;
    if parent.file_name().is_none_or(|name| name != "jobs")
        || fs::symlink_metadata(parent)?.is_symlink()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Ownership report is outside its job directory",
        ));
    }
    if fs::symlink_metadata(path)?.is_symlink() || !fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Ownership report is not a regular file",
        ));
    }
    Ok(parent.canonicalize()?.join(
        path.file_name()
            .ok_or_else(|| io::Error::other("Ownership report name missing"))?,
    ))
}
fn append(path: &Path, value: &Value) -> io::Result<()> {
    let path = validate(path)?;
    let lock = DataLock::for_file(&path)?;
    let _guard = lock.acquire(Duration::from_secs(5))?;
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new().append(true).open(&path)?;
    if file.metadata()?.len().saturating_add(bytes.len() as u64) > LIMIT {
        return Err(io::Error::other("Ownership report exceeds its size limit"));
    }
    file.write_all(&bytes)?;
    file.sync_all()
}
pub fn initialize(path: &Path) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    drop(options.open(path)?);
    append(path, &json!({"version":2,"event":"header"}))
}
pub fn begin_spawn() -> io::Result<Option<String>> {
    let Some(path) = report_path()? else {
        return Ok(None);
    };
    let scope = std::env::var(SCOPE_ENV).ok();
    begin_scoped(&path, scope.as_deref()).map(Some)
}
pub fn begin_at(path: &Path) -> io::Result<String> {
    begin_scoped(path, None)
}
fn begin_scoped(path: &Path, scope: Option<&str>) -> io::Result<String> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce)
        .map_err(|_| io::Error::other("Ownership transfer identity unavailable"))?;
    let attempt = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    append(
        path,
        &json!({"event":"pending","attempt":attempt,"scope":scope}),
    )?;
    Ok(attempt)
}
pub fn cancel_spawn(attempt: Option<&str>) -> io::Result<()> {
    if let Some(attempt) = attempt {
        let path = report_path()?
            .ok_or_else(|| io::Error::other("Ownership report disappeared during transfer"))?;
        append(&path, &json!({"event":"cancelled","attempt":attempt}))?;
    }
    Ok(())
}
pub fn record(tree: &Value, attempt: Option<&str>) -> io::Result<()> {
    if let Some(attempt) = attempt {
        let path = report_path()?
            .ok_or_else(|| io::Error::other("Ownership report disappeared during transfer"))?;
        record_at(&path, tree, attempt)?;
    }
    Ok(())
}
pub fn record_at(path: &Path, tree: &Value, attempt: &str) -> io::Result<()> {
    let value = json!({"event":"unit","attempt":attempt,"pid":tree["pid"],
        "pidToken":tree["token"],"tree":tree["tree"]});
    validate_unit(&value)?;
    append(path, &value)
}
fn validate_unit(value: &Value) -> io::Result<()> {
    let pid = value["pid"]
        .as_u64()
        .filter(|pid| *pid > 0 && *pid <= u64::from(u32::MAX));
    let valid = match value["tree"]["kind"].as_str() {
        Some("pgid") => {
            pid.is_some()
                && value["tree"]["group"].as_u64() == pid
                && value["pidToken"].as_u64().is_some_and(|token| token > 0)
        }
        Some("job") => {
            pid.is_some()
                && value["tree"]["group"]
                    .as_str()
                    .is_some_and(|group| !group.trim().is_empty() && !group.contains('\0'))
                && (value["pidToken"].is_null()
                    || value["pidToken"].as_u64().is_some_and(|token| token > 0))
        }
        _ => false,
    };
    if !valid {
        return Err(io::Error::other(
            "Ownership unit has an invalid tree identity",
        ));
    }
    Ok(())
}
pub fn read(path: &Path) -> io::Result<Report> {
    read_scoped(path, None)
}
pub fn read_scoped(path: &Path, scope: Option<&str>) -> io::Result<Report> {
    let path = validate(path)?;
    let lock = DataLock::for_file(&path)?;
    let _guard = lock.acquire(Duration::from_secs(5))?;
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(io::Error::other("Ownership report exceeds its size limit"));
    }
    let mut trees = Vec::new();
    let mut pending = BTreeMap::new();
    let mut resolved = BTreeSet::new();
    let mut damaged = false;
    let mut header = false;
    for (number, line) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
        if !line.ends_with(b"\n") {
            damaged = true;
        }
        let value: Value = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(_) => {
                damaged = true;
                continue;
            }
        };
        if value["event"] == "header" {
            if number != 0 || header || value["version"].as_u64() != Some(2) {
                damaged = true;
            } else {
                header = true;
            }
            continue;
        }
        let Some(attempt) = value["attempt"].as_str().filter(|value| !value.is_empty()) else {
            damaged = true;
            continue;
        };
        if !header {
            damaged = true;
            continue;
        }
        match value["event"].as_str() {
            Some("pending") => {
                let attempt_scope = match value.get("scope") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(value)) if !value.is_empty() && value.len() <= 128 => {
                        Some(value.clone())
                    }
                    _ => {
                        damaged = true;
                        continue;
                    }
                };
                if resolved.contains(attempt)
                    || pending.insert(attempt.to_owned(), attempt_scope).is_some()
                {
                    damaged = true;
                }
            }
            Some("cancelled" | "unit") if pending.contains_key(attempt) => {
                if value["event"] == "unit" {
                    if validate_unit(&value).is_err() {
                        damaged = true;
                        continue;
                    }
                    let unit = json!({"pid":value["pid"],"pidToken":value["pidToken"],"tree":value["tree"]});
                    if scope.is_none_or(|scope| {
                        pending.get(attempt).and_then(Option::as_deref) == Some(scope)
                    }) && !trees.contains(&unit)
                    {
                        trees.push(unit);
                    }
                }
                pending.remove(attempt);
                resolved.insert(attempt.to_owned());
            }
            _ => damaged = true,
        }
    }
    let complete = header
        && !damaged
        && !pending
            .values()
            .any(|pending_scope| scope.is_none_or(|scope| pending_scope.as_deref() == Some(scope)));
    Ok(Report {
        trees,
        complete,
        problem: (!complete).then(|| {
            "Ownership report is missing, damaged or has an unfinished spawn transfer".into()
        }),
    })
}
pub fn for_job(record: &Value, root: &Path, id: &str) -> io::Result<Option<PathBuf>> {
    let Some(value) = record.get("units") else {
        return Ok(None);
    };
    let path = PathBuf::from(
        value
            .as_str()
            .ok_or_else(|| io::Error::other("Job ownership report path is not text"))?,
    );
    let expected = root.join("jobs").join(format!("{id}.units"));
    if validate(&path)? != validate(&expected)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Ownership report does not belong to this job",
        ));
    }
    Ok(Some(expected))
}
pub fn empty(path: &Path) -> io::Result<bool> {
    empty_scoped(path, None)
}
pub fn empty_scoped(path: &Path, scope: Option<&str>) -> io::Result<bool> {
    let report = read_scoped(path, scope)?;
    if !report.complete {
        return Ok(false);
    }
    for tree in report.trees {
        if !super::recorded_tree::empty(&tree)? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn settle(path: &Path, budget: Duration) -> io::Result<bool> {
    settle_scoped(path, budget, None)
}
pub fn settle_scoped(path: &Path, budget: Duration, scope: Option<&str>) -> io::Result<bool> {
    let end = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| io::Error::other("Invalid ownership cleanup budget"))?;
    let report = read_scoped(path, scope)?;
    for tree in report.trees {
        if !super::recorded_tree::settle(&tree, end.saturating_duration_since(Instant::now()))? {
            return Ok(false);
        }
    }
    empty_scoped(path, scope)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_pending_and_truncated_receipts_never_claim_empty() -> io::Result<()> {
        let temporary = crate::common::TempDirectory::new("unit-receipts")?;
        let jobs = temporary.path().join("jobs");
        fs::create_dir(&jobs)?;
        let path = jobs.join("fixture.units");
        initialize(&path)?;
        assert!(read(&path)?.complete);
        assert!(empty(&path)?);
        append(&path, &json!({"event":"pending","attempt":"a"}))?;
        assert!(!read(&path)?.complete);
        assert!(!empty(&path)?);
        append(&path, &json!({"event":"cancelled","attempt":"a"}))?;
        assert!(read(&path)?.complete);
        append(
            &path,
            &json!({"event":"pending","attempt":"ancestor","scope":null}),
        )?;
        append(
            &path,
            &json!({"event":"pending","attempt":"local","scope":"call"}),
        )?;
        assert!(!read_scoped(&path, Some("call"))?.complete);
        append(&path, &json!({"event":"cancelled","attempt":"local"}))?;
        assert!(read_scoped(&path, Some("call"))?.complete);
        assert!(
            !read(&path)?.complete,
            "Local scope hid its parent's pending transfer"
        );
        OpenOptions::new()
            .append(true)
            .open(&path)?
            .write_all(b"{")?;
        assert!(!read(&path)?.complete);
        assert!(
            !read_scoped(&path, Some("call"))?.complete,
            "Scoped read hid global corruption"
        );
        temporary.cleanup()
    }
}
