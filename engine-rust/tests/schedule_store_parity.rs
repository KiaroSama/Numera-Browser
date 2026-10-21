use numera_core::storage::schedules::{ScheduleStore, due_schedules, next_run_at};
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn schedule_text_admission_matches_original_without_replacing_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::RepoError};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("schedule-text-admission")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys,uuid
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.scheduler import ScheduleStore
from run_logging import run_log
uuid.uuid4=lambda:uuid.UUID('a'*32)
entry={'id':'fixture','profileId':'profile','everyHours':1,'minutes':5,
       'preset':'general','enabled':True,'lastRun':None,'createdAt':0,
       'future':{'kept':['日本語',True]}}
raws=[bytes([255]),bytes([0xc3,0x28]),bytes([0xe2,0x82]),b'[]',b'{broken',None,
      json.dumps([entry]).encode('utf-8')]
operations=['list','get','add','update','delete','claim','release','record']
jobs=[(operation,raw) for operation in operations for raw in raws]
jobs += [('invalid-update',raw) for raw in raws[:3]]
observed=[]
with run_log('schedule-text-admission-oracle',parent/'logs') as log:
    for operation,raw in jobs:
        root=parent/str(len(observed));root.mkdir(parents=True)
        store=ScheduleStore(root)
        if raw is not None:store.path.write_bytes(raw)
        try:
            if operation=='list':result=store.list()
            elif operation=='get':result=store.get('fixture')
            elif operation=='add':result=store.add(profile_id='added-profile',every_hours=1,now=1000.0)
            elif operation=='update':result=store.update('fixture',{'enabled':False})
            elif operation=='delete':result=store.delete('fixture')
            elif operation=='claim':result=store.claim_due('fixture','fixture-worker',1000.0)
            elif operation=='release':result=store.release('fixture')
            elif operation=='record':result=store.record('fixture',lastRun=1000.0,lastOutcome='done')
            else:result=store.update('fixture',{'everyHours':-1})
            error=None
        except (UnicodeDecodeError,ValueError) as exc:result,error=None,type(exc).__name__
        after=store.path.read_bytes() if store.path.exists() else None
        observed.append({'operation':operation,'raw':list(raw) if raw is not None else None,
                         'result':result,'error':error,'after':list(after) if after is not None else None})
    log.info('Observed schedule text cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original schedule text observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 59, "Schedule text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("schedules.json");
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
        }
        let store = ScheduleStore::new(root);
        let operation = case["operation"].as_str().ok_or("Operation missing")?;
        let result: Result<Value, RepoError> = match operation {
            "list" => store.list().map(Value::Array),
            "get" => store
                .get("fixture")
                .map(|value| value.unwrap_or(Value::Null)),
            "add" => store.add(
                "added-profile",
                serde_json::json!({"everyHours":1}),
                Some(1000.0),
            ),
            "update" => store
                .update("fixture", &serde_json::json!({"enabled":false}))
                .map(|value| value.unwrap_or(Value::Null)),
            "delete" => store.delete("fixture").map(Value::Bool),
            "claim" => store
                .claim_due("fixture", "fixture-worker", 1000.0)
                .map(Value::Bool),
            "release" => store.release("fixture").map(|()| Value::Null),
            "record" => store
                .record(
                    "fixture",
                    serde_json::json!({"lastRun":1000.0,"lastOutcome":"done"})
                        .as_object()
                        .ok_or("Outcome missing")?
                        .clone(),
                )
                .map(|value| value.unwrap_or(Value::Null)),
            "invalid-update" => store
                .update("fixture", &serde_json::json!({"everyHours":-1}))
                .map(|value| value.unwrap_or(Value::Null)),
            _ => return Err("Unknown schedule operation".into()),
        };
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "Schedule text admission differs: operation={operation} case={index}"
        );
        let mut expected = case["result"].clone();
        let mut after: Option<Vec<u8>> = serde_json::from_value(case["after"].clone())?;
        match result {
            Ok(value) => {
                if operation == "add" {
                    let id = value["id"].as_str().ok_or("Added id missing")?;
                    assert_eq!(id.len(), 12);
                    assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
                    assert_eq!(expected["id"], "aaaaaaaaaaaa");
                    expected["id"] = id.into();
                    let text = String::from_utf8(after.take().ok_or("Added bytes missing")?)?;
                    assert_eq!(text.matches("aaaaaaaaaaaa").count(), 1);
                    after = Some(text.replacen("aaaaaaaaaaaa", id, 1).into_bytes());
                }
                assert_eq!(
                    value, expected,
                    "Schedule result differs: operation={operation} case={index}"
                );
            }
            Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData => {
                assert_eq!(case["error"], "UnicodeDecodeError")
            }
            Err(RepoError::Message(_)) if operation == "invalid-update" => {
                assert_eq!(case["error"], "ValueError")
            }
            Err(error) => return Err(error.into()),
        }
        assert_eq!(
            fs::read(&path).ok(),
            after,
            "Schedule bytes differ: operation={operation} case={index}"
        );
        if !case["error"].is_null() {
            assert_eq!(
                fs::read(&path).ok(),
                raw,
                "Refused schedule changed evidence"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn schedule_claim_is_single_occurrence_and_record_clears_only_its_claim()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/schedule-store.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Schedule cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let temporary = common::TempDirectory::new("schedule-store")?;
        let root = temporary.path().to_path_buf();
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join("schedules.json");
            let before = serde_json::to_vec(&case["initial"])?;
            fs::write(&path, &before)?;
            let store = ScheduleStore::new(root.clone());
            let now = case["now"].as_f64().ok_or("Now missing")?;
            let rows = store.list()?;
            assert_eq!(
                serde_json::to_value(due_schedules(&rows, now)?)?,
                case["due"]
            );
            assert_eq!(serde_json::to_value(next_run_at(&rows[0])?)?, case["next"]);
            assert_eq!(
                Value::Bool(fs::read(&path)? == before),
                case["read_unchanged"]
            );
            assert_eq!(
                Value::Bool(store.claim_due("fixture-schedule", "fixture-worker", now)?),
                case["claimed"]
            );
            assert_eq!(serde_json::to_value(store.list()?)?, case["after_claim"]);
            assert_eq!(
                Value::Bool(store.claim_due("fixture-schedule", "second-worker", now + 1.0)?),
                case["again"]
            );
            store.release("fixture-schedule")?;
            assert_eq!(
                store.get("fixture-schedule")?,
                Some(case["released"].clone())
            );
            let outcome = serde_json::json!({"lastRun":now,"lastOutcome":"done","lastError":null});
            assert_eq!(
                store.record(
                    "fixture-schedule",
                    outcome.as_object().ok_or("Outcome missing")?.clone()
                )?,
                Some(case["outcome"].clone())
            );
            assert_eq!(serde_json::to_value(store.list()?)?, case["after_record"]);
            let error = store
                .record(
                    "fixture-schedule",
                    serde_json::json!({"minutes":1})
                        .as_object()
                        .ok_or("Patch missing")?
                        .clone(),
                )
                .err()
                .ok_or("Invalid outcome accepted")?;
            assert_eq!(
                Some(error.to_string().as_str()),
                case["owned_error"].as_str()
            );
            assert!(store.get("missing")?.is_none());
            assert_eq!(
                Value::Bool(store.delete("fixture-schedule")?),
                case["deleted"]
            );
            assert_eq!(
                Value::Bool(store.delete("fixture-schedule")?),
                case["deleted_again"]
            );
            assert_eq!(serde_json::to_value(store.list()?)?, case["final"]);
            Ok(())
        })();
        temporary.cleanup()?;
        result?;
    }
    Ok(())
}
