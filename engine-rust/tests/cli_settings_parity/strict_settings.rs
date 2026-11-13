use super::{Application, BTreeMap, Duration, Value, cli, common, json};
use numera_core::{ownership::registry::Registry, storage::repo::RepoError};
use std::fs;

#[test]
fn settings_commands_preserve_original_decoding_refusals_and_early_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("strict-settings-cli")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/settings_commands_preserve_original_decoding_refusals_and_early_errors.json"
    ))?;
    assert_eq!(cases.len(), 56);
    let installed = BTreeMap::from([
        ("camoufox".into(), false),
        ("cloak".into(), false),
        ("patchright".into(), false),
        ("playwright".into(), true),
    ]);
    let mut parser = cli::registry();
    parser.build();
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("config.json");
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
        }
        let arguments = case["argv"]
            .as_array()
            .ok_or("CLI arguments missing")?
            .iter()
            .map(|value| value.as_str().ok_or("CLI argument is not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let matches = parser
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(arguments).chain(["--json"]))?;
        let values = cli::reflection::values(&parser, &matches);
        let args = values.as_object().ok_or("CLI namespace missing")?;
        let repo = cli::application::open_store(&root, args, None)?;
        let sessions = Registry::new(root.clone(), "fixture-host".into(), None)?;
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
        let result = services.execute(args, 1000.0, |_| panic!("Settings fixture prompted"));
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        let invalid = matches!(&result, Err(RepoError::Io(error))
            if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Strict settings CLI admission differs: case={index} argv={}",
            case["argv"]
        );
        match result {
            Ok(mut outcome) => {
                assert!(
                    case["error"].is_null(),
                    "Original command refused: {}",
                    case["argv"]
                );
                if outcome.payload.get("root").is_some() {
                    assert_eq!(outcome.payload["root"], json!(root));
                    outcome.payload["root"] = "fixture-root".into();
                }
                assert_eq!(
                    json!(outcome.exit),
                    case["exit"],
                    "CLI exit differs: {}",
                    case["argv"]
                );
                assert_eq!(
                    outcome.payload, case["payload"],
                    "CLI payload differs: {}",
                    case["argv"]
                );
            }
            Err(error) if !invalid => {
                assert_eq!(Some(error.to_string().as_str()), case["error"].as_str())
            }
            Err(_) => {}
        }
        assert_eq!(
            fs::read(path).ok(),
            raw,
            "CLI changed settings bytes: {}",
            case["argv"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}
