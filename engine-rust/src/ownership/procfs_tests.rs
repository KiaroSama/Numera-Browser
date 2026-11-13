use super::{PROCFS_ROOT, record_empty};
use crate::common::TempDirectory;
use serde_json::json;
use std::fs;

#[test]
fn malformed_group_observations_match_original_without_releasing_claims()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDirectory::new("procfs-observation")?;
    let document = ownership_oracle::read("procfs-observation")?;
    let cases = document["cases"].clone();
    let running: Vec<bool> = serde_json::from_value(document["running"].clone())?;
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

#[path = "../../tests/common/ownership_oracle.rs"]
mod ownership_oracle;
