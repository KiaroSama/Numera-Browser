use numera_core::{ownership::capture, storage::first_run::run_if_needed};
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command, time::Duration};

#[test]
fn advisory_profile_count_matches_original_when_destination_scan_is_denied()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("import profile count")?;
    let root: std::path::PathBuf = temporary.path().components().collect();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args(["-I", "-B", "-c", r#"
import json,shutil,sys
from pathlib import Path
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import firstrun
from run_logging import run_log

def prepare(case,populated):
    source,destination=case/'source 日本語',case/'destination with spaces'
    (source/'profiles').mkdir(parents=True)
    (destination/'profiles').mkdir(parents=True)
    (source/'schedules.json').write_bytes(b'independent')
    if populated:
        (source/'profiles/a.json').write_bytes(b'profile\x00\xff')
        (destination/'profiles/kept.json').write_bytes(b'destination wins')
    return source,destination

rows=[]
with run_log('import-count-oracle',root/'logs') as log:
    for denied in (True,False):
        for populated in (False,True):
            case=root/str(len(rows))
            source,destination=prepare(case,populated)
            profiles=destination/'profiles'
            try:
                if denied:
                    profiles.chmod(0o300)
                    try:
                        list(profiles.iterdir())
                    except PermissionError:
                        pass
                    else:
                        raise AssertionError('Destination scan denial is ineffective')
                with patch('numera.firstrun.legacy_dirs',return_value=[source]):
                    first=firstrun.run_if_needed(destination)
                    again=firstrun.run_if_needed(destination)
                marker=list((destination/'.imported').read_bytes())
            finally:
                profiles.chmod(0o700)
            assert not first['failed'] and again is None
            assert first['profiles']==(0 if denied else 2 if populated else 0)
            rows.append(dict(denied=denied,populated=populated,first=first,again=again,marker=marker))
            shutil.rmtree(case)
            prepare(case,populated)
    log.info('Observed advisory profile-count cases=%s',len(rows))
print(json.dumps(rows,ensure_ascii=True))
"#])
        .arg(repository)
        .arg(&root);
    let mut observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(15),
            idle: Duration::from_secs(10),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    );
    if let Err(error) = &mut observed {
        error.settle(Duration::from_secs(5))?;
    }
    // A terminated observer cannot execute its Python finally block.
    for index in 0..4 {
        let profiles = root
            .join(index.to_string())
            .join("destination with spaces/profiles");
        match fs::set_permissions(profiles, fs::Permissions::from_mode(0o700)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let observed = observed?;
    assert_eq!(
        observed.exit,
        0,
        "Original advisory count observer failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(rows.len(), 4, "Advisory profile-count observations missing");
    for (index, row) in rows.iter().enumerate() {
        let case = root.join(index.to_string());
        let source = case.join("source 日本語");
        let destination = case.join("destination with spaces");
        let profiles = destination.join("profiles");
        let denied = row["denied"].as_bool().ok_or("Denial missing")?;
        let populated = row["populated"].as_bool().ok_or("Input missing")?;
        if denied {
            fs::set_permissions(&profiles, fs::Permissions::from_mode(0o300))?;
        }
        let scan = fs::read_dir(&profiles);
        let first = run_if_needed(&destination, std::slice::from_ref(&source));
        // Restore the owned directory before errors, assertions or recursive cleanup.
        fs::set_permissions(&profiles, fs::Permissions::from_mode(0o700))?;
        if denied {
            assert_eq!(
                scan.err().map(|error| error.kind()),
                Some(std::io::ErrorKind::PermissionDenied)
            );
        } else {
            drop(scan?);
        }
        for base in [&source, &destination] {
            assert_eq!(fs::read(base.join("schedules.json"))?, b"independent");
            if populated {
                assert_eq!(fs::read(base.join("profiles/a.json"))?, b"profile\x00\xff");
            }
        }
        if populated {
            assert_eq!(fs::read(profiles.join("kept.json"))?, b"destination wins");
        }
        let first = match first {
            Ok(result) => result,
            Err(error) if denied && error.kind() == std::io::ErrorKind::PermissionDenied => {
                panic!("First-run advisory profile count differs: {error}")
            }
            Err(error) => return Err(format!("First-run observer failed: {error}").into()),
        };
        assert_eq!(
            serde_json::to_value(first)?,
            row["first"],
            "First-run advisory profile count differs case={index}"
        );
        assert_eq!(
            serde_json::to_value(run_if_needed(&destination, std::slice::from_ref(&source))?)?,
            row["again"]
        );
        assert_eq!(
            serde_json::to_value(fs::read(destination.join(".imported"))?)?,
            row["marker"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}
