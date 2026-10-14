use numera_core::{
    browser::{
        adapter::Adapter,
        extensions,
        launch::{self, PreparedLaunch},
    },
    models::Profile,
    ownership::session::Session,
};
use serde_json::{Map, Value, json};
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
fn native_extension_paths_preserve_original_order_home_and_directory_filter()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("extension-launch-paths")?;
    let root = temporary.path().to_owned();
    let cwd = root.join("working");
    let home = root.join("home");
    for path in [
        cwd.join("extension α"),
        home.join("extension"),
        cwd.join("comma,folder"),
    ] {
        std::fs::create_dir_all(path)?;
    }
    std::fs::write(cwd.join("not-directory"), b"fixture")?;
    #[cfg(windows)]
    let drive_entries = {
        use std::path::{Component, Prefix};
        let drive = match cwd.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive) => drive,
                _ => return Err("Fixture drive missing".into()),
            },
            _ => return Err("Fixture prefix missing".into()),
        };
        let other = if drive == b'Z' { b'Y' } else { b'Z' };
        assert!(
            extensions::paths(&[format!("{}:extension α", char::from(other))], &cwd, &home)
                .is_err()
        );
        vec![format!("{}:extension α", char::from(drive))]
    };
    #[cfg(not(windows))]
    let drive_entries: Vec<String> = Vec::new();
    let entries: Vec<String> = vec![
        "extension α".into(),
        "~/extension".into(),
        "missing".into(),
        "not-directory".into(),
        "extension α/../extension α".into(),
        "comma,folder".into(),
        "extension α".into(),
    ]
    .into_iter()
    .chain(drive_entries)
    .collect();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let profile = Profile::from_record(&json!({
        "id":"a", "name":"fixture", "fingerprint":fixture["cases"][0]["input"],
        "extensions":entries,
    }))?;
    let script = r#"
import json, sys
from numera.launch_options import _extension_args
from numera.models import Profile
for line in sys.stdin:
    request = json.loads(line)
    result = {} if request['operation'] == 'hello' else {'flags':_extension_args(Profile.from_dict(request['profile']))}
    print(json.dumps({'protocol':1, 'id':request['id'], 'ok':True, 'result':result}), flush=True)
"#;
    let mut command = Command::new(&python);
    command
        .args(["-u", "-c", script])
        .current_dir(&cwd)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("PYTHONUTF8", "1");
    let engine = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("engine");
    command.env("PYTHONPATH", engine);
    let mut source = Adapter::spawn(
        &root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let expected = source.call(
        "extension-flags",
        Map::from_iter([("profile".into(), serde_json::to_value(&profile)?)]),
        Duration::from_secs(5),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let paths = extensions::paths(&entries, &cwd, &home)?;
    assert_eq!(paths.len(), if cfg!(windows) { 6 } else { 5 });
    #[cfg(windows)]
    assert_eq!(
        paths[0], paths[5],
        "Drive-relative path used ambient rather than supplied CWD"
    );
    assert_eq!(paths[0], paths[2]);
    assert_eq!(paths[0], paths[4]);
    let capture = root.join("open.json");
    let script = r#"
import json, os, pathlib, sys
for line in sys.stdin:
    request = json.loads(line)
    if request['operation'] == 'hello':
        result = {'backends':{'playwright':{'installed':True}}}
    elif request['operation'] == 'open':
        pathlib.Path(os.environ['FIXTURE_CAPTURE']).write_text(json.dumps(request), encoding='utf-8')
        result = {'opened':True, 'backend':'playwright'}
    else:
        raise ValueError('Unexpected operation')
    print(json.dumps({'protocol':1, 'id':request['id'], 'ok':True, 'result':result}), flush=True)
"#;
    let mut command = Command::new(&python);
    command
        .args(["-u", "-c", script])
        .env("FIXTURE_CAPTURE", &capture);
    let session = Session::admit(&root, "a", "fixture-host")?;
    let mut adapter = launch::open(
        session,
        profile,
        &mut command,
        PreparedLaunch {
            backend: "playwright",
            headless: true,
            extra_args: &[],
            cwd: &cwd,
            home: &home,
            country: None,
            relay: None,
            binary_options: Map::new(),
        },
        1000.0,
    )?;
    adapter.finish(Duration::from_secs(5))?;
    drop(adapter);
    let frame: Value = serde_json::from_slice(&std::fs::read(capture)?)?;
    let flags: Vec<_> = frame["options"]["args"]
        .as_array()
        .ok_or("Launch args missing")?
        .iter()
        .filter(|value| {
            value.as_str().is_some_and(|value| {
                value.starts_with("--load-extension=")
                    || value.starts_with("--disable-extensions-except=")
            })
        })
        .cloned()
        .collect();
    assert_eq!(json!(flags), expected["flags"]);
    assert!(extensions::paths(&[], Path::new("relative"), &home).is_err());
    assert!(extensions::paths(&["~unknown/extension".into()], &cwd, &home).is_err());
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn portable_backend_selection_matches_original_bundle_options_and_overrides()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::browser::bundled;
    let temporary = common::TempDirectory::new("bundled-resources")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(python);
    command.args(["-u", "-c", r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from bundled_cases import cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT'])}}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",temporary.path().join("source"))
        .env("PYTHONPATH",std::env::join_paths([repository.join("engine"),repository.join("scripts/migration")])?);
    let mut source = Adapter::spawn(
        temporary.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let observations = source.call("bundled-options", Map::new(), Duration::from_secs(15))?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let cases = observations["cases"]
        .as_array()
        .ok_or("Bundle cases missing")?;
    assert_eq!(cases.len(), 7);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let browsers = root.join("browsers");
        for backend in ["playwright", "patchright", "camoufox", "cloak"] {
            std::fs::create_dir_all(browsers.join(backend))?;
            std::fs::write(
                browsers.join(backend).join("browser.bin"),
                backend.as_bytes(),
            )?;
        }
        std::fs::create_dir_all(browsers.join("camoufox/addons/UBO"))?;
        std::fs::write(browsers.join("camoufox/addons/UBO/manifest.json"), b"{}")?;
        std::fs::write(
            browsers.join("manifest.json"),
            serde_json::to_vec(&case["manifest"])?,
        )?;
        let environment = serde_json::from_value(case["environment"].clone())?;
        let selected = bundled::select(
            Some(&browsers),
            case["engine"].as_str().ok_or("Backend missing")?,
            &root.join("data"),
            &environment,
            &["UBO".into()],
        )?;
        let normalized = |value: Value| -> Result<Value, Box<dyn std::error::Error>> {
            let root = root.canonicalize()?;
            let text = root.to_str().ok_or("Fixture root nontext")?;
            let text = text.strip_prefix(r"\\?\").unwrap_or(text);
            fn paths(value: &mut Value, root: &str) {
                match value {
                    Value::String(text) => *text = text.replace(root, "$ROOT"),
                    Value::Array(values) => values.iter_mut().for_each(|value| paths(value, root)),
                    Value::Object(values) => {
                        values.values_mut().for_each(|value| paths(value, root))
                    }
                    _ => {}
                }
            }
            let mut value = value;
            paths(&mut value, text);
            Ok(value)
        };
        assert_eq!(
            normalized(Value::Object(selected.options))?,
            case["options"]
        );
        assert_eq!(
            normalized(serde_json::to_value(selected.environment)?)?,
            case["changes"]
        );
        assert_eq!(bundled::discover(&root)?, Some(browsers.clone()));
        if case["engine"] == "camoufox" {
            let source = browsers.join("camoufox-data/mmdb/fixture.mmdb");
            std::fs::create_dir_all(source.parent().ok_or("GeoIP parent missing")?)?;
            std::fs::write(&source, b"fixture-database")?;
            let file = std::fs::File::options().write(true).open(&source)?;
            let modified = std::time::UNIX_EPOCH + Duration::from_secs(1700000000);
            file.set_times(std::fs::FileTimes::new().set_modified(modified))?;
            drop(file);
            let mut manifest = case["manifest"].clone();
            manifest["_geoip"] = json!({"camoufox":"camoufox-data"});
            std::fs::write(
                browsers.join("manifest.json"),
                serde_json::to_vec(&manifest)?,
            )?;
            let data = root.join("data");
            let selected = bundled::select(
                Some(&browsers),
                "camoufox",
                &data,
                &environment,
                &["UBO".into()],
            )?;
            assert_eq!(selected.geoip, Some(data.join("browser-cache/camoufox")));
            let cache = data.join("browser-cache/camoufox/mmdb/fixture.mmdb");
            assert_eq!(std::fs::read(&cache)?, b"fixture-database");
            assert_eq!(std::fs::metadata(&cache)?.modified()?, modified);
            std::fs::write(&cache, b"vendor-refreshed")?;
            bundled::select(
                Some(&browsers),
                "camoufox",
                &data,
                &environment,
                &["UBO".into()],
            )?;
            assert_eq!(std::fs::read(&cache)?, b"vendor-refreshed");
            let capture = root.join("packaged-open.json");
            let script = root.join("capture-sdk.py");
            std::fs::write(
                &script,
                r#"
import json,os,pathlib,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'backends':{'camoufox':{'installed':True}}}}),flush=True)
request=json.loads(sys.stdin.readline())
pathlib.Path(os.environ['FIXTURE_CAPTURE']).write_text(json.dumps({'frame':request,'geoip':os.environ.get('NUMERA_ADAPTER_GEOIP_DIR')}),encoding='utf-8')
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'opened':True,'backend':'camoufox'}}),flush=True)
sys.stdin.readline()
"#,
            )?;
            let runtime = numera_core::browser::runtime::Runtime {
                python: std::env::var_os("NUMERA_ORACLE_PYTHON")
                    .ok_or("Fixture interpreter missing")?
                    .into(),
                adapter: script,
                cwd: root.clone(),
                home: root.clone(),
                logs: root.join("logs"),
                country: None,
                relay: None,
                binary_options: Map::new(),
                deployment: Some(numera_core::browser::runtime::Deployment {
                    app: root.clone(),
                    environment: environment.clone(),
                    default_addons: vec!["UBO".into()],
                }),
            };
            let fingerprint: Value =
                serde_json::from_str(include_str!("fixtures/init-script.json"))?;
            let profile = Profile::from_record(
                &json!({"id":"packaged","name":"fixture","engine":"camoufox","fingerprint":fingerprint["cases"][0]["input"]}),
            )?;
            let session = Session::admit(&data, "packaged", "fixture-host")?;
            // Capture location is part of the owned script, never a global environment mutation.
            let code = std::fs::read_to_string(&runtime.adapter)?.replace(
                "os.environ['FIXTURE_CAPTURE']",
                &serde_json::to_string(capture.to_str().ok_or("Capture path nontext")?)?,
            );
            std::fs::write(&runtime.adapter, code)?;
            let mut adapter = runtime.open_lifetime(session, profile, true, false, 1000.0)?;
            adapter.finish(Duration::from_secs(5))?;
            drop(adapter);
            let observed: Value = serde_json::from_slice(&std::fs::read(capture)?)?;
            assert_eq!(
                observed["geoip"],
                serde_json::to_value(data.join("browser-cache").join("camoufox"))?
            );
            assert_eq!(observed["frame"]["options"]["ff_version"], "152");
            assert_eq!(
                observed["frame"]["options"]["exclude_addons"],
                json!(["UBO"])
            );
            assert_eq!(
                Path::new(
                    observed["frame"]["options"]["executable_path"]
                        .as_str()
                        .ok_or("Executable path missing")?
                )
                .canonicalize()?,
                browsers
                    .join("camoufox")
                    .join("browser.bin")
                    .canonicalize()?
            );
            assert!(matches!(
                numera_core::ownership::claims::read(&data, "packaged"),
                numera_core::ownership::claims::Claim::Absent
            ));
            manifest["camoufox"]["executable"] = "../outside.bin".into();
            std::fs::write(
                browsers.join("manifest.json"),
                serde_json::to_vec(&manifest)?,
            )?;
            assert!(
                bundled::select(
                    Some(&browsers),
                    "camoufox",
                    &data,
                    &environment,
                    &["UBO".into()]
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&cache)?, b"vendor-refreshed");
        }
    }
    temporary.cleanup()?;
    Ok(())
}
