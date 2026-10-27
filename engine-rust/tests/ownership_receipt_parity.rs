use numera_core::ownership::{capture, unit_report};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};
mod common;

#[test]
fn receipt_tokens_match_original_without_resolving_invalid_transfers()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("receipt-token-parity")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import unit_report
from run_logging import run_log
root.mkdir(parents=True)
path=root/'fixture.units'
prefix='{"version":2,"event":"header"}\n{"event":"pending","attempt":"fixture"}\n'
tokens=[('absent',None),('null',None),('false',False),('true',True),('zero',0),
        ('negative',-1),('one',1),('positive',7),('u64-max',2**64-1),
        ('float',1.0),('string','7'),('array',[]),('object',{})]
inputs=[]
for kind in ('job','pgid'):
    for label,token in tokens:
        value={'pid':7,'tree':{'kind':kind,'group':'fixture-job' if kind=='job' else 7}}
        if label!='absent':value['pidToken']=token
        inputs.append((kind+'-'+label,value))
for label,group in [('empty',''),('blank',' \t'),('nul','bad\0name'),('number',7),('bool',False)]:
    inputs.append(('job-name-'+label,{'pid':7,'pidToken':1,'tree':{'kind':'job','group':group}}))
for label,group in [('mismatch',8),('bool',True)]:
    inputs.append(('pgid-group-'+label,{'pid':7,'pidToken':1,'tree':{'kind':'pgid','group':group}}))
rows=[]
with run_log('receipt-token-oracle',root/'logs') as log:
    for label,value in inputs:
        before=copy.deepcopy(value)
        record={**value,'event':'unit','attempt':'fixture'}
        raw=prefix+json.dumps(record,separators=(',',':'))+'\n'
        path.write_text(raw,encoding='utf-8',newline='')
        report=unit_report.read(path)
        trees=[{'pid':tree.pid,'pidToken':tree.token,'tree':tree.as_record()} for tree in report.trees]
        assert path.read_bytes()==raw.encode('utf-8') and value==before,'Original receipt observation mutated inputs'
        rows.append({'label':label,'input':value,'raw':raw,'complete':report.complete,
                     'problem':report.problem is not None,'trees':trees})
    assert len(rows)==33 and sum(row['complete'] for row in rows)==8,'Original receipt token domain differs'
    log.info('Observed receipt token cases=%s',len(rows))
    print(json.dumps({'prefix':prefix,'cases':rows},ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original receipt token observer failed");
    let document: Value = serde_json::from_slice(&observed.stdout)?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Receipt cases missing")?;
    assert_eq!(cases.len(), 33, "Receipt token observations missing");
    let prefix = document["prefix"]
        .as_str()
        .ok_or("Receipt prefix missing")?;
    let jobs = temporary.path().join("jobs");
    fs::create_dir(&jobs)?;
    let path = jobs.join("fixture.units");
    for case in cases {
        let raw = case["raw"].as_str().ok_or("Receipt bytes missing")?;
        fs::write(&path, raw)?;
        let report = unit_report::read(&path)?;
        assert_eq!(
            report.complete,
            case["complete"] == true,
            "Receipt completeness differs: {}",
            case["label"]
        );
        assert_eq!(report.problem.is_some(), case["problem"] == true);
        assert_eq!(json!(report.trees), case["trees"]);
        assert_eq!(
            fs::read(&path)?,
            raw.as_bytes(),
            "Receipt read changed bytes"
        );
        let scoped = unit_report::read_scoped(&path, Some("unrelated"))?;
        assert_eq!(scoped.complete, case["complete"] == true);
        assert_eq!(scoped.problem.is_some(), case["problem"] == true);
        assert!(
            scoped.trees.is_empty(),
            "Unrelated receipt scope leaked a unit"
        );

        fs::write(&path, prefix)?;
        let input = &case["input"];
        let result = unit_report::record_at(
            &path,
            &json!({"pid":input["pid"],"token":input["pidToken"],"tree":input["tree"]}),
            "fixture",
        );
        assert_eq!(
            result.is_ok(),
            case["complete"] == true,
            "Receipt write admission differs: {}",
            case["label"]
        );
        if result.is_ok() {
            let report = unit_report::read(&path)?;
            assert!(report.complete && report.problem.is_none());
            assert_eq!(json!(report.trees), case["trees"]);
        } else {
            assert_eq!(
                fs::read(&path)?,
                prefix.as_bytes(),
                "Refused receipt write changed bytes"
            );
            assert!(
                !unit_report::read(&path)?.complete,
                "Refusal resolved its pending transfer"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}
