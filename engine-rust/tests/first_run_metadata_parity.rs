#![cfg(target_os = "linux")]

use numera_core::{ownership::capture, storage::first_run::import_from};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};

mod common;

#[test]
fn imported_attributes_match_original_without_replacing_destination_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    const OBSERVER: &str = r#"
import json,os,shutil,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:3])
operation=sys.argv[3]
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.firstrun import import_from
from run_logging import run_log

def attributes(path,label):
    os.setxattr(path,'user.numera.fixture',label)
    os.setxattr(path,'user.numera.empty',b'')

def prepare(case,variant):
    source,destination=case/'source 日本語',case/'destination with spaces'
    (source/'profiles/new/nested').mkdir(parents=True)
    destination.mkdir(parents=True)
    for name,raw in [('profiles/new/state.bin',bytes([0,255,10,13])),
                     ('profiles/new/nested/deep.bin',b'deep'),('tokens.json',b'fixture')]:
        path=source/name
        path.write_bytes(raw)
        attributes(path,b'source\x00\xff')
        path.chmod(0o440)
    for name in ['profiles','profiles/new','profiles/new/nested']:
        attributes(source/name,b'source directory')
    if variant!='new':
        (destination/'profiles/new').mkdir(parents=True)
        attributes(destination/'profiles',b'destination root')
        attributes(destination/'profiles/new',b'destination directory')
        (destination/'tokens.json').write_bytes(b'destination file')
        attributes(destination/'tokens.json',b'destination\x00\xff')
    if variant=='existing':
        (destination/'profiles/new/nested').mkdir()
        attributes(destination/'profiles/new/nested',b'destination nested')
        path=destination/'profiles/new/state.bin'
        path.write_bytes(b'destination state')
        attributes(path,b'destination state attributes')
    return source,destination

def snapshot(base):
    rows={}
    for path in sorted(base.rglob('*')):
        if '.locks' in path.relative_to(base).parts:
            continue
        row={'attributes':{name:list(os.getxattr(path,name)) for name in sorted(os.listxattr(path))}}
        if path.is_file():
            row['bytes']=list(path.read_bytes())
        rows[path.relative_to(base).as_posix()]=row
    return rows

with run_log('first-run-attributes-oracle',root/'logs') as log:
    rows=[]
    for variant in ['new','existing','mixed']:
        case=root/variant
        if operation=='original':
            source,destination=prepare(case,variant)
            before=snapshot(source)
            first=import_from(source,destination)
            again=import_from(source,destination)
            assert not first['failed'] and not again['failed']
            assert snapshot(source)==before
            rows.append({'source':str(source),'destination':str(destination),
                         'first':first,'again':again,'source_state':before,
                         'written':snapshot(destination)})
            shutil.rmtree(case)
            prepare(case,variant)
        else:
            source,destination=case/'source 日本語',case/'destination with spaces'
            rows.append({'source_state':snapshot(source),'written':snapshot(destination)})
    log.info('Observed first-run extended attributes cases=%s mode=%s',len(rows),operation)
print(json.dumps(rows,ensure_ascii=True))
"#;
    let temporary = common::TempDirectory::new("first-run-attributes")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let observe = |operation: &str| -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let mut command = Command::new(
            std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
        );
        command
            .args(["-I", "-B", "-c", OBSERVER])
            .arg(repository)
            .arg(temporary.path())
            .arg(operation);
        let output = capture::run(
            &mut command,
            capture::Limits {
                wall: Duration::from_secs(20),
                idle: Duration::from_secs(15),
                bytes: 131072,
                cleanup: Duration::from_secs(5),
            },
        )?;
        assert_eq!(
            output.exit,
            0,
            "Attribute observer failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let expected = observe("original")?;
    assert_eq!(expected.len(), 3, "Attribute observations missing");
    for case in &expected {
        let source = Path::new(case["source"].as_str().ok_or("Source missing")?);
        let destination = Path::new(case["destination"].as_str().ok_or("Destination missing")?);
        assert_eq!(
            serde_json::to_value(import_from(source, destination)?)?,
            case["first"]
        );
        assert_eq!(
            serde_json::to_value(import_from(source, destination)?)?,
            case["again"]
        );
    }
    let actual = observe("readback")?;
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(
            actual["source_state"], expected["source_state"],
            "Source attributes changed"
        );
        assert_eq!(
            actual["written"], expected["written"],
            "First-run extended attributes differ: case={index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
