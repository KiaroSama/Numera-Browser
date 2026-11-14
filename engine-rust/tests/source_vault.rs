//! One native test for every original vault source case, including each pytest
//! parameter expansion. Child workers re-execute the same case for real abrupt
//! exits; production hooks exist only with the test-support Cargo feature.
use numera_core::{
    conversion,
    models::Profile,
    proxy::Proxy,
    storage::{
        atomic::write_json, config::ConfigStore, dashboard::DashboardStore, pool::ProxyPool,
        repo::ProfileRepo,
    },
    vault::{
        self, completion,
        journal::{self, JournalState},
        recovery,
        testing::{self, Action, Event},
        transaction,
    },
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
mod common;
macro_rules! source_test {
    ($name:ident,$id:literal,$body:block) => {
        #[test]
        #[doc=$id]
        fn $name() {
            if worker() {
                return;
            }
            $body
        }
    };
}
const OLD: &str = "source-old-master";
const NEW: &str = "source-new-master";
const MASTER: &str = "master";
const VALUES: [&str; 3] = ["engine-secret", "dash-secret", "pool-secret"];
fn descriptor(password: &str, label: &str) -> Value {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let salt = STANDARD.encode(label.as_bytes());
    json!({"salt":salt,"verifier":vault::encrypt(password,&salt,"numera-vault-ok").expect("verifier")})
}
fn descriptors() -> &'static [Value; 3] {
    static DATA: OnceLock<[Value; 3]> = OnceLock::new();
    DATA.get_or_init(|| {
        [
            descriptor(NEW, "source-new-salt!"),
            descriptor(OLD, "source-old-salt!"),
            descriptor("foreign", "source-foreign!"),
        ]
    })
}
fn repo(root: &Path, password: Option<&str>) -> ProfileRepo {
    ProfileRepo::new(root.to_owned(), password.map(str::to_owned))
}
fn pool(root: &Path, password: Option<&str>) -> ProxyPool {
    ProxyPool::new(root.to_owned(), password.map(str::to_owned))
}
fn dash(root: &Path, password: Option<&str>) -> DashboardStore {
    DashboardStore::new(root.to_owned(), password.map(str::to_owned))
}
fn read(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).expect("stored bytes")).expect("stored JSON")
}
fn write(root: &Path, name: &str, value: Value) {
    write_json(&root.join(name), &value).expect("write owned test record");
}
fn config(root: &Path) -> Value {
    if root.join("config.json").exists() {
        read(root, "config.json")
    } else {
        json!({})
    }
}
fn active(root: &Path) -> Value {
    config(root)["vault"].clone()
}
fn raw_pair(root: &Path, id: &str) -> (Value, Value) {
    (
        read(root, &format!("profiles/{id}.json")),
        read(root, &format!("dashboard/{id}.json")),
    )
}
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, at: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        if !at.exists() {
            return;
        }
        for entry in fs::read_dir(at).expect("snapshot directory") {
            let path = entry.expect("snapshot entry").path();
            if path.is_dir() {
                visit(root, &path, out);
            } else if path.extension().is_some_and(|v| v == "json") {
                out.insert(
                    path.strip_prefix(root)
                        .expect("owned path")
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(&path).expect("snapshot bytes"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn model(name: &str, seed: i64, password: Option<&str>) -> Profile {
    let mut p = conversion::to_engine(
        &json!({"id":format!("source-{seed}"),"name":name,"os":"Windows","seed":seed}),
        1000.0,
    )
    .expect("profile");
    if let Some(password) = password {
        p.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","password":password}),
                false,
            )
            .expect("proxy"),
        );
    }
    p
}
fn saved(root: &Path, name: &str, seed: i64, password: Option<&str>) -> Profile {
    repo(root, None)
        .save(model(name, seed, password), 1000.0)
        .expect("save source profile")
}
fn secret(p: &Profile) -> Value {
    p.proxy
        .as_ref()
        .map_or(Value::Null, |p| p.to_record()["password"].clone())
}
fn pool_entry(host: &str, password: Value) -> Value {
    json!({"type":"HTTP","host":host,"port":8080,"user":"u","pass":password})
}
fn enable(root: &Path, password: &str) -> usize {
    transaction::enable(root, password).expect("enable vault")
}
fn assert_no_plain(root: &Path, password: &str) {
    let found = completion::problems(root, &active(root), Some(password)).expect("scan completion");
    assert!(found.is_empty(), "{found:?}");
}
fn minimal(root: &Path) {
    write(
        root,
        "profiles/p1.json",
        json!({"id":"p1","name":"one","proxy":{"server":"http://h:1","password":VALUES[0]}}),
    );
    write(
        root,
        "dashboard/p1.json",
        json!({"id":"p1","name":"one","proxy":{"host":"h","pass":VALUES[1]}}),
    );
    write(
        root,
        "proxies.json",
        json!([{"id":"pool","host":"h","pass":VALUES[2]}]),
    );
    write(root, "config.json", json!({}));
}
fn tokens(root: &Path) -> [Value; 3] {
    [
        read(root, "profiles/p1.json")["proxy"]["password"].clone(),
        read(root, "dashboard/p1.json")["proxy"]["pass"].clone(),
        read(root, "proxies.json")[0]["pass"].clone(),
    ]
}
fn reseal(root: &Path, password: &str, desc: &Value, plain_pool: bool) {
    let mut values = Vec::new();
    for (i, value) in VALUES.iter().enumerate() {
        values.push(if plain_pool && i == 2 {
            value.to_string()
        } else {
            vault::encrypt(password, desc["salt"].as_str().expect("salt"), value).expect("encrypt")
        });
    }
    for (name, node, field, index) in [
        ("profiles/p1.json", true, "password", 0),
        ("dashboard/p1.json", true, "pass", 1),
        ("proxies.json", false, "pass", 2),
    ] {
        let mut record = read(root, name);
        if node {
            record["proxy"][field] = values[index].clone().into();
        } else {
            record[0][field] = values[index].clone().into();
        }
        write(root, name, record);
    }
}
fn prove(root: &Path, password: &str, desc: &Value) {
    for (token, expected) in tokens(root).iter().zip(VALUES) {
        let token = token.as_str().expect("secret text");
        assert!(vault::is_envelope(token));
        assert_eq!(
            vault::decrypt(password, desc["salt"].as_str().expect("salt"), token)
                .expect("decrypt actual credential"),
            expected
        );
    }
}
fn journal(root: &Path, op: &str, new: &Value, old: &Value) {
    write(
        root,
        "vault-migration.json",
        json!({"version":1,"op":op,"new":new,"old":if op=="rotate"{old.clone()}else{Value::Null},"pid":null}),
    );
}
fn setup(root: &Path, op: &str, which: &str, generation: &str, plain: bool) {
    let [new, old, foreign] = descriptors();
    minimal(root);
    reseal(
        root,
        if generation == "old" { OLD } else { NEW },
        if generation == "old" { old } else { new },
        plain,
    );
    let value = match which {
        "new" => new.clone(),
        "old" => old.clone(),
        "foreign" => foreign.clone(),
        "bad-verifier" => {
            let mut v = new.clone();
            v["verifier"] = "damaged".into();
            v
        }
        "missing-verifier" => json!({"salt":new["salt"]}),
        "false" => json!(false),
        "list" => json!([]),
        "dict" => json!({}),
        "null" | "absent" => Value::Null,
        _ => panic!("unknown descriptor"),
    };
    write(
        root,
        "config.json",
        if which == "absent" {
            json!({})
        } else {
            json!({"vault":value})
        },
    );
    journal(root, op, new, old);
}
fn recover(root: &Path) -> Result<bool, recovery::RecoveryError> {
    recovery::recover(root, Some(NEW), Some(OLD))
}
fn code(error: &recovery::RecoveryError, wanted: &str) {
    assert!(error.codes.contains(&wanted), "{error:?}");
}
fn error_at(
    operation: &'static str,
    filename: &'static str,
    errno: i32,
) -> impl FnMut(&Event) -> io::Result<Action> {
    let mut fired = false;
    move |event| {
        if !fired
            && event.operation == operation
            && (filename.is_empty() || event.path.file_name().is_some_and(|s| s == filename))
        {
            fired = true;
            // Frozen IDs carry Linux errno spellings. Inject the equivalent
            // native fsync/FlushFileBuffers errors on each host.
            #[cfg(unix)]
            let native_errno = if operation == "fsync-file" && errno == 95 {
                libc::ENOTSUP
            } else {
                errno
            };
            #[cfg(not(unix))]
            let native_errno = if cfg!(windows) && operation == "fsync-file" {
                match errno {
                    22 => 87,
                    95 => 50,
                    other => other,
                }
            } else {
                errno
            };
            return Err(io::Error::from_raw_os_error(native_errno));
        }
        Ok(Action::Continue)
    }
}
fn assert_rollback_failure(error: &recovery::RecoveryError, path: &str, errno: i32) {
    use numera_core::storage::atomic::RollbackFailed;
    use std::error::Error;
    assert_eq!(error.kind, recovery::RecoveryErrorKind::RollbackFailed);
    let carried = error
        .source()
        .and_then(|e| e.downcast_ref::<RollbackFailed>())
        .expect("typed rollback source");
    assert!(std::ptr::eq(
        carried,
        error
            .rollback
            .as_ref()
            .expect("public rollback evidence")
            .as_ref()
    ));
    let primary = carried
        .cause
        .get_ref()
        .and_then(|e| e.downcast_ref::<recovery::RecoveryError>())
        .expect("original primary recovery failure");
    assert_eq!(primary.kind, recovery::RecoveryErrorKind::Io);
    assert_eq!(
        primary
            .source()
            .and_then(|e| e.downcast_ref::<io::Error>())
            .expect("original syscall failure")
            .raw_os_error(),
        Some(errno)
    );
    let failed = carried
        .failures
        .iter()
        .find(|(p, _)| p.ends_with(path))
        .expect("failed restore path retained");
    assert_eq!(failed.1.raw_os_error(), Some(errno));
    assert!(carried.source().is_some());
}
fn http(root: &Path, password: Option<&str>, method: &str, uri: &str, body: Value) -> (u16, Value) {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let app = numera_core::http::application::router(
                root.to_owned(),
                "",
                "",
                password.map(str::to_owned),
                "source-vault-host",
                BTreeMap::new(),
            )
            .await
            .expect("router");
            let response = app
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .body(if body.is_null() {
                            Body::empty()
                        } else {
                            Body::from(serde_json::to_vec(&body).expect("request JSON"))
                        })
                        .expect("request"),
                )
                .await
                .expect("response");
            let status = response.status().as_u16();
            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("response bytes");
            (
                status,
                serde_json::from_slice(&bytes).expect("response JSON"),
            )
        })
}
fn worker() -> bool {
    let Ok(text) = std::env::var("NUMERA_SOURCE_VAULT_TASK") else {
        return false;
    };
    let task: Value = serde_json::from_str(&text).expect("child task");
    let root = Path::new(task["root"].as_str().expect("child root"));
    let point = task["point"].as_str().expect("crash point").to_owned();
    testing::with_observer(
        move |event| {
            let name = event
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let parent = event
                .path
                .parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if event.operation == point
                || event.operation == "after-secret-write" && (name == point || parent == point)
            {
                std::process::exit(73);
            }
            Ok(Action::Continue)
        },
        || match task["action"].as_str().expect("child action") {
            "enable" => {
                transaction::enable(root, MASTER).expect("child enable");
            }
            "rotate" => {
                transaction::rotate(root, MASTER, NEW).expect("child rotate");
            }
            "recover-master" => {
                recovery::recover(root, Some(MASTER), None).expect("child recover enable");
            }
            "recover" => {
                recover(root).expect("child recovery");
            }
            _ => panic!("unknown worker action"),
        },
    );
    panic!("worker did not reach crash checkpoint")
}
fn crash(root: &Path, action: &str, point: &str) {
    use numera_core::ownership::capture;
    let name = std::thread::current()
        .name()
        .expect("reported native test name")
        .to_owned();
    let mut cmd =
        std::process::Command::new(std::env::current_exe().expect("native test executable"));
    cmd.args(["--exact", &name, "--nocapture", "--test-threads=1"])
        .env(
            "NUMERA_SOURCE_VAULT_TASK",
            json!({"root":root,"action":action,"point":point}).to_string(),
        );
    let outcome = capture::run(
        &mut cmd,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )
    .expect("owned crash worker");
    assert_eq!(
        outcome.exit,
        73,
        "child failed before checkpoint: {}",
        String::from_utf8_lossy(&outcome.stderr)
    );
}

fn cli<R>(
    root: &Path,
    password: Option<&str>,
    action: impl FnOnce(
        &numera_core::cli::profile_commands::Profiles<'_>,
        &numera_core::cli::pool_commands::Pool<'_>,
    ) -> R,
) -> R {
    let repository = repo(root, password);
    let config = ConfigStore::new(root.to_owned());
    let installed = BTreeMap::new();
    let profiles = numera_core::cli::profile_commands::Profiles {
        repo: &repository,
        config: &config,
        installed: &installed,
        host: "source-vault-host",
    };
    let store = pool(root, password);
    let commands = numera_core::cli::pool_commands::Pool {
        store: &store,
        profiles: &profiles,
    };
    action(&profiles, &commands)
}
fn populated(root: &Path) -> Profile {
    let engine = saved(root, "engine-one", 1, Some("ENGINE-SECRET"));
    dash(root,None).save(json!({"name":"dash-one","os":"Windows","proxy":{"type":"HTTP","host":"5.6.7.8","port":"8080","user":"u","pass":"DASHBOARD-SECRET"}}),Default::default(),None,None,1000.0).expect("dashboard secret");
    pool(root, None)
        .add_many(vec![
            json!({"type":"SOCKS5","host":"9.9.9.9","port":"1080","user":"u","pass":"POOL-SECRET"}),
        ])
        .expect("pool secret");
    engine
}
fn pool_vault(root: &Path) -> String {
    saved(root, "p1", 1, None);
    saved(root, "p2", 2, None);
    let rows = pool(root, None)
        .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
        .expect("pool");
    enable(root, MASTER);
    rows[0]["id"].as_str().expect("pool ID").to_owned()
}
fn added_http(root: &Path) -> String {
    let (status, result) = http(
        root,
        None,
        "POST",
        "/api/proxies",
        json!({"proxies":[pool_entry("h.example",json!("secret"))]}),
    );
    assert_eq!(status, 200, "{result}");
    result["proxies"][0]["id"]
        .as_str()
        .expect("added ID")
        .to_owned()
}
fn diverge(root: &Path, id: &str) {
    let mut record = read(root, &format!("dashboard/{id}.json"));
    record["proxy"]["pass"] = "dashboard".into();
    write(root, &format!("dashboard/{id}.json"), record);
}
fn listed(root: &Path, password: Option<&str>, id: &str) -> Value {
    let (status, result) = http(root, password, "GET", "/api/profiles", Value::Null);
    assert_eq!(status, 200, "{result}");
    result
        .as_array()
        .expect("profile list")
        .iter()
        .find(|p| p["id"] == id)
        .expect("listed profile")
        .clone()
}

fn leaked(root: &Path, needle: &str) -> Vec<PathBuf> {
    fn scan(at: &Path, needle: &str, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(at).expect("read owned root") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                scan(&path, needle, out);
            } else if fs::read_to_string(&path).is_ok_and(|s| s.contains(needle)) {
                out.push(path);
            }
        }
    }
    let mut hits = vec![];
    scan(root, needle, &mut hits);
    hits
}
// Hold a real migration after a selected file was sealed. A separately-started
// reader/writer must remain blocked on the root transaction until publication.
fn frozen(
    root: &Path,
    point: &str,
    rotate: bool,
    operation: impl FnOnce(&Path) -> Value + Send + 'static,
) -> Value {
    use std::sync::mpsc;
    let root_owned = root.to_owned();
    let point = point.to_owned();
    let (inside_tx, inside_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let migrator = std::thread::spawn(move || {
        let mut stopped = false;
        testing::with_observer(
            move |e| {
                if !stopped
                    && e.operation == "after-secret-write"
                    && (e.path.file_name().is_some_and(|s| s == point.as_str())
                        || e.path
                            .parent()
                            .and_then(Path::file_name)
                            .is_some_and(|s| s == point.as_str()))
                {
                    stopped = true;
                    inside_tx.send(()).expect("announce paused migration");
                    release_rx
                        .recv_timeout(Duration::from_secs(30))
                        .expect("release migration");
                }
                Ok(Action::Continue)
            },
            || {
                if rotate {
                    transaction::rotate(&root_owned, MASTER, NEW)
                } else {
                    transaction::enable(&root_owned, MASTER)
                }
            },
        )
        .expect("winning migration")
    });
    inside_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("migration reached actual credential write");
    let (start_tx, start_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let other_root = root.to_owned();
    let other = std::thread::spawn(move || {
        start_tx.send(()).expect("announce attempting boundary");
        let value = operation(&other_root);
        done_tx.send(()).expect("announce completed boundary");
        value
    });
    start_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("reader/writer attempting boundary");
    let blocked = done_rx.recv_timeout(Duration::from_millis(50)).is_err();
    release_tx.send(()).expect("release owner");
    migrator.join().expect("migration thread");
    let value = other.join().expect("reader/writer thread");
    assert!(blocked, "operation saw an uncommitted migration");
    value
}

#[path = "source_vault/vault_active_descriptor.rs"]
mod vault_active_descriptor;

#[path = "source_vault/vault_completion_everywhere.rs"]
mod vault_completion_everywhere;

#[path = "source_vault/vault_completion_invariant.rs"]
mod vault_completion_invariant;

#[path = "source_vault/vault_recovery_integrity.rs"]
mod vault_recovery_integrity;

#[path = "source_vault/vault_commit_protocol.rs"]
mod vault_commit_protocol;

#[path = "source_vault/vault_crash_recovery.rs"]
mod vault_crash_recovery;

#[path = "source_vault/vault_recovery_completeness.rs"]
mod vault_recovery_completeness;

#[path = "source_vault/vault.rs"]
mod vault_cases;

#[path = "source_vault/vault_dashboard.rs"]
mod vault_dashboard;

#[path = "source_vault/repo_secrets.rs"]
mod repo_secrets;

#[path = "source_vault/pooled_proxy_secret_intent.rs"]
mod pooled_proxy_secret_intent;

#[path = "source_vault/proxy_removal_intent.rs"]
mod proxy_removal_intent;

#[path = "source_vault/locked_cli_and_vault.rs"]
mod locked_cli_and_vault;

#[path = "source_vault/vault_covers_the_root.rs"]
mod vault_covers_the_root;

#[path = "source_vault/vault_rotation.rs"]
mod vault_rotation;

#[path = "source_vault/vault_propagation.rs"]
mod vault_propagation;

#[path = "source_vault/vault_read_isolation.rs"]
mod vault_read_isolation;

#[path = "source_vault/vault_transaction.rs"]
mod vault_transaction;
