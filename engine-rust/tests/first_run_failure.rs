use numera_core::storage::first_run::run_if_needed;
use std::{fs, path::PathBuf};

#[cfg(unix)]
mod common;

#[cfg(unix)]
#[test]
fn fifo_import_matches_original_refusal_continuation_and_retry()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use serde_json::Value;
    use std::{path::Path, process::Command, time::Duration};

    const TEST: &str = "fifo_import_matches_original_refusal_continuation_and_retry";
    if let Some(document) = std::env::var_os("NUMERA_FIFO_IMPORT_FIXTURE") {
        let cases: Vec<Value> = serde_json::from_slice(&fs::read(document)?)?;
        for case in cases {
            let source = PathBuf::from(case["source"].as_str().ok_or("Source missing")?);
            let destination =
                PathBuf::from(case["destination"].as_str().ok_or("Destination missing")?);
            let first = run_if_needed(&destination, std::slice::from_ref(&source))?;
            assert_eq!(
                serde_json::to_value(first)?,
                case["first"],
                "First-run FIFO refusal differs"
            );
            assert_eq!(
                destination.join(".imported").exists(),
                case["marked"].as_bool().ok_or("Marker result missing")?
            );
            assert_eq!(
                fs::read(destination.join("schedules.json"))?,
                b"independent"
            );
            assert_eq!(fs::read(source.join("schedules.json"))?, b"independent");
            let pipe = source.join(case["pipe"].as_str().ok_or("Pipe missing")?);
            use std::os::unix::fs::FileTypeExt;
            assert!(fs::symlink_metadata(&pipe)?.file_type().is_fifo());
            fs::remove_file(&pipe)?;
            fs::write(&pipe, b"retried")?;
            let again = run_if_needed(&destination, std::slice::from_ref(&source))?;
            assert_eq!(serde_json::to_value(again)?, case["again"]);
            assert!(destination.join(".imported").is_file());
            let expected: Vec<u8> = serde_json::from_value(case["written"].clone())?;
            assert_eq!(
                fs::read(destination.join(case["pipe"].as_str().ok_or("Pipe missing")?))?,
                expected
            );
            assert_eq!(fs::read(&pipe)?, b"retried");
        }
        return Ok(());
    }
    let temporary = common::TempDirectory::new("fifo-import")?;
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
import json,os,shutil,stat,sys
from pathlib import Path
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import firstrun
from run_logging import run_log

def prepare(source,destination,pipe,kept):
    (source/'profiles').mkdir(parents=True)
    destination.mkdir(parents=True)
    (source/'schedules.json').write_bytes(b'independent')
    os.mkfifo(source/pipe)
    if kept:
        target=destination/pipe
        target.parent.mkdir(parents=True,exist_ok=True)
        target.write_bytes(b'destination wins')

with run_log('fifo-import-oracle',root/'logs') as log:
    rows=[]
    for pipe in ['profiles/pipe.json','tokens.json']:
        for kept in [False,True]:
            case=root/'cases'/str(len(rows))
            source,destination=case/'source 日本語',case/'destination with spaces'
            prepare(source,destination,pipe,kept)
            with patch('numera.firstrun.legacy_dirs',return_value=[source]):
                first=firstrun.run_if_needed(destination)
                marked=(destination/'.imported').exists()
                assert bool(first['failed']) is not kept
                assert marked is kept
                assert (destination/'schedules.json').read_bytes()==b'independent'
                assert stat.S_ISFIFO((source/pipe).stat().st_mode)
                (source/pipe).unlink()
                (source/pipe).write_bytes(b'retried')
                again=firstrun.run_if_needed(destination)
                assert (destination/'.imported').is_file()
                written=list((destination/pipe).read_bytes())
            rows.append({'source':str(source),'destination':str(destination),'pipe':pipe,
                         'first':first,'marked':marked,'again':again,'written':written})
            shutil.rmtree(case)
            prepare(source,destination,pipe,kept)
    log.info('Observed FIFO refusal and retry cases=%s',len(rows))
print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path());
    let limits = || capture::Limits {
        wall: Duration::from_secs(10),
        idle: Duration::from_secs(8),
        bytes: 65536,
        cleanup: Duration::from_secs(5),
    };
    let original = capture::run(&mut command, limits())?;
    assert_eq!(original.exit, 0, "Original FIFO import observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 4, "FIFO import observations missing");
    let document = temporary.path().join("observations.json");
    fs::write(&document, original.stdout)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env("NUMERA_FIFO_IMPORT_FIXTURE", &document);
    let native = match capture::run(&mut command, limits()) {
        Ok(output) => output,
        Err(mut error) if error.kind() == std::io::ErrorKind::TimedOut => {
            error.settle(Duration::from_secs(5))?;
            panic!("First-run FIFO refusal differs: {error}");
        }
        Err(error) => return Err(error.into()),
    };
    assert_eq!(
        native.exit,
        0,
        "Native FIFO import observer failed: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    let stdout = String::from_utf8(native.stdout)?;
    assert!(
        stdout.contains("running 1 test") && stdout.contains("test result: ok. 1 passed; 0 failed"),
        "Native FIFO import observer did not run"
    );
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn failed_first_run_remains_retryable_and_never_overwrites_the_obstruction()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("first-run-failure-{}", std::process::id()));
    fs::create_dir_all(root.join("source/profiles"))?;
    fs::create_dir_all(root.join("destination"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(root.join("source/profiles/a.json"), "fixture original α")?;
        fs::write(root.join("destination/profiles"), "caller obstruction")?;
        let failed = run_if_needed(&root.join("destination"), &[root.join("source")])?
            .ok_or("Import not attempted")?;
        assert!(!failed.failed.is_empty());
        assert!(
            !root.join("destination/.imported").exists(),
            "Partial import was marked complete"
        );
        assert_eq!(
            fs::read_to_string(root.join("destination/profiles"))?,
            "caller obstruction"
        );
        assert_eq!(
            fs::read_to_string(root.join("source/profiles/a.json"))?,
            "fixture original α"
        );
        fs::remove_file(root.join("destination/profiles"))?;
        let retried = run_if_needed(&root.join("destination"), &[root.join("source")])?
            .ok_or("Import could not retry")?;
        assert!(retried.failed.is_empty());
        assert!(root.join("destination/.imported").exists());
        assert_eq!(
            fs::read_to_string(root.join("destination/profiles/a.json"))?,
            "fixture original α"
        );
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
