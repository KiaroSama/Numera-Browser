//! Root-scoped updater exclusion shared with the retained Python engine.
use crate::storage::{
    atomic::write_json,
    locks::{DataLock, resolved},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};

const LIMIT: Duration = Duration::from_secs(30);
const MAX_RECORD: u64 = 65_536;

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
        let _guard = lock.acquire(LIMIT)?;
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
        let _guard = lock.acquire(LIMIT)?;
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
