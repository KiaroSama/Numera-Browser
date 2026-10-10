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
