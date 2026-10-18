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

mod common;

#[test]
fn complete_first_run_summaries_match_original_paths_and_copy_results()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::first_run::import_from};
    use std::{process::Command, time::Duration};

    let temporary = common::TempDirectory::new("first-run-summary")?;
    let inputs: PathBuf = temporary.path().components().collect();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import firstrun
from run_logging import run_log

with run_log('first-run-summary-oracle',root/'logs') as log:
    rows=[]
    source_files={'profiles/a.json':b'{"name":"source"}',
                  'user-data/a/state.bin':bytes([0,255,10,13]),
                  'tokens.json':b'{"fixture":true}'}
    for operation in ['import_from','run_if_needed']:
        for variant in ['new','overlap','empty','marked']:
            case=root/'cases'/str(len(rows))
            source,destination=case/'source 日本語',case/'destination with spaces'
            destination.mkdir(parents=True)
            before={} if variant=='empty' else source_files
            initial=({'profiles/a.json':b'{"name":"destination"}',
                      'tokens.json':b'{"destination":true}'} if variant=='overlap' else {})
            if variant=='marked':
                initial['.imported']=b'existing marker\n'
            for base,entries in [(source,before),(destination,initial)]:
                for name,raw in entries.items():
                    path=base/name
                    path.parent.mkdir(parents=True,exist_ok=True)
                    path.write_bytes(raw)
            with patch('numera.firstrun.legacy_dirs',return_value=[source]):
                call=(lambda:firstrun.import_from(source,destination)) if operation=='import_from' else (lambda:firstrun.run_if_needed(destination))
                result=call()
                again=call()
            written={path.relative_to(destination).as_posix():list(path.read_bytes())
                     for path in sorted(destination.rglob('*'))
                     if path.is_file() and '.locks' not in path.relative_to(destination).parts
                     and path.name!='.imported'}
            rows.append({'operation':operation,'variant':variant,
                         'source':{name:list(raw) for name,raw in before.items()},
                         'initial':{name:list(raw) for name,raw in initial.items()},
                         'result':result,'again':again,'written':written,
                         'marker_present':(destination/'.imported').exists(),
                         'source_unchanged':all((source/name).read_bytes()==raw for name,raw in before.items())})
    log.info('Observed complete first-run summaries cases=%s',len(rows))
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
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original first-run summary observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 8, "First-run summary observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = inputs.join("cases").join(index.to_string());
        // Recreate the same input paths, not normalized expected results.
        fs::remove_dir_all(&root)?;
        let source = root.join("source 日本語");
        let destination = root.join("destination with spaces");
        fs::create_dir_all(&destination)?;
        for (base, key) in [(&source, "source"), (&destination, "initial")] {
            for (name, raw) in case[key].as_object().ok_or("Files missing")? {
                let path = base.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(path, fixture_bytes(raw)?)?;
            }
        }
        let call = || -> Result<Value, Box<dyn std::error::Error>> {
            match case["operation"].as_str() {
                Some("import_from") => {
                    Ok(serde_json::to_value(import_from(&source, &destination)?)?)
                }
                Some("run_if_needed") => Ok(serde_json::to_value(run_if_needed(
                    &destination,
                    std::slice::from_ref(&source),
                )?)?),
                _ => Err("Unknown first-run operation".into()),
            }
        };
        assert_eq!(
            call()?,
            case["result"],
            "Complete first-run summary differs: case={index}"
        );
        assert_eq!(call()?, case["again"], "Repeated first-run result differs");
        assert_eq!(case["source_unchanged"], true);
        for (name, raw) in case["source"].as_object().ok_or("Source missing")? {
            assert_eq!(
                fs::read(source.join(name))?,
                fixture_bytes(raw)?,
                "Source bytes changed"
            );
        }
        let mut written = Map::new();
        file_bytes(&destination, &destination, &mut written)?;
        assert_eq!(
            Value::Object(written),
            case["written"],
            "Destination bytes differ"
        );
        let marker = destination.join(".imported");
        assert_eq!(
            marker.try_exists()?,
            case["marker_present"]
                .as_bool()
                .ok_or("Marker verdict missing")?
        );
        if case["variant"] == "marked" {
            assert_eq!(
                fs::read(marker)?,
                fixture_bytes(&case["initial"][".imported"])?,
                "Existing marker changed"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

fn fixture_bytes(value: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(value.clone())?)
}

fn file_bytes(
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
            file_bytes(base, &path, out)?;
        } else {
            out.insert(
                path.strip_prefix(base)?
                    .to_str()
                    .ok_or("NonUnicode path")?
                    .replace('\\', "/"),
                serde_json::to_value(fs::read(&path)?)?,
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
            // These immutable fixtures cover the original path-independent subset.
            // The observer above compares complete summaries at both public seams.
            let mut result = serde_json::to_value(result)?;
            if let Some(summary) = result.as_object_mut() {
                summary.remove("source");
                summary.remove("destination");
            }
            assert_eq!(result, case["result"]);
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
