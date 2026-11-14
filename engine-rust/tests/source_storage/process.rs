use super::*;
fn marker(root: &Path, who: &str, stage: &str) -> PathBuf {
    root.join(format!("worker-{who}-{stage}"))
}
fn signal(root: &Path, who: &str, stage: &str) {
    fs::write(marker(root, who, stage), b"ready").expect("worker signal");
}
pub(super) fn wait(path: &Path) {
    let end = Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(
            Instant::now() < end,
            "worker rendezvous timed out: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}
pub(super) fn release(root: &Path, who: &str) {
    signal(root, who, "release");
}
pub(super) fn reached(root: &Path, who: &str, stage: &str) {
    wait(&marker(root, who, stage));
}
fn park(root: &Path, who: &str, stage: &str) {
    signal(root, who, stage);
    wait(&marker(root, who, "release"));
}
pub(super) fn start(
    test: &str,
    root: &Path,
    who: &str,
    task: &str,
    extra: &str,
) -> thread::JoinHandle<capture::Output> {
    let mut command = Command::new(std::env::current_exe().expect("test binary"));
    command
        .args(["--exact", test, "--nocapture"])
        .env("NUMERA_STORAGE_WORKER", task)
        .env("NUMERA_STORAGE_ROOT", root)
        .env("NUMERA_STORAGE_WHO", who)
        .env("NUMERA_STORAGE_EXTRA", extra);
    thread::spawn(move || {
        capture::run(
            &mut command,
            capture::Limits {
                wall: Duration::from_secs(30),
                idle: Duration::from_secs(30),
                bytes: 65536,
                cleanup: Duration::from_secs(5),
            },
        )
        .expect("owned worker capture")
    })
}
pub(super) fn output(handle: thread::JoinHandle<capture::Output>) -> capture::Output {
    let output = handle.join().expect("worker capture thread");
    assert_eq!(
        output.exit,
        0,
        "native worker failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
pub(super) fn finish(handle: thread::JoinHandle<capture::Output>) {
    output(handle);
}
pub(super) fn worker() {
    let Some(task) = std::env::var_os("NUMERA_STORAGE_WORKER") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("NUMERA_STORAGE_ROOT").expect("worker root"));
    let who = std::env::var("NUMERA_STORAGE_WHO").expect("worker identity");
    let extra = std::env::var("NUMERA_STORAGE_EXTRA").unwrap_or_default();
    let task = task.to_str().expect("worker task");
    match task {
        "delete-skip" => {
            let deleted = testing::with_observer(
                |event| {
                    Ok(if event.operation == "before-session-reclaim" {
                        Action::Skip
                    } else {
                        Action::Continue
                    })
                },
                || store(&root).delete(&extra, "source-storage-host", 1001.0),
            )
            .expect("logical delete");
            assert!(deleted);
        }
        "restore-skip" => {
            testing::with_observer(
                |event| {
                    Ok(if event.operation == "before-restore-reclaim" {
                        Action::Skip
                    } else {
                        Action::Continue
                    })
                },
                || store(&root).restore(Path::new(&extra), false, "source-storage-host", 1001.0),
            )
            .expect("committed restore");
        }
        "log-failure" => {
            let log =
                numera_core::runtime_log::RuntimeLog::new(Path::new(&extra), "source-storage");
            assert!(log.path().is_none());
        }
        "name-create" => {
            park(&root, &who, "ready");
            let mut p = profile("twin", if who == "first" { 1 } else { 2 });
            p.id = format!("native-worker-{who}");
            let result = store(&root).save(p, 1000.0);
            write(
                &root,
                &format!("worker-{who}-result.json"),
                json!({"won":result.is_ok()}),
            );
        }
        "claim" => {
            park(&root, &who, "ready");
            let won = ScheduleStore::new(root.clone())
                .claim_due(&extra, &who, 1000.0)
                .expect("claim due");
            write(
                &root,
                &format!("worker-{who}-result.json"),
                json!({"won":won}),
            );
        }
        "first-run" => {
            park(&root, &who, "ready");
            let result =
                numera_core::storage::first_run::run_if_needed(&root, &[PathBuf::from(extra)])
                    .expect("first run");
            write(
                &root,
                &format!("worker-{who}-result.json"),
                json!({"won":result.is_some_and(|s|!s.copied.is_empty())}),
            );
        }
        "dead-pid" => {
            write(
                &root,
                &format!("worker-{who}-result.json"),
                json!({"pid":std::process::id(),"token":process_identity::started_at(std::process::id()).expect("kernel birth").expect("live owner")}),
            );
            park(&root, &who, "ready");
        }
        "birth" => {
            let pid = extra.parse().expect("observed pid");
            write(
                &root,
                &format!("worker-{who}-result.json"),
                json!({"token":process_identity::started_at(pid).expect("kernel birth").expect("live owner")}),
            );
        }
        "edit-inside" => {
            store(&root)
                .edit(
                    &extra,
                    |p| {
                        park(&root, &who, "inside");
                        p.notes = "from the cli".into();
                        Ok(None)
                    },
                    1001.0,
                )
                .expect("edit inside transaction");
        }
        "edit-note" => {
            park(&root, &who, "ready");
            signal(&root, &who, "trying");
            edit_note(&root, &extra, "kept");
            signal(&root, &who, "edited");
        }
        "patch-tags" => {
            park(&root, &who, "ready");
            signal(&root, &who, "trying");
            dash(&root)
                .patch(
                    &extra,
                    |mut row| {
                        row["tags"] = json!(["from-the-api"]);
                        Ok((row, Default::default()))
                    },
                    None,
                    None,
                    1001.0,
                )
                .expect("patch tags");
        }
        "restore-before-fail" | "restore-transfer" => {
            let directory = root.clone();
            let name = who.clone();
            let fail = task == "restore-before-fail";
            let result = testing::with_observer(
                move |event| {
                    if event.operation
                        == if fail {
                            "before-restore-lease"
                        } else {
                            "before-restore-transfer"
                        }
                    {
                        park(&directory, &name, if fail { "staged" } else { "swapping" });
                    }
                    if fail && event.operation == "before-restore-transfer" {
                        return Err(io::Error::other("in use"));
                    }
                    Ok(Action::Continue)
                },
                || store(&root).restore(Path::new(&extra), false, "source-storage-host", 1001.0),
            );
            assert_eq!(result.is_err(), fail);
        }
        _ => panic!("unknown storage worker task {task}"),
    }
    std::process::exit(0);
}
