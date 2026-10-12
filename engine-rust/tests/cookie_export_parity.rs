use numera_core::browser::{adapter::Adapter, cookie_format, cookie_normalize};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
fn netscape_export_matches_original_bytes_and_refusals() -> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("cookie-export-source")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let script = r#"
import json, sys
request = json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}), flush=True)
request = json.loads(sys.stdin.readline())
from cookie_export_cases import cases
from cookie_parse_cases import cases as parse_cases
from cookie_normalize_cases import cases as normalize_cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(), 'parse_cases':parse_cases(), 'normalization':normalize_cases()}}), flush=True)
"#;
    let mut command = Command::new(python);
    command
        .args(["-u", "-c", script])
        .env(
            "PYTHONPATH",
            format!(
                "{}{}{}",
                repository.join("scripts/migration").display(),
                if cfg!(windows) { ";" } else { ":" },
                repository.join("engine").display()
            ),
        )
        .env("PYTHONUTF8", "1");
    let mut source = Adapter::spawn(
        temporary.path(),
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let observations = source.call(
        "observe-export",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    temporary.cleanup()?;
    let cases = observations["cases"]
        .as_array()
        .ok_or("Export observations missing")?;
    assert_eq!(cases.len(), 24);
    for case in cases {
        let rows = case["cookies"].as_array().ok_or("Cookie rows missing")?;
        assert_eq!(
            cookie_format::dumps(rows, "json")?,
            case["json"].as_str().ok_or("Original JSON missing")?,
            "{} JSON bytes",
            case["name"]
        );
        assert_eq!(
            cookie_format::dumps(rows, "unknown")?,
            cookie_format::dumps(rows, "json")?
        );
        let output = cookie_format::to_netscape(rows);
        if case["expected"]["failed"] == true {
            assert!(
                output.is_err(),
                "{} accepted malformed cookie",
                case["name"]
            );
        } else {
            assert_eq!(
                output?,
                case["expected"]["text"]
                    .as_str()
                    .ok_or("Expected text missing")?,
                "{}",
                case["name"]
            );
        }
    }
    let parsed = observations["parse_cases"]
        .as_array()
        .ok_or("Parse observations missing")?;
    assert_eq!(parsed.len(), 24);
    for case in parsed {
        let actual = cookie_format::parse_netscape(case["text"].as_str().ok_or("Text missing")?);
        if case["expected"]["failed"] == true {
            assert!(actual.is_err(), "{} expected parse refusal", case["name"]);
        } else {
            assert_eq!(
                serde_json::to_value(actual?)?,
                case["expected"]["cookies"],
                "{}",
                case["name"]
            );
        }
    }
    let normalized = observations["normalization"]["normalize"]
        .as_array()
        .ok_or("Normalization missing")?;
    assert_eq!(normalized.len(), 46);
    for case in normalized {
        assert_eq!(
            cookie_normalize::normalize(&case["input"])?.unwrap_or(Value::Null),
            case["expected"],
            "{}",
            case["input"]
        );
    }
    let inputs = observations["normalization"]["parse"]
        .as_array()
        .ok_or("Format inputs missing")?;
    assert_eq!(inputs.len(), 14);
    for case in inputs {
        assert_eq!(
            serde_json::to_value(cookie_normalize::parse(
                case["text"].as_str().ok_or("Input text missing")?
            )?)?,
            case["expected"]
        );
    }
    assert!(cookie_format::to_netscape(&[Value::Null]).is_err());
    Ok(())
}
