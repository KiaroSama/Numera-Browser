use numera_core::{browser::adapter::Adapter, cli};
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
fn command_registry_matches_original_schemas_arguments_and_parse_views()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let root = common::TempDirectory::new("cli-registry-source")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(python);
    command
        .args([
            "-u",
            "-c",
            r#"
import json,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from cli_registry_cases import cases,output_cases
result=cases();result['output']=output_cases()
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
"#,
        ])
        .env("PYTHONUTF8", "1")
        .env(
            "PYTHONPATH",
            std::env::join_paths([
                repository.join("scripts/migration"),
                repository.join("engine"),
            ])?,
        );
    let mut source = Adapter::spawn(
        root.path(),
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-registry",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    root.cleanup()?;
    let tools = cli::reflection::tools();
    assert!(!tools.is_empty());
    assert_eq!(serde_json::to_value(&tools)?, document["tools"]);
    let mut registry = cli::registry();
    registry.build();
    for case in document["parse"]
        .as_array()
        .ok_or("Parser observations missing")?
    {
        let argv = std::iter::once("numera").chain(
            case["argv"]
                .as_array()
                .ok_or("Arguments missing")?
                .iter()
                .map(|v| v.as_str().expect("Source argv text")),
        );
        let actual = registry.clone().try_get_matches_from(argv);
        if case["exit"] == 0 {
            assert_eq!(
                cli::reflection::values(&registry, &actual?),
                case["values"],
                "{}",
                case["argv"]
            );
        } else {
            assert_eq!(
                actual
                    .expect_err("Accepted invalid source arguments")
                    .exit_code(),
                case["exit"].as_i64().ok_or("Exit missing")? as i32,
                "{}",
                case["argv"]
            );
        }
    }
    for case in document["argv"]
        .as_array()
        .ok_or("Automation argv observations missing")?
    {
        assert_eq!(
            serde_json::to_value(cli::reflection::argv(
                case["name"].as_str().ok_or("Tool missing")?,
                case["arguments"]
                    .as_object()
                    .ok_or("Tool arguments missing")?
            )?)?,
            case["argv"]
        );
    }
    assert!(cli::reflection::argv("missing", &Default::default()).is_err());
    let version =
        cli::parse(["numera", "--version"]).expect_err("Version should return display response");
    assert_eq!(version.exit_code(), 0);
    assert!(version.to_string().contains(env!("CARGO_PKG_VERSION")));
    let help = cli::parse(["numera", "--help"]).expect_err("Help should return display response");
    assert_eq!(help.exit_code(), 0);
    assert!(
        help.to_string()
            .contains("Browser fingerprint & profile manager.")
    );
    let output_cases = document["output"]
        .as_array()
        .ok_or("Source output cases missing")?;
    assert_eq!(output_cases.len(), 4);
    for case in output_cases {
        let mut output = cli::output::Output::new(
            Vec::new(),
            Vec::new(),
            case["json"] == true,
            case["tty"] == true,
        );
        output.say("مرحبا Fixture", Some(cli::output::Tone::Cyan))?;
        let mut input = std::io::Cursor::new(
            case["input"]
                .as_str()
                .ok_or("Source input missing")?
                .as_bytes(),
        );
        match output.confirm("Continue? [y/N] ", &mut input) {
            Ok(confirmed) => assert_eq!(serde_json::json!(confirmed), case["confirmed"]),
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"]
                    .as_str()
                    .ok_or("Source confirm error missing")?
            ),
        }
        if case["json"] == true {
            assert_eq!(input.position(), 0, "JSON confirmation consumed stdin");
        }
        let payload = serde_json::json!({"text":"مرحبا 😀","values":[true,null,123],"nested":{"key":"value"}});
        assert_eq!(output.emit(&payload, 7)?, 7);
        if case["json"] == true {
            assert!(
                output.emit(&payload, 7).is_err(),
                "Output emitted a second JSON document"
            );
        }
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"].as_str().ok_or("Source stdout missing")?
        );
        assert_eq!(
            String::from_utf8(stderr)?,
            case["stderr"].as_str().ok_or("Source stderr missing")?
        );
    }
    let mut limited = [0u8; 1];
    let mut output = cli::output::Output::new(&mut limited[..], Vec::new(), true, false);
    assert!(output.emit(&serde_json::json!({"ok":true}), 0).is_err());
    assert!(
        output.emitted(),
        "Partial JSON write allowed a retry document"
    );
    assert!(output.emit(&serde_json::json!({"ok":false}), 1).is_err());
    Ok(())
}
