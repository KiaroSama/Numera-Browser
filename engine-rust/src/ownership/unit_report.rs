//! Versioned receipts keep interrupted trusted spawn transfers visibly pending.
use crate::storage::locks::DataLock;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
struct ParsedReport {
    report: Report,
    diagnostic: Option<Value>,
}
fn identity_hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
/// Bounded metadata from the same receipt parser used for settlement. Unknown
/// JSON fields, arbitrary names, paths and argument values are never copied.
pub fn diagnostic(path: &Path, budget: Duration, scope: Option<&str>) -> io::Result<Value> {
    let deadline = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| io::Error::other("Invalid ownership diagnostic budget"))?;
    read_parsed(path, scope, Some(deadline), true)?
        .diagnostic
        .ok_or_else(|| io::Error::other("Ownership diagnostic projection is missing"))
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
    read_scoped_deadline(path, scope, None)
}
fn read_scoped_deadline(
    path: &Path,
    scope: Option<&str>,
    deadline: Option<Instant>,
) -> io::Result<Report> {
    Ok(read_parsed(path, scope, deadline, false)?.report)
}
fn read_parsed(
    path: &Path,
    scope: Option<&str>,
    deadline: Option<Instant>,
    capture_diagnostic: bool,
) -> io::Result<ParsedReport> {
    let path = validate(path)?;
    let lock = DataLock::for_file(&path)?;
    let _guard = lock.acquire(deadline.map_or(Duration::from_secs(5), |deadline| {
        deadline.saturating_duration_since(Instant::now())
    }))?;
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
    let mut lines = 0u64;
    let mut header_events = 0u64;
    let mut invalid_headers = 0u64;
    let mut malformed_json = 0u64;
    let mut invalid_events = 0u64;
    let mut invalid_units = 0u64;
    let mut tails = 0u64;
    let mut pending_events = 0u64;
    let mut unit_events = 0u64;
    let mut cancelled_events = 0u64;
    for (number, line) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
        lines += 1;
        if !line.ends_with(b"\n") {
            damaged = true;
            tails += 1;
        }
        let value: Value = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(_) => {
                damaged = true;
                malformed_json += 1;
                continue;
            }
        };
        if value["event"] == "header" {
            header_events += 1;
            if number != 0 || header || value["version"].as_u64() != Some(2) {
                damaged = true;
                invalid_headers += 1;
            } else {
                header = true;
            }
            continue;
        }
        let Some(attempt) = value["attempt"].as_str().filter(|value| !value.is_empty()) else {
            damaged = true;
            invalid_events += 1;
            continue;
        };
        if !header {
            damaged = true;
            invalid_events += 1;
            continue;
        }
        match value["event"].as_str() {
            Some("pending") => {
                pending_events += 1;
                let attempt_scope = match value.get("scope") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(value)) if !value.is_empty() && value.len() <= 128 => {
                        Some(value.clone())
                    }
                    _ => {
                        damaged = true;
                        invalid_events += 1;
                        continue;
                    }
                };
                if resolved.contains(attempt)
                    || pending.insert(attempt.to_owned(), attempt_scope).is_some()
                {
                    damaged = true;
                    invalid_events += 1;
                }
            }
            Some("cancelled" | "unit") if pending.contains_key(attempt) => {
                if value["event"] == "unit" {
                    if validate_unit(&value).is_err() {
                        damaged = true;
                        invalid_units += 1;
                        continue;
                    }
                    unit_events += 1;
                    let unit = json!({"pid":value["pid"],"pidToken":value["pidToken"],"tree":value["tree"]});
                    if scope.is_none_or(|scope| {
                        pending.get(attempt).and_then(Option::as_deref) == Some(scope)
                    }) && !trees.contains(&unit)
                    {
                        trees.push(unit);
                    }
                } else {
                    cancelled_events += 1;
                }
                pending.remove(attempt);
                resolved.insert(attempt.to_owned());
            }
            _ => {
                damaged = true;
                invalid_events += 1;
            }
        }
    }
    let complete = header
        && !damaged
        && !pending
            .values()
            .any(|pending_scope| scope.is_none_or(|scope| pending_scope.as_deref() == Some(scope)));
    let diagnostic = capture_diagnostic.then(|| {
    let pending_selected = pending
        .values()
        .filter(|pending_scope| scope.is_none_or(|scope| pending_scope.as_deref() == Some(scope)))
        .count();
    let pending_rows: Vec<_> = pending
        .iter()
        .take(32)
        .map(|(attempt, scope)| json!({"attempt_hash":identity_hash(attempt),"scope_hash":scope.as_deref().map(identity_hash)}))
        .collect();
    let tree_rows: Vec<_> = trees
        .iter()
        .take(32)
        .map(|tree| {
            let mut row =
                json!({"pid":tree["pid"],"birth":tree["pidToken"],"kind":tree["tree"]["kind"]});
            if tree["tree"]["kind"] == "pgid" {
                row["group"] = tree["tree"]["group"].clone();
            } else {
                row["group_hash"] = tree["tree"]["group"].as_str().map(identity_hash).into();
            }
            row
        })
        .collect();
    json!({
        "schema":1,"complete":complete,"header_valid":header,"damaged":damaged,
        "bytes":bytes.len(),"lines":lines,"header_events":header_events,
        "invalid_header_events":invalid_headers,"malformed_json_lines":malformed_json,
        "invalid_event_lines":invalid_events,"invalid_unit_events":invalid_units,
        "truncated_tail_lines":tails,"pending_events":pending_events,
        "unit_events":unit_events,"cancelled_events":cancelled_events,
        "resolved_attempts":resolved.len(),"pending_total":pending.len(),
        "pending_selected":pending_selected,"selected_scope_hash":scope.map(identity_hash),
        "pending":pending_rows,"omitted_pending":pending.len().saturating_sub(32),
        "tree_count":trees.len(),"trees":tree_rows,"omitted_trees":trees.len().saturating_sub(32),
        "kernel_state":"not_queried"
    })
    });
    Ok(ParsedReport {
        report: Report {
            trees,
            complete,
            problem: (!complete).then(|| {
                "Ownership report is missing, damaged or has an unfinished spawn transfer".into()
            }),
        },
        diagnostic,
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
    empty_scoped_deadline(path, scope, None)
}
fn empty_scoped_deadline(
    path: &Path,
    scope: Option<&str>,
    deadline: Option<Instant>,
) -> io::Result<bool> {
    let report = read_scoped_deadline(path, scope, deadline)?;
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
    settle_with(path, budget, scope, super::recorded_tree::settle)
}
/// Stop every known owned unit, then reread the receipt. A producer's cleanup
/// can publish a separately grouped SDK; its late record remains part of this
/// same bounded proof. Damaged/pending unchanged evidence is never retried into
/// a fabricated empty success.
pub fn settle_with(
    path: &Path,
    budget: Duration,
    scope: Option<&str>,
    mut settle: impl FnMut(&Value, Duration) -> io::Result<bool>,
) -> io::Result<bool> {
    let end = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| io::Error::other("Invalid ownership cleanup budget"))?;
    let mut report = read_scoped_deadline(path, scope, Some(end))?;
    loop {
        if Instant::now() >= end {
            return Ok(false);
        }
        for tree in &report.trees {
            if !settle(tree, end.saturating_duration_since(Instant::now()))? {
                return Ok(false);
            }
        }
        let latest = read_scoped_deadline(path, scope, Some(end))?;
        if latest.trees != report.trees {
            report = latest;
            continue;
        }
        if !latest.complete {
            return Ok(false);
        }
        if empty_scoped_deadline(path, scope, Some(end))? {
            return Ok(true);
        }
        let reread = read_scoped_deadline(path, scope, Some(end))?;
        if reread.trees == latest.trees {
            return Ok(false);
        }
        report = reread;
    }
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
        append(
            &path,
            &json!({"event":"pending","attempt":"a","argv":["DIAGNOSTIC-PRIVATE-ARG"],"path":"DIAGNOSTIC-PRIVATE-PATH"}),
        )?;
        assert!(!read(&path)?.complete);
        assert!(!empty(&path)?);
        let observed = diagnostic(&path, Duration::ZERO, None)?;
        assert_eq!(observed["pending_total"], 1);
        assert_eq!(observed["pending_selected"], 1);
        assert_eq!(observed["malformed_json_lines"], 0);
        assert_eq!(observed["header_valid"], true);
        assert_eq!(observed["kernel_state"], "not_queried");
        let serialized = observed.to_string();
        assert!(!serialized.contains("DIAGNOSTIC-PRIVATE-ARG"));
        assert!(!serialized.contains("DIAGNOSTIC-PRIVATE-PATH"));
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
        let scoped = diagnostic(&path, Duration::ZERO, Some("call"))?;
        assert_eq!(scoped["complete"], true);
        assert_eq!(scoped["pending_total"], 1);
        assert_eq!(scoped["pending_selected"], 0);
        assert_eq!(scoped["resolved_attempts"], 2);
        assert_eq!(
            scoped["selected_scope_hash"].as_str().map(str::len),
            Some(64)
        );
        assert!(!scoped.to_string().contains("\"call\""));
        assert!(
            !read(&path)?.complete,
            "Local scope hid its parent's pending transfer"
        );
        OpenOptions::new()
            .append(true)
            .open(&path)?
            .write_all(b"{")?;
        assert!(!read(&path)?.complete);
        let tail = diagnostic(&path, Duration::ZERO, None)?;
        assert_eq!(tail["complete"], false);
        assert_eq!(tail["damaged"], true);
        assert_eq!(tail["truncated_tail_lines"], 1);
        assert_eq!(tail["malformed_json_lines"], 1);
        assert!(
            !read_scoped(&path, Some("call"))?.complete,
            "Scoped read hid global corruption"
        );
        temporary.cleanup()
    }
}
