use numera_core::{
    browser::adapter::Adapter,
    cli::{self, application::Application},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};
mod common;

#[test]
fn settings_folder_token_and_schedule_services_match_original_verdicts()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("cli-settings-parity")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(python);
    command.args(["-u","-c",r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from cli_settings_cases import cases,human_cases
from cli_automation_output_cases import cases as automation_output
from cli_extension_output_cases import cases as extension_output
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT']),'human':human_cases(os.environ['FIXTURE_ROOT']),'automation':automation_output(os.environ['FIXTURE_ROOT']),'extensions':extension_output(os.path.join(os.environ['FIXTURE_ROOT'],'human-extensions'))}}),flush=True)
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
        "observe-settings-cli",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let root = temporary.path().join("native");
    let repo = cli::application::open_store(&root, &serde_json::Map::new(), None)?;
    let fingerprint =
        numera_core::fingerprint::generate(numera_core::fingerprint::GenerateOptions {
            seed: 42,
            ..Default::default()
        })?;
    repo.save(numera_core::models::Profile::from_record(&json!({"id":"fixture","name":"Fixture","fingerprint":fingerprint,"created_at":1000.0,"updated_at":1000.0}))?,1000.0)?;
    let installed = BTreeMap::from([
        ("playwright".into(), true),
        ("patchright".into(), false),
        ("camoufox".into(), true),
        ("cloak".into(), false),
    ]);
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
    let cases = document["cases"]
        .as_array()
        .ok_or("Settings source cases missing")?;
    assert_eq!(cases.len(), 29);
    let mut registry = cli::registry();
    registry.build();
    let mut schedule_id: Option<String> = None;
    for case in cases {
        let mut argv = case["argv"]
            .as_array()
            .ok_or("Source argv missing")?
            .iter()
            .map(|v| v.as_str().expect("Source argv text").to_owned())
            .collect::<Vec<_>>();
        if let Some(id) = &schedule_id {
            for argument in &mut argv {
                if argument == "schedulecase" {
                    *argument = id.clone();
                }
            }
        }
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera".to_owned()).chain(argv))?;
        let values = cli::reflection::values(&registry, &matches);
        let args = values.as_object().ok_or("Native command args missing")?;
        let mut outcome =
            services.execute(args, 1000.0, |_| panic!("Settings fixture prompted"))?;
        if outcome.payload.get("root").is_some() {
            assert_eq!(outcome.payload["root"], json!(root));
            outcome.payload["root"] = "fixture-root".into();
        }
        if values["schedule_command"] == "add" {
            let id = outcome.payload["id"]
                .as_str()
                .ok_or("Schedule identity missing")?;
            assert_eq!(id.len(), 12);
            schedule_id = Some(id.to_owned());
        }
        if let Some(id) = &schedule_id {
            normalize_id(&mut outcome.payload, id);
        }
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Source exit missing")? as i32,
            "{}",
            case["argv"]
        );
        assert_eq!(outcome.payload, case["payload"], "{}", case["argv"]);
    }
    let human = document["human"]
        .as_array()
        .ok_or("Source human settings cases missing")?;
    assert_eq!(human.len(), 52);
    let human_installed = BTreeMap::from([
        ("camoufox".into(), false),
        ("cloak".into(), false),
        ("patchright".into(), false),
        ("playwright".into(), true),
    ]);
    for case in human {
        let argv = std::iter::once("numera").chain(
            case["argv"]
                .as_array()
                .ok_or("Human settings argv missing")?
                .iter()
                .map(|value| value.as_str().expect("Source argv text")),
        );
        let matches = registry.clone().try_get_matches_from(argv)?;
        let values = cli::reflection::values(&registry, &matches);
        let args = values
            .as_object()
            .ok_or("Human settings namespace missing")?;
        let mut payload = case["payload"].clone();
        if values["command"] != "vault" {
            let human_root = temporary.path().join(if case["tty"] == true {
                "human-native-tty"
            } else {
                "human-native-pipe"
            });
            let repo = cli::application::open_store(&human_root, args, None)?;
            let sessions = numera_core::ownership::registry::Registry::new(
                human_root.clone(),
                "fixture-host".into(),
                None,
            )?;
            let application = Application {
                root: &human_root,
                home: &human_root,
                repo: &repo,
                installed: &human_installed,
                host: "fixture-host",
                password: None,
                environment_token: false,
                sessions: &sessions,
                browser: None,
            };
            let mut outcome =
                application.execute(args, 1000.0, |_| panic!("Human settings prompted"))?;
            assert_eq!(
                outcome.exit,
                case["exit"].as_i64().ok_or("Human settings exit missing")? as i32,
                "{}",
                case["argv"]
            );
            if outcome.payload.get("root").is_some() {
                assert_eq!(outcome.payload["root"], json!(human_root));
                outcome.payload["root"] = case["payload"]["root"].clone();
            }
            assert_eq!(outcome.payload, case["payload"], "{}", case["argv"]);
            payload = outcome.payload;
            assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        }
        let mut output =
            cli::output::Output::new(Vec::new(), Vec::new(), false, case["tty"] == true);
        assert!(cli::settings_output::render(
            &mut output,
            values
                .as_object()
                .ok_or("Human settings namespace missing")?,
            &payload
        )?);
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Source human settings output missing")?,
            "{}",
            case["argv"]
        );
        assert!(stderr.is_empty());
    }
    let automation = document["automation"]
        .as_array()
        .ok_or("Automation output cases missing")?;
    assert_eq!(automation.len(), 32);
    for case in automation {
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(
                case["argv"]
                    .as_array()
                    .ok_or("Automation argv missing")?
                    .iter()
                    .map(|value| value.as_str().expect("Source argv text")),
            ),
        )?;
        let values = cli::reflection::values(&registry, &matches);
        let mut output =
            cli::output::Output::new(Vec::new(), Vec::new(), false, case["tty"] == true);
        assert!(cli::automation_output::render(
            &mut output,
            values.as_object().ok_or("Automation namespace missing")?,
            &case["payload"],
            1000.0,
            case.get("resolvedName").filter(|value| !value.is_null())
        )?);
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Automation source bytes missing")?,
            "{}",
            case["argv"]
        );
        assert!(stderr.is_empty());
    }
    let extensions = document["extensions"]
        .as_array()
        .ok_or("Extension output cases missing")?;
    assert_eq!(extensions.len(), 32);
    for case in extensions {
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(
                case["argv"]
                    .as_array()
                    .ok_or("Extension argv missing")?
                    .iter()
                    .map(|value| value.as_str().expect("Source argv text")),
            ),
        )?;
        let values = cli::reflection::values(&registry, &matches);
        let args = values.as_object().ok_or("Extension namespace missing")?;
        let extension_root = temporary.path().join(if case["tty"] == true {
            "extensions-native-tty"
        } else {
            "extensions-native-pipe"
        });
        let repo = cli::application::open_store(&extension_root, args, None)?;
        if repo.get("fixture")?.is_none() {
            repo.save(
                numera_core::models::Profile::from_record(
                    &json!({"id":"fixture","name":"Fixture","fingerprint":fingerprint}),
                )?,
                1000.0,
            )?;
        }
        let sessions = numera_core::ownership::registry::Registry::new(
            extension_root.clone(),
            "fixture-host".into(),
            None,
        )?;
        let home = Path::new(case["home"].as_str().ok_or("Extension home missing")?);
        let application = Application {
            root: &extension_root,
            home,
            repo: &repo,
            installed: &human_installed,
            host: "fixture-host",
            password: None,
            environment_token: false,
            sessions: &sessions,
            browser: None,
        };
        let mut output =
            cli::output::Output::new(Vec::new(), Vec::new(), false, case["tty"] == true);
        let outcome = application.execute_with_output(
            args,
            1000.0,
            |_| panic!("Extension fixture prompted"),
            || false,
            || panic!("Extension read stdin"),
            &mut output,
        )?;
        assert_eq!(outcome.payload, case["payload"], "{}", case["argv"]);
        assert_eq!(
            i64::from(outcome.exit),
            case["exit"].as_i64().ok_or("Extension exit missing")?
        );
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Extension source output missing")?,
            "{}",
            case["argv"]
        );
        assert!(stderr.is_empty());
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
    }
    temporary.cleanup()?;
    Ok(())
}
#[test]
fn shared_cli_open_uses_both_resume_passwords_before_exposing_stores()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/rotation-recovery.json"))?;
    let case = &fixture["cases"][0];
    let new_password = case["password"]
        .as_str()
        .ok_or("Fixture new password missing")?;
    let temporary = common::TempDirectory::new("cli-resume")?;
    let root = temporary.path();
    for (name, value) in [
        ("config.json", &case["config"]),
        ("proxies.json", &case["pool"]),
        ("vault-migration.json", &case["journal"]),
    ] {
        std::fs::write(root.join(name), serde_json::to_vec(value)?)?;
    }
    let before = ["config.json", "proxies.json", "vault-migration.json"]
        .into_iter()
        .map(|path| std::fs::read(root.join(path)).map(|bytes| (path, bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut args = json!({"command":"vault", "vault_command":"resume", "password":"", "new_password":"fixture-new-master"});
    let absent = cli::application::open_store(root, args.as_object().ok_or("Args missing")?, None)
        .err()
        .ok_or("Resume opened without its old password")?;
    assert_eq!(
        absent.to_string(),
        "this needs a master password — pass --password or set NUMERA_PASSWORD (a --json run never prompts)"
    );
    assert!(root.join("vault-migration.json").exists());
    let absent = cli::application::open_store(
        root,
        args.as_object().ok_or("Args missing")?,
        Some("fixture-old-master"),
    )
    .err()
    .ok_or("Resume incorrectly used inherited key instead of source prompt refusal")?;
    assert_eq!(
        absent.to_string(),
        "this needs a master password — pass --password or set NUMERA_PASSWORD (a --json run never prompts)"
    );
    args["password"] = "fixture-old-master".into();
    args["new_password"] = Value::Null;
    let absent = cli::application::open_store(
        root,
        args.as_object().ok_or("Args missing")?,
        Some("fixture-old-master"),
    )
    .err()
    .ok_or("Resume opened without its new password")?;
    assert_eq!(
        absent.to_string(),
        "this needs the password the rotation was moving to - pass --new-password (a --json run never prompts)"
    );
    assert!(root.join("vault-migration.json").exists());
    for (path, bytes) in &before {
        assert_eq!(
            &std::fs::read(root.join(path))?,
            bytes,
            "Missing credentials changed {path}"
        );
    }
    args["new_password"] = new_password.into();
    let repo = cli::application::open_store(
        root,
        args.as_object().ok_or("Args missing")?,
        Some("fixture-old-master"),
    )?;
    let sessions = numera_core::ownership::registry::Registry::new(
        root.into(),
        "fixture-host".into(),
        Some(new_password.into()),
    )?;
    let installed = BTreeMap::new();
    let application = Application {
        root,
        home: root,
        repo: &repo,
        installed: &installed,
        host: "fixture-host",
        password: Some(new_password),
        environment_token: false,
        sessions: &sessions,
        browser: None,
    };
    let outcome = application.execute(args.as_object().ok_or("Args missing")?, 1000.0, |_| {
        panic!("Resume prompted")
    })?;
    assert_eq!(outcome.exit, 0);
    assert_eq!(
        outcome.payload,
        json!({"ok":true,"resumed":true,"enabled":true})
    );
    assert!(!root.join("vault-migration.json").exists());
    let config: Value = serde_json::from_slice(&std::fs::read(root.join("config.json"))?)?;
    assert_eq!(config, case["persisted"]);
    let stored: Value = serde_json::from_slice(&std::fs::read(root.join("proxies.json"))?)?;
    assert_eq!(
        stored.as_array().ok_or("Pool missing")?.len(),
        case["observed"]
            .as_array()
            .ok_or("Source pool missing")?
            .len()
    );
    for (row, expected) in stored
        .as_array()
        .ok_or("Pool missing")?
        .iter()
        .zip(case["observed"].as_array().ok_or("Source pool missing")?)
    {
        assert_eq!(
            numera_core::vault::decrypt(
                new_password,
                config["vault"]["salt"].as_str().ok_or("Salt missing")?,
                row["pass"].as_str().ok_or("Envelope missing")?
            )?,
            expected["pass"].as_str().ok_or("Source secret missing")?
        );
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
        Value::Object(object) => {
            for (key, value) in object {
                if key == "id" && value.as_str() == Some(id) {
                    *value = "schedulecase".into();
                } else {
                    normalize_id(value, id);
                }
            }
        }
        _ => {}
    }
}
