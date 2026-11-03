use super::{PROCFS_ROOT, record_empty};
use crate::{common::TempDirectory, ownership::capture};
use serde_json::json;
use std::{fs, path::Path, process::Command, time::Duration};

#[test]
fn malformed_group_observations_match_original_without_releasing_claims()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDirectory::new("procfs-observation")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let cases = json!([
        {},
        {"101": "101 (fixture) S 1 123"},
        {"101": "101 (fixture) Z 1 123"},
        {"101": "101 (fixture) S 1 987"},
        {"101": "101 (fixture) S 1 invalid"},
        {"101": "101 (fixture) S 1"},
        {"101": "incomplete stat"},
        {"101": "101 (name with ) parentheses) S 1 123"},
        {"101": "101 (fixture) S 1 invalid", "102": "102 (fixture) S 1 123"},
        {"101": "101 (fixture) S 1 invalid", "102": "102 (fixture) S 1 987"},
        {"101": null},
        {"not-a-pid": "incomplete stat"},
        {"101": "101 (fixture) Z 1 invalid"},
        {"101": "101 (fixture) S 1 +123"}
    ]);
    let document = temporary.path().join("cases.json");
    fs::write(&document, serde_json::to_vec(&cases)?)?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
from unittest.mock import patch
repository,document,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import posix_session
from run_logging import run_log
cases=json.loads(document.read_text(encoding='utf-8'))
rows=[]
with run_log('procfs-observation-oracle',parent/'logs') as log:
    for index,case in enumerate(cases):
        root=parent/str(index);root.mkdir(parents=True)
        for pid,text in case.items():
            entry=root/pid;entry.mkdir()
            if text is not None:(entry/'stat').write_text(text,encoding='utf-8')
        before={str(p.relative_to(root)):p.read_bytes() for p in root.glob('*/stat')}
        with patch.object(posix_session,'Path',lambda value:root if value=='/proc' else Path(value)):
            running=posix_session._group_is_running(123)
        assert all((root/name).read_bytes()==raw for name,raw in before.items())
        rows.append(running)
    assert rows[4] is True,'Malformed original group did not retain ownership'
    assert rows[2] is False,'Original zombie control did not run'
    log.info('Observed process table cases=%s',len(rows))
print(json.dumps(rows))
"#]).arg(repository).arg(&document).arg(temporary.path().join("original"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original process-table observer failed");
    let running: Vec<bool> = serde_json::from_slice(&original.stdout)?;
    let cases = cases.as_array().ok_or("Process table cases missing")?;
    assert_eq!(running.len(), cases.len());

    struct RestoreRoot(std::path::PathBuf);
    impl Drop for RestoreRoot {
        fn drop(&mut self) {
            PROCFS_ROOT.with(|root| *root.borrow_mut() = self.0.clone());
        }
    }
    let previous = PROCFS_ROOT.with(|root| root.borrow().clone());
    let restore = RestoreRoot(previous);
    for (index, (case, running)) in cases.iter().zip(running).enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let mut protected = Vec::new();
        for (pid, text) in case.as_object().ok_or("Process table inputs missing")? {
            let entry = root.join(pid);
            fs::create_dir(&entry)?;
            if let Some(text) = text.as_str() {
                let path = entry.join("stat");
                fs::write(&path, text.as_bytes())?;
                protected.push((path, text.as_bytes().to_vec()));
            }
        }
        PROCFS_ROOT.with(|current| *current.borrow_mut() = root);
        let observed = record_empty(&json!({"tree":{"kind":"pgid","group":123}}));
        assert_eq!(
            !matches!(observed, Ok(true)),
            running,
            "Process group uncertainty differs: case={index}"
        );
        if index == 4 || index == 12 {
            assert!(observed.is_err(), "Malformed group must remain unknown");
        }
        for (path, bytes) in protected {
            assert_eq!(
                fs::read(path)?,
                bytes,
                "Process observation changed fixture bytes"
            );
        }
    }
    drop(restore);
    temporary.cleanup()?;
    Ok(())
}
