use numera_core::storage::extensions;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;

#[test]
fn extension_path_whitespace_matches_original_validation() -> Result<(), Box<dyn std::error::Error>>
{
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

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
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.store import unloadable_extensions
from run_logging import run_log

with run_log('extension-text-oracle',root/'logs') as log:
    protected=[root/'valid/manifest.json',root/'valid/fixture.bin']
    before=[path.read_bytes() for path in protected]
    rows=[]
    for marker in map(chr,[0x1c,0x1d,0x1e,0x1f,0x85,0xa0,0x2003,0x202f,
                           0x3000,0x20,0x58,0x394]):
        wrap=lambda text: marker+str(text)+marker
        for entries in [[wrap(root/'valid')],[wrap(root/'missing-manifest')],
                        [wrap(root/'absent')],[wrap('https://example.invalid/extension')],
                        [marker],[wrap(root/'valid'),'',wrap('https://example.invalid/extension')]]:
            rows.append({'entries':entries,'result':unloadable_extensions(entries),
                         'unchanged':[path.read_bytes()==raw for path,raw in zip(protected,before)]})
    log.info('Observed extension text cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(&inputs)
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original extension text observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
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
