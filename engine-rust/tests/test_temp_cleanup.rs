mod common;

#[test]
fn failed_case_unwinds_only_its_exclusively_owned_directory()
-> Result<(), Box<dyn std::error::Error>> {
    let mut created = None;
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let temporary = common::TempDirectory::new("unwind-fixture").expect("Create test root");
        created = Some(temporary.path().to_owned());
        std::fs::write(temporary.path().join("state.json"), b"{}").expect("Write owned state");
        panic!("Controlled test-case failure");
    }));
    assert!(failure.is_err());
    assert!(!created.ok_or("Owned path missing")?.exists());
    let ordinary = common::TempDirectory::new("ordinary-fixture")?;
    let path = ordinary.path().to_owned();
    ordinary.cleanup()?;
    assert!(!path.exists());
    Ok(())
}
