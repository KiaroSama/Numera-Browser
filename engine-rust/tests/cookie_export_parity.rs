use numera_core::browser::{adapter::Adapter, cookie_format, cookie_normalize};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};
mod common;

fn assert_numeric_expiry_cookie(actual: &Value, expected: &Value) {
    let expiry = |cookie: &Value| {
        cookie["expires"]
            .as_f64()
            .filter(|number| number.is_finite())
            .expect("Cookie expiry must be finite")
            .to_bits()
    };
    assert_eq!(expiry(actual), expiry(expected), "Cookie expiry differs");
    let (mut actual, mut expected) = (actual.clone(), expected.clone());
    actual
        .as_object_mut()
        .expect("Cookie must be an object")
        .remove("expires");
    expected
        .as_object_mut()
        .expect("Cookie must be an object")
        .remove("expires");
    assert_eq!(actual, expected, "Non-expiry cookie fields differ");
}

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
from cookie_parse_cases import cases as parse_cases, whitespace_cases
from cookie_normalize_cases import cases as normalize_cases, expiry_overflow_cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(), 'parse_cases':parse_cases(), 'normalization':normalize_cases(), 'whitespace':whitespace_cases(), 'expiry_overflow':expiry_overflow_cases()}}), flush=True)
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
    let whitespace = observations["whitespace"]
        .as_array()
        .ok_or("Whitespace observations missing")?;
    assert_eq!(whitespace.len(), 16);
    for case in whitespace {
        let text = case["text"].as_str().ok_or("Cookie text missing")?;
        assert_eq!(
            serde_json::to_value(cookie_normalize::parse(text)?)?,
            case["parsed"],
            "{} cookie format detection",
            case["name"]
        );
        assert_eq!(
            serde_json::to_value(cookie_format::parse_netscape(text)?)?,
            case["netscape"],
            "{} Netscape fields",
            case["name"]
        );
    }
    let overflow = observations["expiry_overflow"]
        .as_array()
        .ok_or("Numeric expiry observations missing")?;
    assert_eq!(overflow.len(), 16);
    for case in overflow {
        let actual = cookie_normalize::normalize(&case["input"]);
        assert_eq!(
            actual.is_err(),
            case["normalized"]["failed"] == true,
            "{} numeric expiry normalization verdict",
            case["name"]
        );
        if let Ok(cookie) = actual {
            assert_numeric_expiry_cookie(
                &cookie.ok_or("Normalized expiry cookie missing")?,
                &case["normalized"]["value"],
            );
        }
        let parsed = case["parsed"]
            .as_array()
            .ok_or("Expiry parse observations missing")?;
        assert_eq!(parsed.len(), 3);
        for observation in parsed {
            let text = observation["text"]
                .as_str()
                .ok_or("Expiry input text missing")?;
            let actual = cookie_normalize::parse(text);
            assert_eq!(
                actual.is_err(),
                observation["expected"]["failed"] == true,
                "{} numeric expiry parse verdict",
                case["name"]
            );
            if let Ok(cookies) = actual {
                let expected = observation["expected"]["value"]
                    .as_array()
                    .ok_or("Parsed expiry cookies missing")?;
                assert_eq!(cookies.len(), expected.len());
                for (actual, expected) in cookies.iter().zip(expected) {
                    assert_numeric_expiry_cookie(actual, expected);
                }
            }
        }
    }
    assert!(cookie_format::to_netscape(&[Value::Null]).is_err());
    Ok(())
}
