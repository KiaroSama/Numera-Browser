use numera_core::cli;
#[path = "cli_registry_parity/variadic.rs"]
mod variadic;
#[path = "cli_registry_parity/version.rs"]
mod version;

#[test]
fn integer_options_preserve_original_digit_bound_before_zero_normalization()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::Value;

    // Frozen execution of the immutable source; native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/integer_options_preserve_original_digit_bound_before_zero_normalization.json"
    ))?;
    assert_eq!(cases.len(), 96, "Integer option observations missing");
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in cases.iter().enumerate() {
        let argv = case["argv"]
            .as_array()
            .ok_or("Integer arguments missing")?
            .iter()
            .map(|value| value.as_str().ok_or("Integer argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(argv));
        let exit = actual.as_ref().err().map_or(0, clap::Error::exit_code);
        assert_eq!(
            i64::from(exit),
            case["exit"].as_i64().ok_or("Original exit missing")?,
            "CLI integer digit admission differs: case={index}"
        );
        if let Ok(matches) = actual {
            assert_eq!(
                cli::reflection::values(&registry, &matches),
                case["values"],
                "CLI integer value differs: case={index}"
            );
        }
    }
    Ok(())
}

#[test]
fn separate_option_values_match_original_token_classification()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::Value;

    // Frozen execution of the immutable source; native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/separate_option_values_match_original_token_classification.json"
    ))?;
    assert_eq!(cases.len(), 584, "Option token observations missing");
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in cases.iter().enumerate() {
        let argv = case["argv"]
            .as_array()
            .ok_or("Option arguments missing")?
            .iter()
            .map(|value| value.as_str().ok_or("Option argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = cli::parse(std::iter::once("numera").chain(argv));
        assert_eq!(
            i64::from(actual.as_ref().err().map_or(0, clap::Error::exit_code)),
            case["exit"]
                .as_i64()
                .ok_or("Original option exit missing")?,
            "CLI option token admission differs: case={index}"
        );
        if let Ok(matches) = actual {
            assert_eq!(
                cli::reflection::values(&registry, &matches),
                case["values"],
                "CLI option token value differs: case={index}"
            );
        }
    }
    Ok(())
}

#[test]
fn positional_values_preserve_original_admission_and_option_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::Value;

    // Frozen execution of the immutable source; native assertions below remain live.
    let document: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/positional_values_preserve_original_admission_and_option_boundaries.json"
    ))?;
    let occurrences = document["occurrences"]
        .as_u64()
        .ok_or("Occurrences missing")?;
    assert!(
        occurrences > 30,
        "Original positional inventory is incomplete"
    );
    let cases = document["rows"]
        .as_array()
        .ok_or("Positional rows missing")?;
    assert_eq!(cases.len() as u64, occurrences * 16 * 4 + 27);
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in cases.iter().enumerate() {
        let argv = case["argv"]
            .as_array()
            .ok_or("Positional arguments missing")?
            .iter()
            .map(|v| v.as_str().ok_or("Argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = cli::parse(std::iter::once("numera").chain(argv.iter().copied()));
        let expected_exit = case["exit"].as_i64().ok_or("Original exit missing")?;
        if expected_exit == 2
            && argv.first() == Some(&"cookies")
            && argv.get(1) == Some(&"export")
            && actual.is_ok()
        {
            // 0.37.0 makes `cookies export` file optional (stdout or --out).
            continue;
        }
        assert_eq!(
            i64::from(actual.as_ref().err().map_or(0, clap::Error::exit_code)),
            expected_exit,
            "CLI positional token admission differs: case={index}"
        );
        if let Ok(matches) = actual {
            assert_eq!(
                project_legacy_parse_values(
                    cli::reflection::values(&registry, &matches),
                    &case["values"]
                ),
                case["values"],
                "CLI positional token value differs: case={index}"
            );
            let (mut command, mut parsed) = (&registry, &matches);
            while let Some((name, child)) = parsed.subcommand() {
                command = command
                    .find_subcommand(name)
                    .ok_or("Parsed command missing")?;
                parsed = child;
            }
            for arg in command.get_positionals() {
                let id = arg.get_id().as_str();
                let raw = parsed.get_raw(id).map_or_else(Vec::new, |values| {
                    values.map(|value| value.to_str()).collect::<Vec<_>>()
                });
                let expected = &case["values"][id];
                let expected = expected.as_array().map_or_else(
                    || expected.as_str().map(Some).into_iter().collect::<Vec<_>>(),
                    |values| values.iter().map(Value::as_str).collect(),
                );
                assert_eq!(
                    raw, expected,
                    "CLI raw positional bytes differ: case={index}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn short_help_preserves_original_explicit_tail_admission() -> Result<(), Box<dyn std::error::Error>>
{
    use serde_json::Value;

    // Frozen execution of the immutable source; native assertions below remain live.
    let document: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/short_help_preserves_original_explicit_tail_admission.json"
    ))?;
    let scopes = document["scopes"]
        .as_u64()
        .ok_or("Command scopes missing")?;
    assert!(scopes > 40, "Original command inventory is incomplete");
    let cases = document["rows"]
        .as_array()
        .ok_or("Short-help rows missing")?;
    assert_eq!(cases.len() as u64, scopes * 12 + 21);
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in cases.iter().enumerate() {
        let argv = case["argv"]
            .as_array()
            .ok_or("Short-help arguments missing")?
            .iter()
            .map(|v| v.as_str().ok_or("Argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = cli::parse(std::iter::once("numera").chain(argv));
        assert_eq!(
            i64::from(actual.as_ref().err().map_or(0, clap::Error::exit_code)),
            case["exit"].as_i64().ok_or("Original exit missing")?,
            "CLI short-help token admission differs: case={index}"
        );
        if let Ok(matches) = actual {
            assert_eq!(
                cli::reflection::values(&registry, &matches),
                case["values"],
                "CLI short-help value differs: case={index}"
            );
        }
    }
    Ok(())
}

fn project_legacy_parse_values(
    mut actual: serde_json::Value,
    expected: &serde_json::Value,
) -> serde_json::Value {
    if let (Some(actual_map), Some(expected_map)) = (actual.as_object_mut(), expected.as_object()) {
        for key in [
            "out",
            "mac_address_mode",
            "mac_address",
            "device_name",
            "bridge",
            "token",
        ] {
            if !expected_map.contains_key(key)
                && actual_map.get(key) == Some(&serde_json::Value::Null)
            {
                actual_map.remove(key);
            }
        }
        if !expected_map.contains_key("allow_remote_bridge")
            && actual_map.get("allow_remote_bridge") == Some(&serde_json::Value::Bool(false))
        {
            actual_map.remove("allow_remote_bridge");
        }
    }
    actual
}

#[test]
fn command_registry_matches_original_schemas_arguments_and_parse_views()
-> Result<(), Box<dyn std::error::Error>> {
    let document: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/characterization/command_registry_matches_original_schemas_arguments_and_parse_views.json"
    ))?;
    let tools = cli::reflection::tools();
    assert!(!tools.is_empty());
    let actual_tools = serde_json::to_value(&tools)?;
    let actual_by_name: std::collections::BTreeMap<&str, &serde_json::Value> = actual_tools
        .as_array()
        .ok_or("Actual tools array missing")?
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(|name| (name, tool)))
        .collect();
    for expected_tool in document["tools"]
        .as_array()
        .ok_or("Expected tools array missing")?
    {
        let name = expected_tool["name"]
            .as_str()
            .ok_or("Expected tool name missing")?;
        let actual_tool = actual_by_name
            .get(name)
            .copied()
            .ok_or_else(|| format!("Legacy tool '{name}' missing from current registry"))?;
        if name != "cookies_export" {
            assert_eq!(
                actual_tool["description"], expected_tool["description"],
                "{name}"
            );
        }
        assert_eq!(
            actual_tool["inputSchema"]["type"], expected_tool["inputSchema"]["type"],
            "{name}"
        );
        let expected_props = expected_tool["inputSchema"]["properties"]
            .as_object()
            .ok_or("Expected tool properties missing")?;
        let actual_props = actual_tool["inputSchema"]["properties"]
            .as_object()
            .ok_or("Actual tool properties missing")?;
        for (prop, expected_schema) in expected_props {
            if name == "cookies_export" && matches!(prop.as_str(), "file" | "detach") {
                assert_eq!(
                    actual_props.get(prop).and_then(|v| v.get("type")),
                    expected_schema.get("type"),
                    "{name}.{prop}"
                );
                continue;
            }
            assert_eq!(
                actual_props.get(prop),
                Some(expected_schema),
                "{name}.{prop}"
            );
        }
        if name != "cookies_export" {
            assert_eq!(
                actual_tool["inputSchema"].get("required"),
                expected_tool["inputSchema"].get("required"),
                "{name} required"
            );
        }
    }
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
                project_legacy_parse_values(
                    cli::reflection::values(&registry, &actual?),
                    &case["values"]
                ),
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
        let tool_name = case["name"].as_str().ok_or("Tool missing")?;
        let mut actual = cli::reflection::argv(
            tool_name,
            case["arguments"]
                .as_object()
                .ok_or("Tool arguments missing")?,
        )?;
        if tool_name == "cookies_export" {
            actual = actual
                .into_iter()
                .flat_map(|token| {
                    if let Some((flag, value)) = token
                        .strip_prefix("--")
                        .and_then(|rest| rest.split_once('='))
                    {
                        vec![format!("--{flag}"), value.to_owned()]
                    } else {
                        vec![token]
                    }
                })
                .collect();
        }
        assert_eq!(serde_json::to_value(actual)?, case["argv"]);
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
    assert_eq!(output_cases.len(), 39);
    for (index, case) in output_cases.iter().enumerate() {
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
            Ok(confirmed) => assert_eq!(
                serde_json::json!(confirmed),
                case["confirmed"],
                "CLI confirmation decision differs: {index}"
            ),
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"]
                    .as_str()
                    .ok_or("Source confirm error missing")?
            ),
        }
        assert_eq!(
            input.position(),
            case["consumed"]
                .as_u64()
                .ok_or("Source consumed count missing")?,
            "CLI confirmation input consumption differs: {index}"
        );
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
