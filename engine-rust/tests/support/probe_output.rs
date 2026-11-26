use numera_core::cli::{self, output::Output, probe_output};
use serde_json::Value;

pub fn compare(document: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let cases = document.as_array().ok_or("Human probe cases missing")?;
    assert_eq!(cases.len(), 62);
    let mut registry = cli::registry();
    registry.build();
    for case in cases {
        let argv = case["argv"].as_array().ok_or("Human probe argv missing")?;
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(
                argv.iter()
                    .map(|value| value.as_str().expect("Source argv text")),
            ),
        )?;
        let values = cli::reflection::values(&registry, &matches);
        let operation = values["command"]
            .as_str()
            .ok_or("Human probe operation missing")?;
        let reference = values["ref"]
            .as_str()
            .ok_or("Human probe reference missing")?;
        for json in [false, true] {
            let mut output = Output::new(Vec::new(), Vec::new(), json, case["tty"] == true);
            if !case["name"].is_null() {
                assert!(probe_output::start(
                    &mut output,
                    operation,
                    &case["name"],
                    case["proxy"].as_str()
                )?);
            }
            assert!(probe_output::render(
                &mut output,
                operation,
                reference,
                &case["payload"]
            )?);
            let code = case["exit"].as_i64().ok_or("Source human exit missing")? as i32;
            let json_code = if matches!(operation, "trust" | "align") && !case["name"].is_null() {
                0
            } else {
                code
            };
            output.emit(&case["payload"], if json { json_code } else { code })?;
            let (stdout, stderr) = output.into_parts();
            if json {
                let decoded: Value = serde_json::from_slice(&stdout)?;
                assert_eq!(decoded, case["payload"]);
                assert_eq!(stdout.last(), Some(&b'\n'));
                assert!(!stdout.contains(&0x1b));
            } else {
                assert_eq!(
                    String::from_utf8(stdout)?,
                    case["stdout"]
                        .as_str()
                        .ok_or("Source human probe output missing")?,
                    "{}",
                    case["argv"]
                );
            }
            assert!(stderr.is_empty());
        }
    }
    Ok(())
}

pub fn run(
    output_cases: &Value,
    operation: &str,
    expected: &Value,
    application: &cli::application::Application<'_>,
    engine: Option<&str>,
    json: bool,
) -> Result<numera_core::cli::profile_commands::Outcome, Box<dyn std::error::Error>> {
    let case = output_cases
        .as_array()
        .ok_or("Human probe cases missing")?
        .iter()
        .find(|case| {
            case["tty"] == false
                && case["argv"][0] == operation
                && &case["payload"] == expected
                && !case["name"].is_null()
        })
        .ok_or("Public probe output expectation missing")?;
    let mut output = Output::new(Vec::new(), Vec::new(), json, false);
    let mut argv = vec!["numera", operation];
    if operation == "proxy" {
        argv.push("test");
    }
    argv.push("fixture");
    if let Some(engine) = engine {
        argv.extend(["--engine", engine]);
    }
    if json {
        argv.push("--json");
    }
    let mut registry = cli::registry();
    registry.build();
    let matches = registry.clone().try_get_matches_from(argv)?;
    let values = cli::reflection::values(&registry, &matches);
    let outcome = application.execute_browser_with_output(
        values.as_object().ok_or("Probe args missing")?,
        1000.0,
        || false,
        &mut output,
    )?;
    assert_eq!(&outcome.payload, expected);
    assert_eq!(
        outcome.exit,
        if json && matches!(operation, "trust" | "align") {
            0
        } else {
            case["exit"].as_i64().ok_or("Source exit missing")? as i32
        }
    );
    output.emit(&outcome.payload, outcome.exit)?;
    let (stdout, stderr) = output.into_parts();
    if json {
        assert_eq!(serde_json::from_slice::<Value>(&stdout)?, *expected);
    } else {
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Public human probe output missing")?
        );
    }
    assert!(stderr.is_empty());
    Ok(outcome)
}
