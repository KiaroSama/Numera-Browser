use numera_core::browser::{adapter::Adapter, cookie_format};
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
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases()}}), flush=True)
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
    assert!(cookie_format::to_netscape(&[Value::Null]).is_err());
    Ok(())
}
