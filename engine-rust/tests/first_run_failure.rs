use numera_core::storage::first_run::run_if_needed;
use std::{fs, path::PathBuf};

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
