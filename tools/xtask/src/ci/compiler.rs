//! Scoped compiler receipt projection and bounded diagnostic cache sanitization.
//! Raw rustc tracing is never published; timing covers observed receipts only.
use super::{Result, fail};
use numera_core::ownership::file_stat::{self, Stat};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    time::Instant,
};
pub const TARGET: &str = "rustc_codegen_ssa::back::link";
pub const LINE_BYTES: usize = 65536;
#[derive(Clone, Copy)]
pub struct Limits {
    pub bytes: u64,
    pub total: u64,
    pub line: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 2 * 1024 * 1024,
            total: 32 * 1024 * 1024,
            line: LINE_BYTES,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
    pub size: u64,
    pub modified: i128,
    pub changed: i128,
    pub links: u64,
}
pub fn identity(stat: &Stat, platform: &str) -> Identity {
    Identity {
        device: stat.device,
        inode: stat.inode,
        size: stat.size,
        modified: stat.modified_ns,
        changed: if platform == "win32" {
            stat.birth_ns.unwrap_or(stat.changed_ns)
        } else {
            stat.changed_ns
        },
        links: stat.links,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheError {
    pub cause: &'static str,
    pub errno: u32,
    pub winerror: u32,
    pub kind: Option<io::ErrorKind>,
}
impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Compiler diagnostic cache could not be sanitized")
    }
}
impl std::error::Error for CacheError {}
impl CacheError {
    pub fn new(cause: &'static str) -> Self {
        Self {
            cause,
            errno: 0,
            winerror: 0,
            kind: None,
        }
    }
    pub fn codes(cause: &'static str, errno: &Value, winerror: &Value) -> Self {
        let code = |v: &Value| {
            v.as_u64()
                .filter(|v| *v <= u32::MAX as u64)
                .map_or(0, |v| v as u32)
        };
        Self {
            cause,
            errno: code(errno),
            winerror: code(winerror),
            kind: None,
        }
    }
}
impl From<io::Error> for CacheError {
    fn from(error: io::Error) -> Self {
        let code = error
            .raw_os_error()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        Self {
            cause: "os",
            errno: if cfg!(windows) { 0 } else { code },
            winerror: if cfg!(windows) { code } else { 0 },
            kind: Some(error.kind()),
        }
    }
}
type CacheResult<T> = std::result::Result<T, CacheError>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Read,
    Write,
    Flush,
    Replace,
}
/// Typed hooks surround actual filesystem boundaries; the default implementation
/// always executes the real read/write/sync/replace operations.
pub trait Access {
    fn before(&mut self, _step: Step, _path: &Path) -> CacheResult<()> {
        Ok(())
    }
    fn path(&mut self, path: &Path) -> CacheResult<Stat> {
        Ok(file_stat::path(path)?)
    }
    fn file(&mut self, file: &File) -> CacheResult<Stat> {
        Ok(file_stat::file(file)?)
    }
}
pub struct Native;
impl Access for Native {}
pub type Seen = BTreeMap<PathBuf, Identity>;
fn regular<A: Access>(access: &mut A, path: &Path, directory: bool) -> CacheResult<Stat> {
    let stat = access.path(path)?;
    if stat.reparse
        || if directory {
            !stat.directory
        } else {
            !stat.regular || stat.links != 1
        }
    {
        return Err(CacheError::new("ownership"));
    }
    Ok(stat)
}
fn json(bytes: &[u8]) -> CacheResult<Value> {
    super::decode(bytes).map_err(|error| {
        let text = error.to_string();
        CacheError::new(if text.contains("Duplicate compiler field") {
            "duplicate"
        } else if text.contains("Invalid compiler number") || text.contains("number out of range") {
            "number"
        } else if text.contains("recursion limit") {
            "recursion"
        } else {
            "json"
        })
    })
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn replace<A: Access>(
    path: &Path,
    bytes: &[u8],
    before: &Stat,
    access: &mut A,
    platform: &str,
) -> CacheResult<()> {
    let parent = path.parent().ok_or_else(|| CacheError::new("ownership"))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| CacheError::new("validation"))?
        .as_nanos();
    let temporary =
        Temporary(parent.join(format!("compiler-phase-{}-{nonce}", std::process::id())));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary.0)?;
    access.before(Step::Write, path)?;
    file.write_all(bytes)?;
    access.before(Step::Flush, path)?;
    file.flush()?;
    file.sync_all()?;
    file.seek(SeekFrom::Start(0))?;
    let mut check = Vec::new();
    Read::by_ref(&mut file)
        .take(bytes.len() as u64 + 1)
        .read_to_end(&mut check)?;
    if check != bytes {
        return Err(CacheError::new("readback"));
    }
    drop(file);
    if identity(&regular(access, path, false)?, platform) != identity(before, platform) {
        return Err(CacheError::new("identity"));
    }
    access.before(Step::Replace, path)?;
    fs::rename(&temporary.0, path)?;
    Ok(())
}
pub fn sanitize(root: &Path, seen: &mut Seen) -> CacheResult<u64> {
    sanitize_with(
        root,
        seen,
        &mut Native,
        Limits::default(),
        if cfg!(windows) {
            "win32"
        } else {
            std::env::consts::OS
        },
    )
}
pub fn sanitize_with<A: Access>(
    root: &Path,
    seen: &mut Seen,
    access: &mut A,
    limits: Limits,
    platform: &str,
) -> CacheResult<u64> {
    let mut parent = root.to_owned();
    for part in ["engine-rust", "target", "debug", ".fingerprint"] {
        parent.push(part);
        match regular(access, &parent, true) {
            Ok(_) => (),
            Err(error) if error.kind == Some(io::ErrorKind::NotFound) => return Ok(0),
            Err(error) => return Err(error),
        }
    }
    let directory = regex::Regex::new(r"^[A-Za-z0-9_-]+-[0-9a-f]{16}$")
        .map_err(|_| CacheError::new("validation"))?;
    let member = regex::Regex::new(r"^output-[A-Za-z0-9_-]{1,160}$")
        .map_err(|_| CacheError::new("validation"))?;
    let mut removed = 0;
    let mut files = 0;
    let mut total = 0;
    for (number, entry) in fs::read_dir(&parent)?.enumerate() {
        if number >= 4096 {
            return Err(CacheError::new("directories"));
        }
        let entry = entry?;
        if !directory.is_match(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let path = entry.path();
        regular(access, &path, true)?;
        for (index, entry) in fs::read_dir(path)?.enumerate() {
            if index >= 512 {
                return Err(CacheError::new("members"));
            }
            let entry = entry?;
            if !member.is_match(&entry.file_name().to_string_lossy()) {
                continue;
            }
            files += 1;
            if files > 512 {
                return Err(CacheError::new("files"));
            }
            let path = entry.path();
            let before = regular(access, &path, false)?;
            if before.size > limits.bytes || before.size > limits.total - total {
                return Err(CacheError::new("bytes"));
            }
            if seen.get(&path) == Some(&identity(&before, platform)) {
                continue;
            }
            total += before.size;
            access.before(Step::Read, &path)?;
            let mut file = File::open(&path)?;
            if identity(&access.file(&file)?, platform) != identity(&before, platform) {
                return Err(CacheError::new("identity"));
            }
            let mut raw = Vec::new();
            Read::by_ref(&mut file)
                .take(limits.bytes + 1)
                .read_to_end(&mut raw)?;
            drop(file);
            if raw.len() as u64 != before.size {
                return Err(CacheError::new("size"));
            }
            let mut kept = Vec::new();
            let mut count = 0;
            for line in raw.split_inclusive(|byte| *byte == b'\n') {
                let text = std::str::from_utf8(line).map_err(|_| CacheError::new("encoding"))?;
                let event = if text.trim_start().starts_with('{') {
                    Some(json(line)?)
                } else {
                    None
                };
                if event
                    .as_ref()
                    .is_some_and(|v| v["target"] == TARGET && v["level"] == "INFO")
                {
                    count += 1;
                } else {
                    if line.len() > limits.line {
                        return Err(CacheError::new("line"));
                    }
                    kept.extend_from_slice(line);
                }
            }
            if count > 0 {
                replace(&path, &kept, &before, access, platform)?;
                removed += count;
            }
            let current = regular(access, &path, false)?;
            if count == 0 && identity(&current, platform) != identity(&before, platform) {
                return Err(CacheError::new("identity"));
            }
            seen.insert(path, identity(&current, platform));
        }
    }
    Ok(removed)
}
fn rust_path(text: &str) -> Result<String> {
    let quoted = text
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or("Invalid Rust path representation")?;
    let mut result = String::new();
    let mut chars = quoted.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            if ch == '"' {
                return fail("Invalid Rust path representation");
            }
            result.push(ch);
            continue;
        }
        let escaped = chars.next().ok_or("Invalid Rust path escape")?;
        result.push(match escaped {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '0' => '\0',
            'u' => {
                if chars.next() != Some('{') {
                    return fail("Invalid Rust scalar");
                }
                let mut digits = String::new();
                loop {
                    let digit = chars.next().ok_or("Invalid Rust scalar")?;
                    if digit == '}' {
                        break;
                    }
                    if digits.len() >= 6 || !digit.is_ascii_hexdigit() {
                        return fail("Invalid Rust scalar");
                    }
                    digits.push(digit);
                }
                char::from_u32(u32::from_str_radix(&digits, 16)?).ok_or("Invalid Rust scalar")?
            }
            _ => return fail("Invalid Rust path escape"),
        });
    }
    Ok(result)
}
pub struct Projection {
    manifest: String,
    archives: PathBuf,
    index: usize,
    archive: Option<String>,
    start: Option<u64>,
    end: Option<u64>,
    markers: usize,
    artifacts: usize,
    fresh: bool,
    invalid: bool,
    finished: bool,
    testing: bool,
    clock: Box<dyn FnMut() -> u64>,
    pub events: Vec<String>,
    pending: [Vec<u8>; 2],
    overflow: [bool; 2],
    pub received: bool,
}
impl Projection {
    pub fn new(root: &Path, index: usize) -> Self {
        let started = Instant::now();
        Self::with_clock(root, index, move || {
            u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
        })
    }
    pub fn with_clock(root: &Path, index: usize, clock: impl FnMut() -> u64 + 'static) -> Self {
        Self {
            manifest: root
                .join("engine-rust/Cargo.toml")
                .to_string_lossy()
                .into_owned(),
            archives: root.join("engine-rust/target/debug/deps"),
            index,
            archive: None,
            start: None,
            end: None,
            markers: 0,
            artifacts: 0,
            fresh: false,
            invalid: false,
            finished: false,
            testing: false,
            clock: Box::new(clock),
            events: Vec::new(),
            pending: Default::default(),
            overflow: [false; 2],
            received: false,
        }
    }
    pub fn line(&mut self, text: &str) {
        if self.finished || self.testing {
            self.events.push(text.into());
            return;
        }
        if text == "running 1 test" {
            self.testing = true;
            self.events.push(text.into());
            return;
        }
        if !text.trim_start().starts_with('{') {
            if text.starts_with("Owned ") || text.starts_with("Verification ") {
                self.events.push(text.into());
            }
            return;
        }
        if self.event(text).is_err() {
            self.invalid = true;
        }
    }
    fn event(&mut self, text: &str) -> Result<()> {
        if text.len() > LINE_BYTES {
            return fail("Compiler line bound exceeded");
        }
        let event = json(text.as_bytes())?;
        if !event.is_object() {
            return fail("Invalid compiler record");
        }
        if event["reason"] == "build-finished" {
            let success = event["success"]
                .as_bool()
                .ok_or("Invalid build completion")?;
            self.finished = true;
            self.invalid |= !success;
            return Ok(());
        }
        let target = &event["target"];
        if event["manifest_path"] != self.manifest
            || target["name"] != "numera_core"
            || target["kind"] != serde_json::json!(["lib"])
        {
            return Ok(());
        }
        let message = if event["reason"] == "compiler-message" {
            let message = &event["message"];
            if message["level"] != "INFO" || message["target"] != TARGET {
                return Ok(());
            }
            let Some(value) = message["fields"]["message"]
                .as_str()
                .and_then(|s| s.strip_prefix("preparing rlib to "))
            else {
                return Ok(());
            };
            let archive = rust_path(value)?;
            let path = Path::new(&archive);
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("Unexpected archive path")?;
            if path.parent() != Some(self.archives.as_path())
                || !regex::Regex::new(r"^libnumera_core-[0-9a-f]{16}\.rlib$")?.is_match(name)
            {
                return fail("Unexpected archive path");
            }
            self.markers += 1;
            if self.markers != 1 || self.artifacts > 0 {
                return fail("Archive event order differs");
            }
            self.archive = Some(archive);
            let elapsed = (self.clock)();
            self.start = Some(elapsed);
            format!("event=archive-start elapsed_ns={elapsed}")
        } else if event["reason"] == "compiler-artifact" {
            if event["profile"]["test"] != false {
                return Ok(());
            }
            self.artifacts += 1;
            if self.artifacts != 1 {
                return fail("Artifact evidence differs");
            }
            self.fresh = event["fresh"]
                .as_bool()
                .ok_or("Artifact freshness missing")?;
            if self.fresh {
                return Ok(());
            }
            let start = self.start.ok_or("Archive receipt missing")?;
            let filenames = event["filenames"]
                .as_array()
                .ok_or("Archive filenames missing")?;
            if filenames
                .iter()
                .filter(|name| name.as_str() == self.archive.as_deref())
                .count()
                != 1
            {
                return fail("Archive artifact differs");
            }
            let elapsed = (self.clock)();
            if elapsed < start {
                return fail("Compiler clock differs");
            }
            self.end = Some(elapsed);
            format!(
                "event=archive-complete elapsed_ns={elapsed} archive_to_artifact_ns={}",
                elapsed - start
            )
        } else {
            return Ok(());
        };
        self.events.push(format!(
            "Historical compiler index={} {message}",
            self.index
        ));
        Ok(())
    }
    pub fn status(&self) -> &'static str {
        if self.invalid || !self.finished || self.artifacts != 1 {
            "unavailable"
        } else if self.fresh {
            "fresh"
        } else if self.markers == 1 && self.end.is_some() {
            "complete"
        } else {
            "unavailable"
        }
    }
    pub fn chunk(&mut self, index: usize, bytes: &[u8]) {
        if index >= 2 {
            self.invalid = true;
            return;
        }
        self.received = true;
        for byte in bytes {
            if *byte == b'\n' {
                if !self.overflow[index] {
                    let pending = std::mem::take(&mut self.pending[index]);
                    match std::str::from_utf8(&pending) {
                        Ok(text) => self.line(text.trim_end_matches('\r')),
                        Err(_) => self.invalid = true,
                    }
                }
                self.pending[index].clear();
                self.overflow[index] = false;
            } else if !self.overflow[index] {
                if self.pending[index].len() >= LINE_BYTES {
                    self.pending[index].clear();
                    self.overflow[index] = true;
                    self.invalid = true;
                } else {
                    self.pending[index].push(*byte);
                }
            }
        }
    }
    fn drain(&mut self) {
        for index in 0..2 {
            if !self.pending[index].is_empty() && !self.overflow[index] {
                let pending = std::mem::take(&mut self.pending[index]);
                match std::str::from_utf8(&pending) {
                    Ok(text) => self.line(text),
                    Err(_) => self.invalid = true,
                }
            }
        }
    }
}
thread_local! {static CURRENT:RefCell<Option<Rc<RefCell<Projection>>>>=const{RefCell::new(None)};}
pub fn observe_current(index: usize, bytes: &[u8]) {
    CURRENT.with(|slot| {
        if let Some(current) = slot.borrow().as_ref() {
            current.borrow_mut().chunk(index, bytes);
        }
    })
}
pub fn active() -> bool {
    CURRENT.with(|slot| slot.borrow().is_some())
}
pub fn environment(parent: &BTreeMap<String, String>, enabled: bool) -> BTreeMap<String, String> {
    let mut value = parent.clone();
    if enabled {
        value.retain(|key, _| !key.starts_with("RUSTC_LOG"));
        value.extend([
            ("RUSTC_LOG".into(), format!("{TARGET}=info")),
            ("RUSTC_LOG_FORMAT_JSON".into(), "1".into()),
            ("RUSTC_LOG_COLOR".into(), "never".into()),
        ]);
    }
    value
}
pub fn configure(command: &mut Command, enabled: bool) {
    if !enabled {
        return;
    }
    let mut names = std::env::vars_os()
        .map(|(name, _)| name)
        .chain(command.get_envs().map(|(name, _)| name.to_owned()))
        .filter(|name| name.to_string_lossy().starts_with("RUSTC_LOG"))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    for name in names {
        command.env_remove(name);
    }
    command
        .env("RUSTC_LOG", format!("{TARGET}=info"))
        .env("RUSTC_LOG_FORMAT_JSON", "1")
        .env("RUSTC_LOG_COLOR", "never");
}
pub struct Scope {
    projection: Rc<RefCell<Projection>>,
    old: Option<Rc<RefCell<Projection>>>,
    enabled: bool,
}
impl Scope {
    pub fn new(root: &Path, index: usize, enabled: bool) -> Self {
        let projection = Rc::new(RefCell::new(Projection::new(root, index)));
        let old = if enabled {
            CURRENT.with(|slot| slot.replace(Some(projection.clone())))
        } else {
            None
        };
        Self {
            projection,
            old,
            enabled,
        }
    }
    pub fn with_projection(projection: Projection, enabled: bool) -> Self {
        let projection = Rc::new(RefCell::new(projection));
        let old = if enabled {
            CURRENT.with(|slot| slot.replace(Some(projection.clone())))
        } else {
            None
        };
        Self {
            projection,
            old,
            enabled,
        }
    }
    pub fn line(&self, text: &str) {
        if self.enabled {
            self.projection.borrow_mut().line(text);
        }
    }
    pub fn captured(&self, stdout: &[u8], stderr: &[u8]) {
        if self.enabled && !self.projection.borrow().received {
            // Buffered command output cannot establish receipt timing. Preserve
            // semantic libtest evidence while marking compiler timing unavailable.
            let mut projection = self.projection.borrow_mut();
            projection.invalid = true;
            for raw in [stdout, stderr] {
                for text in String::from_utf8_lossy(raw).lines() {
                    if projection.testing
                        || text == "running 1 test"
                        || text.starts_with("Owned ")
                        || text.starts_with("Verification ")
                    {
                        projection.line(text);
                    }
                }
            }
        }
    }
    pub fn for_each_line(&self, mut accept: impl FnMut(&str)) {
        self.projection.borrow_mut().drain();
        for line in &self.projection.borrow().events {
            accept(line);
        }
    }
    pub fn finish<T>(
        self,
        result: Result<T>,
        cleanup: CacheResult<u64>,
    ) -> (Result<T>, Vec<String>) {
        if !self.enabled {
            return (result, Vec::new());
        }
        let mut projection = self.projection.borrow_mut();
        projection.drain();
        let removed = match &cleanup {
            Ok(removed) => *removed,
            Err(error) => {
                let index = projection.index;
                projection.events.push(format!("Historical compiler index={index} status=cache-cleanup-failed cause={} errno={} winerror={}",error.cause,error.errno,error.winerror));
                0
            }
        };
        let index = projection.index;
        let status = projection.status();
        projection.events.push(format!("Historical compiler index={index} status={status} coverage=receipt-interval-not-exclusive-compiler cache_records_removed={removed}"));
        let events = std::mem::take(&mut projection.events);
        drop(projection);
        let result = match result {
            Err(original) => Err(original),
            Ok(value) => cleanup
                .map(|_| value)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>),
        };
        (result, events)
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if self.enabled {
            CURRENT.with(|slot| {
                slot.replace(self.old.take());
            });
        }
    }
}
pub fn cache_policy(steps: &Value) -> Result<()> {
    let caches = steps
        .as_array()
        .ok_or("Steps missing")?
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|name| name.starts_with("Swatinem/rust-cache@"))
        })
        .collect::<Vec<_>>();
    if caches.len() != 1 {
        return fail("Historical compiler diagnostics must not enter Windows caches");
    }
    let options = &caches[0]["with"];
    let legacy = options
        == &serde_json::json!({"workspaces":"engine-rust","save-if":"${{ runner.os != 'Windows' }}"});
    let modern = options.as_object().is_some_and(|v| v.len() == 3)
        && options["save-if"] == false
        && options["cache-workspace-crates"] == false
        && options["workspaces"].as_str().is_some_and(|paths| {
            paths.lines().collect::<std::collections::BTreeSet<_>>()
                == std::collections::BTreeSet::from(["engine-rust", "tools/xtask"])
        });
    if !legacy && !modern {
        return fail("Historical compiler diagnostics must not enter Windows caches");
    }
    Ok(())
}
