use numera_core::storage::extensions;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn installed_extensions_preserve_localized_name_and_contained_icon_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/extension-resources.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Extension cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("extension-resource-{}-{index}", std::process::id()));
        let folder = root.join("extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&folder)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (name, value) in case["files"].as_object().ok_or("Resource files missing")? {
                let path = folder.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    value.as_str().ok_or("Text resource missing")?.as_bytes(),
                )?;
            }
            fs::write(root.join("extensions/outside.png"), b"outside")?;
            let description = extensions::describe(&folder)?;
            assert_eq!(description.name, case["expected_name"], "{}", case["name"]);
            let icon = description
                .icon
                .map(|path| {
                    path.strip_prefix(&folder)
                        .map(|path| path.to_string_lossy().replace('\\', "/"))
                })
                .transpose()?;
            assert_eq!(
                serde_json::to_value(icon)?,
                case["icon"],
                "{}",
                case["name"]
            );
            let ids: Vec<_> = extensions::installed(&root)?
                .into_iter()
                .map(|entry| entry.id)
                .collect();
            assert_eq!(serde_json::to_value(ids)?, case["installed_ids"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
