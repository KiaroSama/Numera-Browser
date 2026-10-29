use numera_core::{
    browser::runtime::Runtime,
    http::scheduler::Scheduler,
    ownership::{capture, claims, registry::Registry},
    storage::{atomic::write_json, schedules::ScheduleStore},
};
use serde_json::Value;
use std::{fs, path::Path, process::Command, sync::Arc, time::Duration};

#[test]
fn skipped_malformed_schedule_ids_do_not_abort_later_occurrences()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("scheduler-row-isolation")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import json,os,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.server import create_app
from numera.scheduler import ScheduleStore
from run_logging import run_log
base={'profileId':'gone','everyHours':1,'minutes':1,'preset':'news',
      'enabled':True,'lastRun':None,'createdAt':0,'future':{'kept':'日本語'}}
heads=[base]+[{**base,'id':sid} for sid in [None,7,[],{},'', 'missing-profile']]
broken={'id':'later','profileId':'bad','everyHours':1,'minutes':1,
        'preset':'news','enabled':True,'lastRun':None,'createdAt':0}
profile={'id':'bad','name':'bad','seed':42,
         'proxy':{'type':'INVALID','host':'fixture.test','port':'80'}}
observed=[]
with run_log('scheduler-row-isolation-oracle',parent/'logs') as log:
    for head in heads:
        root=parent/str(len(observed));root.mkdir(parents=True)
        with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api'}):
            app=create_app(data_dir=str(root),start_scheduler=False)
        (root/'dashboard').mkdir(exist_ok=True)
        (root/'dashboard/bad.json').write_text(json.dumps(profile),encoding='utf-8')
        schedules=ScheduleStore(root)
        rows=[head,broken]
        schedules._save(rows)
        try:
            with patch('time.time',return_value=1000.0):app.state.scheduler_tick()
            error=None
        except Exception as exc:error=type(exc).__name__
        observed.append({'rows':rows,'profile':profile,'error':error,'stored':schedules.list()})
    log.info('Observed scheduler row-isolation cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original scheduler row observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 7, "Scheduler row observations missing");
    for (index, case) in cases.iter().enumerate() {
        assert!(case["error"].is_null(), "Original row failure escaped");
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("dashboard"))?;
        let profile = serde_json::to_vec(&case["profile"])?;
        fs::write(root.join("dashboard/bad.json"), &profile)?;
        write_json(&root.join("schedules.json"), &case["rows"])?;
        let sessions = Arc::new(Registry::new(root.clone(), "fixture-host".into(), None)?);
        let scheduler = Scheduler::new(
            root.clone(),
            None,
            "fixture-host".into(),
            sessions.clone(),
            Runtime {
                python: root.join("must-not-start-python"),
                adapter: root.join("must-not-start-sdk"),
                cwd: root.clone(),
                home: root.clone(),
                logs: root.join("sdk-logs"),
                country: None,
                relay: None,
                binary_options: Default::default(),
                deployment: None,
            },
        )?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let verdict = scheduler.tick();
            assert!(
                verdict.is_ok(),
                "Scheduler row isolation differs: case={index} verdict={verdict:?}"
            );
            let mut actual = ScheduleStore::new(root.clone()).list()?;
            assert_eq!(actual.len(), 2);
            assert_eq!(actual[0], case["rows"][0], "Skipped schedule changed");
            let timestamp = actual[1]["lastFailedAt"]
                .as_f64()
                .ok_or("Later row was not processed")?;
            assert!(timestamp.is_finite() && timestamp > 1700000000.0);
            assert_eq!(case["stored"][1]["lastFailedAt"], 1000.0);
            // Only the real tick's wall-clock failure timestamp is canonicalized.
            actual[1]["lastFailedAt"] = case["stored"][1]["lastFailedAt"].clone();
            assert_eq!(
                Value::Array(actual),
                case["stored"],
                "Later row bookkeeping differs"
            );
            assert_eq!(fs::read(root.join("dashboard/bad.json"))?, profile);
            assert!(!root.join("profiles/bad.json").exists());
            assert!(
                !root.join("sdk-logs").exists(),
                "Failed conversion activated SDK"
            );
            assert!(!sessions.owns("bad")?);
            assert!(matches!(claims::read(&root, "bad"), claims::Claim::Absent));
            Ok(())
        })();
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        drop(scheduler);
        drop(sessions);
        result?;
    }
    temporary.cleanup()?;
    Ok(())
}
