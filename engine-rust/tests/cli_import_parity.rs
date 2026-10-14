use numera_core::{
    browser::adapter::Adapter,
    cli::{self, profile_commands::Profiles},
    import_format,
    storage::{config::ConfigStore, repo::ProfileRepo},
};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};
mod common;

#[test]
fn vendor_cli_import_preserves_generated_identity_secret_intent_and_skip_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("cli-import")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let mut command = Command::new(python);
    command.args(["-u","-c",r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from cli_import_cases import cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT'])}}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",temporary.path().join("source")).env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        temporary.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-import-cli",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let root = temporary.path().join("native");
    let repo = ProfileRepo::open(root.clone(), None, None)?;
    let config = ConfigStore::new(root.clone());
    let installed = BTreeMap::new();
    let services = Profiles {
        repo: &repo,
        config: &config,
        installed: &installed,
        host: "fixture-host",
    };
    let sessions =
        numera_core::ownership::registry::Registry::new(root.clone(), "fixture-host".into(), None)?;
    let application = cli::application::Application {
        root: &root,
        home: &root,
        repo: &repo,
        installed: &installed,
        host: "fixture-host",
        password: None,
        environment_token: false,
        sessions: &sessions,
        browser: None,
    };
    let cases = document["cases"].as_array().ok_or("Vendor cases missing")?;
    assert_eq!(cases.len(), 10);
    let mut native_id: Option<String> = None;
    for case in cases {
        let filename = case["file"].as_str().ok_or("Import filename missing")?;
        let path = root.join(filename);
        std::fs::write(&path, serde_json::to_vec(&case["input"])?)?;
        let mut outcome = if filename == "vendor.json" {
            services.import_vendor(&import_format::vendor(&case["input"])?, 1000.0, || Ok(42))?
        } else {
            let mut registry = cli::registry();
            registry.build();
            let matches = registry.clone().try_get_matches_from([
                "numera",
                "import-from",
                path.to_str().ok_or("Fixture path not Unicode")?,
                "--json",
            ])?;
            let values = cli::reflection::values(&registry, &matches);
            application.execute(
                values.as_object().ok_or("Import args missing")?,
                1000.0,
                |_| panic!("Import fixture prompted"),
            )?
        };
        let profiles = repo.list(1000.0)?;
        if filename == "invalid.json" {
            assert_eq!(
                profiles.len(),
                1,
                "Mixed invalid import partially wrote its valid row"
            );
            for directory in ["profiles", "dashboard"] {
                let records = std::fs::read_dir(root.join(directory))?
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|value| value == "json")
                    })
                    .count();
                assert_eq!(
                    records, 1,
                    "Mixed invalid import added a {directory} record"
                );
            }
        }
        if native_id.is_none()
            && let Some(profile) = profiles.first()
        {
            assert_eq!(profile.id.len(), 12);
            assert!(profile.id.bytes().all(|b| b.is_ascii_hexdigit()));
            native_id = Some(profile.id.clone());
        }
        if let Some(id) = &native_id {
            normalize_id(&mut outcome.payload, id);
        }
        if let Some(error) = outcome.payload["error"]
            .as_str()
            .filter(|e| e.starts_with("couldn't read "))
            .map(str::to_owned)
        {
            outcome.payload["error"] = error
                .replacen(path.to_str().ok_or("Import path missing")?, filename, 1)
                .into();
        }
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Source import exit missing")? as i32
        );
        assert_eq!(outcome.payload, case["payload"]);
        let mut records = serde_json::to_value(profiles)?;
        if let Some(id) = &native_id {
            normalize_id(&mut records, id);
        }
        assert_eq!(records, case["profiles"]);
        for kind in ["profiles", "dashboard"] {
            let id = native_id.as_ref().ok_or("Imported identity missing")?;
            let persisted: Value = serde_json::from_slice(&std::fs::read(
                root.join(kind).join(format!("{id}.json")),
            )?)?;
            assert_eq!(
                persisted["proxy"], case["persisted_proxy"][kind],
                "{kind} imported credential lost"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}
fn normalize_id(value: &mut Value, id: &str) {
    match value {
        Value::Array(rows) => {
            for row in rows {
                normalize_id(row, id);
            }
        }
        Value::Object(rows) => {
            for (key, value) in rows {
                if key == "id" && value.as_str() == Some(id) {
                    *value = "vendorfixture".into();
                } else {
                    normalize_id(value, id);
                }
            }
        }
        _ => {}
    }
}
