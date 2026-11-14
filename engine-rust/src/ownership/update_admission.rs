//! Root-scoped updater exclusion shared with the retained Python engine.
use crate::storage::{
    atomic::write_json,
    locks::{DataLock, resolved},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
use subtle::ConstantTimeEq;
const MAX_RECORD: u64 = 65_536;
const MAX_RECORDS: usize = 10_000;
pub const GATE: &str = ".updates/admission.json";

fn busy() -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        "An update has closed admission; finish update recovery before retrying",
    )
}

fn regular_directory(path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(busy());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(busy());
        }
    }
    Ok(true)
}

fn read_record(path: &Path) -> io::Result<Value> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_RECORD {
        return Err(busy());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(busy());
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err(busy());
    }
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(busy)
}
fn records(directory: &Path, job_sidecars: bool) -> io::Result<Vec<PathBuf>> {
    if !regular_directory(directory)? {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    let mut count = 0usize;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(busy());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(busy());
            }
        }
        count += 1;
        if count > MAX_RECORDS {
            return Err(busy());
        }
        match path.extension().and_then(|value| value.to_str()) {
            Some("json") => paths.push(path),
            Some("log" | "exit") if job_sidecars => {}
            Some("units") if job_sidecars => {
                // Native receipts are durable ownership too, even if their
                // command leader already exited. Never skip unfinished units.
                if !path.with_extension("json").is_file() || !super::unit_report::empty(&path)? {
                    return Err(busy());
                }
            }
            Some("launch" | "ready" | "cancel" | "go" | "admitted") if job_sidecars => {
                if !path.with_extension("json").is_file() {
                    return Err(busy());
                }
            }
            _ => return Err(busy()),
        }
    }
    paths.sort();
    Ok(paths)
}
fn valid_ticket(ticket: &str) -> bool {
    ticket.len() == 64
        && ticket
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn ticket_hash(ticket: &str) -> String {
    Sha256::digest(ticket.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn matches_ticket(record: &Value, state: &str, hash: &str) -> bool {
    record["schema"] == 1
        && record["state"] == state
        && record["ticket_sha256"]
            .as_str()
            .is_some_and(|expected| expected.as_bytes().ct_eq(hash.as_bytes()).unwrap_u8() == 1)
}
fn recovery_refusal(reason: &'static str) -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        format!(
            "update recovery startup refused: {reason}; finish update rollback/recovery before normal startup"
        ),
    )
}

/// Durable exclusion for an external updater. Only a matching, precommitted
/// ticket reopens admission; neither age nor a dead owner implies quiescence.
pub struct Admission {
    root: PathBuf,
    host: String,
}
impl Admission {
    pub fn new(root: &Path, host: &str) -> io::Result<Self> {
        if host.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Update host identity is missing",
            ));
        }
        Ok(Self {
            root: resolved(root)?,
            host: host.into(),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn gate(&self) -> PathBuf {
        self.root.join(GATE)
    }
    pub fn assert_open(&self) -> io::Result<()> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        assert_open_locked(&self.root)
    }
    fn idle(&self, local_busy: impl FnOnce() -> io::Result<bool>) -> io::Result<()> {
        if local_busy()? {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "active or uncertain owned work defers this update",
            ));
        }
        if !records(&self.root.join(".updates/work"), false)?.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "active or uncertain admitted work defers this update",
            ));
        }
        for path in records(&self.root.join("sessions"), false)? {
            let record = read_record(&path)?;
            if record["session"].as_str().is_none_or(str::is_empty)
                || super::claims::state(&super::claims::Claim::Record(record), &self.host)
                    != super::claims::State::Free
            {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "active or uncertain browser session defers this update",
                ));
            }
        }
        let jobs = crate::jobs::JobStore::new(self.root.clone());
        for path in records(&self.root.join("jobs"), true)? {
            let record = read_record(&path)?;
            if !super::recorded_tree::verified(&record)
                || !matches!(record["tree"]["kind"].as_str(), Some("pgid" | "job"))
                || record["pid"]
                    .as_u64()
                    .is_none_or(|pid| pid == 0 || pid > u64::from(u32::MAX))
            {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "uncertain job ownership defers this update",
                ));
            }
            let id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(busy)?;
            let current = jobs.get(id, now()?)?.ok_or_else(busy)?;
            if crate::conversion::truth(&current["running"])
                || !super::recorded_tree::empty(&record)?
            {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "active owned job defers this update",
                ));
            }
        }
        Ok(())
    }
    pub fn quiesce(
        &self,
        ticket: Option<&str>,
        local_busy: impl FnOnce() -> io::Result<bool>,
    ) -> io::Result<String> {
        let ticket = match ticket {
            Some(ticket) => ticket.to_owned(),
            None => {
                let mut bytes = [0u8; 32];
                getrandom::fill(&mut bytes).map_err(io::Error::other)?;
                bytes.iter().map(|byte| format!("{byte:02x}")).collect()
            }
        };
        if !valid_ticket(&ticket) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "invalid precommitted update recovery ticket",
            ));
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        assert_open_locked(&self.root)?;
        let pid = std::process::id();
        let record = json!({"schema":1,"state":"quiesced","ticket_sha256":ticket_hash(&ticket),"owner_pid":pid,"owner_token":super::identity::started_at(pid)?,"created_at":now()?});
        write_json(&self.gate(), &record)?;
        if let Err(error) = self.idle(local_busy) {
            if read_record(&self.gate())? != record {
                return Err(busy());
            }
            fs::remove_file(self.gate())?;
            crate::vault::durability::sync_dir(&self.root.join(".updates"))?;
            return Err(error);
        }
        crate::vault::durability::sync_file(&self.gate())?;
        crate::vault::durability::sync_dir(&self.root.join(".updates"))?;
        Ok(ticket)
    }
    pub fn resume(&self, ticket: &str) -> io::Result<()> {
        if !valid_ticket(ticket) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "invalid update recovery ticket",
            ));
        }
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        if !regular_directory(&self.root.join(".updates"))? {
            return Err(busy());
        }
        let hash = ticket_hash(ticket);
        let receipt = self.root.join(".updates/resumed.json");
        match fs::symlink_metadata(self.gate()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if !matches_ticket(&read_record(&receipt)?, "resumed", &hash) {
                    return Err(busy());
                }
                crate::vault::durability::sync_dir(&self.root.join(".updates"))?;
                return Ok(());
            }
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        if !matches_ticket(&read_record(&self.gate())?, "quiesced", &hash) {
            return Err(busy());
        }
        if let Ok(metadata) = fs::symlink_metadata(&receipt)
            && (metadata.file_type().is_symlink() || !metadata.is_file())
        {
            return Err(busy());
        }
        #[cfg(feature = "test-support")]
        crate::vault::testing::event("before-update-resume-receipt", &receipt)?;
        write_json(
            &receipt,
            &json!({"schema":1,"state":"resumed","ticket_sha256":hash}),
        )?;
        crate::vault::durability::sync_file(&receipt)?;
        fs::remove_file(self.gate())?;
        crate::vault::durability::sync_dir(&self.root.join(".updates"))
    }
    fn recovery_preflight(&self, first_import: bool, ticket: Option<&str>) -> io::Result<()> {
        let updates = self.root.join(".updates");
        if !regular_directory(&updates)
            .map_err(|_| recovery_refusal("the durable admission directory is unsafe"))?
        {
            return Err(recovery_refusal(
                "the durable admission directory is unsafe",
            ));
        }
        let ticket = ticket
            .filter(|ticket| valid_ticket(ticket))
            .ok_or_else(|| recovery_refusal("the matching private recovery ticket is required"))?;
        let gate = read_record(&self.gate())
            .map_err(|_| recovery_refusal("the durable admission gate cannot be read"))?;
        if !matches_ticket(&gate, "quiesced", &ticket_hash(ticket)) {
            return Err(recovery_refusal(
                "the recovery ticket does not match the durable admission gate",
            ));
        }
        for name in ["profiles", "user-data", "dashboard"] {
            if !regular_directory(&self.root.join(name)).map_err(|_| {
                recovery_refusal("a required existing data directory is missing or linked")
            })? {
                return Err(recovery_refusal(
                    "a required existing data directory is missing or linked",
                ));
            }
        }
        match fs::symlink_metadata(self.root.join("vault-migration.json")) {
            Ok(_) => {
                return Err(recovery_refusal(
                    "vault-migration.json requires an independent vault recovery",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(recovery_refusal("vault-migration.json cannot be observed")),
        }
        if first_import {
            let metadata = fs::symlink_metadata(self.root.join(".imported"))
                .map_err(|_| recovery_refusal("legacy import has not completed"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(recovery_refusal("legacy import has not completed"));
            }
        }
        if crate::http::auth_policy::read(&self.root)
            == crate::http::auth_policy::AuthPolicy::Absent
            && matches!(
                crate::http::tokens::TokenStore::new(self.root.clone()).state(),
                crate::http::tokens::TokenState::Tokens
                    | crate::http::tokens::TokenState::Unreadable
            )
        {
            return Err(recovery_refusal(
                "existing API tokens still require policy adoption",
            ));
        }
        Ok(())
    }
    /// Recovery constructors run under a read-only root transaction; ordinary
    /// startup reserves work before the first mutating constructor is reached.
    pub fn with_startup<T>(
        &self,
        first_import: bool,
        ticket: Option<&str>,
        operation: impl FnOnce(bool) -> io::Result<T>,
    ) -> io::Result<T> {
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        let closed = fs::symlink_metadata(self.gate()).is_ok()
            || fs::symlink_metadata(self.root.join(".updates"))
                .is_ok_and(|metadata| metadata.file_type().is_symlink());
        if closed {
            self.recovery_preflight(first_import, ticket)?;
            return operation(true);
        }
        let _reservation = Reservation::begin(&self.root, "application startup")?;
        operation(false)
    }
}
fn now() -> io::Result<f64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs_f64())
}

/// The caller must hold this root's DataLock::transaction through its decision.
/// Any gate bytes, including malformed data or a dangling link, close admission.
pub(crate) fn assert_open_locked(root: &Path) -> io::Result<()> {
    let updates = root.join(".updates");
    if !regular_directory(&updates)? {
        return Ok(());
    }
    regular_directory(&updates.join("work"))?;
    match fs::symlink_metadata(updates.join("admission.json")) {
        Ok(_) => Err(busy()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// A reservation exists before process birth and outlives asynchronous admission.
/// Unknown child cleanup retains the receipt; neither age nor owner death clears it.
pub struct Reservation {
    root: PathBuf,
    path: Option<PathBuf>,
    record: Value,
}
impl Reservation {
    pub fn begin(root: &Path, what: &'static str) -> io::Result<Self> {
        let root = resolved(root)?;
        let lock = DataLock::transaction(&root)?;
        let _guard = lock.acquire_configured()?;
        assert_open_locked(&root)?;
        let updates = root.join(".updates");
        if !regular_directory(&updates)? {
            fs::create_dir(&updates)?;
        }
        let work = updates.join("work");
        if !regular_directory(&work)? {
            fs::create_dir(&work)?;
        }
        let mut identity = [0u8; 16];
        getrandom::fill(&mut identity).map_err(io::Error::other)?;
        identity[6] = (identity[6] & 0x0f) | 0x40;
        identity[8] = (identity[8] & 0x3f) | 0x80;
        let id = identity
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = work.join(format!("{id}.json"));
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Admission identity already exists",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let pid = std::process::id();
        let token = super::identity::started_at(pid)?
            .ok_or_else(|| io::Error::other("Admission owner birth identity is unavailable"))?;
        let record = json!({"schema":1,"pid":pid,"token":token,"what":what});
        write_json(&path, &record)?;
        Ok(Self {
            root,
            path: Some(path),
            record,
        })
    }

    pub(crate) fn for_command(
        root: &Path,
        args: &serde_json::Map<String, Value>,
    ) -> io::Result<Option<Self>> {
        let command = args.get("command").and_then(Value::as_str);
        if command == Some("stop")
            || (command == Some("jobs")
                && args.get("jobs_command").and_then(Value::as_str) == Some("stop"))
        {
            return Ok(None);
        }
        Self::begin(root, "native-command").map(Some)
    }

    pub fn release(&mut self) -> io::Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        let lock = DataLock::transaction(&self.root)?;
        let _guard = lock.acquire_configured()?;
        if !regular_directory(&self.root.join(".updates"))?
            || !regular_directory(&self.root.join(".updates/work"))?
        {
            return Err(busy());
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_RECORD {
            return Err(busy());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(busy());
            }
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_RECORD + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_RECORD {
            return Err(busy());
        }
        let record: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if record != self.record {
            return Err(io::Error::other(
                "Admission ownership changed; recovery evidence was preserved",
            ));
        }
        fs::remove_file(path)?;
        self.path = None;
        Ok(())
    }

    /// Leave a durable blocker when the owner cannot prove its children settled.
    pub fn retain(&mut self) {
        self.path = None;
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.release().is_err() {
            eprintln!(
                "Update admission reservation could not be released; recovery evidence remains"
            );
        }
    }
}
