use super::common;
use numera_core::{jobs::JobStore, ownership::capture};
use serde_json::{Value, json};
use std::{fs, io, path::Path, process::Command, time::Duration};

#[test]
fn job_text_admission_matches_original_without_replacing_identity_or_outcome()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("job-text")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import jobs
from run_logging import run_log
identity=b'{"id":"sample","pid":0,"state":"done","started":900.0,"note":"keep"}'
outcome=b'{"state":"failed","exit":2,"finished":950.0,"extra":"retained"}'
inputs=[None,b'{"broken":',b'[]',b'{}',None,b'\xff',b'{"note":"\xc3"}',b'{"note":"\x80"}',b'{"note":"\xe2\x82"}']
rows=[]
def content(path):
    return list(path.read_bytes()) if path.exists() else None
with run_log('job-text-oracle',parent/'logs') as log,patch.object(jobs.time,'time',return_value=1000.0):
    for role in ('identity','outcome'):
        for index,raw in enumerate(inputs):
            if index==4:
                raw=identity if role=='identity' else outcome
            for operation in ('get','listing','record_exit'):
                root=parent/str(len(rows))
                directory=root/'jobs'
                directory.mkdir(parents=True)
                record=directory/'sample.json'
                exit_file=directory/'sample.exit'
                before_identity=raw if role=='identity' else identity
                before_outcome=raw if role=='outcome' else outcome
                for path,data in ((record,before_identity),(exit_file,before_outcome)):
                    if data is not None:
                        path.write_bytes(data)
                try:
                    if operation=='get':
                        value=jobs.get(root,'sample')
                    elif operation=='listing':
                        value=jobs.listing(root)
                    else:
                        value=jobs.record_exit(str(record),7)
                    result={'ok':value}
                except UnicodeDecodeError:
                    result={'error':'InvalidData'}
                rows.append({'operation':operation,'identity':None if before_identity is None else list(before_identity),
                             'outcome':None if before_outcome is None else list(before_outcome),
                             'result':result,'after_identity':content(record),'after_outcome':content(exit_file)})
    log.info('Observed job text cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original job text observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 54, "Job text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let directory = root.join("jobs");
        fs::create_dir_all(&directory)?;
        let record = directory.join("sample.json");
        let outcome = directory.join("sample.exit");
        for (path, key) in [(&record, "identity"), (&outcome, "outcome")] {
            if !case[key].is_null() {
                fs::write(path, serde_json::from_value::<Vec<u8>>(case[key].clone())?)?;
            }
        }
        let store = JobStore::new(root);
        let result = match case["operation"].as_str() {
            Some("get") => store.get("sample", 1000.0).map(|value| json!(value)),
            Some("listing") => store.listing(1000.0).map(|value| json!(value)),
            Some("record_exit") => store.record_exit("sample", 7, 1000.0).map(|()| Value::Null),
            _ => return Err("Unknown job text operation".into()),
        };
        let actual = match result {
            Ok(value) => json!({"ok":value}),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                json!({"error":"InvalidData"})
            }
            Err(error) => return Err(error.into()),
        };
        assert_eq!(
            actual, case["result"],
            "Job text admission differs: {index}"
        );
        for (path, key) in [(&record, "after_identity"), (&outcome, "after_outcome")] {
            let expected: Option<Vec<u8>> = serde_json::from_value(case[key].clone())?;
            let actual = match fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            assert_eq!(actual, expected, "Job text bytes differ: {index}/{key}");
        }
    }
    temporary.cleanup()?;
    Ok(())
}
