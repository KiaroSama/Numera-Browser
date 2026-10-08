use numera_core::pool::{parse_line, to_line};
use serde_json::Value;

#[test]
fn pasted_proxy_forms_and_round_trips_match_the_source() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/pool-parse.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Parse cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let parsed = parse_line(case["line"].as_str().ok_or("Line missing")?);
        assert_eq!(parsed.clone().unwrap_or(Value::Null), case["parsed"]);
        if let Some(record) = parsed {
            let line = to_line(&record)?;
            assert_eq!(
                line,
                case["rendered"].as_str().ok_or("Rendered line missing")?
            );
            assert_eq!(parse_line(&line).unwrap_or(Value::Null), case["reparsed"]);
        } else {
            assert!(case["rendered"].is_null());
        }
    }
    Ok(())
}
