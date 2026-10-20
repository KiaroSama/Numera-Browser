use super::tree::{OwnedTree, record_empty};
use crate::storage::{atomic::write_json, locks::DataLock, paths::id_path};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub enum Claim {
    Absent,
    Unreadable,
    Record(Value),
}
#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Free,
    Busy,
    Unknown,
}
fn path(root: &Path, id: &str) -> io::Result<PathBuf> {
    id_path(&root.join("sessions"), id, ".json").map_err(io::Error::other)
}
pub fn read(root: &Path, id: &str) -> Claim {
    let path = match path(root, id) {
        Ok(path) => path,
        Err(_) => return Claim::Absent,
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Claim::Absent,
        Err(_) => return Claim::Unreadable,
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Claim::Record(value),
        _ => Claim::Unreadable,
    }
}
pub fn state(claim: &Claim, host: &str) -> State {
    let record = match claim {
        Claim::Absent => return State::Free,
        Claim::Unreadable => return State::Unknown,
        Claim::Record(v) => v,
    };
    if record.as_object().is_some_and(|v| v.is_empty()) {
        return State::Free;
    }
    if (crate::conversion::truth(&record["host"]) && record["host"] != host)
        || record["session"].as_str().is_none_or(str::is_empty)
    {
        return State::Unknown;
    }
    if record["pid"].as_u64().is_none() {
        return State::Unknown;
    }
    if !record["token"].is_null() && !(record["token"].is_number() || record["token"].is_string()) {
        return State::Unknown;
    }
    match record_empty(record) {
        Ok(true) => State::Free,
        Ok(false) => State::Busy,
        Err(_) => State::Unknown,
    }
}

pub fn describe(claim: &Claim, host: &str) -> String {
    let record = match claim {
        Claim::Unreadable => return "a session claim that cannot be read - repair or remove `sessions/<id>.json` by hand once you know no browser is using this profile".into(),
        Claim::Absent => return "no session".into(),
        Claim::Record(record) if record.as_object().is_some_and(|v| v.is_empty()) => return "no session".into(),
        Claim::Record(record) => record,
    };
    let what = record["what"]
        .as_str()
        .filter(|v| !v.is_empty())
        .unwrap_or("a session");
    let pid = record.get("pid").map_or("None".into(), |v| v.to_string());
    if state(claim, host) != State::Unknown {
        return format!("{what} started by pid {pid}, whose browser is still running");
    }
    if crate::conversion::truth(&record["host"]) && record["host"] != host {
        let other = record["host"]
            .as_str()
            .map_or_else(|| record["host"].to_string(), str::to_owned);
        return format!("{what} claimed by pid {pid} on {other}, which this process cannot verify");
    }
    format!(
        "{what} claimed by pid {pid} in a form this version cannot check - remove `sessions/<id>.json` by hand once you know no browser is using this profile"
    )
}

pub fn refuse_if_busy(root: &Path, id: &str, host: &str) -> io::Result<()> {
    let claim = read(root, id);
    if state(&claim, host) != State::Free {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            format!(
                "this profile's browser is still open: {}. Stop it first.",
                describe(&claim, host)
            ),
        ));
    }
    Ok(())
}

/// The caller holds the source-compatible session lease through this lifetime.
pub fn take(root: &Path, id: &str, tree: &OwnedTree, host: &str, what: &str) -> io::Result<String> {
    let lock = DataLock::transaction(root)?;
    let _guard = lock.acquire_configured()?;
    super::update_admission::assert_open_locked(root)?;
    if state(&read(root, id), host) != State::Free {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "This profile remains owned by a busy or unverifiable session; stop or repair it before opening",
        ));
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let session = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let mut record = tree.record();
    let fields = json!({"session":session,"host":host,"what":what,"started":SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_secs_f64()});
    record
        .as_object_mut()
        .ok_or_else(|| io::Error::other("Tree record invalid"))?
        .extend(
            fields
                .as_object()
                .ok_or_else(|| io::Error::other("Claim fields invalid"))?
                .clone(),
        );
    write_json(&path(root, id)?, &record)?;
    Ok(session)
}
pub fn clear(root: &Path, id: &str, host: &str) -> io::Result<bool> {
    let lock = DataLock::transaction(root)?;
    let _guard = lock.acquire_configured()?;
    let claim = read(root, id);
    if !matches!(&claim,Claim::Record(record) if !record.as_object().is_some_and(|v|v.is_empty()))
        || state(&claim, host) != State::Free
    {
        return Ok(false);
    }
    match fs::remove_file(path(root, id)?) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}
pub fn release(root: &Path, id: &str, session: &str, tree: &mut OwnedTree) -> io::Result<bool> {
    let lock = DataLock::transaction(root)?;
    let _guard = lock.acquire_configured()?;
    let Claim::Record(record) = read(root, id) else {
        return Ok(false);
    };
    if record["session"] != session || !tree.empty()? || !record_empty(&record)? {
        return Ok(false);
    }
    match fs::remove_file(path(root, id)?) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}
