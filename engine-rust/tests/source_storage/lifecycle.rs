use super::*;
fn library(directory: &Path, name: &str) -> PathBuf {
    let mut entries = fs::read_dir(directory)
        .expect("dependencies")
        .map(|e| e.expect("dependency").path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("lib{name}-")) && n.ends_with(".rlib"))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|p| {
        fs::metadata(p)
            .expect("library metadata")
            .modified()
            .expect("library time")
    });
    entries.pop().expect("linked native dependency")
}
pub(super) fn fixture() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let deps = std::env::current_exe()
                .expect("test")
                .parent()
                .expect("dependency directory")
                .to_owned();
            let folder = deps.join(format!("source-storage-fixture-{}", std::process::id()));
            fs::create_dir(&folder).expect("owned fixture folder");
            let binary = folder.join(if cfg!(windows) {
                "helper.exe"
            } else {
                "helper"
            });
            let mut command =
                Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
            command
                .arg("--edition=2024")
                .arg("--crate-name=source_storage_helper")
                .arg("-Cdebuginfo=0")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/source_storage/helper.rs"))
                .arg("--extern")
                .arg(format!(
                    "numera_core={}",
                    library(&deps, "numera_core").display()
                ))
                .arg("--extern")
                .arg(format!(
                    "serde_json={}",
                    library(&deps, "serde_json").display()
                ))
                .arg("-L")
                .arg(format!("dependency={}", deps.display()))
                .arg("-o")
                .arg(&binary);
            let result = capture::run(
                &mut command,
                capture::Limits {
                    wall: Duration::from_secs(60),
                    idle: Duration::from_secs(60),
                    bytes: 65536,
                    cleanup: Duration::from_secs(5),
                },
            )
            .expect("owned native compiler");
            assert_eq!(
                result.exit,
                0,
                "native fixture compilation: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            binary
        })
        .clone()
}
pub(super) fn runtime(root: &Path) -> numera_core::browser::runtime::Runtime {
    numera_core::browser::runtime::Runtime {
        adapter: fixture(),
        cwd: root.to_owned(),
        home: root.to_owned(),
        logs: root.join("logs"),
        country: None,
        relay: None,
        binary_options: Default::default(),
        registry: None,
        deployment: None,
    }
}
pub(super) fn scheduler(root: &Path) -> (numera_core::http::scheduler::Scheduler, Arc<Registry>) {
    let sessions = Arc::new(
        Registry::new(root.to_owned(), "source-storage-host".into(), None).expect("registry"),
    );
    let scheduler = numera_core::http::scheduler::Scheduler::new(
        root.to_owned(),
        None,
        "source-storage-host".into(),
        sessions.clone(),
        runtime(root),
    )
    .expect("scheduler");
    (scheduler, sessions)
}
pub(super) fn schedule(root: &Path, name: &str, seed: i64) -> (Profile, Value) {
    let p = saved(root, name, seed);
    let row = ScheduleStore::new(root.to_owned())
        .add(
            &p.id,
            json!({"everyHours":1,"minutes":1,"preset":"general"}),
            Some(0.0),
        )
        .expect("schedule");
    (p, row)
}
pub(super) fn row(root: &Path, id: &str) -> Value {
    ScheduleStore::new(root.to_owned())
        .get(id)
        .expect("get schedule")
        .expect("schedule")
}
pub(super) fn shutdown(registry: &Registry) {
    let errors = registry.shutdown(Duration::from_secs(10));
    assert!(errors.is_empty(), "owned shutdown: {errors:?}");
}
