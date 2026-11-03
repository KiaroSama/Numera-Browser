use numera_core::pool::normalize;
use serde_json::Value;

mod common;

#[test]
fn pooled_proxy_integer_and_whitespace_normalization_matches_original()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, pool::normalize_many};
    use serde_json::json;
    use std::{fs, path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("pool-normalization")?;
    let base =
        json!({"type":"HTTP","host":"host.test","port":"8080","user":"fixture","pass":"synthetic"});
    let mut inputs = Vec::new();
    for port in [
        "8_080", "٨٠٨٠", "+00080", "-00001", "0", "65535", "65536", "-0", " 80 ", "8.0", "8e1",
        "8__0", "_80", "80_", "+_80", "", "++80", "²",
    ] {
        let mut row = base.clone();
        row["port"] = port.into();
        inputs.push(row);
    }
    for port in [
        "0".repeat(100) + "80",
        "9".repeat(100),
        "-".to_owned() + &"9".repeat(100),
        "0".repeat(4298) + "80",
        "0".repeat(4299) + "80",
    ] {
        let mut row = base.clone();
        row["port"] = port.into();
        inputs.push(row);
    }
    for scalar in [0x1c, 0x1d, 0x1e, 0x1f, 0xa0, 0x2003] {
        let space = char::from_u32(scalar).ok_or("Invalid whitespace")?;
        for field in ["host", "country", "port"] {
            let mut row = base.clone();
            let value = match field {
                "host" => "host.test",
                "country" => "us",
                _ => "80",
            };
            row[field] = format!("{space}{value}{space}").into();
            inputs.push(row);
        }
    }
    for port in [
        Value::Null,
        json!(true),
        json!(80),
        json!(80.0),
        json!([]),
        json!({}),
    ] {
        let mut row = base.clone();
        row["port"] = port;
        inputs.push(row);
    }
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
import copy,json,sys
from pathlib import Path
repository,parent,input_file=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.proxypool import normalize,normalize_many
from run_logging import run_log
inputs=json.loads(input_file.read_text(encoding='utf-8'))
def observe(function,value):
    before=copy.deepcopy(value)
    try: result={'result':function(value),'error':None}
    except ValueError as error: result={'result':None,'error':str(error)}
    assert value==before,'Original pool normalization changed input'
    return result
with run_log('pool-normalization-oracle',parent/'logs') as log:
    rows=[observe(normalize,value) for value in inputs]
    batches=[observe(normalize_many,[value]) for value in inputs]
    log.info('Observed pooled normalization cases=%s',len(rows))
    print(json.dumps({'rows':rows,'batches':batches},ensure_ascii=True))
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
    assert_eq!(
        original.exit, 0,
        "Original pool normalization observer failed"
    );
    let expected: Value = serde_json::from_slice(&original.stdout)?;
    assert_eq!(inputs.len(), 47);
    let observe = |result: std::io::Result<Value>| match result {
        Ok(value) => json!({"result":value,"error":null}),
        Err(error) => json!({"result":null,"error":error.to_string()}),
    };
    for (index, value) in inputs.iter().enumerate() {
        let before = value.clone();
        assert_eq!(
            observe(normalize(value)),
            expected["rows"][index],
            "Pooled proxy normalization differs"
        );
        assert_eq!(
            observe(normalize_many(&json!([value])).map(Value::Array)),
            expected["batches"][index],
            "Pooled proxy batch normalization differs"
        );
        assert_eq!(value, &before);
    }
    assert_eq!(fs::read(&input)?, serde_json::to_vec(&inputs)?);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn structured_proxy_fields_are_normalized_or_refused_like_the_source()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/pool-normalize.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Normalize cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        match normalize(&case["input"]) {
            Ok(value) => {
                assert!(case["error"].is_null(), "case {index}");
                assert_eq!(value, case["result"], "case {index}");
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "case {index}"
            ),
        }
    }
    Ok(())
}
