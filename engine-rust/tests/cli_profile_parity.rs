use numera_core::{
    browser::adapter::Adapter,
    cli::{self, application::Application},
    storage::{config::ConfigStore, repo::ProfileRepo},
};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};
mod common;

#[test]
fn profile_command_services_match_original_payloads_and_verdicts()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("cli-profile-parity")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(python);
    command.args(["-u","-c",r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from cli_profile_cases import cases,human_cases
from cli_fleet_output_cases import cases as fleet_cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT']),'human':human_cases(os.environ['FIXTURE_ROOT'])+fleet_cases(),'mutationHuman':cases(os.path.join(os.environ['FIXTURE_ROOT'],'mutation-human'),human=True)}}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",temporary.path().join("source"))
        .env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        temporary.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-profile-cli",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let root = temporary.path().join("native");
    let repo = ProfileRepo::open(root.clone(), None, None)?;
    let config = ConfigStore::new(root.clone());
    let installed = BTreeMap::from([("playwright".into(), true), ("camoufox".into(), true)]);
    let sessions =
        numera_core::ownership::registry::Registry::new(root.clone(), "fixture-host".into(), None)?;
    let services = Application {
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
    let mut native_id = None;
    let mut registry = cli::registry();
    registry.build();
    let cases = document["cases"]
        .as_array()
        .ok_or("Profile CLI cases missing")?;
    assert_eq!(cases.len(), 24);
    for (index, case) in cases.iter().enumerate() {
        let argv = std::iter::once("numera").chain(
            case["argv"]
                .as_array()
                .ok_or("Source argv missing")?
                .iter()
                .map(|v| v.as_str().expect("Source argv text")),
        );
        let matches = registry.clone().try_get_matches_from(argv)?;
        let values = cli::reflection::values(&registry, &matches);
        let args = values.as_object().ok_or("CLI values missing")?;
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let mutation_output = matches!(
            values["command"].as_str(),
            Some("apply" | "delete" | "stop")
        );
        let mut outcome = if mutation_output {
            services.execute_with_output(
                args,
                1000.0,
                |_| panic!("Confirmed fixture prompted"),
                || false,
                || panic!("Mutation read stdin"),
                &mut output,
            )?
        } else {
            services.execute(args, 1000.0, |_| panic!("Confirmed fixture prompted"))?
        };
        if mutation_output {
            let (stdout, stderr) = output.into_parts();
            assert_eq!(
                String::from_utf8(stdout)?,
                document["mutationHuman"][index]["stdout"]
                    .as_str()
                    .ok_or("Mutation human bytes missing")?,
                "{}",
                case["argv"]
            );
            assert!(stderr.is_empty());
        }
        if native_id.is_none()
            && let Some(id) = outcome.payload["profile"]["id"].as_str()
        {
            assert_eq!(id.len(), 12);
            assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(repo.get(id)?.is_some());
            native_id = Some(id.to_owned());
        }
        if let Some(id) = &native_id {
            normalize_id(&mut outcome.payload, id);
        }
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Source exit missing")? as i32,
            "{}",
            case["argv"]
        );
        assert_eq!(outcome.payload, case["payload"], "{}", case["argv"]);
        for kind in ["profiles", "dashboard"] {
            let id = native_id.as_ref().ok_or("Native persisted ID missing")?;
            let path = root.join(kind).join(format!("{id}.json"));
            let stored: Value = if path.exists() {
                serde_json::from_slice(&std::fs::read(&path)?)?
            } else {
                assert!(
                    case["persisted_proxy"][kind].is_null(),
                    "Profile disappeared early"
                );
                Value::Null
            };
            assert_eq!(
                stored["proxy"], case["persisted_proxy"][kind],
                "{kind} proxy intent lost for {}",
                case["argv"]
            );
        }
    }
    assert_eq!(
        repo.list(1000.0)?.len(),
        0,
        "Delete left a profile or duplicate create wrote another"
    );
    assert_eq!(
        config.read()?["default_engine"],
        "playwright",
        "Refused create changed default engine"
    );
    let human = document["human"]
        .as_array()
        .ok_or("Human profile cases missing")?;
    assert_eq!(human.len(), 62);
    for case in human {
        let mut output =
            cli::output::Output::new(Vec::new(), Vec::new(), false, case["tty"] == true);
        if case["fleet"] == true {
            assert!(cli::fleet_output::render(
                &mut output,
                case["command"].as_str().ok_or("Fleet command missing")?,
                &case["payload"]
            )?);
        } else if case["direct"] == true {
            assert!(cli::profile_output::render(
                &mut output,
                case["command"].as_str().ok_or("Human command missing")?,
                &case["payload"]
            )?);
        } else {
            let profile = numera_core::models::Profile::from_record(&case["profile"])?;
            repo.save(profile.clone(), 1000.0)?;
            if case["command"] == "summary" {
                cli::profile_output::summary(&mut output, &profile, case["full"] == true)?;
            } else {
                let command = case["command"].as_str().ok_or("Human command missing")?;
                let argv = if command == "list" {
                    vec!["numera", "list"]
                } else {
                    vec!["numera", command, "fixture"]
                };
                let matches = registry.clone().try_get_matches_from(argv)?;
                let values = cli::reflection::values(&registry, &matches);
                let outcome = services.execute_with_output(
                    values.as_object().ok_or("Human namespace missing")?,
                    1000.0,
                    |_| Ok(false),
                    || false,
                    || panic!("Human profile read stdin"),
                    &mut output,
                )?;
                assert_eq!(
                    i64::from(outcome.exit),
                    case["exit"].as_i64().ok_or("Human exit missing")?
                );
            }
        }
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Source human output missing")?,
            "{} tty={}",
            case["command"],
            case["tty"]
        );
        assert!(stderr.is_empty());
    }
    temporary.cleanup()?;
    Ok(())
}
fn normalize_id(value: &mut Value, id: &str) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_id(value, id);
            }
        }
        Value::Object(object) => {
            if let Some(changes) = object.get_mut("changes").and_then(Value::as_object_mut)
                && let Some(fields) = changes.remove(id)
            {
                changes.insert("fixture".into(), fields);
            }
            for (key, value) in object {
                if key == "id" && value.as_str() == Some(id) {
                    *value = "fixture".into();
                } else {
                    normalize_id(value, id);
                }
            }
        }
        _ => {}
    }
}
