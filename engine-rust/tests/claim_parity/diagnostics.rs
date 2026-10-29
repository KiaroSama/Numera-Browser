use super::common;
use numera_core::ownership::{
    capture,
    claims::{self, Claim},
};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};

#[test]
fn claim_diagnostics_match_original_truth_and_display_without_changing_records()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("claim-diagnostics")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import sessions
from run_logging import run_log
values=[None,False,True,0,7,'','warmup',"a'b",[],['x',True,None],{}, {'value':[False,None]}]
hosts=[None,'','fixture-local-host','foreign',True,['remote'],{'a':False}]
rows=[]
with run_log('claim-diagnostics-oracle',parent/'logs') as log,patch('platform.node',return_value='fixture-local-host'):
    for what in values:
        for pid in values:
            for host in hosts:
                raw={'what':copy.deepcopy(what),'pid':copy.deepcopy(pid),'host':copy.deepcopy(host)}
                before=copy.deepcopy(raw)
                assert sessions.state(raw)=='unknown','Fixture unexpectedly queried a live tree'
                rows.append({'input':before,'description':sessions.describe(raw),'kind':'record'})
                assert raw==before,'Original claim diagnostic changed input'
    for value,kind in ((None,'absent'),(sessions.UNREADABLE,'unreadable'),({},'record')):
        rows.append({'input':{} if kind=='record' else None,'description':sessions.describe(value),'kind':kind})
    log.info('Observed claim diagnostic cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1048576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original claim diagnostic observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 1011, "Claim diagnostic observations missing");
    for (index, case) in cases.iter().enumerate() {
        let claim = match case["kind"].as_str() {
            Some("absent") => Claim::Absent,
            Some("unreadable") => Claim::Unreadable,
            Some("record") => Claim::Record(case["input"].clone()),
            _ => return Err("Unknown claim observation".into()),
        };
        assert_eq!(
            claims::describe(&claim, "fixture-local-host"),
            case["description"]
                .as_str()
                .ok_or("Original diagnostic missing")?,
            "Claim diagnostic display differs: {index}",
        );
        if let Claim::Record(record) = claim {
            assert_eq!(record, case["input"], "Native diagnostic changed claim");
        }
    }
    temporary.cleanup()?;
    Ok(())
}
