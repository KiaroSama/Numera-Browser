use super::*;
use std::time::UNIX_EPOCH;

#[derive(Deserialize)]
struct Retention {
    count: usize,
    retained: Vec<String>,
}

fn stamp(path: &Path, seconds: u64) -> Result<(), Box<dyn std::error::Error>> {
    fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(UNIX_EPOCH + Duration::from_secs(seconds))?;
    Ok(())
}

fn name(index: usize) -> String {
    format!("native-retention_2020-01-01_00-00-00_UTC.{}.log", index + 1)
}

#[test]
fn run_retention_matches_original_mtime_order_and_namespace()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("retained logs α")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut original = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    original.args(["-I", "-B", "-c", r#"
import json,logging,os,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import logs
rows=[]
for count in (0,1,19,20,21,27):
    data=root/str(count)
    directory=data/'logs'
    directory.mkdir(parents=True)
    for index in range(count):
        path=directory/f'numera_2020-01-01_00-00-00_UTC.{index+1}.log'
        path.write_text(str(index),encoding='utf-8')
        # Reverse filename order deliberately; retention is by modification time.
        os.utime(path,(1000000000+count-index,)*2)
    foreign=directory/'other_2020-01-01_00-00-00_UTC.log'
    foreign.write_text('retained',encoding='utf-8')
    current=logs.setup(data)
    assert current is not None
    retained=sorted(p.read_text(encoding='utf-8') for p in directory.glob('numera_*.log') if p!=current)
    assert foreign.read_text(encoding='utf-8')=='retained'
    rows.append(dict(count=count,retained=retained))
    logger=logging.getLogger(logs.LOGGER_NAME)
    for handler in list(logger.handlers):
        logger.removeHandler(handler)
        handler.close()
print(json.dumps(rows))
"#]).arg(repository).arg(temporary.path().join("original"));
    let output = capture::run(
        &mut original,
        capture::Limits {
            wall: Duration::from_secs(10),
            idle: Duration::from_secs(10),
            bytes: 128 * 1024,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(output.exit, 0, "Original retention observer failed");
    assert!(
        output.stderr.is_empty(),
        "Original retention observer emitted diagnostics"
    );
    let observations: Vec<Retention> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        observations.iter().map(|row| row.count).collect::<Vec<_>>(),
        [0, 1, 19, 20, 21, 27]
    );
    for row in observations {
        let directory = temporary.path().join(format!("native/{}", row.count));
        fs::create_dir_all(&directory)?;
        for index in 0..row.count {
            let path = directory.join(name(index));
            fs::write(&path, index.to_string())?;
            stamp(&path, 1_000_000_000 + (row.count - index) as u64)?;
        }
        let foreign = directory.join("other_2020-01-01_00-00-00_UTC.log");
        let malformed = directory.join("native-retention_notes.log");
        fs::write(&foreign, b"retained")?;
        fs::write(&malformed, b"not a run log")?;
        let log = RuntimeLog::new(&directory, "native-retention");
        let current = log.path().ok_or("Native retention log missing")?;
        log.message("INFO", "fixture", "Retention ready");
        let mut actual = Vec::new();
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path != current && path != foreign && path != malformed {
                actual.push(fs::read_to_string(path)?);
            }
        }
        actual.sort();
        assert_eq!(
            actual, row.retained,
            "Runtime log retention differs count={}",
            row.count
        );
        assert_eq!(fs::read(&foreign)?, b"retained");
        assert_eq!(fs::read(&malformed)?, b"not a run log");
        assert!(fs::read_to_string(current)?.contains("[INFO] [fixture] Retention ready"));
        drop(log);
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn active_logs_survive_new_runs_and_become_prunable_after_close()
-> Result<(), Box<dyn std::error::Error>> {
    if let Some(directory) = std::env::var_os("NUMERA_ACTIVE_LOG_RETENTION") {
        for index in 0..24 {
            let log = RuntimeLog::new(Path::new(&directory), "native-retention");
            log.message("INFO", "fixture", "Completed run");
            let path = log.path().ok_or("Completed log missing")?.to_owned();
            drop(log);
            stamp(&path, 1_000_000_000 + index)?;
        }
        return Ok(());
    }
    let temporary = common::TempDirectory::new("active log retention")?;
    let directory = temporary.path();
    let active = RuntimeLog::new(directory, "native-retention");
    let active_path = active.path().ok_or("Active log missing")?.to_owned();
    stamp(&active_path, 900_000_000)?;
    let mut child = Command::new(std::env::current_exe()?);
    child
        .args([
            "--exact",
            "retention::active_logs_survive_new_runs_and_become_prunable_after_close",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NUMERA_ACTIVE_LOG_RETENTION", directory);
    let output = capture::run(
        &mut child,
        capture::Limits {
            wall: Duration::from_secs(10),
            idle: Duration::from_secs(10),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(output.exit, 0, "Concurrent retention child failed");
    assert!(
        output.stderr.is_empty(),
        "Concurrent retention emitted diagnostics"
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.lines().any(|line| line == "running 1 test"));
    assert!(stdout.contains("\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "));
    assert!(active_path.is_file(), "Active runtime log was pruned");
    active.message("INFO", "fixture", "Still active");
    assert!(fs::read_to_string(&active_path)?.contains("Still active"));
    stamp(&active_path, 900_000_000)?;
    drop(active);
    let next = RuntimeLog::new(directory, "native-retention");
    assert!(next.path().is_some());
    assert!(!active_path.exists(), "Closed runtime log was not pruned");
    drop(next);
    assert_eq!(fs::read_dir(directory)?.count(), 20);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn pruning_failure_keeps_the_new_file_sink_usable() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(directory) = std::env::var_os("NUMERA_LOG_PRUNE_FAILURE") {
        let log = RuntimeLog::new(Path::new(&directory), "native-retention");
        let path = log.path().ok_or("Pruning failure disabled file logging")?;
        log.message("INFO", "fixture", "Logging survived pruning failure");
        assert!(fs::read_to_string(path)?.contains("Logging survived pruning failure"));
        return Ok(());
    }
    let temporary = common::TempDirectory::new("log pruning failure")?;
    let directory = temporary.path();
    for index in 0..20 {
        let path = directory.join(name(index));
        fs::write(&path, b"retained history")?;
        stamp(&path, 1_000_000_000 + index as u64)?;
    }
    let blocked = directory.join(name(0));
    let original = fs::metadata(&blocked)?.permissions();
    let mut denied = original.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        denied.set_mode(0o0);
    }
    #[cfg(windows)]
    denied.set_readonly(true);
    fs::set_permissions(&blocked, denied)?;
    let mut child = Command::new(std::env::current_exe()?);
    child
        .args([
            "--exact",
            "retention::pruning_failure_keeps_the_new_file_sink_usable",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NUMERA_LOG_PRUNE_FAILURE", directory);
    let output = capture::run(
        &mut child,
        capture::Limits {
            wall: Duration::from_secs(5),
            idle: Duration::from_secs(5),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    );
    let restore = fs::set_permissions(&blocked, original);
    let output = output?;
    restore?;
    assert_eq!(output.exit, 0, "Pruning failure disabled file logging");
    assert!(
        output.stderr.is_empty(),
        "Pruning failure enabled console fallback"
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.lines().any(|line| line == "running 1 test"));
    assert!(stdout.contains("\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "));
    assert_eq!(
        fs::read(&blocked)?,
        b"retained history",
        "Blocked history was removed"
    );
    assert_eq!(
        fs::read_dir(directory)?.count(),
        21,
        "Pruning error was not exercised"
    );
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn retention_preserves_nonregular_entries_and_unrelated_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("log retention selection")?;
    let directory = temporary.path();
    for index in 0..25 {
        let path = directory.join(name(index));
        fs::write(&path, index.to_string())?;
        stamp(&path, 1_000_000_000 + index as u64)?;
    }
    let retained_directory = directory.join(name(100));
    fs::create_dir(&retained_directory)?;
    fs::write(retained_directory.join("evidence"), b"untouched")?;
    let sibling = directory.join("native-retention-other_2020-01-01_00-00-00_UTC.log");
    fs::write(&sibling, b"other component")?;
    let linked = directory.join(name(103));
    fs::hard_link(&sibling, &linked)?;
    stamp(&linked, 800_000_000)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&sibling, directory.join(name(101)))?;
        std::os::unix::fs::symlink("absent", directory.join(name(102)))?;
    }
    let log = RuntimeLog::new(directory, "native-retention");
    assert!(log.path().is_some());
    assert_eq!(fs::read(retained_directory.join("evidence"))?, b"untouched");
    assert_eq!(fs::read(&sibling)?, b"other component");
    assert_eq!(
        fs::read(&linked)?,
        b"other component",
        "Hardlinked log was pruned"
    );
    #[cfg(unix)]
    for index in [101, 102] {
        assert!(fs::symlink_metadata(directory.join(name(index)))?.is_symlink());
    }
    log.message("INFO", "fixture", "Selection complete");
    assert!(fs::read_to_string(log.path().ok_or("Log missing")?)?.contains("Selection complete"));
    drop(log);
    assert_eq!(
        (0..25)
            .filter(|index| directory.join(name(*index)).exists())
            .count(),
        19
    );
    temporary.cleanup()?;
    Ok(())
}
