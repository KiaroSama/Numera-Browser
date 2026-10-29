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

#[test]
fn legacy_discovery_preserves_original_order_and_never_marks_denied_sources_complete()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{io, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("legacy discovery α")?;
    let inputs: PathBuf = temporary.path().components().collect();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut original = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    original.args(["-I", "-B", "-c", r#"
import json,os,subprocess,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import firstrun
rows=[]
for index,variant in enumerate(('denied-only','denied-first','none','missing','empty','profiles-first','proxies-first','self')):
    root=parent/str(index)
    destination=root/'destination'
    destination.mkdir(parents=True)
    (destination/'untouched.bin').write_bytes(b'preserved destination')
    first,second=root/'first',root/'second'
    if variant not in ('none','missing','empty','self'):
        if variant=='proxies-first':
            first.mkdir();(first/'proxies.json').write_bytes(b'[]')
        else:
            (first/'profiles').mkdir(parents=True)
            (first/'profiles/a.json').write_bytes(b'{"name":"first"}')
    if variant=='empty':first.mkdir()
    if variant not in ('denied-only','none'):
        (second/'profiles').mkdir(parents=True)
        (second/'profiles/b.json').write_bytes(b'{"name":"second"}')
    candidates=([destination,second] if variant=='self' else [first,second])
    def access(denied):
        if os.name=='nt':
            args=['/deny','*S-1-1-0:(OI)(CI)(RD,REA,RA)'] if denied else ['/remove:d','*S-1-1-0']
            subprocess.run(['icacls',str(first),*args],check=True,timeout=5,
                           stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        else:first.chmod(0 if denied else 0o700)
    denied=variant.startswith('denied')
    result=None;refused=False
    try:
        if denied:access(True)
        with patch('numera.firstrun.legacy_dirs',return_value=candidates):
            try:result=firstrun.run_if_needed(destination)
            except PermissionError:refused=True
    finally:
        if denied:access(False)
    assert refused==denied,'Original fixture did not cause the expected discovery refusal'
    marker=destination/'.imported'
    before=list(marker.read_bytes()) if marker.exists() else None
    assert (before is None) if denied else (before is not None)
    with patch('numera.firstrun.legacy_dirs',return_value=candidates):
        retry=firstrun.run_if_needed(destination)
    rows.append(dict(variant=variant,result=result,refused=refused,marker=before,
                     retry=retry,retried_marker=list(marker.read_bytes())))
print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(&inputs);
    let observed = capture::run(
        &mut original,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original legacy discovery observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 8, "Legacy discovery observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = inputs.join(index.to_string());
        fs::remove_dir_all(&root)?;
        let destination = root.join("destination");
        fs::create_dir_all(&destination)?;
        fs::write(destination.join("untouched.bin"), b"preserved destination")?;
        let first = root.join("first");
        let second = root.join("second");
        let variant = case["variant"]
            .as_str()
            .ok_or("Discovery variant missing")?;
        if !matches!(variant, "none" | "missing" | "empty" | "self") {
            if variant == "proxies-first" {
                fs::create_dir(&first)?;
                fs::write(first.join("proxies.json"), b"[]")?;
            } else {
                fs::create_dir_all(first.join("profiles"))?;
                fs::write(first.join("profiles/a.json"), br#"{"name":"first"}"#)?;
            }
        }
        if variant == "empty" {
            fs::create_dir(&first)?;
        }
        if !matches!(variant, "denied-only" | "none") {
            fs::create_dir_all(second.join("profiles"))?;
            fs::write(second.join("profiles/b.json"), br#"{"name":"second"}"#)?;
        }
        let candidates = if variant == "self" {
            vec![destination.clone(), second.clone()]
        } else {
            vec![first.clone(), second.clone()]
        };
        let mut source_before = Map::new();
        file_bytes(&root, &root, &mut source_before)?;
        let denied = case["refused"]
            .as_bool()
            .ok_or("Discovery verdict missing")?;
        let attempt = (|| -> io::Result<_> {
            if denied {
                discovery_access(&first, true)?;
            }
            run_if_needed(&destination, &candidates)
        })();
        // Restore the owned fixture before assertions or recursive cleanup.
        if denied {
            discovery_access(&first, false)?;
        }
        let actual = match attempt {
            Ok(result) => {
                assert!(!denied, "Legacy discovery refusal differs: case={index}");
                serde_json::to_value(result)?
            }
            Err(error) => {
                assert!(
                    denied && error.kind() == io::ErrorKind::PermissionDenied,
                    "Legacy discovery refusal differs: case={index}, kind={:?}",
                    error.kind()
                );
                Value::Null
            }
        };
        assert_eq!(
            actual, case["result"],
            "Legacy discovery selected source differs"
        );
        let marker = destination.join(".imported");
        let marker_text = if marker.try_exists()? {
            serde_json::to_value(fs::read(&marker)?)?
        } else {
            Value::Null
        };
        assert_eq!(
            marker_text, case["marker"],
            "Legacy discovery marker differs"
        );
        assert_eq!(
            fs::read(destination.join("untouched.bin"))?,
            b"preserved destination"
        );
        if denied {
            let mut unchanged = Map::new();
            file_bytes(&root, &root, &mut unchanged)?;
            assert_eq!(
                unchanged, source_before,
                "Denied discovery changed fixture bytes"
            );
        }
        let retried = run_if_needed(&destination, &candidates)?;
        assert_eq!(
            serde_json::to_value(retried)?,
            case["retry"],
            "Discovery retry differs"
        );
        assert_eq!(
            fs::read(marker)?,
            fixture_bytes(&case["retried_marker"])?,
            "Discovery retry marker bytes differ"
        );
        for candidate in [&first, &second] {
            for name in ["profiles/a.json", "profiles/b.json", "proxies.json"] {
                let path = candidate.join(name);
                if let Some(before) = source_before.get(
                    path.strip_prefix(&root)?
                        .to_str()
                        .ok_or("Discovery fixture path missing")?
                        .replace('\\', "/")
                        .as_str(),
                ) {
                    assert_eq!(
                        fs::read(path)?,
                        fixture_bytes(before)?,
                        "Legacy source changed"
                    );
                }
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

fn discovery_access(path: &Path, denied: bool) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if denied { 0 } else { 0o700 }),
        )
    }
    #[cfg(windows)]
    {
        use numera_core::ownership::capture;
        use std::{io, process::Command, time::Duration};
        let mut command = Command::new("icacls");
        command.arg(path).args(if denied {
            ["/deny", "*S-1-1-0:(OI)(CI)(RD,REA,RA)"]
        } else {
            ["/remove:d", "*S-1-1-0"]
        });
        let output = capture::run(
            &mut command,
            capture::Limits {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(5),
                bytes: 65536,
                cleanup: Duration::from_secs(3),
            },
        )
        .map_err(io::Error::other)?;
        if output.exit == 0 {
            Ok(())
        } else {
            Err(io::Error::other(
                "Owned legacy fixture permission change failed",
            ))
        }
    }
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
