use numera_core::storage::first_run;
use serde_json::{Map, Value};
use std::{fs, path::Path};

fn snapshot(base: &Path, directory: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let mut result = Map::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .strip_prefix(base)?
            .to_str()
            .ok_or("NonUnicode fixture path")?
            .replace('\\', "/");
        if path.is_dir() {
            result.insert(name, Value::Null);
            result.extend(
                snapshot(base, &path)?
                    .as_object()
                    .ok_or("Snapshot missing")?
                    .clone(),
            );
        } else {
            result.insert(name, serde_json::to_value(fs::read(path)?)?);
        }
    }
    Ok(Value::Object(result))
}

#[test]
fn new_subtrees_preserve_original_nested_lock_entries() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("nested first-run α")?;
    let root: std::path::PathBuf = temporary.path().components().collect();
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = super::support::load(
        include_str!(
            "../fixtures/characterization/new_subtrees_preserve_original_nested_lock_entries.json"
        ),
        &root,
    )?;
    assert_eq!(rows.len(), 3, "Nested-copy observations missing");
    for (row, mode) in rows.iter().zip(["new", "mixed", "existing"]) {
        assert_eq!(row["mode"], mode);
        let source = Path::new(row["source"].as_str().ok_or("Source missing")?);
        let destination = Path::new(row["destination"].as_str().ok_or("Destination missing")?);
        fs::create_dir_all(source.join("profiles/.locks"))?;
        fs::write(
            source.join("profiles/.locks/root.lock"),
            b"ignored merge lock",
        )?;
        fs::create_dir_all(source.join("profiles/branch/.locks/nested"))?;
        fs::write(
            source.join("profiles/branch/.locks/state.lock"),
            b"\x00\xffretained state\r\n",
        )?;
        fs::write(source.join("profiles/branch/.locks/nested/empty.lock"), b"")?;
        fs::create_dir_all(source.join("profiles/branch/deeper/.locks"))?;
        fs::write(
            source.join("profiles/branch/deeper/.locks/deep.lock"),
            b"deep state",
        )?;
        fs::write(
            source.join("profiles/branch/deeper/ordinary.bin"),
            b"ordinary\x00",
        )?;
        fs::create_dir_all(destination)?;
        if mode != "new" {
            fs::create_dir_all(destination.join("profiles/branch/.locks"))?;
            fs::write(
                destination.join("profiles/branch/.locks/state.lock"),
                b"destination wins",
            )?;
        }
        if mode == "existing" {
            fs::create_dir(destination.join("profiles/branch/deeper"))?;
        }
        assert_eq!(snapshot(source, source)?, row["before"]);
        let first = first_run::import_from(source, destination)?;
        assert_eq!(
            serde_json::to_value(first)?,
            row["first"],
            "Nested-copy summary differs mode={mode}"
        );
        assert_eq!(
            snapshot(destination, destination)?,
            row["written"],
            "First-run nested copy differs mode={mode}"
        );
        assert_eq!(
            snapshot(source, source)?,
            row["before"],
            "Nested-copy source was modified"
        );
        assert_eq!(
            serde_json::to_value(first_run::import_from(source, destination)?)?,
            row["again"]
        );
        assert_eq!(snapshot(destination, destination)?, row["written"]);
    }
    temporary.cleanup()?;
    Ok(())
}
