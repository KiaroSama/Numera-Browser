use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::storage::extension_install;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn files(base: &Path, directory: &Path, found: &mut Map<String, Value>) -> std::io::Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            files(base, &path, found)?;
        } else {
            let name = path
                .strip_prefix(base)
                .map_err(std::io::Error::other)?
                .to_str()
                .ok_or_else(|| std::io::Error::other("Invalid file name"))?
                .replace('\\', "/");
            found.insert(name, Value::String(STANDARD.encode(fs::read(path)?)));
        }
    }
    Ok(())
}
#[test]
fn extension_install_reuses_or_replaces_and_uninstall_reports_actual_removal()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/extension-install.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Install cases missing")?;
    assert!(!cases.is_empty());
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("extension-install-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let destination = root.join("extensions").join(id);
            for (name, value) in case["initial"].as_object().ok_or("Initial files missing")? {
                fs::create_dir_all(&destination)?;
                fs::write(
                    destination.join(name),
                    value.as_str().ok_or("File missing")?.as_bytes(),
                )?;
            }
            let package = STANDARD.decode(case["package"].as_str().ok_or("Package missing")?)?;
            let name = case["name"].as_str().ok_or("Variant missing")?;
            let mut calls = 0;
            let outcome = if matches!(name, "uninstall" | "already-gone") {
                extension_install::uninstall(&root, id).map(Value::Bool)
            } else {
                extension_install::install(&root,id,name=="force", |_| {calls+=1;Ok(package.clone())}).and_then(|installed|{
                    let path=installed.path.strip_prefix(&root).map_err(std::io::Error::other)?.to_string_lossy().replace('\\',"/");
                    Ok(serde_json::json!({"id":installed.id,"name":installed.name,"path":path,"reused":installed.reused}))
                })
            };
            match outcome {
                Ok(value) => {
                    assert!(case["error"].is_null(), "{name}");
                    assert_eq!(value, case["result"], "{name}");
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{name}"
                ),
            }
            assert_eq!(
                calls,
                case["download_calls"]
                    .as_u64()
                    .ok_or("Call count missing")?
            );
            assert_eq!(
                Value::Bool(destination.exists()),
                case["destination_present"]
            );
            let mut written = Map::new();
            files(&destination, &destination, &mut written)?;
            assert_eq!(Value::Object(written), case["written"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn refused_forced_install_keeps_previous_extension_and_cleans_staging()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/extension-install.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Install cases missing")?;
    let missing = cases
        .iter()
        .find(|case| case["name"] == "missing-manifest")
        .ok_or("Missing-manifest case absent")?;
    let package = STANDARD.decode(missing["package"].as_str().ok_or("Package missing")?)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("extension-refused-{}", std::process::id()));
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let destination = root.join("extensions").join(id);
    fs::create_dir_all(&destination)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let original = b"{\"name\":\"Original\",\"version\":\"1\"}";
        fs::write(destination.join("manifest.json"), original)?;
        fs::write(destination.join("old.txt"), b"old extension")?;
        assert!(extension_install::install(&root, id, true, |_| Ok(package)).is_err());
        assert_eq!(fs::read(destination.join("manifest.json"))?, original);
        assert_eq!(fs::read(destination.join("old.txt"))?, b"old extension");
        assert_eq!(fs::read_dir(root.join("extensions"))?.count(), 1);
        assert!(
            extension_install::install(&root, id, true, |_| Err(std::io::Error::other(
                "controlled download failure"
            )))
            .is_err()
        );
        assert_eq!(fs::read(destination.join("manifest.json"))?, original);
        assert_eq!(fs::read_dir(root.join("extensions"))?.count(), 1);
        // Download is outside root ownership; a concurrently committed install wins
        // when force was not requested, rather than being overwritten afterward.
        fs::remove_dir_all(&destination)?;
        let valid = cases
            .iter()
            .find(|case| case["name"] == "new")
            .ok_or("New install case absent")?;
        let concurrent = extension_install::install(&root, id, false, |_| {
            fs::create_dir_all(&destination)?;
            fs::write(destination.join("manifest.json"), original)?;
            fs::write(destination.join("old.txt"), b"concurrent install")?;
            STANDARD
                .decode(
                    valid["package"]
                        .as_str()
                        .ok_or_else(|| std::io::Error::other("Package missing"))?,
                )
                .map_err(std::io::Error::other)
        })?;
        assert!(
            concurrent.reused,
            "Concurrent installed extension wins the transaction-time recheck"
        );
        assert_eq!(
            fs::read(destination.join("old.txt"))?,
            b"concurrent install"
        );
        assert_eq!(fs::read_dir(root.join("extensions"))?.count(), 1);
        assert!(
            !fs::read_dir(&root)?.any(|entry| entry.is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("extension-install-")
            })),
            "Private staging must be removed after refusal or recheck"
        );
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
