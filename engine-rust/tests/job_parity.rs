use numera_core::{
    browser::adapter::Adapter,
    jobs::{self, JobStore},
};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
fn durable_job_argument_and_outcome_policy_matches_source() -> Result<(), Box<dyn std::error::Error>>
{
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let root = common::TempDirectory::new("job-parity")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(python);
    command
        .args([
            "-u",
            "-c",
            r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from job_cases import cases,human_cases
result=cases(os.environ['FIXTURE_ROOT']);result['human']=human_cases()
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
"#,
        ])
        .env("PYTHONUTF8", "1")
        .env("FIXTURE_ROOT", root.path().join("source"))
        .env(
            "PYTHONPATH",
            std::env::join_paths([
                repository.join("scripts/migration"),
                repository.join("engine"),
            ])?,
        );
    let mut source = Adapter::spawn(
        root.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call("observe-jobs", Default::default(), Duration::from_secs(30))?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    for case in document["arguments"]
        .as_array()
        .ok_or("Job argv cases missing")?
    {
        let argv = case["argv"]
            .as_array()
            .ok_or("Job argv missing")?
            .iter()
            .map(|v| v.as_str().expect("Source argv text").to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            json!(jobs::child_argv(&argv, Path::new("fixture-root"))),
            case["expected"]
        );
    }
    for case in document["states"]
        .as_array()
        .ok_or("Job state cases missing")?
    {
        assert_eq!(
            jobs::decorate(
                case["record"]
                    .as_object()
                    .ok_or("Source record missing")?
                    .clone(),
                case["leader"] == true,
                case["survivors"] == true,
                1000.0
            )?,
            case["expected"]
        );
    }
    let native = root.path().join("native");
    std::fs::create_dir_all(native.join("jobs"))?;
    let store = JobStore::new(native.clone());
    let log = native.join("jobs/sample.log");
    let hex = document["log"].as_str().ok_or("Source log bytes missing")?;
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::write(&log, bytes)?;
    let record = json!({"id":"sample","command":"check","pid":null,"started":900.0,"log":log});
    for case in document["tails"]
        .as_array()
        .ok_or("Log tail cases missing")?
    {
        assert_eq!(
            json!(store.tail(&record, case["lines"].as_i64().ok_or("Line count missing")?)?),
            case["expected"]
        );
    }
    std::fs::write(
        native.join("jobs/sample.json"),
        serde_json::to_vec(&record)?,
    )?;
    store.record_exit("sample", 0, 1000.0)?;
    let mut registry = numera_core::cli::registry();
    registry.build();
    for case in document["commands"]
        .as_array()
        .ok_or("Job command cases missing")?
    {
        let args = case["argv"].as_array().ok_or("Job command argv missing")?;
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(
                args.iter()
                    .map(|v| v.as_str().expect("Source argument text")),
            ),
        )?;
        let values = numera_core::cli::reflection::values(&registry, &matches);
        let mut outcome = numera_core::cli::job_commands::run(
            &native,
            "fixture-host",
            values.as_object().ok_or("Job arguments missing")?,
            1000.0,
        )?;
        normalize_log(&mut outcome.payload);
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Source job exit missing")? as i32
        );
        assert_eq!(outcome.payload, case["payload"], "{}", case["argv"]);
    }
    assert!(!log.exists(), "Prune left its finished log");
    store.record_exit("fast", 1, 1000.0)?;
    std::fs::write(
        native.join("jobs/fast.json"),
        serde_json::to_vec(&json!({"id":"fast","command":"check","pid":null,"started":999.0}))?,
    )?;
    let record = store.get("fast", 1000.0)?.ok_or("Fast job missing")?;
    assert_eq!(record["state"], "failed");
    assert_eq!(record["exit"], 1);
    assert_eq!(record["running"], false);
    assert_eq!(
        store.listing(1000.0)?.len(),
        1,
        "Sibling outcome was discovered as a second job"
    );
    assert!(store.record("../foreign").is_err());
    assert!(store.get("missing", 1000.0)?.is_none());
    store.record_exit("fast", 0, 1001.0)?;
    assert_eq!(
        store.get("fast", 1001.0)?.ok_or("Completed job missing")?["state"],
        "done"
    );
    for state in ["done", "failed", "stopped"] {
        let record = json!({"id":"uncertain","pid":777,"state":state});
        let observed = jobs::observed(
            record.as_object().ok_or("Job object missing")?.clone(),
            Err(std::io::Error::other("OS query denied")),
            Ok(false),
            1001.0,
        )?;
        assert_eq!(
            observed["running"], true,
            "Unknown {state} job became prunable"
        );
        assert_eq!(observed["orphaned"], true);
    }
    let sibling_log = native.join("jobs/sibling.log");
    std::fs::write(&sibling_log, b"running job output")?;
    let sibling = json!({"id":"sibling","pid":null,"started":1000.0,"log":sibling_log});
    std::fs::write(
        native.join("jobs/sibling.json"),
        serde_json::to_vec(&sibling)?,
    )?;
    for target in [native.join("jobs/sibling.json"), sibling_log.clone()] {
        let corrupt = json!({"id":"corrupt","pid":null,"state":"done","log":target});
        std::fs::write(
            native.join("jobs/corrupt.json"),
            serde_json::to_vec(&corrupt)?,
        )?;
        assert!(store.tail(&corrupt, 20).is_err());
        assert!(store.prune(1001.0).is_err());
        assert!(native.join("jobs/corrupt.json").exists());
        assert!(native.join("jobs/sibling.json").exists());
        assert_eq!(std::fs::read(&sibling_log)?, b"running job output");
    }
    std::fs::remove_file(native.join("jobs/corrupt.json"))?;
    let foreign = root.path().join("protected.log");
    std::fs::write(&foreign, b"preserve")?;
    let unsafe_record = json!({"id":"unsafe","pid":null,"state":"done","log":foreign});
    std::fs::write(
        native.join("jobs/unsafe.json"),
        serde_json::to_vec(&unsafe_record)?,
    )?;
    assert!(store.tail(&unsafe_record, 20).is_err());
    assert!(store.prune(1001.0).is_err());
    assert!(
        native.join("jobs/unsafe.json").exists(),
        "Invalid log path partially deleted its record"
    );
    assert_eq!(std::fs::read(&foreign)?, b"preserve");
    let unverified = json!({"id":"unverified","pid":777,"pidToken":null,"state":"running"});
    std::fs::write(
        native.join("jobs/unverified.json"),
        serde_json::to_vec(&unverified)?,
    )?;
    let error = store
        .stop("unverified", "fixture-host", 1001.0)
        .err()
        .ok_or("Unverified stop was permitted")?;
    assert!(error.to_string().contains("cannot be stopped safely"));
    assert!(
        !native.join("jobs/unverified.exit").exists(),
        "Refused stop invented an outcome"
    );
    let human = document["human"]
        .as_array()
        .ok_or("Human job output cases missing")?;
    assert_eq!(human.len(), 12);
    let mut registry = numera_core::cli::registry();
    registry.build();
    for case in human {
        let argv = std::iter::once("numera").chain(
            case["argv"]
                .as_array()
                .ok_or("Human job argv missing")?
                .iter()
                .map(|value| value.as_str().expect("Source argv text")),
        );
        let matches = registry.clone().try_get_matches_from(argv)?;
        let values = numera_core::cli::reflection::values(&registry, &matches);
        let mut output = numera_core::cli::output::Output::new(
            Vec::new(),
            Vec::new(),
            false,
            case["tty"] == true,
        );
        assert!(numera_core::cli::job_render::render(
            &mut output,
            values.as_object().ok_or("Human job args missing")?,
            &case["payload"]
        )?);
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"].as_str().ok_or("Human job output missing")?,
            "{}",
            case["argv"]
        );
        assert!(stderr.is_empty());
    }
    recorded_stop(root.path())?;
    root.cleanup()?;
    Ok(())
}
fn recorded_stop(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::{identity, recorded_tree, tree::OwnedTree};
    use std::{process::Stdio, time::Instant};
    let directory = root.join("stop");
    std::fs::create_dir_all(&directory)?;
    let ready = directory.join("ready");
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let mut command = Command::new(python);
    command
        .args([
            "-u",
            "-c",
            r#"
import os,pathlib,subprocess,sys,threading
subprocess.Popen([sys.executable,'-c','import threading; threading.Event().wait(15)'])
pathlib.Path(os.environ['FIXTURE_READY']).write_text('ready',encoding='utf-8')
threading.Event().wait(15)
"#,
        ])
        .env("FIXTURE_READY", &ready)
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut owner = OwnedTree::spawn(&mut command)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            if owner.child_mut().try_wait()?.is_some() {
                return Err("Owned job exited before readiness".into());
            }
            if Instant::now() >= deadline {
                return Err("Owned job readiness timed out".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut record = owner.record();
        let token = record["token"].as_u64().ok_or("Birth token missing")?;
        record["pidToken"] = token.into();
        record["id"] = "live".into();
        record["command"] = "fixture".into();
        record["started"] = 1000.0.into();
        assert_eq!(identity::started_at(owner.pid())?, Some(token));
        let mut reused = record.clone();
        reused["pidToken"] = (token ^ 1).into();
        // A named Windows job remains the exact same kernel capability even if
        // the leader slot changes. POSIX groups rely on the captured birth token.
        if record["tree"]["kind"] == "pgid" {
            assert!(!recorded_tree::owned(&reused)?);
            assert!(!recorded_tree::settle(&reused, Duration::from_millis(50))?);
            assert!(!owner.empty()?, "Reused identity terminated the real owner");
        }
        let store = JobStore::new(directory.clone());
        std::fs::create_dir_all(directory.join("jobs"))?;
        std::fs::write(
            directory.join("jobs/live.json"),
            serde_json::to_vec(&record)?,
        )?;
        assert_eq!(
            store.get("live", 1000.0)?.ok_or("Live job missing")?["running"],
            true
        );
        let stopped = store
            .stop("live", "fixture-host", 1001.0)?
            .ok_or("Stopped job missing")?;
        assert_eq!(stopped["state"], "stopped");
        assert_eq!(stopped["running"], false);
        assert!(owner.empty()?, "Recorded stop left a descendant");
        assert!(recorded_tree::empty(&record)?);
        Ok(())
    })();
    let cleanup = owner.settle(Duration::from_secs(5));
    cleanup?;
    result
}
fn normalize_log(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_log(value);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key == "log" {
                    if let Some(path) = value.as_str() {
                        *value = Path::new(path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .expect("Log basename")
                            .into();
                    }
                } else {
                    normalize_log(value);
                }
            }
        }
        _ => {}
    }
}
