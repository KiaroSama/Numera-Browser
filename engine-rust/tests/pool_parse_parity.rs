use numera_core::pool::{parse_line, to_line};
use serde_json::Value;

mod common;

#[test]
fn pooled_proxy_text_boundaries_match_original_full_results()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, pool::parse_many};
    use serde_json::json;
    use std::{fs, path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("pool-text")?;
    let mut inputs = Vec::new();
    for scalar in [
        0x0a, 0x0d, 0x0b, 0x0c, 0x1c, 0x1d, 0x1e, 0x85, 0x2028, 0x2029,
    ] {
        let separator = char::from_u32(scalar).ok_or("Invalid separator")?;
        inputs.push(format!(
            "http://one.test:80{separator}socks5://two.test:1080"
        ));
    }
    for scalar in [0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0xa0, 0x2003, 0x200b] {
        let space = char::from_u32(scalar).ok_or("Invalid whitespace")?;
        inputs.extend([
            format!("{space}http://host.test:8080:US{space}"),
            format!("{space}#comment{space}"),
            format!("{space}host.test{space}:8080:US{space}"),
            format!("host.test:8080:{space}us-east{space}"),
        ]);
    }
    inputs.extend([
        String::new(),
        "\r\n".into(),
        "one.test:80\r\ntwo.test:81\n".into(),
        "one.test:80,two.test:81".into(),
        "one.test:80\u{1f}two.test:81".into(),
        "HTTPS://user:pass@[2001:db8::1]:443:DE".into(),
        "host.test:8_080".into(),
        "host.test:٨٠٨٠".into(),
    ]);
    let input = temporary.path().join("input.json");
    fs::write(&input, serde_json::to_vec(&inputs)?)?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import json,sys
from pathlib import Path
repository,parent,input_file=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.proxypool import parse_line,parse_many,to_line
from run_logging import run_log
inputs=json.loads(input_file.read_text(encoding='utf-8'))
with run_log('pool-text-oracle',parent/'logs') as log:
    rows=[]
    for text in inputs:
        parsed=parse_line(text)
        rows.append({'line':parsed,'many':parse_many(text),
                     'rendered':None if parsed is None else to_line(parsed)})
    log.info('Observed pooled proxy text cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path())
        .arg(&input);
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original pool text observer failed");
    let expected: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(expected.len(), inputs.len());
    assert_eq!(inputs.len(), 50);
    for (text, expected) in inputs.iter().zip(expected) {
        let parsed = parse_line(text);
        let rendered = parsed.as_ref().map(to_line).transpose()?;
        let actual = json!({"line":parsed,"many":parse_many(text),"rendered":rendered});
        assert_eq!(actual, expected, "Pooled proxy text boundaries differ");
    }
    assert_eq!(fs::read(&input)?, serde_json::to_vec(&inputs)?);
    temporary.cleanup()?;
    Ok(())
}

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
