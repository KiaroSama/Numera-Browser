use numera_core::{
    browser::{adapter::Adapter, extensions, options},
    models::Profile,
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
    let entries = vec![
        "extension α".into(),
        "~/extension".into(),
        "missing".into(),
        "not-directory".into(),
        "extension α/../extension α".into(),
        "comma,folder".into(),
        "extension α".into(),
    ];
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
    assert_eq!(paths.len(), 5);
    assert_eq!(paths[0], paths[2]);
    assert_eq!(paths[0], paths[4]);
    let actual = options::build(
        &profile,
        "playwright",
        "user-data/a",
        true,
        &[],
        &paths,
        None,
    )?;
    let flags: Vec<_> = actual["args"]
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
