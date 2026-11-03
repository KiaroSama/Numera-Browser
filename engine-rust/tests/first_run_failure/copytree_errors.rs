use numera_core::{ownership::capture, storage::first_run::run_if_needed};
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
        if name == ".imported" || name.split('/').any(|part| part == ".locks") {
            continue;
        }
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
fn new_subtree_errors_match_original_continuation_summaries_and_retry()
-> Result<(), Box<dyn std::error::Error>> {
    const TEST: &str =
        "copytree_errors::new_subtree_errors_match_original_continuation_summaries_and_retry";
    if let Some(document) = std::env::var_os("NUMERA_COPYTREE_ERRORS_FIXTURE") {
        let rows: Vec<Value> = serde_json::from_slice(&fs::read(document)?)?;
        for row in rows {
            let source = Path::new(row["source"].as_str().ok_or("Source missing")?);
            let destination = Path::new(row["destination"].as_str().ok_or("Destination missing")?);
            let denied: Vec<String> = serde_json::from_value(row["denied"].clone())?;
            let attempt = || -> Result<_, Box<dyn std::error::Error>> {
                use std::os::unix::fs::PermissionsExt;
                for name in &denied {
                    fs::set_permissions(source.join(name), fs::Permissions::from_mode(0))?;
                }
                let result = run_if_needed(destination, &[source.to_owned()]);
                for name in &denied {
                    fs::set_permissions(source.join(name), fs::Permissions::from_mode(0o700))?;
                }
                Ok(result?)
            };
            let first = attempt()?;
            if row["mode"] == "unreadable-directory" {
                assert!(!destination.join("profiles/branch/b-nested").exists());
            }
            assert_eq!(
                serde_json::to_value(first)?,
                row["first"],
                "First-run copytree failure aggregation differs mode={}",
                row["mode"]
            );
            assert_eq!(snapshot(destination, destination)?, row["written"]);
            assert!(!destination.join(".imported").exists());
            assert_eq!(fs::read(source.join("schedules.json"))?, b"independent");
            if let Some(bytes) = row["nonunicode"].as_array() {
                use std::os::unix::{ffi::OsStrExt, fs::FileTypeExt};
                let name: Vec<u8> = bytes
                    .iter()
                    .map(|byte| {
                        byte.as_u64()
                            .and_then(|value| u8::try_from(value).ok())
                            .ok_or("Invalid fixture byte")
                    })
                    .collect::<Result<_, _>>()?;
                let path = source.join(std::ffi::OsStr::from_bytes(&name));
                assert!(fs::symlink_metadata(&path)?.file_type().is_fifo());
                fs::remove_file(path)?;
            }
            let again = attempt()?;
            assert_eq!(serde_json::to_value(again)?, row["again"]);
            assert_eq!(snapshot(destination, destination)?, row["repeated"]);
            for name in row["pipes"].as_array().ok_or("Pipes missing")? {
                let path = source.join(name.as_str().ok_or("Pipe name missing")?);
                use std::os::unix::fs::FileTypeExt;
                assert!(fs::symlink_metadata(&path)?.file_type().is_fifo());
                fs::remove_file(&path)?;
                fs::write(path, b"retried")?;
            }
            let retried = run_if_needed(destination, &[source.to_owned()])?;
            assert_eq!(serde_json::to_value(retried)?, row["retried"]);
            assert_eq!(snapshot(destination, destination)?, row["complete"]);
            assert!(destination.join(".imported").is_file());
            assert!(run_if_needed(destination, &[source.to_owned()])?.is_none());
        }
        return Ok(());
    }
    let temporary = super::common::TempDirectory::new("copytree-errors")?;
    let root: std::path::PathBuf = temporary.path().components().collect();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut original = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    original
        .args(["-I", "-B", "-c", r#"
import json,os,shutil,stat,sys
from pathlib import Path
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import firstrun
from run_logging import run_log
pipes=['profiles/branch/a-pipe','profiles/branch/b-nested/a-pipe','profiles/branch/c-pipe']
def denied(mode):
    return ['profiles/branch/b-nested' if mode=='unreadable-directory' else 'profiles/branch/z-good.bin'] if mode.startswith('unreadable-') else []
def prepare(case,mode):
    source,destination=case/'source 日本語',case/'destination with spaces'
    branch=source/'profiles/branch'
    branch.mkdir(parents=True)
    os.mkfifo(source/pipes[2])
    (branch/'b-nested').mkdir()
    os.mkfifo(source/pipes[1])
    (branch/'b-nested/z-good.bin').write_bytes(b'nested\x00\xff')
    os.mkfifo(source/pipes[0])
    (branch/'z-good.bin').write_bytes(b'healthy\r\n\x00')
    if mode=='nonunicode':
        os.mkfifo(os.fsencode(source)+b'/profiles/branch/raw-\xff-\xe2\x82-pipe')
    (source/'schedules.json').write_bytes(b'independent')
    destination.mkdir(parents=True)
    if mode in ('existing','mixed'):
        (destination/'profiles/branch').mkdir(parents=True)
    if mode=='mixed':
        (destination/'profiles/branch/a-pipe').write_bytes(b'destination wins')
    return source,destination

def snapshot(base):
    return {p.relative_to(base).as_posix():None if p.is_dir() else list(p.read_bytes())
            for p in base.rglob('*') if p.name!='.imported' and '.locks' not in p.relative_to(base).parts}
rows=[]
with run_log('copytree-errors-oracle',root/'logs') as log:
    for mode in ('new','existing','mixed','unreadable-file','unreadable-directory','nonunicode'):
        case=root/mode
        source,destination=prepare(case,mode)
        def attempt():
            try:
                for name in denied(mode):
                    (source/name).chmod(0)
                return firstrun.run_if_needed(destination)
            finally:
                for name in denied(mode):
                    (source/name).chmod(0o700)
        with patch('numera.firstrun.legacy_dirs',return_value=[source]):
            first=attempt()
            written=snapshot(destination)
            if mode=='unreadable-directory':
                assert not (destination/'profiles/branch/b-nested').exists()
            assert first['failed'] and not (destination/'.imported').exists()
            assert (destination/'schedules.json').read_bytes()==b'independent'
            if mode=='new':
                assert (destination/'profiles/branch/z-good.bin').read_bytes()==b'healthy\r\n\x00'
                assert (destination/'profiles/branch/b-nested/z-good.bin').read_bytes()==b'nested\x00\xff'
            nonunicode=list(b'profiles/branch/raw-\xff-\xe2\x82-pipe') if mode=='nonunicode' else None
            if nonunicode is not None:
                os.unlink(os.fsencode(source)+b'/'+bytes(nonunicode))
            again=attempt()
            repeated=snapshot(destination)
            for name in pipes:
                path=source/name
                assert stat.S_ISFIFO(path.stat().st_mode)
                path.unlink()
                path.write_bytes(b'retried')
            retried=firstrun.run_if_needed(destination)
            complete=snapshot(destination)
            assert not retried['failed'] and (destination/'.imported').is_file()
        if mode=='unreadable-file':
            assert any('Permission denied' in error for error in first['failed'])
        rows.append(dict(mode=mode,source=str(source),destination=str(destination),pipes=pipes,denied=denied(mode),nonunicode=nonunicode,
                         first=first,written=written,again=again,repeated=repeated,
                         retried=retried,complete=complete))
        shutil.rmtree(case)
        prepare(case,mode)
    log.info('Observed new-subtree failure cases=%s',len(rows))
print(json.dumps(rows,ensure_ascii=True))
"#])
        .arg(repository)
        .arg(&root);
    let limits = || capture::Limits {
        wall: Duration::from_secs(15),
        idle: Duration::from_secs(10),
        bytes: 131072,
        cleanup: Duration::from_secs(5),
    };
    let observed = capture::run(&mut original, limits())?;
    assert_eq!(
        observed.exit,
        0,
        "Original copytree failure observer failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(rows.len(), 6, "Copytree failure observations missing");
    let document = root.join("observations.json");
    fs::write(&document, observed.stdout)?;
    let mut native = Command::new(std::env::current_exe()?);
    native
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env("NUMERA_COPYTREE_ERRORS_FIXTURE", &document);
    let observed = capture::run(&mut native, limits())?;
    if observed.exit != 0 {
        let output = String::from_utf8(observed.stdout)?;
        let evidence = String::from_utf8(observed.stderr)?;
        if observed.exit == 101
            && output.contains("running 1 test")
            && output.contains(&format!("test {TEST} ... FAILED"))
            && output.contains("test result: FAILED. 0 passed; 1 failed; 0 ignored")
        {
            assert!(
                !evidence.contains("First-run copytree failure aggregation differs"),
                "First-run copytree failure aggregation differs: {evidence}"
            );
        }
        return Err(format!("Native copytree failure observer failed: {}", evidence).into());
    }
    let output = String::from_utf8(observed.stdout)?;
    assert!(
        output.contains("running 1 test") && output.contains("test result: ok. 1 passed; 0 failed")
    );
    temporary.cleanup()?;
    Ok(())
}
