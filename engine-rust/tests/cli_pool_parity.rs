use numera_core::{
    browser::adapter::Adapter,
    cli::{self, application::Application},
    fingerprint::{GenerateOptions, generate},
    models::Profile,
    storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};
mod common;

#[test]
fn shared_pool_cli_guards_and_assignment_match_original() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = common::TempDirectory::new("cli-pool")?;
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
from cli_pool_cases import cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT']),'human':sum([cases(os.path.join(os.environ['FIXTURE_ROOT'],'human-'+str(tty)),human=True,tty=tty) for tty in [False,True]],[])}}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",temporary.path().join("source")).env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        temporary.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-pool-cli",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let root = temporary.path().join("native");
    let repo = ProfileRepo::open(root.clone(), None, None)?;
    let fingerprint = generate(GenerateOptions {
        seed: 42,
        ..Default::default()
    })?;
    repo.save(Profile::from_record(&json!({"id":"fixture","name":"Fixture","fingerprint":fingerprint,"created_at":1000.0,"updated_at":1000.0}))?,1000.0)?;
    let installed = BTreeMap::new();
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
    std::fs::write(root.join("invalid.txt"), b"http://invalid.test:70000\n")?;
    std::fs::write(
        root.join("proxies.txt"),
        b"https://user:synthetic@[2001:db8::1]:8080:US\nhttp://plain.test:3128:DE\n",
    )?;
    let mut registry = cli::registry();
    registry.build();
    let mut identities = BTreeMap::<String, String>::new();
    let args = json!({"command":"pool","pool_command":"add","source":"-"});
    let error = services
        .execute_with_input(
            args.as_object().ok_or("Input args missing")?,
            1000.0,
            |_| panic!("Input fixture prompted"),
            || false,
            || {
                Err(numera_core::storage::repo::RepoError::Message(
                    "Fixture input failed".into(),
                ))
            },
        )
        .err()
        .ok_or("Input error was swallowed")?;
    assert_eq!(error.to_string(), "Fixture input failed");
    assert!(!root.join("proxies.json").exists());
    let mut detached = args.clone();
    detached["detach"] = true.into();
    assert!(
        services
            .execute_with_input(
                detached.as_object().ok_or("Detached args missing")?,
                1000.0,
                |_| panic!("Detached fixture prompted"),
                || false,
                || panic!("Refused detach consumed input"),
            )
            .is_err()
    );
    let cases = document["cases"].as_array().ok_or("Pool cases missing")?;
    assert_eq!(cases.len(), 19);
    for (index, case) in cases.iter().enumerate() {
        let argv = case["argv"]
            .as_array()
            .ok_or("Pool source argv missing")?
            .iter()
            .map(|v| {
                let text = v.as_str().expect("Source argv text");
                identities.get(text).cloned().unwrap_or(text.into())
            })
            .collect::<Vec<_>>();
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(argv.iter().map(String::as_str)),
        )?;
        let mut values = cli::reflection::values(&registry, &matches);
        if values["pool_command"] == "add" && values["source"] != "-" {
            let path = root.join(values["source"].as_str().ok_or("Pool input missing")?);
            values["source"] = serde_json::to_value(path)?;
        }
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let mut outcome = if matches!(
            values["pool_command"].as_str(),
            Some("test" | "rotate" | "assign")
        ) {
            let config = numera_core::storage::config::ConfigStore::new(root.clone());
            let profiles = cli::profile_commands::Profiles {
                repo: &repo,
                config: &config,
                installed: &installed,
                host: "fixture-host",
            };
            let store = numera_core::storage::pool::ProxyPool::new(root.clone(), None);
            let pool = cli::pool_commands::Pool {
                store: &store,
                profiles: &profiles,
            };
            let ids = values["id"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if values["pool_command"] == "assign" {
                pool.assign_with_output(
                    values.as_object().ok_or("Assign arguments missing")?,
                    1000.0,
                    &mut output,
                )?
            } else if values["pool_command"] == "rotate" {
                pool.rotate_with_output(
                    &ids,
                    |proxy| {
                        numera_core::network::proxy_rotate::rotate_sync(
                            proxy,
                            Duration::from_secs(20),
                        )
                    },
                    || 1000.0,
                    &mut output,
                )?
            } else {
                pool.test_with_output(&ids, values["all"] == true, |proxy| {
                    Ok(if proxy["host"] == "2001:db8::1" {
                        json!({"ok":true,"ip":"192.0.2.1","country":"US","city":null,"latencyMs":10})
                    } else { json!({"ok":false,"error":"fixture network refusal"}) })
                }, || 1000.0, &mut output)?
            }
        } else {
            services.execute_with_input(
                values.as_object().ok_or("Pool arguments missing")?,
                1000.0,
                |_| panic!("Pool fixture prompted"),
                || false,
                || {
                    assert_eq!(values["source"], "-", "Unrelated command read stdin");
                    Ok(
                        "https://user:synthetic@[2001:db8::1]:8080:US\nhttp://plain.test:3128:DE\n"
                            .into(),
                    )
                },
            )?
        };
        if values["pool_command"] == "add"
            && let Some(rows) = outcome.payload["proxies"].as_array()
        {
            for row in rows {
                let expected = if row["host"] == "2001:db8::1" {
                    "poolone00000"
                } else {
                    "pooltwo00000"
                };
                let id = row["id"].as_str().ok_or("Native pool ID missing")?;
                assert_eq!(id.len(), 12);
                assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
                identities.insert(expected.into(), id.into());
            }
        }
        normalize_ids(&mut outcome.payload, &identities);
        let bytes = match std::fs::read(root.join("proxies.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => b"[]".to_vec(),
            Err(error) => return Err(error.into()),
        };
        let mut disk: Value = serde_json::from_slice(&bytes)?;
        normalize_ids(&mut disk, &identities);
        assert_eq!(disk, case["pool"], "{}", case["argv"]);
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Source pool exit missing")? as i32
        );
        assert_eq!(outcome.payload, case["payload"], "{}", case["argv"]);
        let human_case = document["human"]
            .as_array()
            .ok_or("Pool human cases missing")?
            .get(index)
            .ok_or("Pool human expectation missing")?;
        assert_eq!(human_case["tty"], false);
        assert_eq!(
            human_case["argv"],
            serde_json::json!(
                case["argv"]
                    .as_array()
                    .ok_or("Source argv missing")?
                    .iter()
                    .filter(|value| value.as_str() != Some("--json"))
                    .collect::<Vec<_>>()
            )
        );
        assert!(cli::pool_output::render(
            &mut output,
            values.as_object().ok_or("Pool output args missing")?,
            &outcome.payload
        )?);
        let (stdout, stderr) = output.into_parts();
        let mut stdout = String::from_utf8(stdout)?;
        for (source, native) in &identities {
            stdout = stdout.replace(native, source);
        }
        assert_eq!(
            stdout,
            human_case["stdout"]
                .as_str()
                .ok_or("Pool human bytes missing")?,
            "{}",
            case["argv"]
        );
        assert!(stderr.is_empty());
        assert_eq!(
            serde_json::to_value(repo.get("fixture")?.ok_or("Profile vanished")?)?,
            case["profile"],
            "{}",
            case["argv"]
        );
        for kind in ["profiles", "dashboard"] {
            let stored: Value =
                serde_json::from_slice(&std::fs::read(root.join(kind).join("fixture.json"))?)?;
            assert_eq!(
                stored["proxy"], case["persisted_proxy"][kind],
                "{kind} assignment diverged"
            );
        }
    }
    let human = document["human"]
        .as_array()
        .ok_or("Pool human cases missing")?;
    assert_eq!(human.len(), 38);
    for case in human {
        let argv = case["argv"].as_array().ok_or("Pool human argv missing")?;
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(
                argv.iter()
                    .map(|value| value.as_str().expect("Source argv text")),
            ),
        )?;
        let values = cli::reflection::values(&registry, &matches);
        let mut output =
            cli::output::Output::new(Vec::new(), Vec::new(), false, case["tty"] == true);
        match values["pool_command"].as_str() {
            Some("test" | "rotate") => {
                if let Some(results) = case["payload"]["results"].as_array() {
                    for result in results {
                        let proxy = case["pool"]
                            .as_array()
                            .ok_or("Pool observation missing")?
                            .iter()
                            .find(|row| row["id"] == result["id"])
                            .ok_or("Observed result proxy missing")?;
                        if values["pool_command"] == "test" {
                            cli::pool_output::checked(&mut output, proxy, result)?;
                        } else {
                            cli::pool_output::rotated(&mut output, proxy, result)?;
                        }
                    }
                }
            }
            Some("assign") => {
                if let Some(rows) = case["payload"]["profiles"].as_array() {
                    for row in rows {
                        let locale = case["assignmentLocale"].as_str();
                        cli::pool_output::assigned(&mut output, row, locale)?;
                    }
                }
            }
            _ => {}
        }
        let handled = cli::pool_output::render(
            &mut output,
            values.as_object().ok_or("Pool human namespace missing")?,
            &case["payload"],
        )?;
        assert!(handled);
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Pool source human bytes missing")?,
            "{}",
            case["argv"]
        );
        assert!(stderr.is_empty());
    }
    temporary.cleanup()?;
    Ok(())
}
fn normalize_ids(value: &mut Value, ids: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some((source, _)) = ids
                .iter()
                .find(|(_, native)| native.as_str() == text.as_str())
            {
                *text = source.clone();
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize_ids(value, ids);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                normalize_ids(value, ids);
            }
        }
        _ => {}
    }
}
