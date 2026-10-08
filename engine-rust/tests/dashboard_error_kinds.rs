use numera_core::storage::{dashboard::DashboardStore, repo::RepoError};
use serde_json::{Map, Value};
use std::fs;
mod common;

#[test]
fn dashboard_source_refusals_have_stable_typed_dispositions()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/dashboard-store.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Dashboard cases missing")?;
    let mut dispositions = std::collections::BTreeSet::new();
    for case in cases.iter().filter(|case| !case["error"].is_null()) {
        let temporary = common::TempDirectory::new("dashboard-error-kind")?;
        let root = temporary.path().to_path_buf();
        fs::create_dir(root.join("dashboard"))?;
        fs::write(
            root.join("dashboard-config.json"),
            serde_json::to_vec(&case["meta"])?,
        )?;
        for (path, value) in case["initial"]
            .as_object()
            .ok_or("Initial records missing")?
        {
            let path = root.join(path);
            fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
            fs::write(path, serde_json::to_vec(value)?)?;
        }
        let store = DashboardStore::new(root, None);
        let error = store
            .save(
                case["input"].clone(),
                Map::new(),
                case.get("expect_rev").filter(|value| !value.is_null()),
                None,
                1700000000.0,
            )
            .err()
            .ok_or("Refusal was accepted")?;
        assert_eq!(
            error.to_string(),
            case["error"].as_str().ok_or("Source refusal missing")?
        );
        match error {
            RepoError::StaleProfile(_) => {
                dispositions.insert("revision");
            }
            RepoError::NameTaken(_) => {
                dispositions.insert("name");
            }
            RepoError::UnconvertibleProfile(_) => {
                dispositions.insert("conversion");
            }
            other => return Err(format!("Untyped dashboard refusal: {other}").into()),
        }
        temporary.cleanup()?;
    }
    assert_eq!(
        dispositions,
        std::collections::BTreeSet::from(["conversion", "name", "revision"])
    );
    Ok(())
}
