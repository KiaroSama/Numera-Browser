use numera_core::{ownership::capture, storage::first_run};
use serde_json::{Map, Value};
use std::{fs, path::Path, process::Command, time::Duration};

fn snapshot(base: &Path, directory: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let mut result = Map::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .strip_prefix(base)?
            .to_str()
            .ok_or("NonUnicode fixture path")?
            .replace('\\', "/");
        if path.is_dir() {
            result.insert(name, Value::Null);
            result.extend(
                snapshot(base, &path)?
                    .as_object()
                    .ok_or("Snapshot missing")?
                    .clone(),
            );
        } else {
            result.insert(name, serde_json::to_value(fs::read(path)?)?);
        }
    }
    Ok(Value::Object(result))
}

#[test]
fn new_subtrees_preserve_original_nested_lock_entries() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("nested first-run α")?;
    let root: std::path::PathBuf = temporary.path().components().collect();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut original = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    original
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import json,shutil,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import firstrun

def snapshot(base):
    return {p.relative_to(base).as_posix():None if p.is_dir() else list(p.read_bytes())
            for p in base.rglob('*')}

def prepare(case,mode):
    source,destination=case/'source 日本語',case/'destination with spaces'
    directory=source/'profiles'
    (directory/'.locks').mkdir(parents=True)
    (directory/'.locks'/'root.lock').write_bytes(b'ignored merge lock')
    child=directory/'branch'
    (child/'.locks'/'nested').mkdir(parents=True)
    (child/'.locks'/'state.lock').write_bytes(b'\x00\xffretained state\r\n')
    (child/'.locks'/'nested'/'empty.lock').write_bytes(b'')
    (child/'deeper'/'.locks').mkdir(parents=True)
    (child/'deeper'/'.locks'/'deep.lock').write_bytes(b'deep state')
    (child/'deeper'/'ordinary.bin').write_bytes(b'ordinary\x00')
    destination.mkdir(parents=True)
    if mode!='new':
        existing=destination/'profiles'/'branch'/'.locks'
        existing.mkdir(parents=True)
        (existing/'state.lock').write_bytes(b'destination wins')
    if mode=='existing':
        (destination/'profiles'/'branch'/'deeper').mkdir()
    return source,destination

rows=[]
for mode in ('new','mixed','existing'):
    case=root/mode
    source,destination=prepare(case,mode)
    before=snapshot(source)
    first=firstrun.import_from(source,destination)
    written=snapshot(destination)
    again=firstrun.import_from(source,destination)
    assert snapshot(destination)==written
    assert snapshot(source)==before
    rows.append(dict(mode=mode,source=str(source),destination=str(destination),first=first,
                     written=written,before=before,again=again))
    shutil.rmtree(case)
    prepare(case,mode)
print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(&root);
    let observed = capture::run(
        &mut original,
        capture::Limits {
            wall: Duration::from_secs(15),
            idle: Duration::from_secs(10),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original nested-copy observer failed");
    assert!(
        observed.stderr.is_empty(),
        "Original nested-copy observer emitted diagnostics"
    );
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(rows.len(), 3, "Nested-copy observations missing");
    for (row, mode) in rows.iter().zip(["new", "mixed", "existing"]) {
        assert_eq!(row["mode"], mode);
        let source = Path::new(row["source"].as_str().ok_or("Source missing")?);
        let destination = Path::new(row["destination"].as_str().ok_or("Destination missing")?);
        assert_eq!(snapshot(source, source)?, row["before"]);
        let first = first_run::import_from(source, destination)?;
        assert_eq!(
            serde_json::to_value(first)?,
            row["first"],
            "Nested-copy summary differs mode={mode}"
        );
        assert_eq!(
            snapshot(destination, destination)?,
            row["written"],
            "First-run nested copy differs mode={mode}"
        );
        assert_eq!(
            snapshot(source, source)?,
            row["before"],
            "Nested-copy source was modified"
        );
        assert_eq!(
            serde_json::to_value(first_run::import_from(source, destination)?)?,
            row["again"]
        );
        assert_eq!(snapshot(destination, destination)?, row["written"]);
    }
    temporary.cleanup()?;
    Ok(())
}
