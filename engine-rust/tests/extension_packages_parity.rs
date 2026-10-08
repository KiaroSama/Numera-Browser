use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::storage::extension_package;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn files(base: &Path, directory: &Path, found: &mut Map<String, Value>) -> std::io::Result<()> {
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
fn extension_packages_validate_all_paths_before_extracting_any_member()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/extension-packages.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Package cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("extension-package-{}-{index}", std::process::id()));
        let destination = root.join("extension");
        fs::create_dir_all(&destination)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let package = STANDARD.decode(case["package"].as_str().ok_or("Package missing")?)?;
            match extension_package::unpack(
                &package,
                &destination,
                case["crx"].as_bool().ok_or("Format missing")?,
            ) {
                Ok(()) => assert!(case["error"].is_null(), "{}", case["name"]),
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            let mut written = Map::new();
            files(&destination, &destination, &mut written)?;
            assert_eq!(Value::Object(written), case["written"], "{}", case["name"]);
            assert!(!root.join("escaped.txt").exists());
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
