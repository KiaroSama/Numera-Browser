use numera_core::storage::first_run::run_if_needed;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn files(
    base: &Path,
    path: &Path,
    out: &mut Map<String, Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|v| v == ".locks" || v == ".imported")
        {
            continue;
        }
        if path.is_dir() {
            files(base, &path, out)?;
        } else {
            out.insert(
                path.strip_prefix(base)?
                    .to_str()
                    .ok_or("NonUnicode path")?
                    .replace('\\', "/"),
                fs::read_to_string(path)?.into(),
            );
        }
    }
    Ok(())
}

#[test]
fn first_run_copies_without_overwriting_and_never_marks_failed_work_complete()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/first-run.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("first-run-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("destination"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (kind, key) in [("source", "source"), ("destination", "initial")] {
                for (name, text) in case[key].as_object().ok_or("Files missing")? {
                    let path = root.join(kind).join(name);
                    fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                    fs::write(path, text.as_str().ok_or("Text missing")?)?;
                }
            }
            let result = run_if_needed(&root.join("destination"), &[root.join("source")])?;
            assert_eq!(serde_json::to_value(result)?, case["result"]);
            assert!(run_if_needed(&root.join("destination"), &[root.join("source")])?.is_none());
            let mut written = Map::new();
            files(
                &root.join("destination"),
                &root.join("destination"),
                &mut written,
            )?;
            assert_eq!(Value::Object(written), case["written"]);
            assert_eq!(
                root.join("destination/.imported").exists(),
                case["marker_present"]
                    .as_bool()
                    .ok_or("Marker outcome missing")?
            );
            for (name, text) in case["source"].as_object().ok_or("Source missing")? {
                assert_eq!(
                    fs::read_to_string(root.join("source").join(name))?,
                    text.as_str().ok_or("Text missing")?
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
