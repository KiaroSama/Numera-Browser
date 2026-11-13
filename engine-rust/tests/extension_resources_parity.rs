use numera_core::storage::extensions;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[path = "network_characterization/mod.rs"]
mod characterization;
mod common;

#[test]
fn extension_path_whitespace_matches_original_validation() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = common::TempDirectory::new("extension-text")?;
    let inputs = temporary.path().join("extension inputs");
    let valid = inputs.join("valid");
    fs::create_dir_all(&valid)?;
    fs::create_dir_all(inputs.join("missing-manifest"))?;
    let protected = [
        (
            valid.join("manifest.json"),
            b"{\"name\":\"fixture\"}".to_vec(),
        ),
        (valid.join("fixture.bin"), b"\x00fixture\xff".to_vec()),
    ];
    for (path, bytes) in &protected {
        fs::write(path, bytes)?;
    }
    let cases: Vec<Value> = serde_json::from_value(characterization::load(
        include_str!("fixtures/network-characterization/extension-text.json"),
        &inputs,
    )?)?;
    assert_eq!(cases.len(), 72, "Extension text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let entries = case["entries"]
            .as_array()
            .ok_or("Extension entries missing")?
            .iter()
            .map(|value| value.as_str().map(str::to_owned).ok_or("Entry is not text"))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            serde_json::to_value(extensions::unloadable(&entries))?,
            case["result"],
            "Extension path boundary whitespace differs: case={index}"
        );
        assert_eq!(case["unchanged"], serde_json::json!([true, true]));
        for (path, before) in &protected {
            assert_eq!(
                fs::read(path)?,
                *before,
                "Extension validation changed fixture bytes"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn malformed_extension_text_matches_original_refusal_without_changing_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("extension-utf8")?;
    let inputs: PathBuf = temporary.path().components().collect();
    let cases: Vec<Value> = serde_json::from_value(characterization::load(
        include_str!("fixtures/network-characterization/extension-utf8.json"),
        &inputs,
    )?)?;
    assert_eq!(cases.len(), 24, "Extension resource observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = inputs.join("cases").join(index.to_string());
        let folder = root
            .join("extensions")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&folder)?;
        for (name, raw) in case["files"].as_object().ok_or("Resource files missing")? {
            if let Some(raw) = raw.as_str() {
                let bytes = (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()?;
                let path = folder.join(name);
                fs::create_dir_all(path.parent().ok_or("Resource parent missing")?)?;
                fs::write(path, bytes)?;
            }
        }
        let result = match case["operation"].as_str() {
            Some("describe") => extensions::describe(&folder)
                .and_then(|value| serde_json::to_value(value).map_err(std::io::Error::other)),
            Some("installed") => extensions::installed(&root)
                .and_then(|value| serde_json::to_value(value).map_err(std::io::Error::other)),
            _ => return Err("Unknown extension observation".into()),
        };
        assert_eq!(
            result.is_ok(),
            case["error"].is_null(),
            "Extension resource text admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(value, case["result"], "Extension resource result differs"),
            Err(error) => {
                assert_eq!(case["error"], "UnicodeDecodeError");
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("UTF-8"));
            }
        }
        assert_eq!(case["unchanged"], serde_json::json!([true, true, true]));
        for (name, raw) in case["files"].as_object().ok_or("Resource files missing")? {
            let path = folder.join(name);
            if let Some(raw) = raw.as_str() {
                let before = (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()?;
                assert_eq!(fs::read(path)?, before, "Extension resource bytes changed");
            } else {
                assert!(
                    !path.try_exists()?,
                    "Extension resource read created a file"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn extension_icon_integer_ranking_matches_original_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("extension-ranking")?;
    let inputs: PathBuf = temporary.path().components().collect();
    let observed: Value = characterization::load(
        include_str!("fixtures/network-characterization/extension-ranking.json"),
        &inputs,
    )?;
    assert_eq!(
        observed["digit_limit"], 4300,
        "Original integer limit differs"
    );
    let cases = observed["cases"].as_array().ok_or("Icon cases missing")?;
    assert_eq!(cases.len(), 136, "Icon ranking observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = inputs.join("cases").join(index.to_string());
        let folder = root
            .join("extensions")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&folder)?;
        for (name, text) in case["files"].as_object().ok_or("Icon files missing")? {
            let path = folder.join(name);
            fs::create_dir_all(path.parent().ok_or("Icon parent missing")?)?;
            fs::write(
                path,
                text.as_str().ok_or("Icon fixture is not text")?.as_bytes(),
            )?;
        }
        let result = match case["operation"].as_str() {
            Some("describe") => serde_json::to_value(extensions::describe(&folder)?)?,
            Some("installed") => serde_json::to_value(extensions::installed(&root)?)?,
            _ => return Err("Unknown icon observation".into()),
        };
        assert_eq!(
            result, case["result"],
            "Extension icon integer ranking differs: case={index} label={}",
            case["label"]
        );
        assert_eq!(case["unchanged"], serde_json::json!([true, true, true]));
        for (name, text) in case["files"].as_object().ok_or("Icon files missing")? {
            assert_eq!(
                fs::read(folder.join(name))?,
                text.as_str().ok_or("Icon fixture is not text")?.as_bytes(),
                "Icon selection changed fixture bytes"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

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
