//! Each test names one frozen source acceptance case and calls the native
//! production boundary; development generators are not test dependencies.
use numera_core::ownership::{tree::OwnedTree, unit_report};
use numera_xtask::{
    acceptance::*,
    ci::{self, Result},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};
macro_rules! source_test {($name:ident,$id:literal,$body:block)=>{#[test]#[doc=$id]fn $name()->Result<()> $body};}
struct Temp {
    path: PathBuf,
}
impl Temp {
    fn new() -> io::Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-temp");
        fs::create_dir_all(&base)?;
        for _ in 0..32 {
            let path = base.join(format!(
                "acceptance-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other("Cannot create exclusive acceptance root"))
    }
    fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            eprintln!("Acceptance test cleanup failed: {error}");
        }
    }
}
struct Helper {
    executable: PathBuf,
    _directory: Temp,
}
fn helper() -> Result<Arc<Helper>> {
    static CACHE: OnceLock<Mutex<Weak<Helper>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(Weak::new()))
        .lock()
        .map_err(|_| "Native helper cache poisoned")?;
    if let Some(helper) = cache.upgrade() {
        return Ok(helper);
    }
    let directory = Temp::new()?;
    let executable = directory.path().join(if cfg!(windows) {
        "acceptance helper.exe"
    } else {
        "acceptance helper"
    });
    let dependencies = std::env::current_exe()?
        .parent()
        .ok_or("test dependencies directory missing")?
        .to_owned();
    let mut libraries = fs::read_dir(&dependencies)?.collect::<std::result::Result<Vec<_>, _>>()?;
    libraries.retain(|e| {
        e.file_name()
            .to_string_lossy()
            .starts_with("libnumera_core-")
            && e.path().extension().is_some_and(|x| x == "rlib")
    });
    libraries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    let library = libraries
        .last()
        .ok_or("native core library missing")?
        .path();
    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--edition=2024", "-Cdebuginfo=0", "--extern"])
        .arg(format!("numera_core={}", library.display()))
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/source_acceptance/helper.rs"))
        .arg("-o")
        .arg(&executable)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Rust acceptance fixture failed to compile: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let helper = Arc::new(Helper {
        executable,
        _directory: directory,
    });
    *cache = Arc::downgrade(&helper);
    Ok(helper)
}
fn command(helper: &Helper, mode: &str) -> Command {
    let mut command = Command::new(&helper.executable);
    command.arg(mode);
    for key in [
        "NUMERA_HELPER_UNITS",
        "NUMERA_HELPER_SCOPE",
        "NUMERA_UPDATE_TOKEN",
        "NUMERA_UPDATE_TICKET",
        "NUMERA_UPDATE_ACTIVATION",
        "NUMERA_NATIVE_SUPERVISE",
    ] {
        command.env_remove(key);
    }
    command
}
fn receipt(root: &Path, name: &str) -> Result<PathBuf> {
    fs::create_dir_all(root.join("jobs"))?;
    let path = root.join("jobs").join(format!("{name}.units"));
    unit_report::initialize(&path)?;
    Ok(path)
}
fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}
#[path = "source_acceptance/cleanup.rs"]
mod cleanup;
#[path = "source_acceptance/entry.rs"]
mod entry;
#[path = "source_acceptance/http.rs"]
mod http;
#[path = "source_acceptance/live.rs"]
mod live;
#[path = "source_acceptance/replay.rs"]
mod replay;
