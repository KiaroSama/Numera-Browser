use numera_core::{browser::resources, models::Profile, ownership::session::Session};
use serde_json::{Map, Value};
use std::{fs, path::Path};
mod common;

fn snapshot(
    root: &Path,
    path: &Path,
    values: &mut Map<String, Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            snapshot(root, &path, values)?;
        } else {
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("Path not Unicode")?
                .replace('\\', "/");
            let content = fs::read_to_string(&path)?;
            values.insert(
                relative.clone(),
                if relative == "Default/Bookmarks" {
                    serde_json::from_str(&content)?
                } else {
                    content.into()
                },
            );
        }
    }
    Ok(())
}

#[test]
fn clear_then_seed_preserves_original_bookmark_and_file_outcomes()
-> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-resources.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Resource cases missing")?;
    assert_eq!(cases.len(), 9);
    for case in cases {
        let temporary = common::TempDirectory::new("browser-resources")?;
        let directory = temporary.path().join("user-data/a");
        for (relative, content) in case["files"].as_object().ok_or("Files missing")? {
            let path = directory.join(relative);
            fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
            fs::write(path, content.as_str().ok_or("Fixture content missing")?)?;
        }
        let profile = Profile::from_record(&case["input"])?;
        let session = Session::admit(temporary.path(), "a", "fixture-host")?;
        resources::prepare(&session, &profile, 1000.0)?;
        let mut actual = Map::new();
        snapshot(&directory, &directory, &mut actual)?;
        assert_eq!(
            Value::Object(actual),
            case["expected"],
            "Resources {}",
            case["name"]
        );
        drop(session);
        temporary.cleanup()?;
    }
    Ok(())
}

#[test]
fn preparation_refuses_foreign_profile_and_linked_resource_without_touching_target()
-> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-resources.json"))?;
    let mut profile = Profile::from_record(&document["cases"][1]["input"])?;
    let temporary = common::TempDirectory::new("browser-resource-refusal")?;
    let root = temporary.path();
    let session = Session::admit(root, "a", "fixture-host")?;
    profile.id = "other".into();
    assert!(resources::prepare(&session, &profile, 1000.0).is_err());
    assert!(!root.join("user-data/other").exists());
    profile.id = "a".into();
    fs::create_dir_all(root.join("user-data/a"))?;
    fs::create_dir_all(root.join("outside"))?;
    fs::write(root.join("outside/History"), "preserve")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("outside"), root.join("user-data/a/Default"))?;
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(root.join("user-data/a/Default"))
            .arg(root.join("outside"))
            .output()?;
        assert!(
            status.status.success(),
            "Owned junction fixture creation failed"
        );
    }
    assert!(resources::prepare(&session, &profile, 1000.0).is_err());
    assert_eq!(
        fs::read_to_string(root.join("outside/History"))?,
        "preserve"
    );
    #[cfg(unix)]
    fs::remove_file(root.join("user-data/a/Default"))?;
    #[cfg(windows)]
    fs::remove_dir(root.join("user-data/a/Default"))?;
    drop(session);
    temporary.cleanup()?;
    Ok(())
}
